use super::Node;
use crate::support::Fixture;
use arbiter_protocol::{ArtifactRef, NewJob};
use market::{events, nodes};
use rdkafka::{
    ClientConfig,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    producer::FutureRecord,
    util::Timeout,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL and TEST_KAFKA_BROKERS"]
async fn early_bidding_close_does_not_block_following_node_events() {
    let mut fixture = Fixture::new().await;
    let topic = format!("test.voidmarket.node-events.{}", Uuid::new_v4().simple());
    Arc::get_mut(&mut fixture.state.config)
        .unwrap()
        .node_event_topic = topic.clone();
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", &fixture.state.config.kafka_brokers)
        .create()
        .unwrap();
    let options = AdminOptions::new().request_timeout(Some(Duration::from_secs(10)));
    admin
        .create_topics(
            &[NewTopic::new(&topic, 1, TopicReplication::Fixed(1))],
            &options,
        )
        .await
        .unwrap()[0]
        .as_ref()
        .unwrap();

    let node = Node::provision(&fixture, 29).await;
    let job = Uuid::new_v4();
    nodes::enqueue(
        &fixture.state.pool,
        NewJob {
            id: job,
            node_id: node.id,
            input: ArtifactRef {
                id: Uuid::new_v4(),
                digest: [7; 32],
            },
        },
    )
    .await
    .unwrap();

    let producer = events::producer(&fixture.state.config.kafka_brokers).unwrap();
    let closed_id = Uuid::new_v4();
    let closed = json!({
        "schema_version": 1,
        "id": closed_id,
        "kind": "auction.bidding_closed_early",
        "data": { "auction_id": Uuid::new_v4(), "closed_at_ms": 1 },
    });
    let queued = json!({
        "schema_version": 1,
        "id": Uuid::new_v4(),
        "kind": "node.job_submitted",
        "data": { "job_id": job },
    });
    for event in [&closed, &closed, &queued] {
        let payload = serde_json::to_vec(event).unwrap();
        producer
            .send(
                FutureRecord::to(&topic)
                    .partition(0)
                    .key("ordered-auction-events")
                    .payload(&payload),
                Timeout::After(Duration::from_secs(10)),
            )
            .await
            .unwrap();
    }

    let (stop, signal) = watch::channel(false);
    let consumer = tokio::spawn(nodes::consume(fixture.state.clone(), signal));
    let progressed = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let status: String = sqlx::query_scalar("SELECT status FROM arbiter_jobs WHERE id=$1")
                .bind(job)
                .fetch_one(&fixture.state.pool)
                .await
                .unwrap();
            if status == "queued" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    let processed: i64 = sqlx::query_scalar("SELECT count(*) FROM node_event_inbox WHERE id=$1")
        .bind(closed_id)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();

    stop.send(true).unwrap();
    consumer.await.unwrap().unwrap();
    admin.delete_topics(&[&topic], &options).await.unwrap()[0]
        .as_ref()
        .unwrap();
    fixture.cleanup().await;

    assert!(
        progressed.is_ok(),
        "The early-close event must not stall later node work"
    );
    assert_eq!(processed, 1, "Repeated early-close events are idempotent");
}
