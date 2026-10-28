use super::{
    authentication::{self, Actor},
    repository::{self, Row},
};
use crate::{
    events,
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::{HttpRequest, HttpResponse, web};
use arbiter_protocol::{Claim, Completion, JobStatus, Registration};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::Transaction;
use uuid::Uuid;

pub fn configure(config: &mut web::ServiceConfig) {
    config.service(
        web::scope("/node/v1")
            .app_data(web::PayloadConfig::new(arbiter_protocol::BODY_LIMIT))
            .route("/register", web::post().to(register))
            .route("/heartbeat", web::post().to(heartbeat))
            .route("/jobs", web::get().to(jobs))
            .route("/jobs/{id}", web::get().to(job))
            .route("/jobs/{id}/claim", web::post().to(claim))
            .route("/jobs/{id}/renew", web::post().to(renew))
            .route("/jobs/{id}/results", web::post().to(result)),
    );
}

fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|_| ApiError::BadRequest("Invalid node request"))
}

fn reply(value: Value) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(value)
}

async fn finish(
    mut tx: Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    value: Value,
) -> Result<HttpResponse> {
    repository::finish_request(&mut tx, actor, &value).await?;
    tx.commit().await?;
    Ok(reply(value))
}

async fn register(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse> {
    registration(state, request, body, true).await
}

async fn heartbeat(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse> {
    registration(state, request, body, false).await
}

async fn registration(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Bytes,
    first: bool,
) -> Result<HttpResponse> {
    let actor = authentication::authorize(&state, &request, &body, first).await?;
    let input: Registration = parse(&body)?;
    let session_capacity = if input.session_capacity == 0 {
        input.capacity
    } else {
        input.session_capacity
    };
    if input.instance_id != actor.request.instance_id
        || !(1..=4).contains(&input.capacity)
        || !(input.capacity..=64).contains(&session_capacity)
        || input.sequence <= 0
    {
        return Err(ApiError::BadRequest("Invalid node registration"));
    }
    let mut tx = state.pool.begin().await?;
    if let Some(response) = repository::begin_request(&mut tx, &actor).await? {
        return Ok(reply(response));
    }
    let changed = sqlx::query(
        "UPDATE arbiter_nodes
         SET
             instance_id = $2,
             capacity = $3,
             heartbeat_sequence = $4,
             session_capacity = $5,
             heartbeat_until = now() + interval '90 seconds'
         WHERE
             id = $1
             AND (
                 instance_id IS NULL
                 OR instance_id = $2
             )
             AND heartbeat_sequence < $4
             AND enabled",
    )
    .bind(actor.request.node_id)
    .bind(input.instance_id)
    .bind(i16::try_from(input.capacity).map_err(|_| ApiError::BadRequest("Invalid capacity"))?)
    .bind(input.sequence)
    .bind(
        i16::try_from(session_capacity)
            .map_err(|_| ApiError::BadRequest("Invalid session capacity"))?,
    )
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::Conflict(
            "Stale node heartbeat or different instance",
        ));
    }
    events::record_node(
        &mut tx,
        "node.heartbeat",
        json!({"node_id":actor.request.node_id}),
    )
    .await?;
    finish(tx, &actor, json!({"registered":true})).await
}

async fn jobs(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse> {
    let actor = authentication::authorize(&state, &request, &[], false).await?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT
             id,
             node_id,
             input,
             status,
             generation,
             instance_id,
             response,
             output,
             cancel_requested,
             interactive
         FROM arbiter_jobs
         WHERE
             node_id = $1
             AND status IN (
                 'queued',
                 'running',
                 'waiting',
                 'result_pending'
             )
         ORDER BY
             created_at,
             id
         LIMIT 100",
    )
    .bind(actor.request.node_id)
    .fetch_all(&state.pool)
    .await?;
    let jobs = rows.into_iter().map(Row::job).collect::<Result<Vec<_>>>()?;
    Ok(reply(json!(jobs)))
}

async fn job(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let actor = authentication::authorize(&state, &request, &[], false).await?;
    let row: Row = sqlx::query_as(
        "SELECT
             id,
             node_id,
             input,
             status,
             generation,
             instance_id,
             response,
             output,
             cancel_requested,
             interactive
         FROM arbiter_jobs
         WHERE
             node_id = $1
             AND id = $2",
    )
    .bind(actor.request.node_id)
    .bind(*id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let job = row.job()?;
    #[cfg(feature = "demo-mode")]
    let job = {
        let mut job = job;
        if state.chains.is_some() {
            job.demo_release = repository::demo_release(&state.pool, &job).await?;
        }
        job
    };
    Ok(reply(json!(job)))
}

async fn claim(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Bytes,
) -> Result<HttpResponse> {
    let actor = authentication::authorize(&state, &request, &body, false).await?;
    let input: Claim = parse(&body)?;
    if input.job_id != *id || input.instance_id != actor.request.instance_id {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.pool.begin().await?;
    if let Some(response) = repository::begin_request(&mut tx, &actor).await? {
        return Ok(reply(response));
    }
    let capacity: Option<i16> = sqlx::query_scalar(
        "SELECT session_capacity
         FROM arbiter_nodes
         WHERE
             id = $1
             AND heartbeat_until > now()
             AND enabled
         FOR UPDATE",
    )
    .bind(actor.request.node_id)
    .fetch_optional(&mut *tx)
    .await?;
    let capacity = capacity.ok_or(ApiError::Unavailable)?;
    let running: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM arbiter_jobs
         WHERE
             node_id = $1
             AND status IN (
                 'running',
                 'waiting',
                 'result_pending'
             )",
    )
    .bind(actor.request.node_id)
    .fetch_one(&mut *tx)
    .await?;
    if running >= i64::from(capacity) {
        return Err(ApiError::Unavailable);
    }
    let row: Row = sqlx::query_as(
        "UPDATE arbiter_jobs
         SET
             status = 'running',
             generation = generation + 1,
             instance_id = $3,
             lease_until = now() + interval '90 seconds'
         WHERE
             id = $1
             AND node_id = $2
             AND status = 'queued'
             AND generation = $4 RETURNING
             id,
             node_id,
             input,
             status,
             generation,
             instance_id,
             response,
             output,
             cancel_requested,
             interactive",
    )
    .bind(*id)
    .bind(actor.request.node_id)
    .bind(actor.request.instance_id)
    .bind(input.generation)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::Conflict("Job has already been claimed"))?;
    finish(tx, &actor, json!(row.job()?)).await
}

