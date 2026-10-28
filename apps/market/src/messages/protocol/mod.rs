mod release;
mod requests;
mod responses;
mod transitions;

pub(crate) use release::{release_allowed, release_auction_responses};
pub use responses::route_response;

use crate::{
    http::error::{ApiError, Result},
    messages::{clock, repository::lock_session},
    state::AppState,
};
use arbiter_protocol::{Completion, JobStatus};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Keep session-before-job lock ordering consistent with withdrawal and expiry.
pub async fn lock_result_session(connection: &mut PgConnection, result: Uuid) -> Result<()> {
    let session: Option<Uuid> = sqlx::query_scalar(
        "SELECT s.id
         FROM approval_sessions s
         JOIN arbiter_results r ON s.job_id = (r.payload ->> 'job_id')::uuid
         WHERE r.id = $1",
    )
    .bind(result)
    .fetch_optional(&mut *connection)
    .await?;
    if let Some(session) = session {
        lock_session(connection, session).await?;
    }
    Ok(())
}

pub async fn apply_node_result(
    state: &AppState,
    connection: &mut PgConnection,
    result_id: Uuid,
) -> Result<()> {
    let payload: Value = sqlx::query_scalar("SELECT payload FROM arbiter_results WHERE id = $1")
        .bind(result_id)
        .fetch_one(&mut *connection)
        .await?;
    let result: Completion = serde_json::from_value(payload).map_err(|_| ApiError::Internal)?;
    let session: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM approval_sessions WHERE job_id = $1")
            .bind(result.job_id)
            .fetch_optional(&mut *connection)
            .await?;
    let Some(session) = session else {
        return Ok(());
    };
    lock_session(connection, session).await?;
    let status: String = sqlx::query_scalar("SELECT status FROM approval_sessions WHERE id = $1")
        .bind(session)
        .fetch_one(&mut *connection)
        .await?;
    if status != "active" {
        return Ok(());
    }
    if result.status == JobStatus::Completed && !release_allowed(connection, session).await? {
        clock::interrupt(connection, session).await?;
        return Ok(());
    }
    if result.status == JobStatus::Waiting {
        let reference = result.output.ok_or(ApiError::Internal)?;
        requests::publish(state, connection, session, reference, result.generation).await?;
    } else if result.status.terminal() {
        finish_session(connection, session, result.status).await?;
    }
    Ok(())
}

async fn finish_session(
    connection: &mut PgConnection,
    session: Uuid,
    result_status: JobStatus,
) -> Result<()> {
    let status = if result_status == JobStatus::Completed {
        "completed"
    } else {
        "interrupted"
    };
    sqlx::query(
        "UPDATE approval_sessions
         SET
             status = $2,
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(session)
    .bind(status)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE approval_members SET chargeable = FALSE WHERE session_id = $1")
        .bind(session)
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "UPDATE approval_messages
         SET
             status = CASE
                 WHEN
                     status = 'responded'
                     AND $2 = 'completed' THEN 'completed'
                 ELSE 'cancelled'
             END,
             revision = nextval('message_revision')
         WHERE
             session_id = $1
             AND status NOT IN (
                 'completed',
                 'cancelled'
             )",
    )
    .bind(session)
    .bind(status)
    .execute(&mut *connection)
    .await?;
    Ok(())
}
