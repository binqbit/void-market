use super::{admit, frozen, now, published};
use crate::support::{Fixture, Identity};
use actix_web::http::StatusCode;
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

// Inject ready obligations to test metering/fencing separately from the native HE journey.
async fn ready_obligation(
    fixture: &Fixture,
    auction: Uuid,
    left: &Identity,
    right: &Identity,
) -> (Uuid, Uuid) {
    let session = Uuid::new_v4();
    let action = Uuid::new_v4();
    let reference = fixture
        .state
        .custody
        .as_ref()
        .unwrap()
        .artifacts
        .write_json(&true)
        .unwrap();
    let node: Uuid = sqlx::query_scalar("SELECT node_id FROM auctions WHERE id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO approval_sessions(id,title,node_id,comparison,width,status,auction_id,round_generation)
         VALUES($1,'Injected obligation',$2,'greater_or_equal',64,'active',$3,1)",
    )
    .bind(session)
    .bind(node)
    .bind(auction)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    for (position, member) in [left, right].iter().enumerate() {
        sqlx::query(
            "INSERT INTO approval_members(session_id,account_id,position,input_ref) VALUES($1,$2,$3,$4)",
        )
        .bind(session)
        .bind(member.address)
        .bind(position as i16)
        .bind(frozen(fixture, member.address))
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO approval_messages(id,session_id,account_id,request_id,request_ref,generation,kind)
         VALUES($1,$2,$3,$4,$5,1,'number_transfer')",
    )
    .bind(action)
    .bind(session)
    .bind(left.address)
    .bind(reference.id)
    .bind(json!(reference))
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    (session, action)
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn overlapping_ready_obligations_charge_one_shared_interval() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    admit(&fixture, auction, &buyer, 200_000_000).await;
    let (_, first) = ready_obligation(&fixture, auction, &buyer, &seller).await;
    let (_, second) = ready_obligation(&fixture, auction, &buyer, &seller).await;
    let epoch = Uuid::new_v4();
    let tick = now();
    sqlx::query(
        "INSERT INTO approval_runtime(singleton,instance_id,last_tick_ms,healthy) VALUES(TRUE,$1,$2,TRUE)",
    )
    .bind(epoch)
    .bind(tick)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auctions SET status='matching',generation=1,input_root=$2,round_root=$2,
         clock_epoch=$3,last_tick_ms=$4 WHERE id=$1",
    )
    .bind(auction)
    .bind(vec![8u8; 32])
    .bind(epoch)
    .bind(tick - 1000)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET chargeable=TRUE WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auction)
    .bind(buyer.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();

    let snapshot = fixture
        .request("GET", "/v1/messages", Some(&buyer.cookie), Value::Null)
        .await
        .2;
    let request = snapshot["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == first.to_string())
        .unwrap();
    let approved = fixture
        .request(
            "POST",
            &format!("/v1/messages/{first}/approve"),
            Some(&buyer.cookie),
            json!({"request_id":request["request_id"],"revision":request["revision"]}),
        )
        .await;
    assert_eq!(approved.0, StatusCode::ACCEPTED, "{}", approved.2);
    let (remaining, charging, metered_until): (i64, bool, i64) = sqlx::query_as(
        "SELECT p.remaining_ms,p.chargeable,a.last_tick_ms FROM auction_participants p
         JOIN auctions a ON a.id=p.auction_id WHERE p.auction_id=$1 AND p.account_id=$2",
    )
    .bind(auction)
    .bind(buyer.address)
    .fetch_one(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(remaining, 600_000 - (metered_until - (tick - 1000)));
    assert!(
        charging,
        "the second ready obligation still uses the same clock"
    );
    let other_status: String =
        sqlx::query_scalar("SELECT status FROM approval_messages WHERE id=$1")
            .bind(second)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(other_status, "ready");
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn interrupted_check_ends_auction_without_disqualifying_participants() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Buyer").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    admit(&fixture, auction, &buyer, 200_000_000).await;
    let (session, action) = ready_obligation(&fixture, auction, &buyer, &seller).await;
    sqlx::query("UPDATE approval_sessions SET status='interrupted' WHERE id=$1")
        .bind(session)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO auction_checks(session_id,auction_id,generation,slot,kind,left_account,right_account,round_root)
         VALUES($1,$2,1,0,'reserve',$3,$4,$5)",
    )
    .bind(session)
    .bind(auction)
    .bind(buyer.address)
    .bind(seller.address)
    .bind(vec![8u8; 32])
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE auction_participants SET remaining_ms=123456 WHERE auction_id=$1")
        .bind(auction)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE auctions SET status='matching',generation=1,input_root=$2,round_root=$2 WHERE id=$1",
    )
    .bind(auction)
    .bind(vec![8u8; 32])
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    // This metadata test must not generate HE keys or execute its injected request.
    sqlx::query("UPDATE custody_keys SET he_status='failed'")
        .execute(&fixture.state.pool)
        .await
        .unwrap();

    let (stop, signal) = tokio::sync::watch::channel(false);
    let worker = tokio::spawn(market::messages::run(fixture.state.clone(), signal));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status: String = sqlx::query_scalar("SELECT status FROM auctions WHERE id=$1")
                .bind(auction)
                .fetch_one(&fixture.state.pool)
                .await
                .unwrap();
            if status == "interrupted" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the worker must propagate interruption without retrying the check");
    stop.send(true).unwrap();
    worker.await.unwrap().unwrap();

    let view = fixture
        .request(
            "GET",
            &format!("/v1/auctions/{auction}"),
            Some(&buyer.cookie),
            Value::Null,
        )
        .await
        .2;
    assert_eq!(view["reason"], "comparison_interrupted");
    assert_eq!(view["participation"]["status"], "active");
    assert_eq!(view["participation"]["remaining_ms"], 123456);
    assert_eq!(view["participation"]["timer_running"], false);
    assert!(!view["actions"]["withdraw"].as_bool().unwrap());
    let locked: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM auctions WHERE asset_id=$1 AND status IN ('open','matching','awaiting_completion')",
    )
    .bind(asset)
    .fetch_optional(&fixture.state.pool)
    .await
    .unwrap();
    assert!(locked.is_none());
    let snapshot = fixture
        .request("GET", "/v1/messages", Some(&buyer.cookie), Value::Null)
        .await
        .2;
    let request = snapshot["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == action.to_string())
        .unwrap();
    let late = fixture
        .request(
            "POST",
            &format!("/v1/messages/{action}/approve"),
            Some(&buyer.cookie),
            json!({"request_id":request["request_id"],"revision":request["revision"]}),
        )
        .await;
    assert_eq!(late.0, StatusCode::CONFLICT);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn auction_expiry_cancels_the_round_without_resetting_other_participants() {
    let fixture = Fixture::with_custody().await;
    let seller = fixture.register("Seller").await;
    let buyer = fixture.register("Expiring buyer").await;
    let other = fixture.register("Remaining buyer").await;
    let asset = fixture.asset(&seller).await;
    let (auction, _) = published(&fixture, &seller, asset).await;
    admit(&fixture, auction, &buyer, 200_000_000).await;
    admit(&fixture, auction, &other, 300_000_000).await;
    let session = Uuid::new_v4();
    let action = Uuid::new_v4();
    let reference = fixture
        .state
        .custody
        .as_ref()
        .unwrap()
        .artifacts
        .write_json(&true)
        .unwrap();
    let node: Uuid = sqlx::query_scalar("SELECT node_id FROM auctions WHERE id=$1")
        .bind(auction)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO approval_sessions(id,title,node_id,comparison,width,status,auction_id,round_generation)
         VALUES($1,'Deadline check',$2,'greater',64,'active',$3,1)",
    )
    .bind(session)
    .bind(node)
    .bind(auction)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO approval_members(session_id,account_id,position,input_ref) VALUES($1,$2,0,$3)",
    )
    .bind(session)
    .bind(buyer.address)
    .bind(frozen(&fixture, buyer.address))
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO approval_members(session_id,account_id,position,input_ref) VALUES($1,$2,1,$3)",
    )
    .bind(session)
    .bind(other.address)
    .bind(frozen(&fixture, other.address))
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO approval_messages(id,session_id,account_id,request_id,request_ref,generation,kind)
         VALUES($1,$2,$3,$4,$5,1,'number_transfer')",
    )
    .bind(action)
    .bind(session)
    .bind(buyer.address)
    .bind(reference.id)
    .bind(json!(reference))
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let epoch = Uuid::new_v4();
    let now = now();
    sqlx::query(
        "INSERT INTO approval_runtime(singleton,instance_id,last_tick_ms,healthy) VALUES(TRUE,$1,$2,TRUE)",
    )
    .bind(epoch)
    .bind(now)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auctions SET status='matching',generation=1,input_root=$2,round_root=$2,
        clock_epoch=$3,last_tick_ms=$4 WHERE id=$1",
    )
    .bind(auction)
    .bind(vec![8u8; 32])
    .bind(epoch)
    .bind(now - 1000)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET remaining_ms=100,chargeable=TRUE WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auction)
    .bind(buyer.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auction_participants SET remaining_ms=123456 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auction)
    .bind(other.address)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let snapshot = fixture
        .request("GET", "/v1/messages", Some(&buyer.cookie), Value::Null)
        .await
        .2;
    let request = &snapshot["messages"][0];
    let late = fixture
        .request(
            "POST",
            &format!("/v1/messages/{action}/approve"),
            Some(&buyer.cookie),
            json!({"request_id":request["request_id"],"revision":request["revision"]}),
        )
        .await;
    assert_eq!(late.0, StatusCode::CONFLICT, "{}", late.2);
    let current = fixture
        .request(
            "GET",
            &format!("/v1/auctions/{auction}"),
            Some(&other.cookie),
            Value::Null,
        )
        .await
        .2;
    assert_eq!(current["generation"], 2);
    assert_eq!(current["participation"]["remaining_ms"], 123456);
    let expired = fixture
        .request(
            "GET",
            &format!("/v1/auctions/{auction}"),
            Some(&buyer.cookie),
            Value::Null,
        )
        .await
        .2;
    assert_eq!(expired["participation"]["status"], "disqualified");
    let old = fixture
        .request("GET", "/v1/messages", Some(&buyer.cookie), Value::Null)
        .await
        .2;
    assert_eq!(old["messages"][0]["status"], "cancelled");
    fixture.cleanup().await;
}
