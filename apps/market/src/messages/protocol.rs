use super::{clock, model::ActionKind, repository::lock_session};
use crate::{
    http::error::{ApiError, Result},
    nodes,
    state::AppState,
};
use arbiter_protocol::{ArtifactRef, Completion, JobInput, JobStatus};
use check_workflow::messages::{ContextRecord, Contribution, Opening, Request, Response};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Keep session-before-job lock ordering consistent with withdrawal and expiry.
pub async fn lock_result_session(connection: &mut PgConnection, result: Uuid) -> Result<()> {
    let session: Option<Uuid> = sqlx::query_scalar(
        "SELECT s.id FROM approval_sessions s JOIN arbiter_results r
         ON s.job_id=(r.payload->>'job_id')::uuid WHERE r.id=$1",
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
    let payload: Value = sqlx::query_scalar("SELECT payload FROM arbiter_results WHERE id=$1")
        .bind(result_id)
        .fetch_one(&mut *connection)
        .await?;
    let result: Completion = serde_json::from_value(payload).map_err(|_| ApiError::Internal)?;
    let session: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM approval_sessions WHERE job_id=$1")
            .bind(result.job_id)
            .fetch_optional(&mut *connection)
            .await?;
    let Some(session) = session else {
        return Ok(());
    };
    lock_session(connection, session).await?;
    let status: String = sqlx::query_scalar("SELECT status FROM approval_sessions WHERE id=$1")
        .bind(session)
        .fetch_one(&mut *connection)
        .await?;
    if status != "active" {
        return Ok(());
    }
    if result.status == JobStatus::Waiting {
        let reference = result.output.ok_or(ApiError::Internal)?;
        publish(state, connection, session, reference, result.generation).await?;
    } else if result.status.terminal() {
        let status = if result.status == JobStatus::Completed {
            "completed"
        } else {
            "interrupted"
        };
        sqlx::query(
            "UPDATE approval_sessions SET status=$2,
             revision=nextval('message_revision') WHERE id=$1",
        )
        .bind(session)
        .bind(status)
        .execute(&mut *connection)
        .await?;
        sqlx::query("UPDATE approval_members SET chargeable=FALSE WHERE session_id=$1")
            .bind(session)
            .execute(&mut *connection)
            .await?;
        sqlx::query(
            "UPDATE approval_messages SET status=CASE WHEN status='responded' AND
             $2='completed' THEN 'completed' ELSE 'cancelled' END,
             revision=nextval('message_revision') WHERE session_id=$1 AND status NOT IN
             ('completed', 'cancelled')",
        )
        .bind(session)
        .bind(status)
        .execute(connection)
        .await?;
    }
    Ok(())
}

async fn publish(
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
    let (previous, context): (Option<Value>, Option<Value>) =
        sqlx::query_as("SELECT current_request,check_context FROM approval_sessions WHERE id=$1")
            .bind(session)
            .fetch_one(&mut *connection)
            .await?;
    if previous.as_ref() == Some(&json!(reference)) {
        return Ok(());
    }
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
    let saved: Option<ContextRecord> = context
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| ApiError::Internal)?;
    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT account_id FROM approval_members WHERE session_id=$1 ORDER BY position",
    )
    .bind(session)
    .fetch_all(&mut *connection)
    .await?;
    let (context, actions) = describe(&request, previous.as_ref(), saved.as_ref(), &members)?;
    let original: Value = sqlx::query_scalar(
        "SELECT j.input FROM arbiter_jobs j JOIN approval_sessions s
         ON s.job_id=j.id WHERE s.id=$1",
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

    sqlx::query(
        "UPDATE approval_messages SET status='completed',
         revision=nextval('message_revision') WHERE session_id=$1 AND status='responded'",
    )
    .bind(session)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE approval_sessions SET current_request=$2, check_context=$3,
         response_ref=NULL, generation=$4, revision=nextval('message_revision') WHERE
         id=$1",
    )
    .bind(session)
    .bind(json!(reference))
    .bind(json!(context))
    .bind(generation)
    .execute(&mut *connection)
    .await?;
    for (account, kind) in actions {
        sqlx::query(
            "INSERT INTO approval_messages(id, session_id, account_id, request_id,
             request_ref, generation, kind) VALUES($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO
             NOTHING",
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
    clock::meter(connection, session, clock::now_ms()?).await?;
    Ok(())
}

fn describe(
    request: &Request,
    previous: Option<&Request>,
    saved: Option<&ContextRecord>,
    members: &[Uuid],
) -> Result<(ContextRecord, Vec<(Uuid, ActionKind)>)> {
    let context = match request {
        Request::Number { context, .. }
        | Request::Masks { context, .. }
        | Request::AnswerCommitment { context, .. } => context.clone(),
        _ => saved
            .cloned()
            .ok_or(ApiError::Conflict("Missing comparison context"))?,
    };
    if members != context.manifest.owners {
        return Err(ApiError::Forbidden);
    }
    if saved.is_some_and(|saved| json!(saved) != json!(context)) {
        return Err(ApiError::Conflict("Comparison context changed"));
    }
    let id = context
        .restore()
        .map_err(|_| ApiError::Conflict("Invalid comparison context"))?
        .id();
    let both = |kind| {
        members
            .iter()
            .copied()
            .map(|account| (account, kind))
            .collect()
    };
    let actions = match (request, previous) {
        (Request::Number { .. }, None) => vec![(members[0], ActionKind::NumberTransfer)],
        (Request::Masks { .. }, Some(Request::Number { .. })) => both(ActionKind::MaskContribution),
        (Request::AnswerCommitment { .. }, Some(Request::Masks { .. })) => {
            vec![(members[1], ActionKind::AnswerCommitment)]
        }
        (Request::AnswerReveal { context, .. }, Some(Request::AnswerCommitment { .. }))
            if *context == id =>
        {
            vec![(members[1], ActionKind::AnswerReveal)]
        }
        (Request::Openings { context, .. }, Some(Request::AnswerReveal { .. }))
            if *context == id =>
        {
            both(ActionKind::MaskOpening)
        }
        _ => return Err(ApiError::Conflict("Unexpected private-check stage")),
    };
    Ok((context, actions))
}

pub async fn route_response(
    state: &AppState,
    connection: &mut PgConnection,
    message: Uuid,
) -> Result<()> {
    let (session, status): (Uuid, String) =
        sqlx::query_as("SELECT session_id,status FROM approval_messages WHERE id=$1")
            .bind(message)
            .fetch_one(&mut *connection)
            .await?;
    lock_session(connection, session).await?;
    let active: bool =
        sqlx::query_scalar("SELECT status='active' FROM approval_sessions WHERE id=$1")
            .bind(session)
            .fetch_one(&mut *connection)
            .await?;
    if !active || status != "response_pending" {
        return Ok(());
    }
    sqlx::query(
        "UPDATE approval_messages SET status='responded',
         revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(message)
    .execute(&mut *connection)
    .await?;
    let (request, submitted, context, job): (Value, Option<Value>, Value, Uuid) = sqlx::query_as(
        "SELECT current_request, response_ref, check_context, job_id
         FROM approval_sessions WHERE id=$1",
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
        "SELECT count(*) FROM approval_messages WHERE session_id=$1 AND request_id=$2 AND
         status<>'responded'",
    )
    .bind(session)
    .bind(request_ref.id)
    .fetch_one(&mut *connection)
    .await?;
    if pending != 0 {
        return Ok(());
    }
    let references: Vec<Value> = sqlx::query_scalar(
        "SELECT a.response_ref FROM approval_messages a JOIN approval_members m ON
         m.session_id=a.session_id AND m.account_id=a.account_id WHERE a.session_id=$1
         AND a.request_id=$2 ORDER BY m.position",
    )
    .bind(session)
    .bind(request_ref.id)
    .fetch_all(&mut *connection)
    .await?;
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let request: Request = custody
        .artifacts
        .record(&request_ref)
        .map_err(|_| ApiError::Unavailable)?;
    let context: ContextRecord = serde_json::from_value(context).map_err(|_| ApiError::Internal)?;
    let mut pieces = Vec::<Value>::new();
    for reference in references {
        let reference: ArtifactRef =
            serde_json::from_value(reference).map_err(|_| ApiError::Internal)?;
        pieces.push(
            custody
                .artifacts
                .record(&reference)
                .map_err(|_| ApiError::Unavailable)?,
        );
    }
    let response = match request {
        Request::Masks { base, .. } => {
            let contributions: Vec<Contribution> = pieces
                .into_iter()
                .map(serde_json::from_value)
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| ApiError::Internal)?;
            Response::Masks {
                context: context.restore().map_err(|_| ApiError::Internal)?.id(),
                base,
                contributions: contributions.try_into().map_err(|_| ApiError::Internal)?,
            }
        }
        Request::Openings { context, base } => {
            let openings: Vec<Opening> = pieces
                .into_iter()
                .map(serde_json::from_value)
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| ApiError::Internal)?;
            Response::Openings {
                context,
                base,
                openings: openings.try_into().map_err(|_| ApiError::Internal)?,
            }
        }
        _ => serde_json::from_value(pieces.into_iter().next().ok_or(ApiError::Internal)?)
            .map_err(|_| ApiError::Internal)?,
    };
    let response = custody
        .artifacts
        .write_json(&response)
        .map_err(|_| ApiError::Unavailable)?;
    nodes::provide_response_in(
        connection,
        JobInput {
            job_id: job,
            request_id: request_ref.id,
            response: response.clone(),
        },
    )
    .await?;
    sqlx::query("UPDATE approval_sessions SET response_ref=$2 WHERE id=$1")
        .bind(session)
        .bind(json!(response))
        .execute(connection)
        .await?;
    Ok(())
}
