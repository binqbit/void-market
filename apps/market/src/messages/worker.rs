use super::{clock, repository, signing};
use crate::{
    custody::{keys, operations},
    events,
    http::error::{ApiError, Result},
    nodes,
    state::AppState,
};
use arbiter_protocol::{ArtifactRef, NewJob};
use check_workflow::messages::{
    Assurance, CheckManifest, ContextRecord, KeyMaterial, Predicate, Request,
};
use private_check::NumberInput;
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;
use uuid::Uuid;

pub async fn run(state: AppState, mut stop: watch::Receiver<bool>) -> Result<()> {
    if state.custody.is_none() {
        return Ok(());
    }
    let mut ownership = state.pool.acquire().await?;
    ownership.close_on_drop();
    let owned: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_lock(hashtextextended(current_schema(), 732617229))",
    )
    .fetch_one(&mut *ownership)
    .await?;
    if !owned {
        return Err(ApiError::Unavailable);
    }

    // The exclusive worker lock fences the previous process before lease recovery.
    sqlx::query("UPDATE custody_keys SET lease_until=now() WHERE he_status='building'")
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE approval_messages SET lease_until=now() WHERE status='processing'")
        .execute(&state.pool)
        .await?;

    let (clock_stop, clock_signal) = watch::channel(false);
    let clock_failed = Arc::new(AtomicBool::new(false));
    let failed = clock_failed.clone();
    let clock_state = state.clone();
    let clock_task = tokio::spawn(async move {
        let result = clock::run(
            clock_state.clone(),
            Uuid::new_v4(),
            &mut ownership,
            clock_signal,
        )
        .await;
        let result = result.and(ownership.close().await.map_err(ApiError::from));
        if result.is_err() {
            failed.store(true, Ordering::Relaxed);
            clock_state.worker_ready.store(false, Ordering::Relaxed);
        }
        result
    });
    let outcome = work_loop(&state, &mut stop, &clock_failed).await;
    let _ = clock_stop.send(true);
    let clock_result = clock_task.await.map_err(|_| ApiError::Internal)?;
    outcome?;
    clock_result
}

