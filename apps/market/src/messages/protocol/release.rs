use crate::{
    http::error::{ApiError, Result},
    messages::{clock, repository::lock_session},
    nodes,
};
use arbiter_protocol::{ArtifactRef, JobInput};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// The release authority binds an unchanged pair to the current frozen survivor set.
pub(crate) async fn release_allowed(connection: &mut PgConnection, session: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT
             s.auction_id IS NULL
             OR EXISTS
             (
                 SELECT 1
                 FROM auction_checks c
                 JOIN auctions a ON a.id = c.auction_id
                 JOIN auction_participants l
                     ON l.auction_id = a.id
                         AND l.account_id = c.left_account
                 JOIN auction_participants r
                     ON r.auction_id = a.id
                         AND r.account_id = c.right_account
                 WHERE
                     c.session_id = s.id
                     AND a.status = 'matching'
                     AND a.closes_at_ms - 3600000 <= $2
                     AND a.generation = c.generation
                     AND a.input_root IS NOT NULL
                     AND a.round_root = c.round_root
                     AND l.status = 'active'
                     AND r.status = 'active'
             )
         FROM approval_sessions s
         WHERE s.id = $1",
    )
    .bind(session)
    .bind(clock::now_ms()?)
    .fetch_one(connection)
    .await?)
}

pub(crate) async fn release_auction_responses(
    connection: &mut PgConnection,
    auction: Uuid,
) -> Result<()> {
    let blocked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT s.id
         FROM approval_sessions s
         JOIN approval_messages m ON m.session_id = s.id
         WHERE
             s.auction_id = $1
             AND s.status = 'active'
             AND m.status = 'blocked'
             AND m.kind = 'number_transfer'
             AND m.generation = s.generation
             AND m.request_id = (s.current_request ->> 'id')::uuid
         ORDER BY s.id",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    for session in blocked {
        lock_session(connection, session).await?;
        if release_allowed(connection, session).await? {
            sqlx::query(
                "UPDATE approval_messages
                 SET
                     status = 'ready',
                     created_at = now(),
                     revision = nextval('message_revision')
                 WHERE
                     session_id = $1
                     AND status = 'blocked'
                     AND kind = 'number_transfer'",
            )
            .bind(session)
            .execute(&mut *connection)
            .await?;
            clock::meter(connection, session, clock::now_ms()?).await?;
        }
    }
    let sessions: Vec<(Uuid, Uuid, Value, Value)> = sqlx::query_as(
        "SELECT
             s.id,
             s.job_id,
             s.current_request,
             s.response_ref
         FROM approval_sessions s
         JOIN auction_checks c ON c.session_id = s.id
         JOIN auctions a ON a.id = c.auction_id
         JOIN arbiter_jobs j ON j.id = s.job_id
         WHERE
             a.id = $1
             AND s.status = 'active'
             AND s.response_ref IS NOT NULL
             AND c.generation = a.generation
             AND j.status = 'waiting'
             AND j.response IS NULL
             AND j.output = s.current_request
         ORDER BY c.slot",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    for (session, job, request, response) in sessions {
        lock_session(connection, session).await?;
        if release_allowed(connection, session).await? {
            nodes::provide_response_in(
                connection,
                JobInput {
                    job_id: job,
                    request_id: serde_json::from_value::<ArtifactRef>(request)
                        .map_err(|_| ApiError::Internal)?
                        .id,
                    response: serde_json::from_value(response).map_err(|_| ApiError::Internal)?,
                },
            )
            .await?;
        }
    }
    Ok(())
}
