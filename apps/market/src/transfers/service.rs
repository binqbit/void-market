use super::{
    model::{Transfer, TransferAsset},
    repository,
};
use crate::{
    accounts::repository as accounts,
    assets::repository as assets,
    events,
    http::error::{ApiError, Result},
    state::AppState,
};
use serde_json::json;
use uuid::Uuid;

pub async fn transfer(
    state: &AppState,
    actor: Uuid,
    asset_id: Uuid,
    input: TransferAsset,
) -> Result<Transfer> {
    let mut tx = state.pool.begin().await?;
    let asset = assets::lock(&mut tx, asset_id).await?;

    let existing = repository::find(&mut tx, input.request_id).await?;
    if let Some(existing) = existing {
        if existing.sender_id != actor
            || existing.asset_id != asset_id
            || existing.recipient_id != input.recipient_address
        {
            return Err(ApiError::Conflict(
                "Request ID was already used with different input",
            ));
        }
        return Ok(existing);
    }

    if asset.owner_id != actor {
        return Err(ApiError::Forbidden);
    }
    if asset.active_auction_id.is_some() {
        return Err(ApiError::Conflict("This asset is locked in an auction"));
    }
    if actor == input.recipient_address {
        return Err(ApiError::BadRequest("Recipient must differ from sender"));
    }

    let recipient_exists = accounts::exists(&mut tx, input.recipient_address).await?;
    if !recipient_exists {
        return Err(ApiError::BadRequest("Recipient account does not exist"));
    }

    assets::change_owner(&mut tx, asset_id, input.recipient_address).await?;
    let transfer = repository::insert(
        &mut tx,
        input.request_id,
        asset_id,
        actor,
        input.recipient_address,
    )
    .await?;
    events::record(
        &mut tx,
        "AssetTransferred.v1",
        json!({
            "asset_id": asset_id,
            "sender_id": actor,
            "recipient_id": input.recipient_address,
            "transfer_id": input.request_id,
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(transfer)
}
