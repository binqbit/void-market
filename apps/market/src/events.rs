use crate::{http::error::Result, state::AppState};
use rdkafka::{
    ClientConfig,
    producer::{FutureProducer, FutureRecord, Producer},
    util::Timeout,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::{sync::atomic::Ordering, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct Event {
    id: Uuid,
    kind: String,
    data: Value,
}

pub async fn record(connection: &mut PgConnection, kind: &str, data: Value) -> Result<()> {
    sqlx::query(
        "INSERT INTO outbox(id, kind, data)
         VALUES ($1,$2,$3)",
    )
    .bind(Uuid::new_v4())
    .bind(kind)
    .bind(data)
    .execute(connection)
    .await?;
    Ok(())
}

pub fn producer(brokers: &str) -> std::result::Result<FutureProducer, rdkafka::error::KafkaError> {
    ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("enable.idempotence", "true")
        .set("acks", "all")
        .set("message.timeout.ms", "5000")
        .set("socket.timeout.ms", "5000")
        .create()
}

pub async fn deliver_batch(
    state: &AppState,
    producer: &FutureProducer,
) -> std::result::Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut tx = state.pool.begin().await?;
    let events: Vec<Event> = sqlx::query_as(
        "SELECT id, kind, data
         FROM outbox
         WHERE published_at IS NULL
         ORDER BY created_at, id
         LIMIT 20 FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;

    for event in &events {
        let key = event.id.to_string();
        let payload =
            serde_json::to_vec(&json!({"id": event.id, "kind": event.kind, "data": event.data}))?;
        producer
            .send(
                FutureRecord::to(&state.config.event_topic)
                    .key(&key)
                    .payload(&payload),
                Timeout::After(Duration::from_secs(5)),
            )
            .await
            .map_err(|(error, _)| error)?;
        sqlx::query(
            "UPDATE outbox SET published_at = now()
         WHERE id = $1",
        )
        .bind(event.id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(events.len())
}

pub async fn run(
    state: AppState,
    mut stop: watch::Receiver<bool>,
) -> std::result::Result<(), rdkafka::error::KafkaError> {
    let producer = producer(&state.config.kafka_brokers)?;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut tick = 0u64;

    loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = interval.tick() => {
                let result = deliver_batch(&state, &producer).await;
                let ready = result.is_ok() && broker_ready(&producer, state.config.event_topic.clone()).await;
                state.worker_ready.store(ready, Ordering::Relaxed);
                if let Err(error) = result {
                    tracing::warn!(error = %error, "outbox delivery will retry");
                }

                tick += 1;
                if tick.is_multiple_of(60) {
                    remove_expired_sessions(&state.pool).await;
                }
            }
        }
    }

    state.worker_ready.store(false, Ordering::Relaxed);
    Ok(())
}

async fn broker_ready(producer: &FutureProducer, topic: String) -> bool {
    let producer = producer.clone();
    tokio::task::spawn_blocking(move || {
        producer
            .client()
            .fetch_metadata(Some(&topic), Duration::from_secs(2))
            .is_ok_and(|metadata| {
                metadata.topics().iter().any(|entry| {
                    entry.name() == topic
                        && entry.error().is_none()
                        && !entry.partitions().is_empty()
                })
            })
    })
    .await
    .unwrap_or(false)
}

async fn remove_expired_sessions(pool: &sqlx::PgPool) {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
        .execute(pool)
        .await;
    if let Err(error) = result {
        tracing::warn!(error = %error, "expired session cleanup will retry");
    }
}
