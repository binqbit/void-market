use crate::support::Fixture;
use actix_web::http::StatusCode;
use market::chains::{Localnet, config::LocalnetConfig};
use market::settlement::model::{Payload, PaymentPlan};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn transaction_history_is_paginated_and_relative_to_the_viewer() {
    let mut fixture = Fixture::with_custody().await;
    let sender = fixture.register("Buyer").await;
    let recipient = fixture.register("Seller").await;
    let stranger = fixture.register("Stranger").await;
    // History reads need no RPC calls; these endpoints must never be contacted.
    let endpoint = url::Url::parse("http://127.0.0.1:1").unwrap();
    fixture.state.chains = Some(Arc::new(
        Localnet::new(&LocalnetConfig {
            ethereum: endpoint.clone(),
            zcash_node: endpoint.clone(),
            zcash_wallet: endpoint,
            wallet_cookie: std::path::PathBuf::from("unused-history-test-cookie"),
        })
        .unwrap(),
    ));

    let mut ids = Vec::new();
    for created in 1..=4 {
        let id = Uuid::new_v4();
        let payload = Payload::Zcash(PaymentPlan {
            request_amount: None,
            signed_pczt: false,
            sender_account: sender.address,
            sender_address: "test-sender".into(),
            recipient_account: recipient.address,
            recipient_address: "test-recipient".into(),
            value: 150_000_000,
            memo: "test".into(),
            fee: None,
            expiry_height: None,
        });
        let sealed = fixture
            .state
            .custody
            .as_ref()
            .unwrap()
            .vault
            .seal(
                sender.address,
                &format!("chain-operation:{id}"),
                &serde_json::to_vec(&payload).unwrap(),
            )
            .unwrap();
        sqlx::query("INSERT INTO chain_operations(id,account_id,recipient_id,kind,status,payload_sealed,created_at_ms,updated_at_ms) VALUES($1,$2,$3,'payment','ready',$4,$5,$5)")
            .bind(id).bind(sender.address).bind(recipient.address).bind(sealed).bind(created as i64)
            .execute(&fixture.state.pool).await.unwrap();
        ids.push(id);
    }

    let first = fixture
        .request(
            "GET",
            "/v1/me/transactions?limit=2&offset=0",
            Some(&recipient.cookie),
            Value::Null,
        )
        .await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.2.as_array().unwrap().len(), 2);
    assert_eq!(first.2[0]["id"], json!(ids[3]));
    assert_eq!(first.2[0]["direction"], "incoming");
    assert_eq!(first.2[0]["title"], "Incoming payment");
    assert_eq!(first.2[0]["counterparty"]["name"], "Buyer");
    assert_eq!(first.2[0]["can_approve"], false);
    let second = fixture
        .request(
            "GET",
            "/v1/me/transactions?limit=2&offset=2",
            Some(&recipient.cookie),
            Value::Null,
        )
        .await
        .2;
    assert_eq!(second[0]["id"], json!(ids[1]));
    let outgoing = fixture
        .request(
            "GET",
            &format!("/v1/transactions/{}", ids[3]),
            Some(&sender.cookie),
            Value::Null,
        )
        .await
        .2;
    assert_eq!(outgoing["direction"], "outgoing");
    assert_eq!(outgoing["counterparty"]["name"], "Seller");
    assert_eq!(
        fixture
            .request(
                "GET",
                &format!("/v1/transactions/{}", ids[3]),
                Some(&stranger.cookie),
                Value::Null
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(
                "GET",
                "/v1/me/transactions?limit=101",
                Some(&sender.cookie),
                Value::Null
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    fixture.cleanup().await;
}