async fn renew(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Bytes,
) -> Result<HttpResponse> {
    let actor = authentication::authorize(&state, &request, &body, false).await?;
    let input: Claim = parse(&body)?;
    if input.job_id != *id || input.instance_id != actor.request.instance_id {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.pool.begin().await?;
    if let Some(response) = repository::begin_request(&mut tx, &actor).await? {
        return Ok(reply(response));
    }
    let changed = sqlx::query(
        "UPDATE arbiter_jobs
         SET lease_until = now() + interval '90 seconds'
         WHERE
             id = $1
             AND node_id = $2
             AND instance_id = $3
             AND generation = $4
             AND status IN (
                 'running',
                 'waiting',
                 'result_pending'
             )",
    )
    .bind(*id)
    .bind(actor.request.node_id)
    .bind(actor.request.instance_id)
    .bind(input.generation)
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::Conflict("Job lease is no longer owned"));
    }
    finish(tx, &actor, json!({"renewed":true})).await
}

async fn result(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Bytes,
) -> Result<HttpResponse> {
    let actor = authentication::authorize(&state, &request, &body, false).await?;
    let input: Completion = parse(&body)?;
    if input.node_id != actor.request.node_id
        || input.deployment_id != actor.request.deployment_id
        || input.instance_id != actor.request.instance_id
    {
        return Err(ApiError::Forbidden);
    }
    arbiter_protocol::verify_result(&input, &actor.public_key)
        .map_err(|_| ApiError::Unauthorized)?;

    if input.job_id != *id
        || !matches!(
            input.status,
            JobStatus::Waiting
                | JobStatus::Completed
                | JobStatus::Interrupted
                | JobStatus::Failed
                | JobStatus::Unavailable
        )
    {
        return Err(ApiError::BadRequest("Invalid result status"));
    }
    if matches!(input.status, JobStatus::Waiting | JobStatus::Completed) != input.output.is_some() {
        return Err(ApiError::BadRequest("Result output does not match status"));
    }
    let mut tx = state.pool.begin().await?;
    if let Some(response) = repository::begin_request(&mut tx, &actor).await? {
        return Ok(reply(response));
    }
    let existing: Option<(Uuid, Value)> = sqlx::query_as(
        "SELECT
             r.job_id,
             r.payload
         FROM arbiter_results r
         JOIN arbiter_jobs j ON j.id = r.job_id
         WHERE
             r.id = $1
             AND j.node_id = $2",
    )
    .bind(input.id)
    .bind(actor.request.node_id)
    .fetch_optional(&mut *tx)
    .await?;
    let payload = json!(input);
    if let Some((job, original)) = existing {
        if job != *id || original != payload {
            return Err(ApiError::Conflict("Result ID has different contents"));
        }
        return finish(tx, &actor, json!({"recorded":true})).await;
    }
    let row: Row = sqlx::query_as(
        "SELECT
             id,
             node_id,
             input,
             status,
             generation,
             instance_id,
             response,
             output,
             cancel_requested,
             interactive
         FROM arbiter_jobs
         WHERE
             id = $1
             AND node_id = $2
         FOR UPDATE",
    )
    .bind(*id)
    .bind(actor.request.node_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    let current = row.job()?;
    if (current.cancel_requested && input.status != JobStatus::Interrupted)
        || current.instance_id != Some(actor.request.instance_id)
        || current.generation != input.generation
        || !matches!(current.status, JobStatus::Running | JobStatus::Waiting)
        || (input.status != JobStatus::Interrupted
            && input.consumed_response != current.response.as_ref().map(|value| value.id))
    {
        return Err(ApiError::Conflict(
            "Result has stale job ownership or response",
        ));
    }
    if input.status != JobStatus::Interrupted {
        let current_lease: bool =
            sqlx::query_scalar("SELECT lease_until > now() FROM arbiter_jobs WHERE id = $1")
                .bind(*id)
                .fetch_one(&mut *tx)
                .await?;
        if !current_lease {
            return Err(ApiError::Conflict("Job lease has expired"));
        }
    }
    sqlx::query("INSERT INTO arbiter_results(id, job_id, payload) VALUES($1, $2, $3)")
        .bind(input.id)
        .bind(*id)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE arbiter_jobs SET status = 'result_pending' WHERE id = $1")
        .bind(*id)
        .execute(&mut *tx)
        .await?;
    events::record_node(
        &mut tx,
        "node.result_submitted",
        json!({"result_id":input.id,"job_id":input.job_id}),
    )
    .await?;
    finish(tx, &actor, json!({"recorded":true})).await
}
