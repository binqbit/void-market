use super::{
    model::{Asset, RegisterAsset},
    repository,
};
use crate::{
    events,
    http::error::{ApiError, Result},
    media,
    state::AppState,
};
use serde_json::json;
use uuid::Uuid;

pub async fn register(state: &AppState, actor: Uuid, input: RegisterAsset) -> Result<Asset> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 128 || input.description.chars().count() > 2048 {
        return Err(ApiError::BadRequest("Invalid asset name or description"));
    }
    if let Some(image) = &input.image {
        media::validate_owned_image(state, actor, image).await?;
    }

    let mut transaction = state.pool.begin().await?;
    let inserted = repository::insert_if_absent(&mut transaction, actor, &input).await?;
    let asset = repository::lock(&mut transaction, input.request_id).await?;

    if asset.owner_id != actor
        || asset.name != name
        || asset.description != input.description
        || asset.image != input.image
    {
        return Err(ApiError::Conflict(
            "Asset ID is already registered with different ownership or metadata",
        ));
    }

    if inserted {
        events::record(
            &mut transaction,
            "AssetRegistered.v1",
            json!({"asset_id": asset.id, "owner_id": actor}),
        )
        .await?;
    }
    transaction.commit().await?;
    Ok(asset)
}
