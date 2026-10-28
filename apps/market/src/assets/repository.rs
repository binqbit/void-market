use super::model::{Asset, RegisterAsset};
use crate::http::error::{ApiError, Result};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Asset> {
    sqlx::query_as(
        "SELECT a.id, a.name, a.description, a.image, a.owner_id, a.created_at::text, a.nft, a.pending_transaction_id,
         (SELECT id FROM auctions WHERE asset_id=a.id AND status IN
        ('deposit_pending','open','matching','awaiting_completion') OR asset_id=a.id AND custody_status IN
        ('deposit_pending','held','return_pending','delivery_pending','review')) AS active_auction_id
         FROM assets a WHERE a.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

pub async fn lock(connection: &mut PgConnection, id: Uuid) -> Result<Asset> {
    // Identity is immutable. This serializes ownership changes while allowing foreign-key checks.
    sqlx::query_as(
        "SELECT a.id, a.name, a.description, a.image, a.owner_id, a.created_at::text, a.nft, a.pending_transaction_id,
         (SELECT id FROM auctions WHERE asset_id=a.id AND status IN
        ('deposit_pending','open','matching','awaiting_completion') OR asset_id=a.id AND custody_status IN
        ('deposit_pending','held','return_pending','delivery_pending','review')) AS active_auction_id
         FROM assets a WHERE a.id = $1 FOR NO KEY UPDATE OF a",
    )
    .bind(id)
    .fetch_optional(connection)
    .await?
    .ok_or(ApiError::NotFound)
}

pub async fn owned(pool: &PgPool, owner: Uuid, limit: i64, offset: i64) -> Result<Vec<Asset>> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.name, a.description, a.image, a.owner_id, a.created_at::text, a.nft, a.pending_transaction_id,
         (SELECT id FROM auctions WHERE asset_id=a.id AND status IN
        ('deposit_pending','open','matching','awaiting_completion') OR asset_id=a.id AND custody_status IN
        ('deposit_pending','held','return_pending','delivery_pending','review')) AS active_auction_id
         FROM assets a WHERE a.owner_id = $1
         ORDER BY a.created_at DESC, a.id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(owner)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn insert_if_absent(
    connection: &mut PgConnection,
    owner: Uuid,
    input: &RegisterAsset,
) -> Result<bool> {
    let inserted = sqlx::query(
        "INSERT INTO assets(id, name, description, image, owner_id)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(input.request_id)
    .bind(input.name.trim())
    .bind(&input.description)
    .bind(&input.image)
    .bind(owner)
    .execute(connection)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

pub async fn change_owner(connection: &mut PgConnection, id: Uuid, owner: Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE assets SET owner_id = $1
         WHERE id = $2",
    )
    .bind(owner)
    .bind(id)
    .execute(connection)
    .await?;
    Ok(())
}
