use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
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
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub input_per_million: f64,
    pub output_per_million: f64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub max_body_bytes: usize,
    pub max_output_tokens: u64,
    pub monthly_budget_usd: Option<f64>,
    pub retention_days: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_body_bytes: 262_144,
            max_output_tokens: 1024,
            monthly_budget_usd: None,
            retention_days: 30,
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
            || p.retention_days == 0
        {
            bail!("policy limits must be positive; max_body_bytes must not exceed 8 MiB");
        }
        if p.monthly_budget_usd
            .is_some_and(|n| !n.is_finite() || n <= 0.0)
        {
            bail!("monthly_budget_usd must be positive");
        }
        for candidates in self.models.values() {
            if candidates.is_empty() {
                bail!("each model route needs a provider");
            }
            for candidate in candidates {
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
                if candidate.model.is_empty()
                    || candidate.api_key_env.is_empty()
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
    Anthropic,
    Gemini,
}
