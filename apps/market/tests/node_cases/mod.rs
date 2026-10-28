use crate::support::Fixture;
use actix_web::{
    App,
    http::{Method, StatusCode},
    test,
};
use arbiter_protocol::{
    ArtifactRef, Claim, Completion, Job, JobStatus, NewJob, NodeProvision, Registration,
    RequestSignature, SigningKey, public_key, sign, sign_result,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use market::{http::router, nodes};
use serde_json::{Value, json};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

struct Node {
    id: Uuid,
    instance: Uuid,
    deployment: Uuid,
    key: SigningKey,
    token: String,
}
impl Node {
    async fn provision(fixture: &Fixture, seed: u8) -> Self {
        let id = Uuid::new_v4();
        let key = SigningKey::from_bytes(&[seed; 32]);
        let token = format!("fixture-only-node-service-token-{id}");
        let path = fixture.state.config.media_root.join(format!("{id}.token"));
        fs::write(&path, &token).unwrap();
        let node = Self {
            id,
            instance: Uuid::new_v4(),
            deployment: Uuid::new_v4(),
            key,
            token,
        };
        nodes::provision(
            &fixture.state.pool,
            NodeProvision {
                ethereum_address: None,
                deployment_id: node.deployment,
                node_id: node.id,
                name: "Test node".to_owned(),
                public_key: public_key(&node.key),
                token_file: path.to_string_lossy().to_string(),
            },
        )
        .await
        .unwrap();
        node
    }
    fn request(&self, method: &str, path: &str, body: &[u8]) -> RequestSignature {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let mut request = RequestSignature {
            deployment_id: self.deployment,
            node_id: self.id,
            instance_id: self.instance,
            request_id: Uuid::new_v4(),
            issued_at_ms: now,
            expires_at_ms: now + 60_000,
            signature: String::new(),
        };
        sign(&self.key, &mut request, method, path, body);
        request
    }
    async fn send(
        &self,
        fixture: &Fixture,
        method: &str,
        path: &str,
        value: Option<Value>,
    ) -> (StatusCode, Value) {
        let body = value
            .map(|value| serde_json::to_vec(&value).unwrap())
            .unwrap_or_default();
        self.send_signed(
            fixture,
            method,
            path,
            &body,
            &self.request(method, path, &body),
        )
        .await
    }
    async fn send_signed(
        &self,
        fixture: &Fixture,
        method: &str,
        path: &str,
        body: &[u8],
        signed: &RequestSignature,
    ) -> (StatusCode, Value) {
        let app =
            test::init_service(App::new().configure(router::configure(fixture.state.clone())))
                .await;
        let response = test::call_service(
            &app,
            test::TestRequest::default()
                .method(Method::from_bytes(method.as_bytes()).unwrap())
                .uri(path)
                .insert_header(("Authorization", format!("Bearer {}", self.token)))
                .insert_header((
                    "X-Node-Signature",
                    URL_SAFE_NO_PAD.encode(serde_json::to_vec(signed).unwrap()),
                ))
                .insert_header(("Content-Type", "application/json"))
                .set_payload(body.to_vec())
                .to_request(),
        )
        .await;
        let status = response.status();
        let body = test::read_body(response).await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }
    async fn register(&self, fixture: &Fixture) {
        let input = Registration {
            instance_id: self.instance,
            capacity: 1,
            session_capacity: 1,
            sequence: 1,
        };
        assert_eq!(
            self.send(fixture, "POST", "/node/v1/register", Some(json!(input)))
                .await
                .0,
            StatusCode::OK
        );
    }
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn node_authentication_is_separate_and_replays_are_bound() {
    let fixture = Fixture::new().await;
    let node = Node::provision(&fixture, 7).await;
    let payload = serde_json::to_vec(&Registration {
        instance_id: node.instance,
        capacity: 1,
        session_capacity: 1,
        sequence: 1,
    })
    .unwrap();
    let signed = node.request("POST", "/node/v1/register", &payload);
    assert_eq!(
        node.send_signed(&fixture, "POST", "/node/v1/register", &payload, &signed)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        node.send_signed(&fixture, "POST", "/node/v1/register", &payload, &signed)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        node.send_signed(&fixture, "POST", "/node/v1/heartbeat", &payload, &signed)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let mut foreign = signed.clone();
    foreign.deployment_id = Uuid::new_v4();
    sign(
        &node.key,
        &mut foreign,
        "POST",
        "/node/v1/register",
        &payload,
    );
    assert_eq!(
        node.send_signed(&fixture, "POST", "/node/v1/register", &payload, &foreign)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .request("GET", "/node/v1/jobs", None, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let user = fixture.register("Ordinary user").await;
    assert_eq!(
        fixture
            .request("GET", "/node/v1/jobs", Some(&user.cookie), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn jobs_stay_on_their_node_and_only_one_worker_claims_them() {
    let fixture = Fixture::new().await;
    let first = Node::provision(&fixture, 7).await;
    let second = Node::provision(&fixture, 8).await;
    first.register(&fixture).await;
    second.register(&fixture).await;
    let id = Uuid::new_v4();
    nodes::enqueue(
        &fixture.state.pool,
        NewJob {
            id,
            node_id: first.id,
            input: ArtifactRef {
                id: Uuid::new_v4(),
                digest: [1; 32],
            },
        },
    )
    .await
    .unwrap();
    // This fixture positions an already-delivered queue entry; broker delivery is tested separately.
    sqlx::query("UPDATE arbiter_jobs SET status='queued' WHERE id=$1")
        .bind(id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(
        second.send(&fixture, "GET", "/node/v1/jobs", None).await.1,
        json!([])
    );
    assert_eq!(
        second
            .send(&fixture, "GET", &format!("/node/v1/jobs/{id}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let path = format!("/node/v1/jobs/{id}/claim");
    let claim = json!(Claim {
        job_id: id,
        instance_id: first.instance,
        generation: 0
    });
    let (a, b) = tokio::join!(
        first.send(&fixture, "POST", &path, Some(claim.clone())),
        first.send(&fixture, "POST", &path, Some(claim))
    );
    let statuses = [a.0, b.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    let owned: Job = serde_json::from_value(
        first
            .send(&fixture, "GET", &format!("/node/v1/jobs/{id}"), None)
            .await
            .1,
    )
    .unwrap();
    let mut result = Completion {
        deployment_id: first.deployment,
        node_id: first.id,
        instance_id: first.instance,
        signature: String::new(),
        id: Uuid::new_v4(),
        job_id: id,
        generation: owned.generation,
        status: JobStatus::Completed,
        output: Some(ArtifactRef {
            id: Uuid::new_v4(),
            digest: [2; 32],
        }),
        consumed_response: None,
    };
    sign_result(&mut result, &first.key);
    sqlx::query("UPDATE arbiter_jobs SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let path = format!("/node/v1/jobs/{id}/results");
    assert_eq!(
        first
            .send(&fixture, "POST", &path, Some(json!(result)))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // A late participant response must not prevent closing a lost ephemeral attempt.
    sqlx::query("UPDATE arbiter_jobs SET response=$2 WHERE id=$1")
        .bind(id)
        .bind(json!(ArtifactRef {
            id: Uuid::new_v4(),
            digest: [3; 32]
        }))
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    result.status = JobStatus::Interrupted;
    result.output = None;
    sign_result(&mut result, &first.key);
    assert_eq!(
        first
            .send(&fixture, "POST", &path, Some(json!(result)))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        first
            .send(&fixture, "POST", &path, Some(json!(result)))
            .await
            .0,
        StatusCode::OK
    );
    let status: String = sqlx::query_scalar("SELECT status FROM arbiter_jobs WHERE id=$1")
        .bind(id)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(status, "result_pending");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM arbiter_results WHERE job_id=$1")
        .bind(id)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn provisioning_retries_preserve_the_registered_identity() {
    let fixture = Fixture::new().await;
    let key = SigningKey::from_bytes(&[21; 32]);
    let token = fixture
        .state
        .config
        .media_root
        .join("provision-retry.token");
    fs::write(&token, "fixture-only-node-service-token-0123456789").unwrap();
    let mut registration = NodeProvision {
        node_id: Uuid::new_v4(),
        deployment_id: Uuid::new_v4(),
        name: "Retry-safe node".to_owned(),
        public_key: public_key(&key),
        token_file: token.to_string_lossy().into_owned(),
        ethereum_address: Some("0x1234567890123456789012345678901234567890".into()),
    };
    nodes::provision(&fixture.state.pool, registration.clone())
        .await
        .unwrap();
    nodes::provision(&fixture.state.pool, registration.clone())
        .await
        .unwrap();
    registration.ethereum_address = Some("0x2234567890123456789012345678901234567890".into());
    assert!(
        nodes::provision(&fixture.state.pool, registration.clone())
            .await
            .is_err()
    );
    let retained: String =
        sqlx::query_scalar("SELECT ethereum_address FROM arbiter_nodes WHERE id=$1")
            .bind(registration.node_id)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(retained, "0x1234567890123456789012345678901234567890");
    fixture.cleanup().await;
}
