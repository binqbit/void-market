use super::model::Account;
use crate::http::error::{ApiError, Result};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Account> {
    sqlx::query_as(
        "SELECT id AS address, name
         FROM accounts
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

pub async fn exists(connection: &mut PgConnection, id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1
         FROM accounts
         WHERE id = $1)",
    )
    .bind(id)
    .fetch_one(connection)
    .await?)
}
