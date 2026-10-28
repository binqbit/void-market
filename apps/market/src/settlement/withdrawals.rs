use super::{
    auction,
    model::{EvmPlan, Operation, Payload},
    repository::{self, NewOperation},
};
use crate::{
    assets,
    chains::ethereum::{NftMovement, abi},
    demo,
    http::error::{ApiError, Result},
    state::AppState,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawNft {
    pub request_id: Uuid,
    pub recipient_address: String,
}

pub async fn prepare(
    state: &AppState,
    account: Uuid,
    asset: Uuid,
    input: WithdrawNft,
) -> Result<Uuid> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let sender = demo::repository::actor(&state.pool, account).await?;
    let destination = abi::checked_address(input.recipient_address.trim())?;
    if destination == sender.ethereum_address {
        return Err(ApiError::BadRequest(
            "Choose an address outside your platform wallet",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let item = assets::repository::lock(&mut tx, asset).await?;
    let existing: Option<Operation> =
        sqlx::query_as("SELECT * FROM chain_operations WHERE id = $1")
            .bind(input.request_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(existing) = existing {
        if existing.account_id == account
            && existing.asset_id == Some(asset)
            && existing.kind == "withdrawal"
            && matches!(repository::payload(state, &existing)?, Payload::Ethereum(plan) if plan.movement.to == destination)
        {
            return Ok(input.request_id);
        }
        return Err(ApiError::Conflict(
            "Withdrawal request has different details",
        ));
    }
    if item.owner_id != Some(account) {
        return Err(ApiError::Forbidden);
    }
    if item.active_auction_id.is_some() || item.pending_transaction_id.is_some() {
        return Err(ApiError::Conflict(
            "The NFT already has a pending operation",
        ));
    }
    let (collection, token) = auction::descriptor(
        state,
        item.nft
            .as_ref()
            .ok_or(ApiError::Conflict("This asset is not a native NFT"))?,
    )
    .await?;
    if state
        .chains
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .ethereum
        .owner(&collection, &token, json!("latest"))
        .await?
        != sender.ethereum_address
    {
        return Err(ApiError::Conflict(
            "Your platform wallet no longer owns this NFT",
        ));
    }
    let payload = Payload::Ethereum(Box::new(EvmPlan {
        sender: sender.ethereum_address.clone(),
        contract: collection.clone(),
        calldata: abi::transfer(&sender.ethereum_address, &destination, &token)?,
        movement: NftMovement {
            collection,
            token_id: token,
            from: sender.ethereum_address,
            to: destination,
        },
        transaction: None,
        metadata: None,
    }));
    repository::insert(
        state,
        &mut tx,
        NewOperation {
            id: input.request_id,
            account,
            recipient: None,
            auction: None,
            asset: Some(asset),
            kind: "withdrawal",
            payload,
            approval: None,
        },
    )
    .await?;
    sqlx::query("UPDATE assets SET pending_transaction_id = $2 WHERE id = $1")
        .bind(asset)
        .bind(input.request_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(input.request_id)
}

pub async fn cancel(state: &AppState, account: Uuid, id: Uuid) -> Result<()> {
    let original = repository::get(&state.pool, id).await?;
    if original.account_id != account
        || !matches!(original.kind.as_str(), "transfer" | "withdrawal")
    {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.pool.begin().await?;
    let asset = original.asset_id.ok_or(ApiError::Internal)?;
    assets::repository::lock(&mut tx, asset).await?;
    let operation = repository::lock(&mut tx, id).await?;
    if operation.signed_transaction.is_some() || operation.transaction_id.is_some() {
        verify_reverted(state, &operation).await?;
    } else if operation.status != "failed" && operation.approval.is_some() {
        return Err(ApiError::Conflict(
            "An authorized transaction must be reconciled before cancellation",
        ));
    }
    repository::status(&mut tx, id, "cancelled", None).await?;
    sqlx::query(
        "UPDATE assets
         SET pending_transaction_id = NULL
         WHERE
             id = $1
             AND pending_transaction_id = $2",
    )
    .bind(asset)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn verify_reverted(state: &AppState, operation: &Operation) -> Result<()> {
    if operation.status == "failed"
        && operation.error.as_deref() == Some("Ethereum transaction reverted")
        && let Some(id) = operation.transaction_id.as_deref()
    {
        let ethereum = &state.chains.as_ref().ok_or(ApiError::Unavailable)?.ethereum;
        match ethereum.receipt(id).await {
            Err(ApiError::Conflict("Ethereum transaction reverted")) => return Ok(()),
            Err(ApiError::Unavailable) => return Err(ApiError::Unavailable),
            _ => {}
        }
    }
    Err(ApiError::Conflict(
        "An authorized transaction must be reconciled before cancellation",
    ))
}
