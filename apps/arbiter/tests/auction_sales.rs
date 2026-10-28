#[path = "support/trading.rs"]
mod trading;

use reqwest::StatusCode;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use trading::Trading;
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires PostgreSQL, fresh Kafka topics; real 64-bit HE auction and manual approvals"]
async fn native_auction_keeps_tied_bids_private_and_delivers_once_after_seller_ack() {
    let fixture = Trading::new(3).await;
    let asset = Uuid::new_v4();
    fixture
        .post(
            0,
            "/v1/assets",
            json!({"request_id":asset,"name":"Native auction asset"}),
        )
        .await
        .error_for_status()
        .unwrap();
    let auction = Uuid::new_v4();
    let close = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + 3_600_000
        + 30_000;
    let draft: Value = fixture
        .post(
            0,
            "/v1/auctions",
            json!({
                "request_id": auction,
                "asset_id": asset,
                "reserve": "0.00000125",
                "closes_at_ms": close,
            }),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let detail_path = format!("/v1/auctions/{auction}");
    let published: Value = fixture
        .post(
            0,
            &format!("{detail_path}/publish"),
            json!({"revision":draft["revision"],"consent":true}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut bid_requests = Vec::new();
    for owner in [1, 2] {
        let body = json!({
            "request_id": Uuid::new_v4(),
            "amount": "0.00000125",
            "policy_hash": published["policy_hash"],
        });
        let accepted = fixture
            .post(owner, &format!("{detail_path}/bids"), body.clone())
            .await;
        assert_eq!(
            accepted.status(),
            StatusCode::ACCEPTED,
            "{}",
            accepted.text().await.unwrap()
        );
        fixture
            .post(owner, &format!("{detail_path}/bids"), body.clone())
            .await
            .error_for_status()
            .unwrap();
        bid_requests.push(body);
    }
    let before = fixture.get(None, &detail_path).await;
    assert!(
        before["reserve"].is_null()
            && before["your_bid"].is_null()
            && before["agreed_price"].is_null()
    );
    let forbidden = fixture
        .post(
            0,
            &format!("/v1/assets/{asset}/transfers"),
            json!({"request_id":Uuid::new_v4(),"recipient_address":fixture.accounts[1]}),
        )
        .await;
    assert_eq!(forbidden.status(), StatusCode::CONFLICT);

    let mut approved = HashSet::new();
    let mut budgets = HashMap::new();
    let final_state = tokio::time::timeout(Duration::from_secs(1200), async {
        loop {
            let auction = fixture.get(Some(0), &detail_path).await;
            assert_ne!(auction["status"], "interrupted", "{auction}");
            assert_ne!(
                auction["status"], "no_sale",
                "equal-to-reserve qualifying bids must sell"
            );
            if auction["status"] == "awaiting_completion" {
                break auction;
            }

            for owner in 0..3 {
                let snapshot = fixture.get(Some(owner), "/v1/messages").await;
                for request in snapshot["messages"].as_array().unwrap() {
                    if request["status"] != "ready" {
                        continue;
                    }
                    assert_eq!(request["auction_id"], auction["id"]);
                    let id = request["id"].as_str().unwrap();
                    if !approved.insert(id.to_owned()) {
                        continue;
                    }

                    let remaining: i64 = sqlx::query_scalar(
                        "SELECT remaining_ms FROM auction_participants
                         WHERE auction_id=$1 AND account_id=$2",
                    )
                    .bind(auction["id"].as_str().unwrap().parse::<Uuid>().unwrap())
                    .bind(fixture.accounts[owner])
                    .fetch_one(&fixture.state.pool)
                    .await
                    .unwrap();
                    if let Some(previous) = budgets.insert(owner, remaining) {
                        assert!(remaining <= previous, "a new comparison reset time");
                    }

                    let path = format!("/v1/messages/{id}/approve");
                    let body =
                        json!({"request_id":request["request_id"],"revision":request["revision"]});
                    let receipt: Value = fixture
                        .post(owner, &path, body.clone())
                        .await
                        .error_for_status()
                        .unwrap()
                        .json()
                        .await
                        .unwrap();
                    let retry: Value = fixture
                        .post(owner, &path, body)
                        .await
                        .error_for_status()
                        .unwrap()
                        .json()
                        .await
                        .unwrap();
                    assert_eq!(receipt, retry);
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the complete native auction must reach seller acknowledgement");
    assert_eq!(approved.len(), 21);
    assert_eq!(
        final_state["winner"]["address"],
        fixture.accounts[1].to_string()
    );
    assert_eq!(final_state["agreed_price"], "0.00000125");
    assert_eq!(final_state["checks_completed"], 3);
    assert_eq!(final_state["payment_status"], "unverified");
    assert!(fixture.get(Some(2), &detail_path).await["agreed_price"].is_null());
    let public_response = fixture
        .client
        .get(format!("{}{detail_path}", fixture.url))
        .send()
        .await
        .unwrap();
    assert_eq!(public_response.status(), StatusCode::NOT_FOUND);

    let seller = fixture.get(Some(0), "/v1/messages").await;
    assert_eq!(seller["sales"].as_array().unwrap().len(), 1);
    let request = &seller["sales"][0];
    let completion = json!({"request_id":request["request_id"],"generation":request["generation"]});
    assert_eq!(
        fixture
            .post(1, &format!("{detail_path}/complete"), completion.clone())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let receipt: Value = fixture
        .post(0, &format!("{detail_path}/complete"), completion.clone())
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let retry: Value = fixture
        .post(0, &format!("{detail_path}/complete"), completion)
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(receipt, retry);
    let owner: Uuid = sqlx::query_scalar("SELECT owner_id FROM assets WHERE id=$1")
        .bind(asset)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(owner, fixture.accounts[1]);
    let transfers: i64 = sqlx::query_scalar("SELECT count(*) FROM transfers WHERE auction_id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(transfers, 1);
    assert_eq!(
        fixture.get(Some(0), &detail_path).await["status"],
        "completed"
    );
    assert_eq!(
        fixture.get(Some(1), "/v1/me/transfers").await[0]["auction_id"],
        auction.to_string()
    );
    fixture.close().await;
}

#[actix_web::test]
#[ignore = "requires PostgreSQL and fresh Kafka topics; real HE survivor and no-sale workflows"]
async fn native_withdrawal_fences_old_actions_and_preserves_survivor_inputs() {
    let fixture = Trading::new(3).await;
    let (auction, asset, published) = small_auction(&fixture, "0.00000010").await;
    let detail = format!("/v1/auctions/{auction}");
    for (owner, amount) in [(1, "0.00000042"), (2, "0.00000099")] {
        fixture
            .post(
                owner,
                &format!("{detail}/bids"),
                json!({"request_id":Uuid::new_v4(),
            "amount":amount,"policy_hash":published["policy_hash"]}),
            )
            .await
            .error_for_status()
            .unwrap();
    }
    let old = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            if fixture.get(Some(0), &detail).await["status"] != "matching" {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            let snapshot = fixture.get(Some(2), "/v1/messages").await;
            if let Some(request) = snapshot["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|request| {
                    request["status"] == "ready"
                        && request["timer_running"] == true
                        && request["remaining_ms"].as_i64().unwrap() < 600000
                })
            {
                break request.clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("later buyer must receive the exact first obligation");
    let budget_before = fixture.get(Some(1), &detail).await["participation"]["remaining_ms"]
        .as_i64()
        .unwrap();
    let original: Vec<u8> = sqlx::query_scalar("SELECT input_root FROM auctions WHERE id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    fixture
        .post(2, &format!("{detail}/withdraw"), json!({}))
        .await
        .error_for_status()
        .unwrap();
    let stale = fixture
        .post(
            2,
            &format!("/v1/messages/{}/approve", old["id"].as_str().unwrap()),
            json!({"request_id":old["request_id"],"revision":old["revision"]}),
        )
        .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let survivor = fixture.get(Some(1), &detail).await;
    assert_eq!(survivor["generation"], 2);
    assert_eq!(survivor["your_bid"], "0.00000042");
    assert!(survivor["participation"]["remaining_ms"].as_i64().unwrap() <= budget_before);
    approve_until(&fixture, auction, "awaiting_completion").await;
    let awarded = fixture.get(Some(0), &detail).await;
    assert_eq!(
        awarded["winner"]["address"],
        fixture.accounts[1].to_string()
    );
    assert_eq!(awarded["agreed_price"], "0.00000042");
    let retained: Vec<u8> = sqlx::query_scalar("SELECT input_root FROM auctions WHERE id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(original, retained);

    // An unpaid winner may still withdraw. Its old seller-completion request cannot deliver.
    fixture
        .post(1, &format!("{detail}/withdraw"), json!({}))
        .await
        .error_for_status()
        .unwrap();
    assert_eq!(fixture.get(Some(0), &detail).await["status"], "no_sale");
    let late = fixture
        .post(
            0,
            &format!("{detail}/complete"),
            json!({
        "request_id":awarded["completion_request_id"],"generation":awarded["generation"]}),
        )
        .await;
    assert_eq!(late.status(), StatusCode::CONFLICT);
    let owned = fixture.get(Some(0), &format!("/v1/assets/{asset}")).await;
    assert_eq!(owned["owner_id"], fixture.accounts[0].to_string());
    assert!(owned["active_auction_id"].is_null());

    let (below, _, published) = small_auction(&fixture, "0.00000050").await;
    fixture
        .post(
            1,
            &format!("/v1/auctions/{below}/bids"),
            json!({"request_id":Uuid::new_v4(),
        "amount":"0.00000042","policy_hash":published["policy_hash"]}),
        )
        .await
        .error_for_status()
        .unwrap();
    approve_until(&fixture, below, "no_sale").await;
    assert_eq!(
        fixture.get(Some(0), &format!("/v1/auctions/{below}")).await["reason"],
        "reserve_not_met"
    );

    let (empty, empty_asset, _) = small_auction(&fixture, "0").await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if fixture.get(Some(0), &format!("/v1/auctions/{empty}")).await["status"] == "no_sale" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("an auction with no bids must end without human actions");
    assert!(
        fixture
            .get(Some(0), &format!("/v1/assets/{empty_asset}"))
            .await["active_auction_id"]
            .is_null()
    );
    fixture.close().await;
}

async fn small_auction(fixture: &Trading, reserve: &str) -> (Uuid, Uuid, Value) {
    let asset = Uuid::new_v4();
    let auction = Uuid::new_v4();
    fixture
        .post(
            0,
            "/v1/assets",
            json!({"request_id":asset,"name":"Recovery test asset"}),
        )
        .await
        .error_for_status()
        .unwrap();
    let close = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + 3_600_000
        + 15000;
    let draft: Value = fixture
        .post(
            0,
            "/v1/auctions",
            json!({"request_id":auction,"asset_id":asset,
        "reserve":reserve,"closes_at_ms":close}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let published: Value = fixture
        .post(
            0,
            &format!("/v1/auctions/{auction}/publish"),
            json!({"revision":draft["revision"],"consent":true}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    (auction, asset, published)
}

async fn approve_until(fixture: &Trading, auction: Uuid, target: &str) {
    let mut budgets = HashMap::new();
    tokio::time::timeout(Duration::from_secs(600), async {
        loop {
            let status = fixture.get(Some(0), &format!("/v1/auctions/{auction}")).await;
            assert_ne!(status["status"], "interrupted", "{status}");
            if status["status"] == target {
                break;
            }
            assert_ne!(status["status"], "no_sale", "unexpected no-sale while awaiting {target}");

            for owner in 0..fixture.accounts.len() {
                let snapshot = fixture.get(Some(owner), "/v1/messages").await;
                for request in snapshot["messages"].as_array().unwrap() {
                    if request["auction_id"] != auction.to_string() || request["status"] != "ready" {
                        continue;
                    }
                    let remaining: i64 = sqlx::query_scalar(
                        "SELECT remaining_ms FROM auction_participants
                         WHERE auction_id=$1 AND account_id=$2",
                    )
                    .bind(auction)
                    .bind(fixture.accounts[owner])
                    .fetch_one(&fixture.state.pool)
                    .await
                    .unwrap();
                    if let Some(previous) = budgets.insert(owner, remaining) {
                        assert!(remaining <= previous);
                    }

                    fixture
                        .post(
                            owner,
                            &format!("/v1/messages/{}/approve", request["id"].as_str().unwrap()),
                            json!({"request_id":request["request_id"],"revision":request["revision"]}),
                        )
                        .await
                        .error_for_status()
                        .unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the native recovery/no-sale workflow did not finish");
}
