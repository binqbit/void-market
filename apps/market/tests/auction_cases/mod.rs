mod recovery;

use crate::support::{Fixture, Identity};
use actix_web::http::StatusCode;
use arbiter_protocol::{NodeProvision, SigningKey, public_key};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn draft(fixture: &Fixture, seller: &Identity, asset: Uuid) -> Value {
    let (status, _, body) = fixture
        .request(
            "POST",
            "/v1/auctions",
            Some(&seller.cookie),
            json!({
                "request_id": Uuid::new_v4(),
                "asset_id": asset,
                "reserve": "1.25000001",
                "closes_at_ms": now() + 60000,
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

// These helpers isolate metadata/policy behavior. Native publish, admission and HE are
// exercised separately by the arbiter's auction_sales integration test.
async fn published(fixture: &Fixture, seller: &Identity, asset: Uuid) -> (Uuid, Value) {
    let draft = draft(fixture, seller, asset).await;
    let id: Uuid = serde_json::from_value(draft["id"].clone()).unwrap();
    let node = Uuid::new_v4();
    let token = fixture
        .state
        .config
        .media_root
        .join(format!("{node}.token"));
    std::fs::write(&token, "fixture-only-auction-node-token-0123456789").unwrap();
    nodes(fixture, node, &token).await;
    let frozen = frozen(fixture, seller.address);
    sqlx::query(
        "UPDATE auctions SET status='open',node_id=$2,policy_hash=$3,published_at_ms=$4 WHERE id=$1",
    )
    .bind(id)
    .bind(node)
    .bind(vec![7u8; 32])
    .bind(now())
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET frozen_input=$3 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(id)
    .bind(seller.address)
    .bind(&frozen)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    (id, draft)
}

async fn nodes(fixture: &Fixture, node: Uuid, token: &std::path::Path) {
    market::nodes::provision(
        &fixture.state.pool,
        NodeProvision {
            ethereum_address: None,
            node_id: node,
            deployment_id: Uuid::new_v4(),
            name: "Metadata fixture".into(),
            public_key: public_key(&SigningKey::from_bytes(&[13; 32])),
            token_file: token.to_string_lossy().into(),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE arbiter_nodes SET heartbeat_until=now()+interval '1 hour' WHERE id=$1")
        .bind(node)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
}

fn frozen(fixture: &Fixture, owner: Uuid) -> Value {
    let reference = fixture
        .state
        .custody
        .as_ref()
        .unwrap()
        .artifacts
        .write_json(&true)
        .unwrap();
    json!({"owner":owner,"ciphertext":reference,"material":{
        "configuration":vec![0u8;32],"public_fingerprint":vec![0u8;32],"evaluation_fingerprint":vec![0u8;32],
        "public_key":reference,"evaluation_key":reference,
    }})
}

async fn admit(fixture: &Fixture, auction: Uuid, buyer: &Identity, amount: u64) -> Uuid {
    let id = Uuid::new_v4();
    let sealed = fixture
        .state
        .custody
        .as_ref()
        .unwrap()
        .vault
        .seal(
            buyer.address,
            &format!("auction-input:{auction}"),
            &amount.to_le_bytes(),
        )
        .unwrap();
    sqlx::query(
        "INSERT INTO auction_participants(auction_id,account_id,id,input_sealed,frozen_input,created_at_ms)
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(auction)
    .bind(buyer.address)
    .bind(id)
    .bind(sealed)
    .bind(frozen(fixture, buyer.address))
    .bind(now())
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    id
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn draft_amounts_remain_private_and_creation_retries_are_exact() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let other = fixture.register("Other").await;
    let asset = fixture.asset(&seller).await;
    let body = json!({"request_id":Uuid::new_v4(),"asset_id":asset,"reserve":"90071992.54740993","closes_at_ms":now()+60000});
    let (status, _, draft) = fixture
        .request("POST", "/v1/auctions", Some(&seller.cookie), body.clone())
        .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    assert_eq!(draft["reserve"], "90071992.54740993");
    let retry = fixture
        .request("POST", "/v1/auctions", Some(&seller.cookie), body.clone())
        .await;
    assert_eq!(retry.2["id"], draft["id"]);
    let path = format!("/v1/auctions/{}", draft["id"].as_str().unwrap());
    assert_eq!(
        fixture.request("GET", &path, None, Value::Null).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request("GET", &path, Some(&other.cookie), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let mut conflict = body.clone();
    conflict["reserve"] = json!("1");
    assert_eq!(
        fixture
            .request("POST", "/v1/auctions", Some(&seller.cookie), conflict)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &format!("{path}/publish"),
                Some(&seller.cookie),
                json!({"revision":draft["revision"],"consent":false})
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn active_auctions_lock_assets_and_published_policy_is_immutable() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let asset = fixture.asset(&seller).await;
    let (auction, view) = published(&fixture, &seller, asset).await;
    let path = format!("/v1/auctions/{auction}");
    assert_eq!(
        fixture
            .request(
                "POST",
                &format!("/v1/assets/{asset}/transfers"),
                Some(&seller.cookie),
                json!({"request_id":Uuid::new_v4(),"recipient_address":buyer.address})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &format!("{path}/cancel"),
                Some(&seller.cookie),
                json!({})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &format!("{path}/draft"),
                Some(&seller.cookie),
                json!({"revision":view["revision"],"reserve":"9","closes_at_ms":now()+50000})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    let self_bid = fixture
        .request(
            "POST",
            &format!("{path}/bids"),
            Some(&seller.cookie),
            json!({
                "request_id": Uuid::new_v4(),
                "amount": "2",
                "policy_hash": URL_SAFE_NO_PAD.encode([7; 32]),
            }),
        )
        .await;
    assert_eq!(self_bid.0, StatusCode::FORBIDDEN);
    assert_eq!(
        fixture
            .request(
                "POST",
                &format!("{path}/complete"),
                Some(&seller.cookie),
                json!({"request_id":Uuid::new_v4(),"generation":1})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    let public = fixture.request("GET", &path, None, Value::Null).await.2;
    assert!(
        public["reserve"].is_null()
            && public["your_bid"].is_null()
            && public["agreed_price"].is_null()
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn withdrawing_retains_lifetime_capacity_frozen_inputs_and_budgets() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let first = fixture.register("First").await;
    let second = fixture.register("Second").await;
    let third = fixture.register("Third").await;
    let fourth = fixture.register("Fourth").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    let first_id = admit(&fixture, auction, &first, 100_000_001).await;
    admit(&fixture, auction, &second, 200_000_000).await;
    admit(&fixture, auction, &third, 300_000_000).await;
    sqlx::query(
        "UPDATE auctions SET status='matching',generation=1,input_root=$2,round_root=$3 WHERE id=$1",
    )
    .bind(auction)
    .bind(vec![8u8; 32])
    .bind(vec![9u8; 32])
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET remaining_ms=123456 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auction)
    .bind(second.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let path = format!("/v1/auctions/{auction}");
    let result = fixture
        .request(
            "POST",
            &format!("{path}/withdraw"),
            Some(&first.cookie),
            json!({}),
        )
        .await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.2);
    assert_eq!(
        fixture
            .request(
                "POST",
                &format!("{path}/withdraw"),
                Some(&first.cookie),
                json!({})
            )
            .await
            .2,
        result.2
    );
    let view = fixture
        .request("GET", &path, Some(&second.cookie), Value::Null)
        .await
        .2;
    assert_eq!(view["generation"], 2);
    assert_eq!(view["admitted_buyers"], 3);
    assert_eq!(view["active_buyers"], 2);
    assert_eq!(view["participation"]["remaining_ms"], 123456);
    let root: Vec<u8> = sqlx::query_scalar("SELECT input_root FROM auctions WHERE id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(root, vec![8; 32]);
    let own = fixture
        .request("GET", &path, Some(&first.cookie), Value::Null)
        .await
        .2;
    assert_eq!(own["your_bid"], "1.00000001");
    assert_eq!(own["participation"]["status"], "withdrawn");
    let retry = fixture
        .request(
            "POST",
            &format!("{path}/bids"),
            Some(&first.cookie),
            json!({
                "request_id": first_id,
                "amount": "1.00000001",
                "policy_hash": URL_SAFE_NO_PAD.encode([7; 32]),
            }),
        )
        .await;
    assert_eq!(retry.0, StatusCode::ACCEPTED);
    sqlx::query("UPDATE auctions SET status='open' WHERE id=$1")
        .bind(auction)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let additional_bid = fixture
        .request(
            "POST",
            &format!("{path}/bids"),
            Some(&fourth.cookie),
            json!({
                "request_id": Uuid::new_v4(),
                "amount": "4",
                "policy_hash": URL_SAFE_NO_PAD.encode([7; 32]),
            }),
        )
        .await;
    assert_eq!(additional_bid.0, StatusCode::CONFLICT);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn seller_ack_transfers_once_and_races_safely_with_withdrawal() {
    for attempt in 0..6 {
        let concurrent = attempt > 0;
        let fixture = Fixture::with_custody().await;
        let seller = fixture.register("Seller").await;
        let winner = fixture.register("Winner").await;
        let loser = fixture.register("Other buyer").await;
        let asset = fixture.asset(&seller).await;
        let (auction, _) = published(&fixture, &seller, asset).await;
        admit(&fixture, auction, &winner, 125_000_001).await;
        admit(&fixture, auction, &loser, 100_000_000).await;
        let request = Uuid::new_v4();
        sqlx::query(
            "UPDATE auctions SET status='awaiting_completion',generation=1,winner_id=$2,
            completion_request_id=$3,award_hash=$4,input_root=$4,round_root=$4 WHERE id=$1",
        )
        .bind(auction)
        .bind(winner.address)
        .bind(request)
        .bind(vec![8u8; 32])
        .execute(&fixture.state.pool)
        .await
        .unwrap();
        let path = format!("/v1/auctions/{auction}/complete");
        let body = json!({"request_id":request,"generation":1});
        assert_eq!(
            fixture
                .request("POST", &path, Some(&winner.cookie), body.clone())
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let outsider = fixture
            .request(
                "GET",
                &format!("/v1/auctions/{auction}"),
                Some(&loser.cookie),
                Value::Null,
            )
            .await
            .2;
        assert!(outsider["agreed_price"].is_null());
        if concurrent {
            let leave = format!("/v1/auctions/{auction}/withdraw");
            let (complete, withdraw) = tokio::join!(
                fixture.request("POST", &path, Some(&seller.cookie), body.clone()),
                fixture.request("POST", &leave, Some(&winner.cookie), json!({}))
            );
            assert!(
                matches!(complete.0, StatusCode::OK | StatusCode::CONFLICT),
                "completion: {:?} {}",
                complete.0,
                complete.2
            );
            assert!(
                matches!(withdraw.0, StatusCode::OK | StatusCode::CONFLICT),
                "withdrawal: {:?} {}",
                withdraw.0,
                withdraw.2
            );
            assert_ne!(
                complete.0, withdraw.0,
                "completion and withdrawal cannot both succeed"
            );
        } else {
            let completed = fixture
                .request("POST", &path, Some(&seller.cookie), body.clone())
                .await;
            assert_eq!(completed.0, StatusCode::OK, "{}", completed.2);
            assert_eq!(completed.2["payment_status"], "unverified");
            assert_eq!(
                fixture
                    .request("POST", &path, Some(&seller.cookie), body)
                    .await
                    .2,
                completed.2
            );
            let history = fixture
                .request("GET", "/v1/me/transfers", Some(&winner.cookie), Value::Null)
                .await
                .2;
            assert_eq!(history.as_array().unwrap().len(), 1);
            assert_eq!(history[0]["auction_id"], auction.to_string());
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM transfers WHERE auction_id=$1")
            .bind(auction)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        assert!(count <= 1);
        fixture.cleanup().await;
    }
}
