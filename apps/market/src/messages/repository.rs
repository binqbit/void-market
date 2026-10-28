use super::{
    clock,
    model::{ActionKind, Approve, Message, SessionInput, SessionSummary, Snapshot},
    signing,
};
use crate::{
    events,
    http::error::{ApiError, Result},
    state::AppState,
};
use arbiter_protocol::ArtifactRef;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn create_session(state: &AppState, input: SessionInput) -> Result<()> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    if input.title.trim().is_empty()
        || input.title.chars().count() > 128
        || input.participants[0].account_id == input.participants[1].account_id
        || ![8, 16, 32, 64].contains(&input.width)
    {
        return Err(ApiError::BadRequest("Invalid approval session"));
    }
    let maximum = (1u64 << (input.width - 1)) - 1;
    if input
        .participants
        .iter()
        .any(|participant| participant.value > maximum)
    {
        return Err(ApiError::BadRequest(
            "Value exceeds the private-check input range",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let enabled: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM arbiter_nodes WHERE id=$1 AND enabled)")
            .bind(input.node_id)
            .fetch_one(&mut *tx)
            .await?;
    if !enabled {
        return Err(ApiError::BadRequest("Select a registered arbiter node"));
    }
    sqlx::query(
        "INSERT INTO approval_sessions(id,title,node_id,comparison,width) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(input.id)
    .bind(input.title)
    .bind(input.node_id)
    .bind(
        serde_json::to_value(input.comparison)
            .map_err(|_| ApiError::Internal)?
            .as_str()
            .ok_or(ApiError::Internal)?,
    )
    .bind(i16::from(input.width))
    .execute(&mut *tx)
    .await?;
    for (index, participant) in input.participants.iter().enumerate() {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id=$1)")
            .bind(participant.account_id)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            return Err(ApiError::BadRequest("Session participant does not exist"));
        }
        custody.provision(&mut tx, participant.account_id).await?;
        let sealed = custody.vault.seal(
            participant.account_id,
            &format!("session-input:{}", input.id),
            &participant.value.to_le_bytes(),
        )?;
        sqlx::query(
            "INSERT INTO approval_members(session_id, account_id, position, input_sealed)
             VALUES($1,$2,$3,$4)",
        )
        .bind(input.id)
        .bind(participant.account_id)
        .bind(index as i16)
        .bind(sealed)
        .execute(&mut *tx)
        .await?;
    }
    events::record_node(&mut tx, "session.created", json!({"session_id":input.id})).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn lock_session(connection: &mut PgConnection, session: Uuid) -> Result<()> {
    sqlx::query("SELECT id FROM approval_sessions WHERE id=$1 FOR UPDATE")
        .bind(session)
        .fetch_optional(connection)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    session_id: Uuid,
    session_title: String,
    request_id: Uuid,
    revision: i64,
    kind: String,
    status: String,
    participant_status: String,
    session_status: String,
    remaining_ms: i64,
    chargeable: bool,
    last_tick_ms: Option<i64>,
    clock_healthy: bool,
    approval: Option<Value>,
}

pub async fn snapshot(state: &AppState, account: Uuid) -> Result<Snapshot> {
    let now = clock::now_ms()?;
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT a.id, a.session_id, s.title AS session_title, a.request_id, a.revision,
         a.kind, a.status, m.status AS participant_status, s.status AS session_status,
         m.remaining_ms, m.chargeable, s.last_tick_ms, COALESCE(r.healthy AND
         r.instance_id=s.clock_epoch AND $2-r.last_tick_ms BETWEEN 0 AND 5000 AND
         n.enabled AND n.heartbeat_until>now(), FALSE) AS clock_healthy, a.approval FROM
         approval_messages a JOIN approval_sessions s ON s.id=a.session_id JOIN
         approval_members m ON m.session_id=s.id AND m.account_id=a.account_id JOIN
         arbiter_nodes n ON n.id=s.node_id LEFT JOIN approval_runtime r ON r.singleton
         WHERE a.account_id=$1 ORDER BY CASE WHEN a.status='ready' THEN 0 ELSE 1 END,
         a.created_at DESC, a.id LIMIT 100",
    )
    .bind(account)
    .bind(now)
    .fetch_all(&state.pool)
    .await?;
    let mut messages = Vec::with_capacity(rows.len());
    for row in rows {
        let running = row.chargeable
            && row.clock_healthy
            && row.participant_status == "active"
            && row.session_status == "active";
        let elapsed = if running {
            now.saturating_sub(row.last_tick_ms.unwrap_or(now))
                .clamp(0, 5000)
        } else {
            0
        };
        let stage: ActionKind =
            serde_json::from_value(json!(row.kind)).map_err(|_| ApiError::Internal)?;
        messages.push(Message {
            id: row.id,
            session_id: row.session_id,
            session_title: row.session_title,
            request_id: row.request_id,
            revision: row.revision,
            stage,
            status: row.status,
            participant_status: row.participant_status,
            session_status: row.session_status,
            remaining_ms: row.remaining_ms.saturating_sub(elapsed).max(0),
            timer_running: running,
            approval: row.approval,
        });
    }
    let revision = revision(&state.pool, account).await?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_messages a JOIN approval_sessions s ON
         s.id=a.session_id JOIN approval_members m ON m.session_id=s.id AND
         m.account_id=a.account_id WHERE a.account_id=$1 AND a.status='ready' AND
         s.status='active' AND m.status='active'",
    )
    .bind(account)
    .fetch_one(&state.pool)
    .await?;
    let sessions: Vec<SessionSummary> = sqlx::query_as(
        "SELECT s.id, s.title, s.status, m.status AS participation_status,
         m.remaining_ms, (m.chargeable AND COALESCE(r.healthy AND
         r.instance_id=s.clock_epoch AND $2-r.last_tick_ms BETWEEN 0 AND 5000 AND
         n.enabled AND n.heartbeat_until>now(), FALSE)) AS timer_running FROM
         approval_sessions s JOIN approval_members m ON m.session_id=s.id JOIN
         arbiter_nodes n ON n.id=s.node_id LEFT JOIN approval_runtime r ON r.singleton
         WHERE m.account_id=$1 ORDER BY s.created_at DESC LIMIT 100",
    )
    .bind(account)
    .bind(now)
    .fetch_all(&state.pool)
    .await?;
    Ok(Snapshot {
        custody_mode: if state.custody.is_some() {
            "development"
        } else {
            "disabled"
        },
        revision,
        server_time_ms: now,
        pending: pending as usize,
        messages,
        sessions,
    })
}

