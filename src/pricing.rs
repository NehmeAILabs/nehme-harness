/// Estimate cost for given token counts and per-million-token prices.
/// Returns 0.0 if either price is 0.0.
#[allow(dead_code)]
pub fn estimate_cost(
    input_tokens: u64,
    output_tokens: u64,
    input_token_cost: f64,
    output_token_cost: f64,
) -> f64 {
    let input_cost = input_tokens as f64 * input_token_cost / 1_000_000.0;
    let output_cost = output_tokens as f64 * output_token_cost / 1_000_000.0;
    input_cost + output_cost
}

/// Estimate cost accounting for cache hit pricing.
/// When `cached_input_token_cost` is Some, cached tokens are billed at the cheaper rate
/// and the remaining (miss) tokens at the standard rate.
/// When None, all input tokens are billed at the standard rate (falls back to estimate_cost).
pub fn estimate_cost_detailed(
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
    input_token_cost: f64,
    output_token_cost: f64,
    cached_input_token_cost: Option<f64>,
) -> f64 {
    let cache_miss_tokens = input_tokens.saturating_sub(cached_input_tokens);
    let miss_cost = cache_miss_tokens as f64 * input_token_cost / 1_000_000.0;
    let output_cost = output_tokens as f64 * output_token_cost / 1_000_000.0;
    let cache_cost = match cached_input_token_cost {
        Some(cached_cost) => cached_input_tokens as f64 * cached_cost / 1_000_000.0,
        None => cached_input_tokens as f64 * input_token_cost / 1_000_000.0,
    };
    miss_cost + output_cost + cache_cost
}
