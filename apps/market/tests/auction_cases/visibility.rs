use super::{admit, now, published};
use crate::support::Fixture;
use actix_web::http::StatusCode;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use market::auth::tokens;
use serde_json::{Value, json};

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn invalid_sessions_can_browse_public_auctions_but_not_private_data() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    let path = format!("/v1/auctions/{auction}");

    let seller_view = fixture
        .request("GET", &path, Some(&seller.cookie), Value::Null)
        .await;
    assert_eq!(seller_view.0, StatusCode::OK);
    assert_eq!(seller_view.2["reserve"], "1.25000001");

    let secret = &fixture.state.config.jwt_secret;
    let token = tokens::issue(seller.address, secret).unwrap();
    let mut claims = tokens::verify(&token, secret).unwrap();
    claims.iat = 1;
    claims.exp = 2;
    let expired = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .unwrap();

    sqlx::query("DELETE FROM sessions WHERE account_id = $1")
        .bind(seller.address)
        .execute(&fixture.state.pool)
        .await
        .unwrap();

    let cookies = [
        "market_session=invalid".to_owned(),
        "market_session=".to_owned(),
        format!("market_session={}", "x".repeat(4097)),
        format!("market_session={expired}"),
        seller.cookie.clone(),
    ];
    for cookie in &cookies {
        let catalog = fixture
            .request("GET", "/v1/auctions", Some(cookie), Value::Null)
            .await;
        assert_eq!(catalog.0, StatusCode::OK, "{:?}", catalog.2);
        assert_eq!(catalog.2.as_array().unwrap().len(), 1);
        assert_eq!(catalog.2[0]["id"], json!(auction));
        assert!(catalog.2[0]["reserve"].is_null());

        let view = fixture
            .request("GET", &path, Some(cookie), Value::Null)
            .await;
        assert_eq!(view.0, StatusCode::OK);
        assert!(view.2["reserve"].is_null());
        assert!(view.2["your_bid"].is_null());
        assert!(view.2["participation"].is_null());
        assert_eq!(view.2["actions"]["edit"], false);

        for private_path in ["/v1/me", "/v1/me/assets", "/v1/me/auctions", "/v1/me/bids"] {
            assert_eq!(
                fixture
                    .request("GET", private_path, Some(cookie), Value::Null)
                    .await
                    .0,
                StatusCode::UNAUTHORIZED,
                "{private_path} must require a valid session"
            );
        }
    }

    sqlx::query("UPDATE auctions SET closes_at_ms = $2 WHERE id = $1")
        .bind(auction)
        .bind(now() + 3_600_000)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    for cookie in &cookies {
        assert_eq!(
            fixture
                .request("GET", &path, Some(cookie), Value::Null)
                .await
                .0,
            StatusCode::NOT_FOUND,
            "A stale session must not grant access to a closed auction"
        );
    }
    fixture.cleanup().await;
}

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
