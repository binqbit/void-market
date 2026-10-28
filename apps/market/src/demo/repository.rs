use super::model::{Actor, Deployment};
use crate::http::error::{ApiError, Result};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn actor(pool: &PgPool, id: Uuid) -> Result<Actor> {
    sqlx::query_as("SELECT * FROM demo_accounts WHERE account_id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::Forbidden)
}

pub(crate) async fn deployment(pool: &PgPool) -> Result<Deployment> {
    sqlx::query_as("SELECT * FROM localnet_config WHERE singleton")
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::Unavailable)
}
