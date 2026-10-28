use super::{admit, now, published};
use crate::support::Fixture;
use actix_web::http::StatusCode;
use serde_json::{Value, json};

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn closed_auctions_leave_the_market_but_remain_in_participant_history() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let outsider = fixture.register("Outsider").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    admit(&fixture, auction, &buyer, 200_000_000).await;
    let path = format!("/v1/auctions/{auction}");

    let open = fixture
        .request("GET", "/v1/auctions", None, Value::Null)
        .await;
    assert_eq!(open.2.as_array().unwrap().len(), 1);
    assert_eq!(
        fixture.request("GET", &path, None, Value::Null).await.0,
        StatusCode::OK
    );

    // Admission closes by timestamp even before the worker updates the auction status.
    sqlx::query("UPDATE auctions SET closes_at_ms=$2 WHERE id=$1")
        .bind(auction)
        .bind(now() + 3_600_000)
        .execute(&fixture.state.pool)
        .await
        .unwrap();

    for status in [
        "open",
        "matching",
        "awaiting_completion",
        "completed",
        "no_sale",
        "interrupted",
    ] {
        sqlx::query("UPDATE auctions SET status=$2 WHERE id=$1")
            .bind(auction)
            .bind(status)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        for cookie in [
            None,
            Some(outsider.cookie.as_str()),
            Some(buyer.cookie.as_str()),
            Some(seller.cookie.as_str()),
        ] {
            let catalog = fixture
                .request("GET", "/v1/auctions", cookie, Value::Null)
                .await;
            assert_eq!(catalog.0, StatusCode::OK);
            assert_eq!(catalog.2, json!([]), "{status} must not appear in Market");
        }
        for cookie in [None, Some(outsider.cookie.as_str())] {
            assert_eq!(
                fixture.request("GET", &path, cookie, Value::Null).await.0,
                StatusCode::NOT_FOUND
            );
        }
        for cookie in [Some(buyer.cookie.as_str()), Some(seller.cookie.as_str())] {
            assert_eq!(
                fixture.request("GET", &path, cookie, Value::Null).await.0,
                StatusCode::OK
            );
            let history = fixture
                .request("GET", "/v1/me/auctions", cookie, Value::Null)
                .await;
            assert_eq!(history.2.as_array().unwrap().len(), 1);
            assert_eq!(history.2[0]["id"], json!(auction));
        }
    }
    let bids = fixture
        .request("GET", "/v1/me/bids", Some(&buyer.cookie), Value::Null)
        .await;
    assert_eq!(bids.2["bids"][0]["auction_id"], json!(auction));

    // Leaving an auction does not erase the former participant's private history.
    sqlx::query(
        "UPDATE auction_participants SET status='withdrawn' WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auction)
    .bind(buyer.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        fixture
            .request("GET", &path, Some(&buyer.cookie), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn catalog_filters_unavailable_auctions_before_pagination() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let first = fixture.register("First").await;
    let second = fixture.register("Second").await;
    let third = fixture.register("Third").await;
    let outsider = fixture.register("Outsider").await;
    let mut ids = Vec::new();
    for _ in 0..4 {
        let asset = fixture.asset(&seller).await;
        ids.push(published(&fixture, &seller, asset).await.0);
    }
    for (index, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE auctions SET created_at_ms=$2 WHERE id=$1")
            .bind(id)
            .bind(index as i64 + 1)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE auctions SET status='completed' WHERE id=$1")
        .bind(ids[3])
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    for buyer in [&first, &second, &third] {
        admit(&fixture, ids[2], buyer, 200_000_000).await;
    }
    sqlx::query(
        "UPDATE auction_participants SET status='withdrawn' WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(ids[2])
    .bind(third.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();

    let page = fixture
        .request("GET", "/v1/auctions?limit=1", None, Value::Null)
        .await;
    assert_eq!(page.0, StatusCode::OK);
    assert_eq!(page.2.as_array().unwrap().len(), 1);
    assert_eq!(page.2[0]["id"], json!(ids[1]));
    let next = fixture
        .request("GET", "/v1/auctions?limit=1&offset=1", None, Value::Null)
        .await;
    assert_eq!(next.2[0]["id"], json!(ids[0]));
    let full = format!("/v1/auctions/{}", ids[2]);
    assert_eq!(
        fixture
            .request("GET", &full, Some(&outsider.cookie), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request("GET", &full, Some(&third.cookie), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    fixture.cleanup().await;
}
