use super::model::{Operation, Payload};
use crate::{
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn get(pool: &PgPool, id: Uuid) -> Result<Operation> {
    sqlx::query_as("SELECT * FROM chain_operations WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

pub(crate) async fn lock(connection: &mut PgConnection, id: Uuid) -> Result<Operation> {
    sqlx::query_as("SELECT * FROM chain_operations WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or(ApiError::NotFound)
}

pub(crate) fn payload(state: &AppState, operation: &Operation) -> Result<Payload> {
    let bytes = state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .open(
            operation.account_id,
            &format!("chain-operation:{}", operation.id),
            &operation.payload_sealed,
        )?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::Internal)
}

pub(crate) fn sealed(
    state: &AppState,
    owner: Uuid,
    id: Uuid,
    payload: &Payload,
) -> Result<Vec<u8>> {
    state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .seal(
            owner,
            &format!("chain-operation:{id}"),
            &serde_json::to_vec(payload).map_err(|_| ApiError::Internal)?,
        )
}

pub(crate) struct NewOperation<'a> {
    pub id: Uuid,
    pub account: Uuid,
    pub recipient: Option<Uuid>,
    pub auction: Option<Uuid>,
    pub asset: Option<Uuid>,
    pub kind: &'a str,
    pub payload: Payload,
    pub approval: Option<Value>,
}

pub(crate) async fn insert(
    state: &AppState,
    connection: &mut PgConnection,
    operation: NewOperation<'_>,
) -> Result<()> {
    let now = messages::now_ms()?;
    sqlx::query(
        "INSERT INTO chain_operations (
             id,
             account_id,
             recipient_id,
             auction_id,
             asset_id,
             kind,
             payload_sealed,
             approval,
             created_at_ms,
             updated_at_ms
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)",
    )
    .bind(operation.id)
    .bind(operation.account)
    .bind(operation.recipient)
    .bind(operation.auction)
    .bind(operation.asset)
    .bind(operation.kind)
    .bind(sealed(
        state,
        operation.account,
        operation.id,
        &operation.payload,
    )?)
    .bind(operation.approval)
    .bind(now)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn save_payload(
    state: &AppState,
    connection: &mut PgConnection,
    operation: &Operation,
    payload: &Payload,
    status: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE chain_operations
         SET
             payload_sealed = $2,
             status = $3,
             revision = nextval('message_revision'),
             updated_at_ms = $4
         WHERE id = $1",
    )
    .bind(operation.id)
    .bind(sealed(state, operation.account_id, operation.id, payload)?)
    .bind(status)
    .bind(messages::now_ms()?)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn status(
    connection: &mut PgConnection,
    id: Uuid,
    status: &str,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE chain_operations
         SET
             status = $2,
             error = $3,
             revision = nextval('message_revision'),
             updated_at_ms = $4
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(error)
    .bind(messages::now_ms()?)
    .execute(connection)
    .await?;
    Ok(())
}
