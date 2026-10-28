use super::model::Transfer;
use crate::http::error::{ApiError, Result};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn find(connection: &mut PgConnection, id: Uuid) -> Result<Option<Transfer>> {
    Ok(sqlx::query_as(
        "SELECT id, asset_id, sender_id, recipient_id, auction_id, created_at::text
         FROM transfers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(connection)
    .await?)
}

pub async fn insert(
    connection: &mut PgConnection,
    id: Uuid,
    asset: Uuid,
    sender: Uuid,
    recipient: Uuid,
) -> Result<Transfer> {
    Ok(sqlx::query_as(
        "INSERT INTO transfers(id, asset_id, sender_id, recipient_id)
         VALUES ($1, $2, $3, $4)
         RETURNING id, asset_id, sender_id, recipient_id, auction_id, created_at::text",
    )
    .bind(id)
    .bind(asset)
    .bind(sender)
    .bind(recipient)
    .fetch_one(connection)
    .await?)
}

pub async fn get_for_account(pool: &PgPool, id: Uuid, account: Uuid) -> Result<Transfer> {
    sqlx::query_as(
        "SELECT id, asset_id, sender_id, recipient_id, auction_id, created_at::text
         FROM transfers WHERE id = $1 AND (sender_id = $2 OR recipient_id = $2)",
    )
    .bind(id)
    .bind(account)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

pub async fn list_for_account(
    pool: &PgPool,
    account: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Transfer>> {
    Ok(sqlx::query_as(
        "SELECT id, asset_id, sender_id, recipient_id, auction_id, created_at::text
         FROM transfers WHERE sender_id = $1 OR recipient_id = $1
         ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
    )
    .bind(account)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}
