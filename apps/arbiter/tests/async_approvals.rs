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
