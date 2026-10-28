use super::{clock, model::ActionKind, repository::lock_session};
use crate::{
    http::error::{ApiError, Result},
    nodes,
    state::AppState,
};
use arbiter_protocol::{ArtifactRef, Completion, JobInput, JobStatus};
use check_workflow::messages::{ContextRecord, Request};
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
    if result.status == JobStatus::Completed && !release_allowed(connection, session).await? {
        clock::interrupt(connection, session).await?;
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
    if !matches!(request, Request::Masks { .. } | Request::Preparation { .. })
        && !release_allowed(connection, session).await?
    {
        return Err(ApiError::Conflict("Auction inputs have not been frozen"));
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
    if let Request::Preparation {
        context, number, ..
    } = &request
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
            "INSERT INTO approval_messages(id, session_id, account_id, request_id,
             request_ref, generation, kind, status) VALUES($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(session)
        .bind(members[0])
        .bind(reference.id)
        .bind(json!(transfer))
        .bind(generation)
        .bind(ActionKind::NumberTransfer.as_str())
        .bind(status)
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
        Request::Preparation { context, .. }
        | Request::Number { context, .. }
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
        (Request::Preparation { .. }, None) => both(ActionKind::MaskContribution),
        (Request::AnswerCommitment { .. }, Some(Request::Preparation { .. })) => {
            vec![(members[1], ActionKind::AnswerCommitment)]
        }
        (Request::Number { .. }, None) => vec![(members[0], ActionKind::NumberTransfer)],
        (Request::Masks { context, .. }, None) if context.manifest.release_at_ms.is_some() => {
            both(ActionKind::MaskContribution)
        }
        (Request::Number { context, .. }, Some(Request::Masks { .. }))
            if context.manifest.release_at_ms.is_some() =>
        {
            vec![(members[0], ActionKind::NumberTransfer)]
        }
        (Request::Masks { .. }, Some(Request::Number { .. })) => both(ActionKind::MaskContribution),
        (Request::AnswerCommitment { .. }, Some(Request::Masks { .. })) => {
            vec![(members[1], ActionKind::AnswerCommitment)]
        }
        (Request::AnswerCommitment { context, .. }, Some(Request::Number { .. }))
            if context.manifest.release_at_ms.is_some() =>
        {
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
    let references: Vec<(String, Value)> = sqlx::query_as(
        "SELECT a.kind, a.response_ref FROM approval_messages a JOIN approval_members m ON
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
    let response = super::responses::assemble(request, &context, pieces)?;
    let response = custody
        .artifacts
        .write_json(&response)
        .map_err(|_| ApiError::Unavailable)?;
    sqlx::query("UPDATE approval_sessions SET response_ref=$2 WHERE id=$1")
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

/// The release authority binds an unchanged pair to the current frozen survivor set.
pub(crate) async fn release_allowed(connection: &mut PgConnection, session: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT s.auction_id IS NULL OR EXISTS (
         SELECT 1 FROM auction_checks c JOIN auctions a ON a.id=c.auction_id
         JOIN auction_participants l ON l.auction_id=a.id AND l.account_id=c.left_account
         JOIN auction_participants r ON r.auction_id=a.id AND r.account_id=c.right_account
         WHERE c.session_id=s.id AND a.status='matching' AND a.closes_at_ms-3600000<=$2
         AND a.generation=c.generation AND a.input_root IS NOT NULL
         AND a.round_root=c.round_root AND l.status='active' AND r.status='active')
         FROM approval_sessions s WHERE s.id=$1",
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
        "SELECT DISTINCT s.id FROM approval_sessions s JOIN approval_messages m ON m.session_id=s.id
         WHERE s.auction_id=$1 AND s.status='active' AND m.status='blocked'
         AND m.kind='number_transfer' AND m.generation=s.generation
         AND m.request_id=(s.current_request->>'id')::uuid ORDER BY s.id",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    for session in blocked {
        lock_session(connection, session).await?;
        if release_allowed(connection, session).await? {
            sqlx::query(
                "UPDATE approval_messages SET status='ready', created_at=now(),
                 revision=nextval('message_revision')
                 WHERE session_id=$1 AND status='blocked' AND kind='number_transfer'",
            )
            .bind(session)
            .execute(&mut *connection)
            .await?;
            clock::meter(connection, session, clock::now_ms()?).await?;
        }
    }
    let sessions: Vec<(Uuid, Uuid, Value, Value)> = sqlx::query_as(
        "SELECT s.id,s.job_id,s.current_request,s.response_ref
         FROM approval_sessions s JOIN auction_checks c ON c.session_id=s.id
         JOIN auctions a ON a.id=c.auction_id JOIN arbiter_jobs j ON j.id=s.job_id
         WHERE a.id=$1 AND s.status='active' AND s.response_ref IS NOT NULL
         AND c.generation=a.generation AND j.status='waiting' AND j.response IS NULL
         AND j.output=s.current_request
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
