use std::{env, fs};

use olyx_core::{RuntimeMode, RuntimeVerifier, SignedEnvelope};
use serde_json::{Value, json};

use crate::error::CliError;

// 3 MB: comfortably above any real signed artifact (a few KB) while blocking
// an adversarial large-file read from consuming excessive memory.
pub(crate) const MAX_SIGNED_ENVELOPE_FILE_BYTES: u64 = 3 * 1024 * 1024;

pub(crate) fn verify_runtime(
    license_path: Option<&str>,
    policy_path: Option<&str>,
    key_id: Option<&str>,
    public_key_b64: Option<&str>,
    public_key_path: Option<&str>,
) -> Result<olyx_core::RuntimeStatus, CliError> {
    // Runtime activation fails closed: the CLI only succeeds when both the
    // signed artifacts and the licensed runtime modes explicitly allow CLI use.
    let license_path = license_path
        .map(str::to_string)
        .or_else(|| env::var("OLYX_LICENSE_PATH").ok())
        .ok_or_else(|| {
            CliError::usage("missing license path; pass --license or set OLYX_LICENSE_PATH")
        })?;
    let policy_path = policy_path
        .map(str::to_string)
        .or_else(|| env::var("OLYX_POLICY_PACK_PATH").ok())
        .ok_or_else(|| {
            CliError::usage("missing policy path; pass --policy or set OLYX_POLICY_PACK_PATH")
        })?;
    let key_id = key_id
        .map(str::to_string)
        .or_else(|| env::var("OLYX_POLICY_KEY_ID").ok())
        .ok_or_else(|| {
            CliError::usage("missing key id; pass --key-id or set OLYX_POLICY_KEY_ID")
        })?;
    let public_key_b64 = public_key_b64
        .map(str::to_string)
        .or_else(|| env::var("OLYX_POLICY_PUBLIC_KEY_B64").ok())
        .map(Ok)
        .or_else(|| {
            let path = public_key_path
                .map(str::to_string)
                .or_else(|| env::var("OLYX_POLICY_PUBLIC_KEY_PATH").ok())?;
            Some(read_public_key_file(&path))
        })
        .transpose()?
        .ok_or_else(|| {
            CliError::usage(
                "missing public key; pass --public-key-b64, --public-key-file, or set OLYX_POLICY_PUBLIC_KEY_B64",
            )
        })?;

    let license = read_signed_envelope(&license_path)?;
    let policy_pack = read_signed_envelope(&policy_path)?;
    let verifier = RuntimeVerifier::from_public_key_b64(key_id, &public_key_b64)
        .map_err(|e| CliError::message(format!("runtime public key rejected: {e}")))?;
    let runtime = verifier
        .load_runtime_for_mode(&license, &policy_pack, RuntimeMode::Cli)
        .map_err(|e| CliError::message(format!("runtime verification failed: {e}")))?;
    let status = runtime.status();
    if !status.runtime_modes.contains(&RuntimeMode::Cli) {
        return Err(CliError::message(
            "runtime verification failed: license does not include CLI runtime mode",
        ));
    }

    Ok(status)
}

pub(crate) fn runtime_env_status() -> Value {
    json!({
        "license_path_present": env::var("OLYX_LICENSE_PATH").is_ok(),
        "policy_pack_path_present": env::var("OLYX_POLICY_PACK_PATH").is_ok(),
        "key_id_present": env::var("OLYX_POLICY_KEY_ID").is_ok(),
        "public_key_present": env::var("OLYX_POLICY_PUBLIC_KEY_B64").is_ok()
            || env::var("OLYX_POLICY_PUBLIC_KEY_PATH").is_ok(),
        "activation_command": "olyx verify-runtime"
    })
}

fn read_signed_envelope(path: &str) -> Result<SignedEnvelope, CliError> {
    let metadata = fs::metadata(path).map_err(|e| {
        CliError::message(format!(
            "failed to inspect signed envelope file {}: {e}",
            truncate_path(path)
        ))
    })?;
    if metadata.len() > MAX_SIGNED_ENVELOPE_FILE_BYTES {
        return Err(CliError::message(format!(
            "signed envelope file {} exceeds {} bytes",
            truncate_path(path),
            MAX_SIGNED_ENVELOPE_FILE_BYTES
        )));
    }

    let raw = fs::read_to_string(path).map_err(|e| {
        CliError::message(format!(
            "failed to read signed envelope file {}: {e}",
            truncate_path(path)
        ))
    })?;
    serde_json::from_str(&raw).map_err(|e| {
        CliError::message(format!(
            "invalid signed envelope JSON at {}: {e}",
            truncate_path(path)
        ))
    })
}

fn read_public_key_file(path: &str) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|e| {
        CliError::message(format!(
            "failed to read public key file {}: {e}",
            truncate_path(path)
        ))
    })
}

fn truncate_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.len() > 120 {
        format!("{}...", trimmed.chars().take(120).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
