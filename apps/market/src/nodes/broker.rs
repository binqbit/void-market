use super::repository;
use crate::{
    http::error::{ApiError, Result},
    state::AppState,
};
use rdkafka::{
    ClientConfig, Message,
    consumer::{CommitMode, Consumer, StreamConsumer},
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{sync::atomic::Ordering, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    schema_version: u8,
    id: Uuid,
    kind: String,
    data: Value,
}

fn event_id(data: &Value, name: &str) -> Result<Uuid> {
    serde_json::from_value(data[name].clone())
        .map_err(|_| ApiError::BadRequest("Invalid node event"))
}

async fn apply(state: &AppState, bytes: &[u8]) -> Result<()> {
    if bytes.len() > arbiter_protocol::BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    let event: Event =
        serde_json::from_slice(bytes).map_err(|_| ApiError::BadRequest("Invalid node event"))?;
    if event.schema_version != 1 {
        return Err(ApiError::BadRequest("Unsupported node event version"));
    }
    let digest = Sha256::digest(bytes).to_vec();
    let mut tx = state.pool.begin().await?;
    let inserted =
        sqlx::query("INSERT INTO node_event_inbox(id,digest) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(event.id)
            .bind(&digest)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if inserted == 0 {
        let original: Vec<u8> =
            sqlx::query_scalar("SELECT digest FROM node_event_inbox WHERE id=$1")
                .bind(event.id)
                .fetch_one(&mut *tx)
                .await?;
        if original != digest {
            return Err(ApiError::Conflict("Event ID has conflicting contents"));
        }
        return Ok(());
    }

    match event.kind.as_str() {
        "node.job_submitted" => {
            let id = event_id(&event.data, "job_id")?;
            sqlx::query("UPDATE arbiter_jobs SET status='queued' WHERE id=$1 AND status='pending'")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        "node.result_submitted" => {
            let id = event_id(&event.data, "result_id")?;
            crate::messages::lock_result_session(&mut tx, id).await?;
            repository::apply_result(&mut tx, id).await?;
            crate::messages::apply_node_result(state, &mut tx, id).await?;
        }
        "node.response_submitted" => {
            let id = event_id(&event.data, "job_id")?;
            let response = event_id(&event.data, "response_id")?;
            sqlx::query(
                "UPDATE arbiter_jobs
                 SET response=pending_response, pending_response=NULL
                 WHERE id=$1
                 AND status='waiting'
                 AND pending_response->>'id'=$2",
            )
            .bind(id)
            .bind(response.to_string())
            .execute(&mut *tx)
            .await?;
        }
        "message.approved" => {
            let id = event_id(&event.data, "message_id")?;
            sqlx::query(
                "UPDATE approval_messages SET status='queued',
                 revision=nextval('message_revision') WHERE id=$1 AND status='approved'",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        "message.response_ready" => {
            crate::messages::consume_response(state, &mut tx, event_id(&event.data, "message_id")?)
                .await?;
        }
        "node.heartbeat" | "session.created" | "message.expired" | "message.withdrawn" => {}
        _ => return Err(ApiError::BadRequest("Unknown node event")),
    }
    tx.commit().await?;
    Ok(())
}

pub async fn consume(
    state: AppState,
    mut stop: watch::Receiver<bool>,
) -> std::result::Result<(), rdkafka::error::KafkaError> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &state.config.kafka_brokers)
        .set(
            "group.id",
            format!("market-{}", state.config.node_event_topic),
        )
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .set("auto.offset.reset", "earliest")
        .create()?;
    consumer.subscribe(&[&state.config.node_event_topic])?;

    loop {
        tokio::select! {
            _ = stop.changed() => break,
            incoming = consumer.recv() => {
                let message = match incoming {
                    Ok(message) => message,
                    Err(_) => {
                        state.node_ready.store(false, Ordering::Relaxed);
                        continue;
                    }
                };
                let mut reported = false;
                loop {
                    let result = match message.payload() {
                        Some(bytes) => apply(&state, bytes).await,
                        None => Err(ApiError::BadRequest("Empty node event")),
                    };
                    if result.is_ok() {
                        consumer.commit_message(&message, CommitMode::Sync)?;
                        state.node_ready.store(true, Ordering::Relaxed);
                        break;
                    }
                    state.node_ready.store(false, Ordering::Relaxed);
                    if !reported {
                        tracing::warn!(
                            partition = message.partition(),
                            offset = message.offset(),
                            "node event remains pending; intake will retry"
                        );
                        reported = true;
                    }
                    tokio::select! {
                        _ = stop.changed() => return Ok(()),
                        _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                state.node_ready.store(
                    consumer.assignment().is_ok_and(|assignment| assignment.count() > 0),
                    Ordering::Relaxed,
                );
            }
        }
    }
    state.node_ready.store(false, Ordering::Relaxed);
    Ok(())
}
