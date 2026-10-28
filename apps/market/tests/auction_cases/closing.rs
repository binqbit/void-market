use crate::support::Fixture;
use actix_web::http::StatusCode;
#[cfg(feature = "demo-mode")]
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

use super::published;
#[cfg(feature = "demo-mode")]
use super::{admit, now, recovery::ready_obligation};
#[cfg(feature = "demo-mode")]
use actix_web::{App, test};
#[cfg(feature = "demo-mode")]
use arbiter_protocol::{NewJob, RequestSignature, SigningKey, sign};
#[cfg(feature = "demo-mode")]
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use market::chains::{Localnet, config::LocalnetConfig};
use std::sync::Arc;

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn acceleration_is_unavailable_without_a_demo_runtime() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let response = fixture
        .request(
            "POST",
            &format!("/v1/auctions/{}/close-bidding", Uuid::new_v4()),
            Some(&seller.cookie),
            json!({}),
        )
        .await;
    assert_eq!(response.0, StatusCode::NOT_FOUND);
    fixture.cleanup().await;
}

#[cfg(not(feature = "demo-mode"))]
#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn ordinary_build_has_no_close_route_even_with_localnet_enabled() {
    let mut fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    let endpoint = url::Url::parse("http://127.0.0.1:1").unwrap();
    fixture.state.chains = Some(Arc::new(
        Localnet::new(&LocalnetConfig {
            ethereum: endpoint.clone(),
            zcash_node: endpoint.clone(),
            zcash_wallet: endpoint,
            wallet_cookie: "unused-closure-test-cookie".into(),
        })
        .unwrap(),
    ));
    sqlx::query("UPDATE auctions SET execution='localnet' WHERE id=$1")
        .bind(auction)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let detail = format!("/v1/auctions/{auction}");
    let view = fixture
        .request("GET", &detail, Some(&seller.cookie), json!(null))
        .await;
    assert_eq!(view.2["actions"]["close_bidding"], false);
    let response = fixture
        .request(
            "POST",
            &format!("{detail}/close-bidding"),
            Some(&seller.cookie),
            json!({}),
        )
        .await;
    assert_eq!(response.0, StatusCode::NOT_FOUND);
    fixture.cleanup().await;
}

