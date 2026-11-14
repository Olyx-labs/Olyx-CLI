//! Local usage-cost accounting; these values are telemetry, not provider billing.
use crate::config::Provider;
use serde_json::Value;

pub(crate) enum MissingUsage {
    Zero,
    Estimate(f64),
}
fn price(provider: &Provider, input: f64, output: f64) -> f64 {
    (input * provider.input_per_million + output * provider.output_per_million) / 1_000_000.0
}
pub(crate) fn chat(provider: &Provider, output: &Value, missing: MissingUsage) -> f64 {
    let input = output["usage"]["prompt_tokens"].as_u64();
    let completion = output["usage"]["completion_tokens"].as_u64();
    if (input.is_none() || completion.is_none())
        && let MissingUsage::Estimate(cost) = missing
    {
        return cost;
    }
    price(
        provider,
        input.unwrap_or(0) as f64,
        completion.unwrap_or(0) as f64,
    )
}
pub(crate) fn embeddings(provider: &Provider, output: &Value) -> f64 {
    price(
        provider,
        output["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as f64,
        0.0,
    )
}
pub(crate) fn estimate_chat(
    provider: &Provider,
    payload: &Value,
    default_output_limit: u64,
) -> f64 {
    let output_limit = payload
        .get("max_completion_tokens")
        .or_else(|| payload.get("max_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(default_output_limit);
    price(
        provider,
        (payload.to_string().len() + 4096) as f64,
        output_limit as f64,
    )
}

#[cfg(all(test, private_regression_tests))]
mod private_regression_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../private-tests/usage_cost.rs"
    ));
}
