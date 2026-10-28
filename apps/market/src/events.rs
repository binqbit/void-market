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
    stream: String,
}

pub async fn record(connection: &mut PgConnection, kind: &str, data: Value) -> Result<()> {
    sqlx::query("INSERT INTO outbox(id, kind, data) VALUES ($1, $2, $3)")
        .bind(Uuid::new_v4())
        .bind(kind)
        .bind(data)
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn record_node(connection: &mut PgConnection, kind: &str, data: Value) -> Result<()> {
    sqlx::query("INSERT INTO outbox(id, kind, data, stream) VALUES($1, $2, $3, 'nodes')")
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
    let claim = Uuid::new_v4();
    let events: Vec<Event> = sqlx::query_as(
        "UPDATE outbox
         SET
             claim_id = $1,
             claim_until = now() + interval '120 seconds'
         WHERE
             id IN
             (
                 SELECT id
                 FROM outbox
                 WHERE
                     published_at IS NULL
                     AND (
                         claim_until IS NULL
                         OR claim_until < now()
                     )
                 ORDER BY
                     created_at,
                     id
                 LIMIT 20
                 FOR UPDATE SKIP LOCKED
             )
         RETURNING
             id,
             kind,
             data,
             stream",
    )
    .bind(claim)
    .fetch_all(&state.pool)
    .await?;

    for event in &events {
        let key = event
            .data
            .get("job_id")
            .or_else(|| event.data.get("node_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| event.id.to_string());
        let payload = serde_json::to_vec(
            &json!({"schema_version":1,"id":event.id,"kind":event.kind,"data":event.data}),
        )?;
        let topic = if event.stream == "nodes" {
            &state.config.node_event_topic
        } else {
            &state.config.event_topic
        };
        let delivered = producer
            .send(
                FutureRecord::to(topic).key(&key).payload(&payload),
                Timeout::After(Duration::from_secs(5)),
            )
            .await;
        if let Err((error, _)) = delivered {
            let _ = sqlx::query(
                "UPDATE outbox
                 SET
                     claim_id = NULL,
                     claim_until = NULL
                 WHERE claim_id = $1",
            )
            .bind(claim)
            .execute(&state.pool)
            .await;
            return Err(error.into());
        }
        sqlx::query(
            "UPDATE outbox
             SET
                 published_at = now(),
                 claim_id = NULL,
                 claim_until = NULL
             WHERE
                 id = $1
                 AND claim_id = $2",
        )
        .bind(event.id)
        .bind(claim)
        .execute(&state.pool)
        .await?;
    }
    Ok(events.len())
}

pub async fn run(
    state: AppState,
    mut stop: watch::Receiver<bool>,
) -> std::result::Result<(), rdkafka::error::KafkaError> {
    let producer = producer(&state.config.kafka_brokers)?;
    let (children_stop, children_signal) = watch::channel(false);
    let custody_enabled = state.custody.is_some();
    let native_enabled = state.chains.is_some();
    let mut native = tokio::spawn(crate::settlement::run(
        state.clone(),
        children_signal.clone(),
    ));
    let mut custody = tokio::spawn(crate::messages::run(state.clone(), children_signal.clone()));
    let mut consumer = tokio::spawn(crate::nodes::consume(state.clone(), children_signal));
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut tick = 0u64;
    let mut consumer_ended = false;
    let mut custody_ended = false;
    let mut native_ended = false;
    let mut failure = None;
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            result = &mut consumer => {
                consumer_ended = true;
                failure = Some(
                    result.unwrap_or(Err(rdkafka::error::KafkaError::Canceled))
                        .err().unwrap_or(rdkafka::error::KafkaError::Canceled),
                );
                break;
            }
            result = &mut custody, if custody_enabled => {
                custody_ended = true;
                tracing::error!(successful = result.is_ok_and(|result| result.is_ok()), "approval worker stopped");
                failure = Some(rdkafka::error::KafkaError::Canceled);
                break;
            }
            result = &mut native, if native_enabled => {
                native_ended = true;
                tracing::error!(successful = result.is_ok_and(|result|result.is_ok()), "local-chain worker stopped");
                failure = Some(rdkafka::error::KafkaError::Canceled);
                break;
            }
            _ = interval.tick() => {
                let result = deliver_batch(&state, &producer).await;
                let native_ready = if native_enabled {
                    sqlx::query_scalar::<_,bool>(
                        "SELECT EXISTS(
                             SELECT 1
                             FROM localnet_runtime
                             WHERE
                                 singleton
                                 AND healthy
                                 AND $1 - last_tick_ms BETWEEN 0 AND 5000
                         )",
                    )
                        .bind(crate::messages::now_ms().unwrap_or(0)).fetch_one(&state.pool).await.unwrap_or(false)
                } else {true};
                let ready = native_ready && result.is_ok() && state.node_ready.load(Ordering::Relaxed)
                    && broker_ready(&producer, state.config.event_topic.clone()).await;
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
    let _ = children_stop.send(true);
    if !consumer_ended {
        finish_task(&mut consumer).await;
    }
    if !custody_ended {
        finish_task(&mut custody).await;
    }
    if !native_ended {
        finish_task(&mut native).await;
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

async fn finish_task<T>(task: &mut tokio::task::JoinHandle<T>) {
    if tokio::time::timeout(Duration::from_secs(5), &mut *task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
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
    let _ = sqlx::query(
        "DELETE
         FROM arbiter_requests
         WHERE created_at < now() - interval '10 minutes'",
    )
    .execute(pool)
    .await;
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
        .execute(pool)
        .await;
    if let Err(error) = result {
        tracing::warn!(error = %error, "expired session cleanup will retry");
    }
}
