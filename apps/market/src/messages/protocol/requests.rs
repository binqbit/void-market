use super::{release_allowed, transitions};
use crate::{
    http::error::{ApiError, Result},
    messages::{clock, model::ActionKind},
    state::AppState,
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::{ContextRecord, Request};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

pub(super) async fn publish(
    state: &AppState,
    connection: &mut PgConnection,
    session: Uuid,
    reference: ArtifactRef,
    generation: i64,
) -> Result<()> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let request: Request = custody
        .artifacts
        .record(&reference)
        .map_err(|_| ApiError::Conflict("The arbiter request is unavailable"))?;

    let (previous, context): (Option<Value>, Option<Value>) = sqlx::query_as(
        "SELECT
             current_request,
             check_context
         FROM approval_sessions
         WHERE id = $1",
    )
    .bind(session)
    .fetch_one(&mut *connection)
    .await?;
    if previous.as_ref() == Some(&json!(reference)) {
        return Ok(());
    }

    let (context, actions) =
        validate_request(state, connection, session, &request, previous, context).await?;

    begin_request(connection, session, &reference, generation, &context).await?;
    publish_actions(connection, session, &reference, generation, actions).await?;
    publish_number_transfer(state, connection, session, &reference, generation, &request).await?;

    clock::meter(connection, session, clock::now_ms()?).await?;
    Ok(())
}

async fn publish_number_transfer(
    state: &AppState,
    connection: &mut PgConnection,
    session: Uuid,
    reference: &ArtifactRef,
    generation: i64,
    request: &Request,
) -> Result<()> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    if let Request::Preparation {
        context, number, ..
    } = request
    {
        let transfer = custody
            .artifacts
            .write_json(&Request::Number {
                context: context.clone(),
                challenge: number.challenge.clone(),
                reference: number.reference,
            })
            .map_err(|_| ApiError::Unavailable)?;
        let status = if release_allowed(connection, session).await? {
            "ready"
        } else {
            "blocked"
        };

        sqlx::query(
            "INSERT INTO approval_messages (
                 id, session_id, account_id, request_id, request_ref, generation, kind, status
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(session)
        .bind(context.manifest.owners[0])
        .bind(reference.id)
        .bind(json!(transfer))
        .bind(generation)
        .bind(ActionKind::NumberTransfer.as_str())
        .bind(status)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

async fn validate_request(
    state: &AppState,
    connection: &mut PgConnection,
    session: Uuid,
    request: &Request,
    previous: Option<Value>,
    saved: Option<Value>,
) -> Result<(ContextRecord, Vec<(Uuid, ActionKind)>)> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let previous: Option<Request> = previous
        .map(|value| {
            let previous: ArtifactRef =
                serde_json::from_value(value).map_err(|_| ApiError::Internal)?;
            custody
                .artifacts
                .record(&previous)
                .map_err(|_| ApiError::Unavailable)
        })
        .transpose()?;
    let saved: Option<ContextRecord> = saved
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| ApiError::Internal)?;

    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT account_id
         FROM approval_members
         WHERE session_id = $1
         ORDER BY position",
    )
    .bind(session)
    .fetch_all(&mut *connection)
    .await?;
    let (context, actions) =
        transitions::describe(request, previous.as_ref(), saved.as_ref(), &members)?;

    let original: Value = sqlx::query_scalar(
        "SELECT j.input
         FROM arbiter_jobs j
         JOIN approval_sessions s ON s.job_id = j.id
         WHERE s.id = $1",
    )
    .bind(session)
    .fetch_one(&mut *connection)
    .await?;
    let original: ArtifactRef = serde_json::from_value(original).map_err(|_| ApiError::Internal)?;
    let original: check_workflow::messages::CheckManifest = custody
        .artifacts
        .record(&original)
        .map_err(|_| ApiError::Unavailable)?;
    if json!(original) != json!(context.manifest) {
        return Err(ApiError::Conflict(
            "The request differs from the admitted comparison",
        ));
    }

    if !matches!(request, Request::Masks { .. } | Request::Preparation { .. })
        && !release_allowed(connection, session).await?
    {
        return Err(ApiError::Conflict("Auction inputs have not been frozen"));
    }

    Ok((context, actions))
}

async fn begin_request(
    connection: &mut PgConnection,
    session: Uuid,
    reference: &ArtifactRef,
    generation: i64,
    context: &ContextRecord,
) -> Result<()> {
    sqlx::query(
        "UPDATE approval_messages
         SET
             status = 'completed',
             revision = nextval('message_revision')
         WHERE
             session_id = $1
             AND status = 'responded'",
    )
    .bind(session)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE approval_sessions
         SET
             current_request = $2,
             check_context = $3,
             response_ref = NULL,
             generation = $4,
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(session)
    .bind(json!(reference))
    .bind(json!(context))
    .bind(generation)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn publish_actions(
    connection: &mut PgConnection,
    session: Uuid,
    reference: &ArtifactRef,
    generation: i64,
    actions: Vec<(Uuid, ActionKind)>,
) -> Result<()> {
    for (account, kind) in actions {
        sqlx::query(
            "INSERT INTO approval_messages (
                 id, session_id, account_id, request_id, request_ref, generation, kind
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(session)
        .bind(account)
        .bind(reference.id)
        .bind(json!(reference))
        .bind(generation)
        .bind(kind.as_str())
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}