async fn work_loop(
    state: &AppState,
    stop: &mut watch::Receiver<bool>,
    clock_failed: &AtomicBool,
) -> Result<()> {
    loop {
        if clock_failed.load(Ordering::Relaxed) {
            return Err(ApiError::Unavailable);
        }
        if *stop.borrow() {
            return Ok(());
        }
        let progressed = process_one(state).await?
            || prepare_session(state).await?
            || keys::generate_one(state).await?;
        if !progressed {
            tokio::select! {
                _ = stop.changed() => return Ok(()),
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    }
}

async fn prepare_session(state: &AppState) -> Result<bool> {
    let interrupted = sqlx::query(
        "UPDATE approval_sessions s SET status='interrupted',
         revision=nextval('message_revision') WHERE s.status='initializing'
         AND EXISTS (SELECT 1 FROM approval_members m JOIN custody_keys k
         ON k.account_id=m.account_id WHERE m.session_id=s.id AND k.he_status='failed')",
    )
    .execute(&state.pool)
    .await?;
    if interrupted.rows_affected() > 0 {
        return Ok(true);
    }

    let row: Option<(Uuid, Uuid, String, i16)> = sqlx::query_as(
        "SELECT s.id, s.node_id, s.comparison, s.width FROM approval_sessions s WHERE
         s.status='initializing' AND NOT EXISTS(SELECT 1 FROM approval_members m JOIN
         custody_keys k ON k.account_id=m.account_id WHERE m.session_id=s.id AND
         k.he_status<>'ready') ORDER BY s.created_at LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await?;
    let Some((session, node, predicate, width)) = row else {
        return Ok(false);
    };
    let members: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
        "SELECT account_id, input_sealed FROM approval_members
         WHERE session_id=$1 ORDER BY position",
    )
    .bind(session)
    .fetch_all(&state.pool)
    .await?;
    if members.len() != 2 {
        return Err(ApiError::Internal);
    }
    let material = [
        keys::material(&state.pool, members[0].0).await?,
        keys::material(&state.pool, members[1].0).await?,
    ];
    let comparison: Predicate =
        serde_json::from_value(json!(predicate)).map_err(|_| ApiError::Internal)?;
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?.clone();
    let manifest = tokio::task::spawn_blocking(move || {
        prepare_manifest(custody, session, members, material, width as u8, comparison)
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    let mut tx = state.pool.begin().await?;
    repository::lock_session(&mut tx, session).await?;
    let active: bool =
        sqlx::query_scalar("SELECT status='initializing' FROM approval_sessions WHERE id=$1")
            .bind(session)
            .fetch_one(&mut *tx)
            .await?;
    if !active {
        return Ok(true);
    }
    nodes::enqueue_in(
        &mut tx,
        NewJob {
            id: session,
            node_id: node,
            input: manifest,
        },
    )
    .await?;
    sqlx::query("UPDATE arbiter_jobs SET interactive=TRUE WHERE id=$1")
        .bind(session)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE approval_sessions SET status='active', job_id=$1,
         revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(session)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

fn prepare_manifest(
    custody: Arc<crate::custody::Custody>,
    session: Uuid,
    members: Vec<(Uuid, Vec<u8>)>,
    material: [KeyMaterial; 2],
    width: u8,
    comparison: Predicate,
) -> Result<ArtifactRef> {
    let mut inputs = Vec::new();
    for (index, (account, sealed)) in members.iter().enumerate() {
        let bytes = custody
            .vault
            .open(*account, &format!("session-input:{session}"), sealed)?;
        let value = u64::from_le_bytes(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Internal)?,
        );
        let key = &material[index];
        let public = custody
            .artifacts
            .read(&key.public_key, tfhe_math::codec::PUBLIC_KEY_LIMIT)
            .map_err(|_| ApiError::Unavailable)?;
        let public = tfhe_math::codec::import_public_key(
            &public,
            key.binding().map_err(|_| ApiError::Internal)?,
            tfhe_math::codec::EncodingLimits::default(),
        )
        .map_err(|_| ApiError::Unavailable)?;
        let encrypted = NumberInput::encrypt(
            &tfhe_math::EncryptionContext::new(public),
            check_workflow::messages::width(width).map_err(|_| ApiError::Internal)?,
            value,
        )
        .map_err(|_| ApiError::Unavailable)?;
        inputs.push(
            custody
                .artifacts
                .write(encrypted.encoded().as_bytes())
                .map_err(|_| ApiError::Unavailable)?,
        );
    }
    let manifest = CheckManifest {
        schema_version: 1,
        assurance: Assurance::HonestRoleMechanics,
        external_context: sha2::Sha256::digest(session.as_bytes()).into(),
        owners: [members[0].0, members[1].0],
        width,
        comparison,
        keys: material,
        inputs: inputs.try_into().map_err(|_| ApiError::Internal)?,
    };
    custody
        .artifacts
        .write_json(&manifest)
        .map_err(|_| ApiError::Unavailable)
}

#[derive(sqlx::FromRow)]
struct Claimed {
    id: Uuid,
    session_id: Uuid,
    account_id: Uuid,
    request_id: Uuid,
    request_ref: Value,
    generation: i64,
    kind: String,
    approval: Value,
}

async fn process_one(state: &AppState) -> Result<bool> {
    let lease = Uuid::new_v4();
    let row: Option<Claimed> = sqlx::query_as(
        "UPDATE approval_messages SET status='processing', lease_id=$1,
         lease_until=now()+interval '10 minutes', revision=nextval('message_revision')
         WHERE id=(SELECT a.id FROM approval_messages a JOIN approval_sessions s ON
         s.id=a.session_id JOIN approval_members m ON m.session_id=s.id AND
         m.account_id=a.account_id WHERE s.status='active' AND m.status='active' AND
         (a.status='queued' OR (a.status='processing' AND a.lease_until<now())) ORDER BY
         a.created_at LIMIT 1 FOR UPDATE OF a SKIP LOCKED) RETURNING id, session_id,
         account_id, request_id, request_ref, generation, kind, approval",
    )
    .bind(lease)
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let result = execute(state, &row).await;
    let mut tx = state.pool.begin().await?;
    repository::lock_session(&mut tx, row.session_id).await?;
    match result {
        Ok(output) => {
            let changed = sqlx::query(
                "UPDATE approval_messages SET status='response_pending', response_ref=$3,
                 private_state=$4, lease_id=NULL, lease_until=NULL,
                 revision=nextval('message_revision') WHERE id=$1 AND lease_id=$2 AND
                 status='processing'",
            )
            .bind(row.id)
            .bind(lease)
            .bind(json!(output.response))
            .bind(output.private)
            .execute(&mut *tx)
            .await?;
            if changed.rows_affected() == 1 {
                events::record_node(
                    &mut tx,
                    "message.response_ready",
                    json!({"message_id":row.id,"session_id":row.session_id}),
                )
                .await?;
            }
        }
        Err(error) => {
            tracing::warn!(message_id=%row.id,error=%error,"custodial response blocked");
            clock::interrupt(&mut tx, row.session_id).await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

async fn execute(state: &AppState, row: &Claimed) -> Result<operations::Output> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?.clone();
    let reference: ArtifactRef =
        serde_json::from_value(row.request_ref.clone()).map_err(|_| ApiError::Internal)?;
    let request: Request = custody
        .artifacts
        .record(&reference)
        .map_err(|_| ApiError::Unavailable)?;
    let context: Value =
        sqlx::query_scalar("SELECT check_context FROM approval_sessions WHERE id=$1")
            .bind(row.session_id)
            .fetch_one(&state.pool)
            .await?;
    let context: ContextRecord = serde_json::from_value(context).map_err(|_| ApiError::Internal)?;
    let receipt_revision = row.approval["revision"]
        .as_i64()
        .ok_or(ApiError::Forbidden)?;
    let received_ms = row.approval["received_ms"]
        .as_i64()
        .ok_or(ApiError::Forbidden)?;
    signing::verify(
        &row.approval,
        &signing::Binding {
            account: row.account_id,
            session: row.session_id,
            action: row.id,
            request: row.request_id,
            revision: receipt_revision,
            digest: reference.digest,
            purpose: "approve",
            received_ms,
        },
    )?;
    let registered: String =
        sqlx::query_scalar("SELECT signing_public_key FROM custody_keys WHERE account_id=$1")
            .bind(row.account_id)
            .fetch_one(&state.pool)
            .await?;
    if row.approval["public_key"] != registered {
        return Err(ApiError::Forbidden);
    }
    let material = keys::material(&state.pool, row.account_id).await?;
    let mut connection = state.pool.acquire().await?;
    let secret = keys::secret(&mut connection, row.account_id).await?;
    let previous = match row.kind.as_str() {
        "answer_reveal" => Some("answer_commitment"),
        "mask_opening" => Some("mask_contribution"),
        _ => None,
    };
    let prior = if let Some(kind) = previous {
        sqlx::query_as::<_, (Uuid, Vec<u8>)>(
            "SELECT id, private_state FROM approval_messages WHERE session_id=$1 AND
             account_id=$2 AND generation=$3 AND kind=$4 AND private_state IS NOT NULL ORDER
             BY created_at DESC LIMIT 1",
        )
        .bind(row.session_id)
        .bind(row.account_id)
        .bind(row.generation)
        .bind(kind)
        .fetch_optional(&mut *connection)
        .await?
    } else {
        None
    };
    drop(connection);
    let work = operations::Work {
        account: row.account_id,
        message: row.id,
        request,
        context,
        material,
        secret,
        prior,
    };
    tokio::task::spawn_blocking(move || operations::execute(&custody, work))
        .await
        .map_err(|_| ApiError::Internal)?
}
