use crate::error::CliError;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

const PROVIDERS: &[(&str, &str, &str, &str, Option<&str>)] = &[
    (
        "openai",
        "openai",
        "https://api.openai.com/v1",
        "OPENAI_API_KEY",
        Some("gpt-4o-mini"),
    ),
    (
        "huggingface",
        "openai",
        "https://router.huggingface.co/v1",
        "HF_TOKEN",
        None,
    ),
];
fn configuration(
    provider: Option<&str>,
    model: Option<&str>,
    alias: &str,
    base: Option<&str>,
    available: impl Fn(&str) -> bool,
) -> Result<(Value, String), CliError> {
    let detected: Vec<_> = PROVIDERS.iter().filter(|p| available(p.3)).collect();
    let name = match provider {
        Some(name) => name,
        None => match detected.as_slice() {
            [] => "openai",
            [one] => one.0,
            _ => {
                return Err(CliError::usage(
                    "Multiple provider keys found; select one with --provider NAME",
                ))
            }
        },
    };
    let (_, protocol, url, key, default) = PROVIDERS
        .iter()
        .find(|p| p.0 == name)
        .ok_or_else(|| CliError::usage("unknown provider; see the provider setup guide"))?;
    let model = model.or(*default).ok_or_else(|| {
        CliError::usage(
            "this provider requires --model with an available model/deployment identifier",
        )
    })?;
    let base = base.unwrap_or(url);
    if base.is_empty() {
        return Err(CliError::usage(
            "this provider requires --base-url with your resource/project endpoint",
        ));
    }
    if model.is_empty() || alias.is_empty() {
        return Err(CliError::usage("model and alias must not be empty"));
    }
    let entry = json!([{"protocol":protocol,"base_url":base,"model":model,"api_key_env":key}]);
    let mut models = BTreeMap::new();
    models.insert(alias, entry);
    let value = json!({"models":models});
    let parsed: olyx_local_runtime::config::Config =
        serde_json::from_value(value.clone()).map_err(|e| CliError::message(e.to_string()))?;
    parsed
        .validate()
        .map_err(|e| CliError::message(e.to_string()))?;
    Ok((value, (*key).to_owned()))
}
pub(crate) fn create(
    path: &str,
    provider: Option<&str>,
    model: Option<&str>,
    alias: &str,
    base: Option<&str>,
) -> Result<(), CliError> {
    let path = Path::new(path);
    if path.exists() {
        return Err(CliError::usage(
            "configuration already exists; init does not overwrite files",
        ));
    }
    let (value, key) = configuration(provider, model, alias, base, |name| {
        std::env::var(name).is_ok_and(|v| !v.trim().is_empty())
    })?;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    if !directory.as_os_str().is_empty() {
        std::fs::create_dir_all(directory).map_err(|e| CliError::message(e.to_string()))?;
    }
    write_new(path, &value)?;
    let sample = directory.join("request.json");
    if !sample.exists() {
        write_new(
            &sample,
            &json!({"model":alias,"messages":[{"role":"user","content":"Hello!"}]}),
        )?;
    }
    println!(
        "Created {}. Provider credentials are read from {key}; no key was saved.",
        path.display()
    );
    println!("Check configuration: oy check {}", path.display());
    println!(
        "Start application endpoint: oy serve --config {}",
        path.display()
    );
    Ok(())
}
/// Generate an account-free adapter scaffold; the user supplies inference logic.
pub(crate) fn create_plugin(
    path: &str,
    python: &str,
    model: &str,
    alias: &str,
) -> Result<(), CliError> {
    let interpreter = Path::new(python);
    if !interpreter.is_absolute() || !interpreter.is_file() {
        return Err(CliError::usage(
            "--plugin requires an absolute path to an installed Python interpreter",
        ));
    }
    let config = Path::new(path);
    let directory = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if matches!(
        config.file_name().and_then(|name| name.to_str()),
        Some("request.json" | "adapter.py" | "olyx_plugin.py")
    ) {
        return Err(CliError::usage(
            "configuration path conflicts with a generated scaffold file",
        ));
    }
    std::fs::create_dir_all(directory).map_err(|e| CliError::message(e.to_string()))?;
    let script = directory
        .canonicalize()
        .map_err(|e| CliError::message(e.to_string()))?
        .join("adapter.py");
    let value = json!({"models":{alias:[{"protocol":"plugin","model":model,"plugin":{
        "command":python,"args":[script],"capabilities":["chat"]
    }}]}});
    let parsed: olyx_local_runtime::config::Config =
        serde_json::from_value(value.clone()).map_err(|e| CliError::message(e.to_string()))?;
    parsed
        .validate()
        .map_err(|e| CliError::message(e.to_string()))?;
    let files = vec![
        (
            directory.join("adapter.py"),
            include_bytes!("../examples/plugins/adapter.py").to_vec(),
        ),
        (
            directory.join("olyx_plugin.py"),
            include_bytes!("../examples/plugins/olyx_plugin.py").to_vec(),
        ),
        (
            directory.join("request.json"),
            json_bytes(&json!({"model":alias,"messages":[{"role":"user","content":"Hello!"}]}))?,
        ),
        (config.to_path_buf(), json_bytes(&value)?),
    ];
    write_scaffold(&files)?;
    println!("Created {} and a Python chat adapter scaffold. Implement chat() in {} before making requests. No inference is implemented or verified by the scaffold. Plugins run as trusted local programs.", config.display(), script.display());
    Ok(())
}
fn json_bytes(value: &Value) -> Result<Vec<u8>, CliError> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|e| CliError::message(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn write_new(path: &Path, value: &Value) -> Result<(), CliError> {
    write_new_bytes(path, &json_bytes(value)?)
}
fn write_scaffold(files: &[(PathBuf, Vec<u8>)]) -> Result<(), CliError> {
    let mut destinations = std::collections::BTreeSet::new();
    for (path, _) in files {
        if !destinations.insert(path) || path.exists() || path.is_symlink() {
            return Err(CliError::usage(format!(
                "{} already exists or conflicts with another scaffold file",
                path.display()
            )));
        }
    }
    write_scaffold_with(files, |file, bytes| file.write_all(bytes))
}
fn write_scaffold_with(
    files: &[(PathBuf, Vec<u8>)],
    mut write: impl FnMut(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
) -> Result<(), CliError> {
    let mut created = Vec::new();
    let result = (|| {
        for (path, bytes) in files {
            let mut file = open_new(path)?;
            created.push(path);
            write(&mut file, bytes)
                .map_err(|e| CliError::message(format!("cannot write {}: {e}", path.display())))?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut cleanup = Vec::new();
        for path in created.into_iter().rev() {
            if let Err(e) = std::fs::remove_file(path) {
                cleanup.push(format!("{}: {e}", path.display()));
            }
        }
        return if cleanup.is_empty() {
            Err(error)
        } else {
            Err(CliError::message(format!(
                "{error}; could not clean up: {}",
                cleanup.join(", ")
            )))
        };
    }
    Ok(())
}
fn open_new(path: &Path) -> Result<std::fs::File, CliError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| CliError::message(format!("cannot create {}: {e}", path.display())))
}
fn write_new_bytes(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    open_new(path)?
        .write_all(bytes)
        .map_err(|e| CliError::message(e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_one_provider_without_copying_credentials() {
        let (cfg, key) =
            configuration(None, None, "fast", None, |n| n == "OPENAI_API_KEY").unwrap();
        assert_eq!(key, "OPENAI_API_KEY");
        assert_eq!(cfg["models"]["fast"][0]["protocol"], "openai");
        assert_eq!(cfg["models"].as_object().unwrap().len(), 1);
    }
    #[test]
    fn ambiguous_keys_and_missing_huggingface_models_are_explicit() {
        assert!(configuration(None, None, "fast", None, |_| true).is_err());
        assert!(configuration(Some("huggingface"), None, "fast", None, |_| false).is_err());
        let (config, key) = configuration(
            Some("huggingface"),
            Some("example/model"),
            "fast",
            None,
            |_| false,
        )
        .unwrap();
        assert_eq!(key, "HF_TOKEN");
        assert_eq!(config["models"]["fast"][0]["model"], "example/model");
    }
}

#[cfg(all(test, private_regression_tests))]
mod private_regression_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/private-tests/init.rs"
    ));
}