#[cfg(feature = "demo-mode")]
#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn seller_closes_demo_bidding_once_without_replacing_inputs_or_approvals() {
    let mut fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let late_buyer = fixture.register("Late buyer").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    admit(&fixture, auction, &buyer, 200_000_000).await;
    let (session, action) = ready_obligation(&fixture, auction, &buyer, &seller).await;

    let node: Uuid = sqlx::query_scalar("SELECT node_id FROM auctions WHERE id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    let input = fixture
        .state
        .custody
        .as_ref()
        .unwrap()
        .artifacts
        .write_json(&true)
        .unwrap();
    let job = Uuid::new_v4();
    market::nodes::enqueue(
        &fixture.state.pool,
        NewJob {
            id: job,
            node_id: node,
            input: input.clone(),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE approval_sessions SET job_id=$2 WHERE id=$1")
        .bind(session)
        .bind(job)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE approval_sessions s SET check_context=jsonb_build_object('manifest',jsonb_build_object('release_at_ms',a.closes_at_ms-3600000)) FROM auctions a WHERE s.id=$1 AND a.id=s.auction_id")
        .bind(session).execute(&fixture.state.pool).await.unwrap();
    sqlx::query("INSERT INTO auction_checks(session_id,auction_id,generation,slot,kind,left_account,right_account,precomputed) VALUES($1,$2,1,0,'reserve',$3,$4,TRUE)")
        .bind(session).bind(auction).bind(buyer.address).bind(seller.address)
        .execute(&fixture.state.pool).await.unwrap();

    let endpoint = url::Url::parse("http://127.0.0.1:1").unwrap();
    fixture.state.chains = Some(Arc::new(
        Localnet::new(&LocalnetConfig {
            ethereum: endpoint.clone(),
            zcash_node: endpoint.clone(),
            zcash_wallet: endpoint,
            wallet_cookie: "unused-closure-test-cookie".into(),
        })
        .unwrap(),
    ));
    let path = format!("/v1/auctions/{auction}/close-bidding");
    assert!(node_job(&fixture, node, job).await["demo_release"].is_null());
    // A demo-enabled build still cannot accelerate ordinary internal auctions.
    assert_eq!(
        fixture
            .request("POST", &path, Some(&seller.cookie), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE auctions SET execution='localnet',custody_status='held' WHERE id=$1")
        .bind(auction)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let original: (i64, Vec<u8>) =
        sqlx::query_as("SELECT closes_at_ms,policy_hash FROM auctions WHERE id=$1")
            .bind(auction)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    let frozen: Vec<Value> = sqlx::query_scalar(
        "SELECT frozen_input FROM auction_participants WHERE auction_id=$1 ORDER BY account_id",
    )
    .bind(auction)
    .fetch_all(&fixture.state.pool)
    .await
    .unwrap();

    assert_eq!(
        fixture
            .request("POST", &path, Some(&buyer.cookie), json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let first = fixture
        .request("POST", &path, Some(&seller.cookie), json!({}))
        .await;
    assert_eq!(first.0, StatusCode::OK, "{}", first.2);
    assert_eq!(first.2["status"], "matching");
    assert!(first.2["closes_at_ms"].as_i64().unwrap() < original.0);
    assert_eq!(
        first.2["closes_at_ms"].as_i64().unwrap()
            - first.2["bidding_closes_at_ms"].as_i64().unwrap(),
        3_600_000
    );
    assert_eq!(first.2["required_from_ms"], first.2["bidding_closes_at_ms"]);
    assert_eq!(first.2["actions"]["close_bidding"], false);
    let retry = fixture
        .request("POST", &path, Some(&seller.cookie), json!({}))
        .await;
    assert_eq!(retry.0, StatusCode::OK);
    assert_eq!(retry.2["closes_at_ms"], first.2["closes_at_ms"]);
    let after: (i64, Vec<u8>) =
        sqlx::query_as("SELECT closes_at_ms,policy_hash FROM auctions WHERE id=$1")
            .bind(auction)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(after.0, first.2["closes_at_ms"].as_i64().unwrap());
    assert_eq!(original.1, after.1);
    let retained: Vec<Value> = sqlx::query_scalar(
        "SELECT frozen_input FROM auction_participants WHERE auction_id=$1 ORDER BY account_id",
    )
    .bind(auction)
    .fetch_all(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(retained, frozen);
    let message_status: String =
        sqlx::query_scalar("SELECT status FROM approval_messages WHERE id=$1")
            .bind(action)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(message_status, "ready");
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE kind='auction.bidding_closed_early'")
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(events, 1);
    let released = node_job(&fixture, node, job).await;
    assert_eq!(released["demo_release"]["input"], json!(input));
    assert_eq!(
        released["demo_release"]["closed_at_ms"],
        first.2["bidding_closes_at_ms"]
    );

    // Admission remains closed even before the worker advances another step.
    sqlx::query("INSERT INTO demo_accounts(account_id,alias,ethereum_address) VALUES($1,'bob','0x1111111111111111111111111111111111111111')")
        .bind(late_buyer.address).execute(&fixture.state.pool).await.unwrap();
    let late = fixture
        .request(
            "POST",
            &format!("/v1/auctions/{auction}/bids"),
            Some(&late_buyer.cookie),
            json!({"request_id":Uuid::new_v4(),"amount":"3","policy_hash":first.2["policy_hash"]}),
        )
        .await;
    assert_eq!(late.0, StatusCode::CONFLICT);

    // The public snapshot advertises the new deadline without spending time on service waits.
    let snapshot = fixture
        .request("GET", "/v1/messages", Some(&buyer.cookie), Value::Null)
        .await
        .2;
    assert_eq!(
        snapshot["messages"][0]["required_from_ms"],
        first.2["bidding_closes_at_ms"]
    );
    assert_eq!(snapshot["messages"][0]["remaining_ms"], 600000);
    assert!(!snapshot["messages"][0]["timer_running"].as_bool().unwrap());
    assert!(first.2["bidding_closes_at_ms"].as_i64().unwrap() <= now());
    fixture.cleanup().await;
}

#[cfg(feature = "demo-mode")]
async fn node_job(fixture: &Fixture, node: Uuid, job: Uuid) -> Value {
    let deployment: Uuid =
        sqlx::query_scalar("SELECT deployment_id FROM arbiter_nodes WHERE id=$1")
            .bind(node)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    let instance = Uuid::new_v4();
    sqlx::query("UPDATE arbiter_nodes SET instance_id=$2 WHERE id=$1")
        .bind(node)
        .bind(instance)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let path = format!("/node/v1/jobs/{job}");
    let time = now();
    let mut signature = RequestSignature {
        deployment_id: deployment,
        node_id: node,
        instance_id: instance,
        request_id: Uuid::new_v4(),
        issued_at_ms: time,
        expires_at_ms: time + 60000,
        signature: String::new(),
    };
    sign(
        &SigningKey::from_bytes(&[13; 32]),
        &mut signature,
        "GET",
        &path,
        &[],
    );
    let app = test::init_service(
        App::new().configure(market::http::router::configure(fixture.state.clone())),
    )
    .await;
    let request = test::TestRequest::get()
        .uri(&path)
        .insert_header((
            "X-Node-Signature",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&signature).unwrap()),
        ))
        .insert_header((
            "Authorization",
            "Bearer fixture-only-auction-node-token-0123456789",
        ))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    test::read_body_json(response).await
}