pub async fn revision(pool: &PgPool, account: Uuid) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT greatest(COALESCE((SELECT max(a.revision) FROM approval_messages a WHERE
         a.account_id=$1), 0), COALESCE((SELECT max(s.revision) FROM approval_sessions s
         JOIN approval_members m ON m.session_id=s.id WHERE m.account_id=$1), 0))",
    )
    .bind(account)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub async fn approve(state: &AppState, account: Uuid, id: Uuid, input: Approve) -> Result<Value> {
    let mut tx = state.pool.begin().await?;
    let session: Uuid = sqlx::query_scalar(
        "SELECT session_id FROM approval_messages WHERE id=$1 AND account_id=$2",
    )
    .bind(id)
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    lock_session(&mut tx, session).await?;
    let (request, revision, status, reference, approval): (
        Uuid,
        i64,
        String,
        Value,
        Option<Value>,
    ) = sqlx::query_as(
        "SELECT request_id, revision, status, request_ref, approval FROM
         approval_messages WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if request != input.request_id {
        return Err(ApiError::Conflict("The request changed; refresh messages"));
    }
    if let Some(approval) = approval {
        if approval["revision"].as_i64() != Some(input.revision) {
            return Err(ApiError::Conflict(
                "Approval revision differs from the original request",
            ));
        }
        return Ok(approval);
    }
    if revision != input.revision || status != "ready" {
        return Err(ApiError::Conflict(
            "This message is no longer available for approval",
        ));
    }
    let now = clock::now_ms()?;
    clock::meter(&mut tx, session, now).await?;
    let (remaining, member, session_status): (i64, String, String) = sqlx::query_as(
        "SELECT m.remaining_ms, m.status, s.status FROM approval_members m JOIN
         approval_sessions s ON s.id=m.session_id WHERE m.session_id=$1 AND
         m.account_id=$2",
    )
    .bind(session)
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    if member != "active" || session_status != "active" {
        return Err(ApiError::Conflict("Participation is no longer active"));
    }
    if remaining <= 0 {
        clock::terminate(&mut tx, session, account, "disqualified").await?;
        events::record_node(
            &mut tx,
            "message.expired",
            json!({"session_id":session,"account_id":account}),
        )
        .await?;
        tx.commit().await?;
        return Err(ApiError::Conflict("The approval time budget has expired"));
    }
    let reference: ArtifactRef =
        serde_json::from_value(reference).map_err(|_| ApiError::Internal)?;
    let receipt = signing::sign(
        state,
        &mut tx,
        signing::Binding {
            account,
            session,
            action: id,
            request,
            revision,
            digest: reference.digest,
            purpose: "approve",
            received_ms: now,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE approval_messages SET status='approved', approval=$2,
         revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(id)
    .bind(&receipt)
    .execute(&mut *tx)
    .await?;
    clock::refresh_charging(&mut tx, session).await?;
    events::record_node(
        &mut tx,
        "message.approved",
        json!({"message_id":id,"session_id":session}),
    )
    .await?;
    tx.commit().await?;
    Ok(receipt)
}

pub async fn withdraw(state: &AppState, account: Uuid, session: Uuid) -> Result<Value> {
    let mut tx = state.pool.begin().await?;
    lock_session(&mut tx, session).await?;
    let (status, receipt): (String, Option<Value>) = sqlx::query_as(
        "SELECT status, exit_receipt FROM approval_members
         WHERE session_id=$1 AND account_id=$2 FOR UPDATE",
    )
    .bind(session)
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    if status == "withdrawn" {
        return receipt.ok_or(ApiError::Internal);
    }
    if status != "active" {
        return Err(ApiError::Conflict("Participation is no longer active"));
    }
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM approval_sessions WHERE id=$1")
            .bind(session)
            .fetch_one(&mut *tx)
            .await?;
    if !matches!(session_status.as_str(), "initializing" | "active") {
        return Err(ApiError::Conflict("The session has already ended"));
    }
    let now = clock::now_ms()?;
    clock::meter(&mut tx, session, now).await?;
    let remaining: i64 = sqlx::query_scalar(
        "SELECT remaining_ms FROM approval_members WHERE session_id=$1 AND account_id=$2",
    )
    .bind(session)
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    if remaining == 0 {
        clock::terminate(&mut tx, session, account, "disqualified").await?;
        events::record_node(
            &mut tx,
            "message.expired",
            json!({"session_id":session,"account_id":account}),
        )
        .await?;
        tx.commit().await?;
        return Err(ApiError::Conflict("The approval time budget has expired"));
    }

    let receipt = signing::sign(
        state,
        &mut tx,
        signing::Binding {
            account,
            session,
            action: session,
            request: session,
            revision: 0,
            digest: [0; 32],
            purpose: "withdraw",
            received_ms: now,
        },
    )
    .await?;
    clock::terminate(&mut tx, session, account, "withdrawn").await?;
    sqlx::query(
        "UPDATE approval_members SET exit_receipt=$3 WHERE session_id=$1 AND account_id=$2",
    )
    .bind(session)
    .bind(account)
    .bind(&receipt)
    .execute(&mut *tx)
    .await?;
    events::record_node(
        &mut tx,
        "message.withdrawn",
        json!({"session_id":session,"account_id":account}),
    )
    .await?;
    tx.commit().await?;
    Ok(receipt)
}
