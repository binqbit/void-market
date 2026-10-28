use super::authentication::Actor;
use crate::{
    events,
    http::error::{ApiError, Result},
};
use arbiter_protocol::{ArtifactRef, Completion, Job, JobInput, NewJob, NodeProvision};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct Row {
    id: Uuid,
    node_id: Uuid,
    input: Value,
    status: String,
    generation: i64,
    instance_id: Option<Uuid>,
    response: Option<Value>,
    output: Option<Value>,
    cancel_requested: bool,
    interactive: bool,
}

impl Row {
    pub fn job(self) -> Result<Job> {
        let status = serde_json::from_value(json!(self.status)).map_err(|_| ApiError::Internal)?;
        Ok(Job {
            id: self.id,
            node_id: self.node_id,
            input: serde_json::from_value(self.input).map_err(|_| ApiError::Internal)?,
            status,
            generation: self.generation,
            cancel_requested: self.cancel_requested,
            interactive: self.interactive,
            demo_release: None,
            instance_id: self.instance_id,
            response: self
                .response
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| ApiError::Internal)?,
            output: self
                .output
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| ApiError::Internal)?,
        })
    }
}

#[cfg(feature = "demo-mode")]
pub async fn demo_release(
    pool: &PgPool,
    job: &Job,
) -> Result<Option<arbiter_protocol::DemoRelease>> {
    let closed: Option<i64> = sqlx::query_scalar(
        "SELECT a.closes_at_ms - 3600000
         FROM approval_sessions s
         JOIN auction_checks c ON c.session_id = s.id
         JOIN auctions a ON a.id = c.auction_id
         JOIN auction_participants l
             ON l.auction_id = a.id
                 AND l.account_id = c.left_account
         JOIN auction_participants r
             ON r.auction_id = a.id
                 AND r.account_id = c.right_account
         WHERE
             s.job_id = $1
             AND a.node_id = $2
             AND a.execution = 'localnet'
             AND a.status = 'matching'
             AND a.closes_at_ms - 3600000 < (s.check_context -> 'manifest' ->> 'release_at_ms')::bigint
             AND c.precomputed
             AND a.generation = c.generation
             AND a.round_root = c.round_root
             AND l.status = 'active'
             AND r.status = 'active'",
    )
    .bind(job.id)
    .bind(job.node_id)
    .fetch_optional(pool)
    .await?;
    Ok(closed.map(|closed_at_ms| arbiter_protocol::DemoRelease {
        input: job.input.clone(),
        closed_at_ms,
    }))
}

pub async fn provision(pool: &PgPool, input: NodeProvision) -> Result<()> {
    let ethereum_address = input
        .ethereum_address
        .as_deref()
        .map(crate::chains::ethereum::abi::address)
        .transpose()?;
    if input.name.is_empty() || input.name.len() > 64 {
        return Err(ApiError::BadRequest("Invalid node name"));
    }
    let token = std::fs::read_to_string(input.token_file)?;
    let token = token.trim();
    if token.len() < 32 || token.len() > 256 {
        return Err(ApiError::BadRequest(
            "Node token must contain 32 to 256 characters",
        ));
    }
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    sqlx::query(
        "INSERT INTO arbiter_nodes (id, deployment_id, name, public_key, token_hash, ethereum_address)
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
    )
    .bind(input.node_id)
    .bind(input.deployment_id)
    .bind(&input.name)
    .bind(&input.public_key)
    .bind(&token_hash)
    .bind(&ethereum_address)
    .execute(pool)
    .await?;
    let stored: (Uuid, String, String, Vec<u8>, Option<String>) = sqlx::query_as(
        "SELECT
             deployment_id,
             name,
             public_key,
             token_hash,
             ethereum_address
         FROM arbiter_nodes
         WHERE id = $1",
    )
    .bind(input.node_id)
    .fetch_one(pool)
    .await?;
    if stored
        != (
            input.deployment_id,
            input.name,
            input.public_key,
            token_hash,
            ethereum_address,
        )
    {
        return Err(ApiError::Conflict(
            "This node ID is already registered with different identity details",
        ));
    }
    Ok(())
}

