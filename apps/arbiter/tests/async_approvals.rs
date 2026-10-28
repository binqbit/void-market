#[path = "support/trading.rs"]
mod trading;

use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use trading::Trading;
use uuid::Uuid;

async fn masks(fixture: &Trading, owner: usize, expected: usize) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let snapshot = fixture.get(Some(owner), "/v1/messages").await;
            let messages: Vec<Value> = snapshot["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|message| message["status"] == "ready")
                .cloned()
                .collect();
            if messages.len() == expected {
                for message in &messages {
                    assert_eq!(message["stage"], "mask_contribution");
                    assert_eq!(message["step"], 1);
                    assert_eq!(message["timer_running"], false);
                    assert_eq!(message["remaining_ms"], 600_000);
                }
                break messages;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Independent mask approvals must be available before bidding closes")
}

async fn approve(fixture: &Trading, owner: usize, message: &Value) {
    fixture
        .post(
            owner,
            &format!("/v1/messages/{}/approve", message["id"].as_str().unwrap()),
            json!({"request_id": message["request_id"], "revision": message["revision"]}),
        )
        .await
        .error_for_status()
        .unwrap();
}

#[actix_web::test]
#[ignore = "requires PostgreSQL and fresh Kafka topics; real HE account keys"]
async fn early_requests_are_independent_and_do_not_consume_the_approval_budget() {
    let fixture = Trading::new(3).await;
    let asset = Uuid::new_v4();
    fixture
        .post(
            0,
            "/v1/assets",
            json!({"request_id":asset,"name":"Asynchronous approvals"}),
        )
        .await
        .error_for_status()
        .unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let auction = fixture
        .post(
            0,
            "/v1/auctions",
            json!({
                "request_id":Uuid::new_v4(),"asset_id":asset,"reserve":"1",
                "closes_at_ms":now + 7_200_000,
            }),
        )
        .await
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let path = format!("/v1/auctions/{}", auction["id"].as_str().unwrap());
    let published = fixture
        .post(
            0,
            &format!("{path}/publish"),
            json!({"revision":auction["revision"],"consent":true}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        published["seller"]["address"],
        fixture.accounts[0].to_string()
    );

    fixture
        .post(
            1,
            &format!("{path}/bids"),
            json!({"request_id":Uuid::new_v4(),
        "amount":"2","policy_hash":published["policy_hash"]}),
        )
        .await
        .error_for_status()
        .unwrap();
    let first = masks(&fixture, 1, 1).await;
    masks(&fixture, 0, 1).await;

    // The first buyer stays offline. Their wait must not consume the node's CPU slot.
    fixture
        .post(
            2,
            &format!("{path}/bids"),
            json!({"request_id":Uuid::new_v4(),
        "amount":"3","policy_hash":published["policy_hash"]}),
        )
        .await
        .error_for_status()
        .unwrap();
    let second = masks(&fixture, 2, 2).await;
    let retained = masks(&fixture, 1, 2).await;
    assert!(
        retained
            .iter()
            .any(|message| message["id"] == first[0]["id"])
    );
    masks(&fixture, 0, 2).await;

    for request in second {
        fixture
            .post(
                2,
                &format!("/v1/messages/{}/approve", request["id"].as_str().unwrap()),
                json!({"request_id":request["request_id"],"revision":request["revision"]}),
            )
            .await
            .error_for_status()
            .unwrap();
    }
    let view = fixture.get(Some(2), &path).await;
    assert_eq!(view["status"], "open");
    assert_eq!(view["closes_at_ms"], auction["closes_at_ms"]);
    assert_eq!(view["participation"]["remaining_ms"], 600_000);
    assert_eq!(view["participation"]["timer_running"], false);
    assert!(view["winner"].is_null());

    let results: i64 = sqlx::query_scalar("SELECT count(result) FROM auction_checks")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(results, 0);
    fixture.close().await;
}

#[actix_web::test]
#[ignore = "requires PostgreSQL and fresh Kafka topics; real HE account keys"]
async fn frozen_auction_requests_number_before_masks_are_approved() {
    let fixture = Trading::new(2).await;
    let asset = Uuid::new_v4();
    fixture
        .post(
            0,
            "/v1/assets",
            json!({"request_id": asset, "name": "Parallel preparation"}),
        )
        .await
        .error_for_status()
        .unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let auction = fixture
        .post(
            0,
            "/v1/auctions",
            json!({
                "request_id": Uuid::new_v4(), "asset_id": asset, "reserve": "1",
                "closes_at_ms": now + 3_720_000,
            }),
        )
        .await
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let path = format!("/v1/auctions/{}", auction["id"].as_str().unwrap());
    let published = fixture
        .post(
            0,
            &format!("{path}/publish"),
            json!({"revision": auction["revision"], "consent": true}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    fixture
        .post(
            1,
            &format!("{path}/bids"),
            json!({"request_id": Uuid::new_v4(), "amount": "2", "policy_hash": published["policy_hash"]}),
        )
        .await
        .error_for_status()
        .unwrap();
    let buyer_mask = masks(&fixture, 1, 1).await.remove(0);
    let seller_mask = masks(&fixture, 0, 1).await.remove(0);
    let (blocked_id, blocked_request, blocked_revision): (Uuid, Uuid, i64) = sqlx::query_as(
        "SELECT id,request_id,revision FROM approval_messages
         WHERE kind='number_transfer' AND status='blocked'",
    )
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    let premature = fixture
        .post(
            1,
            &format!("/v1/messages/{blocked_id}/approve"),
            json!({"request_id": blocked_request, "revision": blocked_revision}),
        )
        .await;
    assert_eq!(premature.status(), reqwest::StatusCode::CONFLICT);

    let transfer = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let snapshot = fixture.get(Some(1), "/v1/messages").await;
            if let Some(message) = snapshot["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["stage"] == "number_transfer" && item["status"] == "ready")
            {
                break message.clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Number transfer must be requested after freeze even if both masks are pending");
    assert_eq!(fixture.get(Some(1), &path).await["status"], "matching");

    approve(&fixture, 1, &transfer).await;
    let transfer_id = Uuid::parse_str(transfer["id"].as_str().unwrap()).unwrap();
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let status: String =
                sqlx::query_scalar("SELECT status FROM approval_messages WHERE id=$1")
                    .bind(transfer_id)
                    .fetch_one(&fixture.state.pool)
                    .await
                    .unwrap();
            if status == "responded" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Number response must be retained while masks are still pending");
    assert_eq!(fixture.get(Some(1), &path).await["checks_completed"], 0);

    for (owner, message) in [(1, &buyer_mask), (0, &seller_mask)] {
        approve(&fixture, owner, message).await;
    }
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let snapshot = fixture.get(Some(0), "/v1/messages").await;
            if snapshot["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["stage"] == "answer_commitment" && item["status"] == "ready")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Comparison must proceed once all independent responses are collected");

    let result = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let view = fixture.get(Some(0), &path).await;
            assert_ne!(view["status"], "interrupted");
            assert_ne!(view["status"], "no_sale");
            if view["status"] == "awaiting_completion" {
                break view;
            }
            for owner in 0..2 {
                let snapshot = fixture.get(Some(owner), "/v1/messages").await;
                for message in snapshot["messages"].as_array().unwrap() {
                    if message["status"] == "ready" {
                        approve(&fixture, owner, message).await;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Completed approvals must select the buyer and advance settlement");
    assert_eq!(result["winner"]["address"], fixture.accounts[1].to_string());
    assert_eq!(result["checks_completed"], result["checks_total"]);
    fixture.close().await;
}
