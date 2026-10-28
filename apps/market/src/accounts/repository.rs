use super::model::{Account, Profile, UpdateProfile};
use crate::http::error::{ApiError, Result};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Account> {
    sqlx::query_as("SELECT id AS address, name, username FROM accounts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn profile(pool: &PgPool, id: Uuid) -> Result<Profile> {
    sqlx::query_as(
        "SELECT
             id AS address,
             name,
             username,
             email,
             is_demo
         FROM accounts
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

pub async fn update_profile(pool: &PgPool, id: Uuid, input: UpdateProfile) -> Result<Profile> {
    let name = input.name.trim();
    let username = input.username.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::BadRequest("Name must contain 1 to 64 characters"));
    }
    if !(3..=24).contains(&username.len())
        || !username.as_bytes()[0].is_ascii_lowercase()
        || !username
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ApiError::BadRequest(
            "Use 3 to 24 lowercase letters, numbers or underscores, starting with a letter",
        ));
    }

    let updated = sqlx::query_as::<_, Profile>(
        "UPDATE accounts
         SET
             name = $2,
             username = $3
         WHERE
             id = $1
             AND (
                 username IS NULL
                 OR username = $3
             )
         RETURNING
             id AS address,
             name,
             username,
             email,
             is_demo",
    )
    .bind(id)
    .bind(name)
    .bind(username)
    .fetch_optional(pool)
    .await?;
    updated.ok_or(ApiError::Conflict(
        "Your reserved username cannot be changed",
    ))
}

pub async fn exists(connection: &mut PgConnection, id: Uuid) -> Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1)")
            .bind(id)
            .fetch_one(connection)
            .await?,
    )
}
