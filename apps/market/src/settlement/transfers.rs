use super::{
    auction,
    model::{EvmPlan, Payload},
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
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTransfer {
    pub request_id: Uuid,
    pub recipient_address: Uuid,
}

pub async fn prepare(
    state: &AppState,
    account: Uuid,
    asset: Uuid,
    input: NativeTransfer,
) -> Result<Uuid> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let sender = demo::repository::actor(&state.pool, account).await?;
    let recipient = demo::repository::actor(&state.pool, input.recipient_address).await?;
    if account == input.recipient_address {
        return Err(ApiError::BadRequest("Recipient must differ from sender"));
    }
    let mut tx = state.pool.begin().await?;
    let item = assets::repository::lock(&mut tx, asset).await?;
    let existing: Option<(Uuid, Option<Uuid>, Option<Uuid>)> =
        sqlx::query_as("SELECT account_id,recipient_id,asset_id FROM chain_operations WHERE id=$1")
            .bind(input.request_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some((owner, receiver, existing_asset)) = existing {
        if owner == account
            && receiver == Some(input.recipient_address)
            && existing_asset == Some(asset)
        {
            return Ok(input.request_id);
        }
        return Err(ApiError::Conflict("Transfer request has different details"));
    }
    if item.owner_id != account {
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
    let payload = Payload::Ethereum(Box::new(EvmPlan {
        sender: sender.ethereum_address.clone(),
        contract: collection.clone(),
        calldata: abi::transfer(
            &sender.ethereum_address,
            &recipient.ethereum_address,
            &token,
        )?,
        movement: NftMovement {
            collection,
            token_id: token,
            from: sender.ethereum_address,
            to: recipient.ethereum_address,
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
            recipient: Some(input.recipient_address),
            auction: None,
            asset: Some(asset),
            kind: "transfer",
            payload,
            approval: None,
        },
    )
    .await?;
    sqlx::query("UPDATE assets SET pending_transaction_id=$2 WHERE id=$1")
        .bind(asset)
        .bind(input.request_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(input.request_id)
}

pub async fn cancel(state: &AppState, account: Uuid, id: Uuid) -> Result<()> {
    let original = repository::get(&state.pool, id).await?;
    if original.account_id != account || original.kind != "transfer" {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.pool.begin().await?;
    let asset = original.asset_id.ok_or(ApiError::Internal)?;
    assets::repository::lock(&mut tx, asset).await?;
    let operation = repository::lock(&mut tx, id).await?;
    if operation.status != "failed"
        && (operation.approval.is_some() || operation.signed_transaction.is_some())
    {
        return Err(ApiError::Conflict(
            "An authorized transaction must be reconciled before cancellation",
        ));
    }
    repository::status(&mut tx, id, "cancelled", None).await?;
    sqlx::query(
        "UPDATE assets SET pending_transaction_id=NULL WHERE id=$1 AND pending_transaction_id=$2",
    )
    .bind(asset)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
