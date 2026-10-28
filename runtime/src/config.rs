use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_database")]
    pub database: String,
    pub models: BTreeMap<String, Vec<Provider>>,
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<Plugin>,
    /// Explicit additional response identifiers known to share this vector space.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedding_response_models: Vec<String>,
    #[serde(default)]
    pub input_per_million: f64,
    #[serde(default)]
    pub output_per_million: f64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub max_body_bytes: usize,
    pub max_output_tokens: u64,
    pub retention_days: u32,
    pub max_embedding_response_bytes: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_body_bytes: 262_144,
            max_output_tokens: 1024,
            retention_days: 30,
            max_embedding_response_bytes: 67_108_864,
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let mut config: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        config.validate()?;
        if config.database != ":memory:" && std::path::Path::new(&config.database).is_relative() {
            let base = std::path::Path::new(path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            config.database = base.join(&config.database).to_string_lossy().into_owned();
        }
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.models.is_empty() || self.database.is_empty() {
            bail!("models and database are required");
        }
        let p = &self.policy;
        if p.max_body_bytes == 0
            || p.max_body_bytes > 8_388_608
            || p.max_output_tokens == 0
            || p.max_embedding_response_bytes == 0
            || p.max_embedding_response_bytes > 1_073_741_824
            || p.retention_days == 0
        {
            bail!(
                "policy limits must be positive; max_body_bytes must not exceed 8 MiB and max_embedding_response_bytes must not exceed 1 GiB"
            );
        }
        for candidates in self.models.values() {
            if candidates.is_empty() {
                bail!("each model route needs a provider");
            }
            for candidate in candidates {
                if candidate.protocol == Protocol::Plugin {
                    let plugin = candidate
                        .plugin
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("plugin configuration required"))?;
                    if !std::path::Path::new(&plugin.command).is_absolute()
                        || plugin.capabilities.is_empty()
                        || (plugin
                            .capabilities
                            .iter()
                            .any(|c| c == "tools" || c == "streaming")
                            && !plugin.capabilities.iter().any(|c| c == "chat"))
                        || plugin.capabilities.iter().any(|c| {
                            !["chat", "embeddings", "streaming", "tools"].contains(&c.as_str())
                        })
                        || !(100..=300_000).contains(&plugin.timeout_ms)
                        || candidate.model.is_empty()
                        || !candidate.base_url.is_empty()
                    {
                        bail!(
                            "plugin requires an absolute command, declared capabilities, model, timeout 100..300000 ms, and no base_url"
                        );
                    }
                    if !candidate.input_per_million.is_finite()
                        || candidate.input_per_million < 0.0
                        || !candidate.output_per_million.is_finite()
                        || candidate.output_per_million < 0.0
                    {
                        bail!("plugin prices must be finite and nonnegative");
                    }
                    if plugin
                        .env
                        .iter()
                        .any(|name| name.is_empty() || name.contains('=') || name.contains('\0'))
                    {
                        bail!("plugin environment entries must be variable names");
                    }
                    continue;
                }
                if candidate.plugin.is_some() {
                    bail!("plugin settings require protocol plugin");
                }
                let url = reqwest::Url::parse(&candidate.base_url)?;
                let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
                if url.scheme() != "https" && !(url.scheme() == "http" && local) {
                    bail!("provider URLs require HTTPS except on loopback");
                }
                if !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || url.host_str().is_none()
                {
                    bail!("provider URL must not contain credentials, query or fragment");
                }
                if matches!(
                    url.host_str(),
                    Some("169.254.169.254" | "metadata.google.internal")
                ) {
                    bail!("cloud metadata endpoints cannot be providers");
                }
                if candidate
                    .embedding_response_models
                    .iter()
                    .any(|name| name.trim().is_empty())
                {
                    bail!("embedding_response_models must contain nonempty model identifiers");
                }
                if candidate.model.is_empty()
                    || (candidate.api_key_env.is_empty() && !local)
                    || !candidate.input_per_million.is_finite()
                    || candidate.input_per_million < 0.0
                    || !candidate.output_per_million.is_finite()
                    || candidate.output_per_million < 0.0
                {
                    bail!(
                        "provider model, credential environment name, and nonnegative prices are required"
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    #[default]
    Openai,
    Plugin,
}

fn default_database() -> String {
    "data/traces.db".into()
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default = "plugin_timeout")]
    pub timeout_ms: u64,
}
fn plugin_timeout() -> u64 {
    60_000
}

#[cfg(test)]
mod default_tests {
    use super::*;
    #[test]
    fn minimal_configuration_has_local_defaults() {
        let config: Config = serde_json::from_str(r#"{"models":{"fast":[{"base_url":"https://api.openai.com/v1","model":"demo","api_key_env":"OPENAI_API_KEY"}]}}"#).unwrap();
        config.validate().unwrap();
        assert_eq!(config.database, "data/traces.db");
        assert_eq!(config.models["fast"][0].input_per_million, 0.0);
        assert_eq!(config.models["fast"][0].output_per_million, 0.0);
    }
    #[test]
    fn only_loopback_http_can_omit_credentials() {
        let mut config:Config=serde_json::from_value(serde_json::json!({"models":{"test":[{"base_url":"http://127.0.0.1:11434/v1","model":"test"}]}})).unwrap();
        config.validate().unwrap();
        config.models.get_mut("test").unwrap()[0].base_url = "https://example.com/v1".into();
        assert!(config.validate().is_err());
    }
}
