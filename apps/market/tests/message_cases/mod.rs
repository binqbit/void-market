use crate::support::{Fixture, Identity};
use actix_web::http::StatusCode;
use arbiter_protocol::{NodeProvision, SigningKey, public_key};
use check_workflow::messages::Predicate;
use market::messages::model::{ParticipantInput, SessionInput};
use market::{messages, nodes};
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};
use tokio::sync::watch;
use uuid::Uuid;

async fn ready(fixture: &Fixture, first: &Identity, second: &Identity) -> (Uuid, Uuid) {
    let node = Uuid::new_v4();
    let key = SigningKey::from_bytes(&[6; 32]);
    let token = fixture.state.config.media_root.join("node.token");
    std::fs::write(&token, "fixture-only-node-token-01234567890123456789").unwrap();
    nodes::provision(
        &fixture.state.pool,
        NodeProvision {
            ethereum_address: None,
            node_id: node,
            deployment_id: Uuid::new_v4(),
            name: "Fixture node".into(),
            public_key: public_key(&key),
            token_file: token.to_string_lossy().into(),
        },
    )
    .await
    .unwrap();
    let session = Uuid::new_v4();
    messages::create_session(
        &fixture.state,
        SessionInput {
            id: session,
            title: "Approval test".into(),
            node_id: node,
            comparison: Predicate::Greater,
            width: 8,
            participants: [
                ParticipantInput {
                    account_id: first.address,
                    value: 42,
                },
                ParticipantInput {
                    account_id: second.address,
                    value: 17,
                },
            ],
        },
    )
    .await
    .unwrap();
    let reference = fixture
        .state
        .custody
        .as_ref()
        .unwrap()
        .artifacts
        .write_json(&true)
        .unwrap();
    sqlx::query(
        "INSERT INTO arbiter_jobs(id,node_id,input,status,generation) VALUES($1,$2,$3,'waiting',1)",
    )
    .bind(session)
    .bind(node)
    .bind(json!(reference))
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE approval_sessions SET status='active',job_id=$1 WHERE id=$1")
        .bind(session)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let message = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO approval_messages(id, session_id, account_id, request_id,
         request_ref, generation, kind) VALUES($1,$2,$3,$4,$5,1,'number_transfer')",
    )
    .bind(message)
    .bind(session)
    .bind(first.address)
    .bind(reference.id)
    .bind(json!(reference))
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    (session, message)
}

