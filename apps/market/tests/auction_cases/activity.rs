use super::{admit, published, recovery::ready_obligation};
use crate::support::Fixture;
use actix_web::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn bidding_activity_is_private_paginated_and_excludes_selling() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let other = fixture.register("Other").await;
    assert_eq!(
        fixture
            .request("GET", "/v1/me/bids", None, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let empty = fixture
        .request("GET", "/v1/me/bids", Some(&buyer.cookie), Value::Null)
        .await;
    assert_eq!(empty.0, StatusCode::OK);
    assert_eq!(empty.2["stats"]["bids_placed"], 0);
    assert_eq!(empty.2["bids"], json!([]));

    let mut auctions = Vec::new();
    for _ in 0..3 {
        let asset = fixture.asset(&seller).await;
        let (auction, _) = published(&fixture, &seller, asset).await;
        admit(&fixture, auction, &buyer, 125_000_001).await;
        auctions.push(auction);
    }
    admit(&fixture, auctions[0], &other, 999_000_000).await;
    sqlx::query("UPDATE auctions SET status='completed',winner_id=$2 WHERE id=$1")
        .bind(auctions[1])
        .bind(buyer.address)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET status='withdrawn' WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auctions[2])
    .bind(buyer.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let own_asset = fixture.asset(&buyer).await;
    published(&fixture, &buyer, own_asset).await;

    let first = fixture
        .request(
            "GET",
            "/v1/me/bids?limit=1",
            Some(&buyer.cookie),
            Value::Null,
        )
        .await;
    assert_eq!(first.0, StatusCode::OK, "{}", first.2);
    assert_eq!(
        first.2["stats"],
        json!({
            "bids_placed":3,"active_bids":1,"completed_purchases":1,"pending_approvals":0
        })
    );
    assert_eq!(first.2["bids"].as_array().unwrap().len(), 1);
    assert_eq!(first.2["bids"][0]["auction_id"], json!(auctions[0]));
    assert_eq!(first.2["bids"][0]["amount"], "1.25000001");
    assert_eq!(first.2["has_more"], true);
    let last = fixture
        .request(
            "GET",
            "/v1/me/bids?limit=2&offset=1",
            Some(&buyer.cookie),
            Value::Null,
        )
        .await;
    assert_eq!(last.2["stats"], first.2["stats"]);
    assert_eq!(last.2["bids"].as_array().unwrap().len(), 2);
    assert_eq!(last.2["has_more"], false);

    let seller_view = fixture
        .request("GET", "/v1/me/bids", Some(&seller.cookie), Value::Null)
        .await;
    assert_eq!(seller_view.2["stats"]["bids_placed"], 0);
    let other_view = fixture
        .request("GET", "/v1/me/bids", Some(&other.cookie), Value::Null)
        .await;
    assert_eq!(other_view.2["stats"]["bids_placed"], 1);
    assert_eq!(other_view.2["bids"][0]["amount"], "9.99000000");
    assert!(!other_view.2.to_string().contains("1.25000001"));
    assert_eq!(
        fixture
            .request(
                "GET",
                "/v1/me/bids?limit=0",
                Some(&buyer.cookie),
                Value::Null
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn bidding_progress_counts_received_requests_and_current_obligations() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let mut auctions = Vec::new();
    for _ in 0..2 {
        let asset = fixture.asset(&seller).await;
        let (auction, _) = published(&fixture, &seller, asset).await;
        admit(&fixture, auction, &buyer, 200_000_000).await;
        auctions.push(auction);
    }
    let (_, ready) = ready_obligation(&fixture, auctions[0], &buyer, &seller).await;
    ready_obligation(&fixture, auctions[1], &buyer, &seller).await;
    for status in ["completed", "cancelled"] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO approval_messages(id,session_id,account_id,request_id,request_ref,generation,kind,status,approval)
            SELECT $2,session_id,account_id,$2,request_ref,generation,'answer_reveal',$3,$4 FROM approval_messages WHERE id=$1")
            .bind(ready).bind(id).bind(status).bind(json!({"signature":"fixture-only"}))
            .execute(&fixture.state.pool).await.unwrap();
    }
    for status in ["ready", "confirmed", "failed", "cancelled"] {
        sqlx::query("INSERT INTO chain_operations(id,account_id,auction_id,kind,status,payload_sealed,approval,created_at_ms,updated_at_ms)
            VALUES($1,$2,$3,'payment',$4,$5,$6,1,1)")
            .bind(Uuid::new_v4()).bind(buyer.address).bind(auctions[0]).bind(status)
            .bind(vec![0_u8]).bind(if status=="confirmed" {Some(json!({"revision":1}))} else {None})
            .execute(&fixture.state.pool).await.unwrap();
    }

    let activity = fixture
        .request("GET", "/v1/me/bids", Some(&buyer.cookie), Value::Null)
        .await;
    assert_eq!(activity.0, StatusCode::OK, "{}", activity.2);
    assert_eq!(activity.2["stats"]["pending_approvals"], 3);
    let bid = activity.2["bids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bid| bid["auction_id"] == json!(auctions[0]))
        .unwrap();
    assert_eq!(
        bid["approvals"],
        json!({"requested":4,"approved":2,"pending":2})
    );
    let page = fixture
        .request(
            "GET",
            "/v1/me/bids?limit=1",
            Some(&buyer.cookie),
            Value::Null,
        )
        .await;
    assert_eq!(page.2["stats"]["pending_approvals"], 3);

    sqlx::query("UPDATE auctions SET status='awaiting_completion',winner_id=$2 WHERE id=$1")
        .bind(auctions[0])
        .bind(seller.address)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET remaining_ms=0 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auctions[1])
    .bind(buyer.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let finished = fixture
        .request("GET", "/v1/me/bids", Some(&buyer.cookie), Value::Null)
        .await;
    assert_eq!(finished.2["stats"]["pending_approvals"], 0);
    assert_eq!(finished.2["stats"]["active_bids"], 1);
    fixture.cleanup().await;
}
