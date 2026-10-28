use super::model::{FrozenInput, Participant};
use crate::{
    custody::keys,
    http::error::{ApiError, Result},
    state::AppState,
};
use private_check::NumberInput;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn digest<T: serde::Serialize>(domain: &str, value: &T) -> Result<Vec<u8>> {
    let mut digest = Sha256::new();
    digest.update(domain.as_bytes());
    digest.update([0]);
    digest.update(serde_json::to_vec(value).map_err(|_| ApiError::Internal)?);
    Ok(digest.finalize().to_vec())
}

pub fn seal(state: &AppState, auction: Uuid, owner: Uuid, value: u64) -> Result<Vec<u8>> {
    state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .seal(
            owner,
            &format!("auction-input:{auction}"),
            &value.to_le_bytes(),
        )
}

pub fn amount(state: &AppState, auction: Uuid, participant: &Participant) -> Result<u64> {
    let bytes = state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .open(
            participant.account_id,
            &format!("auction-input:{auction}"),
            &participant.input_sealed,
        )?;
    Ok(u64::from_le_bytes(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| ApiError::Internal)?,
    ))
}

pub async fn encrypt(state: &AppState, owner: Uuid, value: u64, width: u8) -> Result<FrozenInput> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?.clone();
    let permit = state
        .auction_preparations
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::Unavailable)?;
    let material = keys::material(&state.pool, owner).await?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let binding = material.binding().map_err(|_| ApiError::Internal)?;
        let encoded = custody
            .artifacts
            .read(&material.public_key, tfhe_math::codec::PUBLIC_KEY_LIMIT)
            .map_err(|_| ApiError::Unavailable)?;
        let public = tfhe_math::codec::import_public_key(
            &encoded,
            binding,
            tfhe_math::codec::EncodingLimits::default(),
        )
        .map_err(|_| ApiError::Unavailable)?;
        let number = NumberInput::encrypt(
            &tfhe_math::EncryptionContext::new(public),
            check_workflow::messages::width(width).map_err(|_| ApiError::Internal)?,
            value,
        )
        .map_err(|_| ApiError::Unavailable)?;
        let ciphertext = custody
            .artifacts
            .write(number.encoded().as_bytes())
            .map_err(|_| ApiError::Unavailable)?;
        Ok(FrozenInput {
            owner,
            material,
            ciphertext,
        })
    })
    .await
    .map_err(|_| ApiError::Internal)?
}
