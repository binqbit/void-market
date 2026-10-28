use crate::http::error::{ApiError, Result};

const SCALE: u64 = 100_000_000;
pub const MAX_AMOUNT: u64 = i64::MAX as u64;

/// Decimal API strings avoid floating-point loss and JavaScript integer limits.
pub fn parse(value: &str) -> Result<u64> {
    if value.is_empty() || value.len() > 32 {
        return Err(ApiError::BadRequest(
            "Enter a ZEC amount with up to eight decimal places",
        ));
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| {
            part.is_empty() || part.len() > 8 || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(ApiError::BadRequest(
            "Enter a ZEC amount with up to eight decimal places",
        ));
    }
    let whole = whole
        .parse::<u64>()
        .map_err(|_| ApiError::BadRequest("ZEC amount is too large"))?;
    let fraction = fraction.unwrap_or("");
    let decimals = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u64>()
            .map_err(|_| ApiError::BadRequest("Invalid ZEC amount"))?
            * 10u64.pow(8 - fraction.len() as u32)
    };
    whole
        .checked_mul(SCALE)
        .and_then(|value| value.checked_add(decimals))
        .filter(|value| *value <= MAX_AMOUNT)
        .ok_or(ApiError::BadRequest("ZEC amount is too large"))
}

pub fn format(value: u64) -> String {
    format!("{}.{:08}", value / SCALE, value % SCALE)
}
