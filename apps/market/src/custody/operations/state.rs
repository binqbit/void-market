use super::{Custody, Work};
use crate::http::error::{ApiError, Result};
use serde::{Serialize, de::DeserializeOwned};

pub(super) fn seal<T: Serialize>(custody: &Custody, work: &Work, value: &T) -> Result<Vec<u8>> {
    custody.vault.seal(
        work.message,
        "response-state",
        &serde_json::to_vec(value).map_err(|_| ApiError::Internal)?,
    )
}

pub(super) fn restore<T: DeserializeOwned>(custody: &Custody, work: &Work) -> Result<T> {
    let (id, sealed) = work.prior.as_ref().ok_or(ApiError::Conflict(
        "The previous response checkpoint is missing",
    ))?;
    let bytes = custody.vault.open(*id, "response-state", sealed)?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::Internal)
}