async fn current(fixture: &Fixture, user: &Identity) -> Value {
    fixture
        .request("GET", "/v1/messages", Some(&user.cookie), Value::Null)
        .await
        .2["messages"][0]
        .clone()
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn approval_is_account_bound_signed_and_idempotent() {
    let fixture = Fixture::with_custody().await;
    let first = fixture.register("Owner one").await;
    let second = fixture.register("Owner two").await;
    let (_, message) = ready(&fixture, &first, &second).await;
    let view = current(&fixture, &first).await;
    let input = json!({"request_id":view["request_id"],"revision":view["revision"]});
    let path = format!("/v1/messages/{message}/approve");
    assert_eq!(
        fixture
            .request("POST", &path, Some(&second.cookie), input.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &path,
                Some(&first.cookie),
                json!({"request_id":Uuid::new_v4(),"revision":view["revision"]})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    let accepted = fixture
        .request("POST", &path, Some(&first.cookie), input.clone())
        .await;
    assert_eq!(accepted.0, StatusCode::ACCEPTED);
    assert_eq!(accepted.2["mode"], "dev_custodial");
    assert!(accepted.2["signature"].as_str().unwrap().len() > 80);
    let retried = fixture
        .request("POST", &path, Some(&first.cookie), input)
        .await;
    assert_eq!(retried.2, accepted.2);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE kind='message.approved'")
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let status = current(&fixture, &first).await;
    assert_eq!(status["status"], "approved");
    assert_eq!(status["timer_running"], false);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn exhausted_budget_disqualifies_but_service_outage_preserves_time() {
    for healthy in [true, false] {
        let fixture = Fixture::with_custody().await;
        let first = fixture.register("Owner one").await;
        let second = fixture.register("Owner two").await;
        let (session, message) = ready(&fixture, &first, &second).await;
        let epoch = Uuid::new_v4();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        sqlx::query("INSERT INTO approval_runtime(singleton,instance_id,last_tick_ms,healthy) VALUES(TRUE,$1,$2,$3)")
            .bind(epoch).bind(now).bind(healthy).execute(&fixture.state.pool).await.unwrap();
        sqlx::query("UPDATE arbiter_nodes SET heartbeat_until=now()+interval '1 minute'")
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE approval_sessions SET clock_epoch=$2,last_tick_ms=$3 WHERE id=$1")
            .bind(session)
            .bind(epoch)
            .bind(now - 1000)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE approval_members SET remaining_ms=500,chargeable=TRUE WHERE session_id=$1 AND account_id=$2")
            .bind(session).bind(first.address).execute(&fixture.state.pool).await.unwrap();
        let view = current(&fixture, &first).await;
        let result = fixture
            .request(
                "POST",
                &format!("/v1/messages/{message}/approve"),
                Some(&first.cookie),
                json!({"request_id":view["request_id"],"revision":view["revision"]}),
            )
            .await;
        assert_eq!(
            result.0,
            if healthy {
                StatusCode::CONFLICT
            } else {
                StatusCode::ACCEPTED
            }
        );
        let (status, remaining): (String, i64) = sqlx::query_as(
            "SELECT status, remaining_ms FROM approval_members
             WHERE session_id=$1 AND account_id=$2",
        )
        .bind(session)
        .bind(first.address)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
        assert_eq!(status, if healthy { "disqualified" } else { "active" });
        assert_eq!(remaining, if healthy { 0 } else { 500 });
        fixture.cleanup().await;
    }
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn timer_disqualifies_an_inactive_user_without_a_browser_request() {
    let fixture = Fixture::with_custody().await;
    let first = fixture.register("Owner one").await;
    let second = fixture.register("Owner two").await;
    let (session, _) = ready(&fixture, &first, &second).await;
    // This test exercises the deadline worker, without starting expensive HE work.
    sqlx::query("UPDATE custody_keys SET he_status='failed'")
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE arbiter_nodes SET heartbeat_until=now()+interval '1 minute'")
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE approval_members SET remaining_ms=100
         WHERE session_id=$1 AND account_id=$2",
    )
    .bind(session)
    .bind(first.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    fixture.state.worker_ready.store(true, Ordering::Relaxed);

    let (stop, signal) = watch::channel(false);
    let worker = tokio::spawn(messages::run(fixture.state.clone(), signal));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status: String = sqlx::query_scalar(
                "SELECT status FROM approval_members WHERE session_id=$1 AND account_id=$2",
            )
            .bind(session)
            .bind(first.address)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
            if status == "disqualified" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("an offline user's expired budget must be enforced by the server");
    stop.send(true).unwrap();
    worker.await.unwrap().unwrap();

    let other: String = sqlx::query_scalar(
        "SELECT status FROM approval_members WHERE session_id=$1 AND account_id=$2",
    )
    .bind(session)
    .bind(second.address)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(other, "active");
    assert_eq!(
        current(&fixture, &first).await["participant_status"],
        "disqualified"
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn concurrent_approval_and_withdrawal_leave_no_live_action() {
    let fixture = Fixture::with_custody().await;
    let first = fixture.register("Owner one").await;
    let second = fixture.register("Owner two").await;
    let (session, message) = ready(&fixture, &first, &second).await;
    let view = current(&fixture, &first).await;
    let approve_path = format!("/v1/messages/{message}/approve");
    let withdraw_path = format!("/v1/approval-sessions/{session}/withdraw");

    let (approval, withdrawal) = tokio::join!(
        fixture.request(
            "POST",
            &approve_path,
            Some(&first.cookie),
            json!({"request_id":view["request_id"],"revision":view["revision"]})
        ),
        fixture.request("POST", &withdraw_path, Some(&first.cookie), json!({})),
    );
    assert!(matches!(
        approval.0,
        StatusCode::ACCEPTED | StatusCode::CONFLICT
    ));
    assert_eq!(withdrawal.0, StatusCode::OK);
    let final_view = current(&fixture, &first).await;
    assert_eq!(final_view["status"], "cancelled");
    assert_eq!(final_view["participant_status"], "withdrawn");
    assert_eq!(final_view["timer_running"], false);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn withdrawing_cancels_only_this_participation_and_keeps_the_receipt() {
    let fixture = Fixture::with_custody().await;
    let first = fixture.register("Owner one").await;
    let second = fixture.register("Owner two").await;
    let (session, message) = ready(&fixture, &first, &second).await;
    let path = format!("/v1/approval-sessions/{session}/withdraw");
    let accepted = fixture
        .request("POST", &path, Some(&first.cookie), json!({}))
        .await;
    assert_eq!(accepted.0, StatusCode::OK);
    assert_eq!(
        fixture
            .request("POST", &path, Some(&first.cookie), json!({}))
            .await
            .2,
        accepted.2
    );
    let other: String = sqlx::query_scalar(
        "SELECT status FROM approval_members WHERE session_id=$1 AND account_id=$2",
    )
    .bind(session)
    .bind(second.address)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(other, "active");
    let cancelled: bool =
        sqlx::query_scalar("SELECT cancel_requested FROM arbiter_jobs WHERE id=$1")
            .bind(session)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert!(cancelled);
    assert_eq!(current(&fixture, &first).await["status"], "cancelled");
    let _ = message;
    fixture.cleanup().await;
}
