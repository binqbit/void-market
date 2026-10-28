mod nft;
mod payment;

use super::{
    execution,
    model::{Operation, Payload},
    repository,
};
use crate::{
    assets, auctions,
    http::error::{ApiError, Result},
    state::AppState,
};
use serde_json::Value;

pub(crate) async fn apply(
    state: &AppState,
    operation: &Operation,
    payload: &Payload,
    evidence: Value,
) -> Result<()> {
    let mut tx = state.pool.begin().await?;
    if let Some(asset) = operation.asset_id {
        assets::repository::lock(&mut tx, asset).await?;
    }
    let parent = if let Some(id) = operation.auction_id {
        Some(auctions::lock(&mut tx, id).await?)
    } else {
        None
    };

    let current = repository::lock(&mut tx, operation.id).await?;
    if current.status == "confirmed" {
        return Ok(());
    }
    if current.transaction_id != operation.transaction_id {
        return Err(ApiError::Conflict("Native transaction identity changed"));
    }

    // Even a faucet receipt must refer to a submitted native transaction.
    operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;
    let bytes = serde_json::to_vec(&evidence).map_err(|_| ApiError::Internal)?;
    sqlx::query("UPDATE chain_operations SET receipt_sealed = $2 WHERE id = $1")
        .bind(operation.id)
        .bind(execution::seal_blob(state, operation, "receipt", &bytes)?)
        .execute(&mut *tx)
        .await?;

    match (operation.kind.as_str(), payload) {
        ("mint", Payload::Ethereum(plan)) => {
            nft::mint(state, &mut tx, operation, plan).await?;
        }
        ("deposit", Payload::Ethereum(_)) => {
            nft::deposit(&mut tx, parent.as_ref().ok_or(ApiError::Internal)?).await?;
        }
        ("return", Payload::Ethereum(_)) => {
            nft::returned(&mut tx, parent.as_ref().ok_or(ApiError::Internal)?).await?;
        }
        ("withdrawal", Payload::Ethereum(plan)) => {
            nft::withdrawal(&mut tx, operation, plan).await?;
        }
        ("delivery" | "transfer", Payload::Ethereum(_)) => {
            nft::delivery(&mut tx, operation, parent.as_ref()).await?;
        }
        ("payment", Payload::Zcash(plan)) => {
            payment::credit(
                state,
                &mut tx,
                operation,
                plan,
                parent.as_ref().ok_or(ApiError::Internal)?,
                &evidence,
            )
            .await?;
        }
        ("airdrop", Payload::Zcash(_)) => {}
        _ => return Err(ApiError::Internal),
    }

    repository::status(&mut tx, operation.id, "confirmed", None).await?;
    tx.commit().await?;
    Ok(())
}
