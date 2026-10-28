use super::release_allowed;
use crate::{
    http::error::{ApiError, Result},
    messages::repository::lock_session,
    nodes,
    state::AppState,
};
use arbiter_protocol::{ArtifactRef, JobInput};
use check_workflow::messages::{ContextRecord, Request};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn route_response(
    state: &AppState,
    connection: &mut PgConnection,
    message: Uuid,
) -> Result<()> {
    let (session, status): (Uuid, String) =
        sqlx::query_as("SELECT session_id, status FROM approval_messages WHERE id = $1")
            .bind(message)
            .fetch_one(&mut *connection)
            .await?;
    lock_session(connection, session).await?;
    let active: bool =
        sqlx::query_scalar("SELECT status = 'active' FROM approval_sessions WHERE id = $1")
            .bind(session)
            .fetch_one(&mut *connection)
            .await?;
    if !active || status != "response_pending" {
        return Ok(());
    }
    sqlx::query(
        "UPDATE approval_messages
         SET
             status = 'responded',
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(message)
    .execute(&mut *connection)
    .await?;
    let (request, submitted, context, job): (Value, Option<Value>, Value, Uuid) = sqlx::query_as(
        "SELECT
             current_request,
             response_ref,
             check_context,
             job_id
         FROM approval_sessions
         WHERE id = $1",
    )
    .bind(session)
    .fetch_one(&mut *connection)
    .await?;
    if submitted.is_some() {
        return Ok(());
    }
    let request_ref: ArtifactRef =
        serde_json::from_value(request).map_err(|_| ApiError::Internal)?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM approval_messages
         WHERE
             session_id = $1
             AND request_id = $2
             AND status <> 'responded'",
    )
    .bind(session)
    .bind(request_ref.id)
    .fetch_one(&mut *connection)
    .await?;
    if pending != 0 {
        return Ok(());
    }
    let response = assemble_response(state, connection, session, &request_ref, context).await?;
    sqlx::query("UPDATE approval_sessions SET response_ref = $2 WHERE id = $1")
        .bind(session)
        .bind(json!(response))
        .execute(&mut *connection)
        .await?;
    if !release_allowed(connection, session).await? {
        return Ok(());
    }
    nodes::provide_response_in(
        connection,
        JobInput {
            job_id: job,
            request_id: request_ref.id,
            response: response.clone(),
        },
    )
    .await?;
    Ok(())
}

async fn assemble_response(
    state: &AppState,
    connection: &mut PgConnection,
    session: Uuid,
    request_ref: &ArtifactRef,
    context: Value,
) -> Result<ArtifactRef> {
    let references: Vec<(String, Value)> = sqlx::query_as(
        "SELECT
             a.kind,
             a.response_ref
         FROM approval_messages a
         JOIN approval_members m
             ON m.session_id = a.session_id
                 AND m.account_id = a.account_id
         WHERE
             a.session_id = $1
             AND a.request_id = $2
         ORDER BY m.position",
    )
    .bind(session)
    .bind(request_ref.id)
    .fetch_all(&mut *connection)
    .await?;
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let request: Request = custody
        .artifacts
        .record(request_ref)
        .map_err(|_| ApiError::Unavailable)?;
    let context: ContextRecord = serde_json::from_value(context).map_err(|_| ApiError::Internal)?;
    let mut pieces = Vec::<(String, Value)>::new();
    for (kind, reference) in references {
        let reference: ArtifactRef =
            serde_json::from_value(reference).map_err(|_| ApiError::Internal)?;
        let value = custody
            .artifacts
            .record(&reference)
            .map_err(|_| ApiError::Unavailable)?;
        pieces.push((kind, value));
    }
    let response = crate::messages::responses::assemble(request, &context, pieces)?;
    let response = custody
        .artifacts
        .write_json(&response)
        .map_err(|_| ApiError::Unavailable)?;
    Ok(response)
}
