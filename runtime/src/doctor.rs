//! Local setup diagnostics. Probes never send credentials or inference content.
use crate::config::Config;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path, time::Duration};

pub async fn run(path: &str, probe: bool) -> Value {
    let mut checks = Vec::new();
    let config = match Config::load(path) {
        Ok(config) => {
            checks.push(json!({"check":"configuration","status":"pass","message":"Configuration is valid."}));
            config
        }
        Err(_) => {
            return json!({"ok":false,"checks":[{"check":"configuration","status":"fail","message":"Configuration could not be read or validated.","hint":"Check the file path and run oy check FILE for validation details."}]});
        }
    };
    let variables: BTreeSet<_> = config
        .models
        .values()
        .flatten()
        .map(|p| p.api_key_env.as_str())
        .filter(|name| !name.is_empty())
        .collect();
    for variable in variables {
        let present = std::env::var(variable).is_ok_and(|key| !key.trim().is_empty());
        checks.push(json!({"check":"provider_credential","variable":variable,"status":if present {"pass"} else {"fail"},"message":if present {"Credential is present; validity is not checked."} else {"Credential is missing or empty."},"hint":"Export the configured variable. Restart a running gateway after credential changes."}));
    }
    match storage(&config.database) {
        Ok(message) => checks.push(json!({"check":"storage","status":"pass","message":message})),
        Err(()) => checks.push(json!({"check":"storage","status":"fail","message":"Trace storage is not readable or writable.","hint":"Check the database path, SQLite integrity, and filesystem permissions."})),
    }
    if probe {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build();
        let endpoints: BTreeSet<_> = config
            .models
            .values()
            .flatten()
            .filter(|p| p.protocol == crate::config::Protocol::Openai)
            .map(|p| p.base_url.as_str())
            .collect();
        if let Ok(client) = client {
            for endpoint in endpoints {
                let host = reqwest::Url::parse(endpoint)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned));
                match client.head(endpoint).send().await {
                    Ok(response) => checks.push(json!({"check":"provider_connectivity","status":"pass","provider":host,"http_status":response.status().as_u16(),"message":"Endpoint responded. This does not validate credentials, model access, or inference readiness."})),
                    Err(_) => checks.push(json!({"check":"provider_connectivity","status":"fail","provider":host,"message":"Endpoint could not be reached within five seconds.","hint":"Check DNS, TLS, network access, and the endpoint. No credentials were sent."})),
                }
            }
        } else {
            checks.push(json!({"check":"provider_connectivity","status":"fail","message":"Could not initialize the network client."}));
        }
    }
    json!({"ok":checks.iter().all(|c| c["status"] != "fail"),"network_probes":probe,"checks":checks})
}

fn storage(database: &str) -> Result<&'static str, ()> {
    if database == ":memory:" {
        return Ok("In-memory storage configured; traces will not persist.");
    }
    let path = Path::new(database);
    if path.exists() {
        if !path.is_file()
            || std::fs::metadata(path)
                .map_err(|_| ())?
                .permissions()
                .readonly()
        {
            return Err(());
        }
        let connection =
            rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|_| ())?;
        let status: String = connection
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
            .map_err(|_| ())?;
        if status != "ok" {
            return Err(());
        }
    }
    let mut parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    while !parent.exists() {
        parent = parent
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
    }
    let name = parent.join(format!(
        ".olyx-doctor-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&name)
        .map_err(|_| ())?;
    drop(file);
    std::fs::remove_file(name).map_err(|_| ())?;
    Ok(
        "SQLite checked read-only when present; parent directory write check passed. No trace retention or schema changes were applied.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_configuration_is_reported_without_echoing_file_contents() {
        let report = run("/definitely-not-an-olyx-config", false).await;
        assert_eq!(report["ok"], false);
        assert_eq!(report["checks"][0]["check"], "configuration");
    }
    #[test]
    fn storage_check_does_not_create_or_migrate_a_database() {
        let path = std::env::temp_dir().join(format!("olyx-doctor-test-{}.db", std::process::id()));
        assert!(!path.exists());
        storage(path.to_str().unwrap()).unwrap();
        assert!(!path.exists());
        assert!(storage(":memory:").is_ok());
    }
}