pub async fn enqueue(pool: &PgPool, input: NewJob) -> Result<()> {
    let mut tx = pool.begin().await?;
    enqueue_in(&mut tx, input).await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn enqueue_in(connection: &mut PgConnection, input: NewJob) -> Result<()> {
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM arbiter_nodes WHERE id = $1 FOR UPDATE")
            .bind(input.node_id)
            .fetch_optional(&mut *connection)
            .await?;
    if enabled != Some(true) {
        return Err(ApiError::Forbidden);
    }

    let existing: Option<(Uuid, Value)> =
        sqlx::query_as("SELECT node_id, input FROM arbiter_jobs WHERE id = $1")
            .bind(input.id)
            .fetch_optional(&mut *connection)
            .await?;
    let payload = json!(input.input);
    if let Some((node, original)) = existing {
        if node != input.node_id || original != payload {
            return Err(ApiError::Conflict("Job ID has different input"));
        }
        return Ok(());
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM arbiter_jobs
         WHERE
             node_id = $1
             AND status IN (
                 'pending',
                 'queued',
                 'running',
                 'waiting',
                 'result_pending'
             )",
    )
    .bind(input.node_id)
    .fetch_one(&mut *connection)
    .await?;
    if pending >= 1000 {
        return Err(ApiError::Unavailable);
    }
    sqlx::query("INSERT INTO arbiter_jobs(id, node_id, input) VALUES($1, $2, $3)")
        .bind(input.id)
        .bind(input.node_id)
        .bind(payload)
        .execute(&mut *connection)
        .await?;
    events::record_node(connection, "node.job_submitted", json!({"job_id":input.id})).await?;
    Ok(())
}

pub async fn provide_response(pool: &PgPool, input: JobInput) -> Result<()> {
    let mut tx = pool.begin().await?;
    provide_response_in(&mut tx, input).await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn provide_response_in(
    connection: &mut PgConnection,
    input: JobInput,
) -> Result<()> {
    let row: Option<(Option<Value>, Option<Value>, Option<Value>)> = sqlx::query_as(
        "SELECT
             output,
             response,
             pending_response
         FROM arbiter_jobs
         WHERE
             id = $1
             AND status = 'waiting'
         FOR UPDATE",
    )
    .bind(input.job_id)
    .fetch_optional(&mut *connection)
    .await?;
    let (request, response, pending) =
        row.ok_or(ApiError::Conflict("Job is not waiting for a response"))?;
    if request
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        != Some(input.request_id.to_string().as_str())
    {
        return Err(ApiError::Conflict("Response refers to an old request"));
    }
    let value = json!(input.response);
    if response.as_ref() == Some(&value) || pending.as_ref() == Some(&value) {
        return Ok(());
    }
    if response.is_some() || pending.is_some() {
        return Err(ApiError::Conflict("A response is already recorded"));
    }
    sqlx::query("UPDATE arbiter_jobs SET pending_response = $2 WHERE id = $1")
        .bind(input.job_id)
        .bind(value)
        .execute(&mut *connection)
        .await?;
    events::record_node(
        connection,
        "node.response_submitted",
        json!({"job_id":input.job_id,"response_id":input.response.id}),
    )
    .await?;
    Ok(())
}

pub async fn begin_request(connection: &mut PgConnection, actor: &Actor) -> Result<Option<Value>> {
    sqlx::query(
        "INSERT INTO arbiter_requests (node_id, request_id, digest)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(actor.request.node_id)
    .bind(actor.request.request_id)
    .bind(&actor.digest)
    .execute(&mut *connection)
    .await?;
    let (digest, response): (Vec<u8>, Option<Value>) = sqlx::query_as(
        "SELECT
             digest,
             response
         FROM arbiter_requests
         WHERE
             node_id = $1
             AND request_id = $2
         FOR UPDATE",
    )
    .bind(actor.request.node_id)
    .bind(actor.request.request_id)
    .fetch_one(&mut *connection)
    .await?;
    if digest != actor.digest {
        return Err(ApiError::Conflict(
            "Replayed request has different signed bytes",
        ));
    }
    Ok(response)
}

pub async fn finish_request(
    connection: &mut PgConnection,
    actor: &Actor,
    response: &Value,
) -> Result<()> {
    sqlx::query(
        "UPDATE arbiter_requests
         SET response = $3
         WHERE
             node_id = $1
             AND request_id = $2",
    )
    .bind(actor.request.node_id)
    .bind(actor.request.request_id)
    .bind(response)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn apply_result(connection: &mut PgConnection, id: Uuid) -> Result<()> {
    let (payload, applied): (Value, bool) =
        sqlx::query_as("SELECT payload, applied FROM arbiter_results WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *connection)
            .await?;
    if applied {
        return Ok(());
    }
    let result: Completion = serde_json::from_value(payload).map_err(|_| ApiError::Internal)?;
    let public: String =
        sqlx::query_scalar("SELECT public_key FROM arbiter_nodes WHERE id = $1 AND enabled")
            .bind(result.node_id)
            .fetch_optional(&mut *connection)
            .await?
            .ok_or(ApiError::Forbidden)?;
    arbiter_protocol::verify_result(&result, &public).map_err(|_| ApiError::Unauthorized)?;

    let changed = sqlx::query(
        "UPDATE arbiter_jobs
         SET
             status = $2,
             output = $3,
             response = NULL,
             pending_response = NULL
         WHERE
             id = $1
             AND generation = $4
             AND status = 'result_pending'
             AND node_id = $5
             AND instance_id = $6",
    )
    .bind(result.job_id)
    .bind(result.status.as_str())
    .bind(result.output.map(|value: ArtifactRef| json!(value)))
    .bind(result.generation)
    .bind(result.node_id)
    .bind(result.instance_id)
    .execute(&mut *connection)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::Conflict("Result ownership changed"));
    }
    sqlx::query("UPDATE arbiter_results SET applied = TRUE WHERE id = $1")
        .bind(id)
        .execute(connection)
        .await?;
    Ok(())
}
