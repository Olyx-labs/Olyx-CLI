use crate::error::CliError;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::Path};

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
    models.insert(alias, entry.clone());
    models.insert(model, entry);
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
fn write_new(path: &Path, value: &Value) -> Result<(), CliError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| CliError::message(format!("cannot create {}: {e}", path.display())))?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| CliError::message(e.to_string()))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
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
        assert!(cfg["models"].get("gpt-4o-mini").is_some());
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
