mod support;

use actix_web::{App, HttpServer, dev::Service, http::StatusCode};
use arbiter::{
    artifacts::Artifacts,
    config::Config,
    crypto::{Assurance, CheckManifest, KeyMaterial, Predicate},
    repository::MIGRATIONS,
    runtime,
};
use arbiter_protocol::{ArtifactRef, JobInput, NewJob, NodeProvision, SigningKey, public_key};
use market::{
    config::Config as MarketConfig, database, events, http::router, nodes, state::AppState,
};
use sqlx::{PgPool, postgres::PgConnectOptions};
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tfhe_math::{KeySet, MathParameters, Width};
use tokio::sync::watch;
use uuid::Uuid;

async fn status(pool: &PgPool, id: Uuid, expected: &str) {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let actual: String = sqlx::query_scalar("SELECT status FROM arbiter_jobs WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
            if actual == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("job did not reach the required durable state");
}

fn node_config(
    database: PgConnectOptions,
    root: PathBuf,
    url: String,
    node: Uuid,
    deployment: Uuid,
    key: SigningKey,
) -> Config {
    Config {
        node_id: node,
        deployment_id: deployment,
        database,
        gateway: url.parse().unwrap(),
        gateway_ca: None,
        listen: "127.0.0.1:0".parse().unwrap(),
        artifacts: root,
        signing_key: key,
        token: "test-only-registered-service-token-123456".to_owned(),
        capacity: 1,
        allow_unverified_mechanics: true,
    }
}

struct NodeProcess(Child);

impl Drop for NodeProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL and fresh TEST_NODE_TOPIC / TEST_KAFKA_BROKERS"]
async fn broker_delivery_lost_ack_and_restart_keep_one_result() {
    let options = PgConnectOptions::from_str(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    let admin = PgPool::connect_with(options.clone()).await.unwrap();
    let suffix = Uuid::new_v4().simple().to_string();
    let market_schema = format!("node_market_{suffix}");
    let node_schema = format!("node_state_{suffix}");
    for schema in [&market_schema, &node_schema] {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
    }
    let market_db = options
        .clone()
        .options([("search_path", market_schema.clone())]);
    let node_db = options.options([("search_path", node_schema.clone())]);
    let pool = PgPool::connect_with(market_db.clone()).await.unwrap();
    database::MIGRATIONS.run(&pool).await.unwrap();
    let local = PgPool::connect_with(node_db.clone()).await.unwrap();
    MIGRATIONS.run(&local).await.unwrap();
    local.close().await;
    let root = std::env::var_os("TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(&suffix);
    let store = Artifacts::open(&root).unwrap();
    let state = AppState::new(
        pool.clone(),
        MarketConfig {
            localnet: None,
            custody: None,
            database: market_db,
            listen: "127.0.0.1:0".parse().unwrap(),
            origin: "http://localhost:3000".parse().unwrap(),
            rp_id: "localhost".to_owned(),
            secure_cookie: false,
            media_root: root.clone(),
            kafka_brokers: std::env::var("TEST_KAFKA_BROKERS").unwrap(),
            event_topic: std::env::var("TEST_EVENT_TOPIC").unwrap(),
            node_event_topic: std::env::var("TEST_NODE_TOPIC").unwrap(),
            jwt_secret: vec![42; 32],
            google: None,
        },
    )
    .unwrap();
    let lost = Arc::new(AtomicBool::new(false));
    let submissions = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server_state = state.clone();
    let server_lost = lost.clone();
    let server_submissions = submissions.clone();
    let server = HttpServer::new(move || {
        let lost = server_lost.clone();
        let submissions = server_submissions.clone();
        App::new()
            .configure(router::configure(server_state.clone()))
            .wrap_fn(move |request, service| {
                let result = request.path().ends_with("/results");
                let lost = lost.clone();
                let submissions = submissions.clone();
                let response = service.call(request);
                async move {
                    let mut response = response.await?;
                    if result {
                        submissions.fetch_add(1, Ordering::SeqCst);
                    }
                    if result
                        && response.status() == StatusCode::OK
                        && !lost.swap(true, Ordering::SeqCst)
                    {
                        *response.response_mut().status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                    }
                    Ok(response)
                }
            })
    })
    .workers(1)
    .disable_signals()
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    let server_task = tokio::spawn(server);
    let (broker_stop, broker_signal) = watch::channel(false);
    let worker = tokio::spawn(events::run(state, broker_signal));
    let id = Uuid::new_v4();
    let deployment = Uuid::new_v4();
    let key = SigningKey::from_bytes(&[17; 32]);
    let token = root.join("test-service.token");
    fs::write(&token, "test-only-registered-service-token-123456").unwrap();
    nodes::provision(
        &pool,
        NodeProvision {
            ethereum_address: None,
            deployment_id: deployment,
            node_id: id,
            name: "Recovery test node".to_owned(),
            public_key: public_key(&key),
            token_file: token.to_string_lossy().into(),
        },
    )
    .await
    .unwrap();
    let config = node_config(
        node_db.clone(),
        root.clone(),
        url.clone(),
        id,
        deployment,
        key.clone(),
    );
    let (stop, signal) = watch::channel(false);
    let node = tokio::spawn(runtime::serve_until(config, signal));
    let missing = ArtifactRef {
        id: Uuid::new_v4(),
        digest: [0; 32],
    };
    let material = KeyMaterial {
        configuration: [0; 32],
        public_fingerprint: [0; 32],
        evaluation_fingerprint: [0; 32],
        public_key: missing.clone(),
        evaluation_key: missing.clone(),
    };
    let manifest = CheckManifest {
        schema_version: 1,
        assurance: Assurance::ProofRequired,
        external_context: [0; 32],
        owners: [Uuid::new_v4(), Uuid::new_v4()],
        width: 8,
        comparison: Predicate::Greater,
        keys: [material.clone(), material],
        inputs: [missing.clone(), missing],
    };
    let job = NewJob {
        id: Uuid::new_v4(),
        node_id: id,
        input: store.write_json(&manifest).unwrap(),
    };
    nodes::enqueue(&pool, job.clone()).await.unwrap();
    status(&pool, job.id, "unavailable").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while submissions.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the node did not retry the unacknowledged result");
    assert!(
        submissions.load(Ordering::SeqCst) >= 2,
        "the lost acknowledgement must cause an exact retry"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM arbiter_results WHERE job_id=$1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // Model a pre-restart HTTP claim that commits after startup recovery's scan.
    let late = Uuid::new_v4();
    let local = PgPool::connect_with(node_db.clone()).await.unwrap();
    let instance: Uuid =
        sqlx::query_scalar("SELECT instance_id FROM node_identity WHERE node_id=$1")
            .bind(id)
            .fetch_one(&local)
            .await
            .unwrap();
    local.close().await;
    sqlx::query(
        "INSERT INTO arbiter_jobs(id, node_id, input, status, generation, instance_id, lease_until)
         VALUES ($1, $2, $3, 'running', 1, $4, now() + interval '90 seconds')",
    )
    .bind(late)
    .bind(id)
    .bind(serde_json::json!(job.input))
    .bind(instance)
    .execute(&pool)
    .await
    .unwrap();
    status(&pool, late, "interrupted").await;

    let keys = [
        KeySet::generate(MathParameters).unwrap(),
        KeySet::generate(MathParameters).unwrap(),
    ];
    let manifest = support::prepare(&store, &keys, [42, 17], Width::U32, Predicate::Greater);
    let encrypted = NewJob {
        id: Uuid::new_v4(),
        node_id: id,
        input: store.write_json(&manifest).unwrap(),
    };
    nodes::enqueue(&pool, encrypted.clone()).await.unwrap();
    let mut participants = support::Participants::new(&keys, store.clone());
    let mut responded = std::collections::HashSet::new();
    tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let (state, output): (String, Option<serde_json::Value>) =
                sqlx::query_as("SELECT status,output FROM arbiter_jobs WHERE id=$1")
                    .bind(encrypted.id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if state == "completed" {
                let output: ArtifactRef = serde_json::from_value(output.unwrap()).unwrap();
                let decoded: serde_json::Value = store.record(&output).unwrap();
                assert_eq!(decoded["value"], true);
                assert_eq!(decoded["assurance"], "honest_role_mechanics");
                break;
            }
            if state == "waiting" {
                let output: ArtifactRef = serde_json::from_value(output.unwrap()).unwrap();
                if responded.insert(output.id) {
                    let request = store.record(&output).unwrap();
                    let response = participants.respond(request);
                    nodes::provide_response(
                        &pool,
                        JobInput {
                            job_id: encrypted.id,
                            request_id: output.id,
                            response: store.write_json(&response).unwrap(),
                        },
                    )
                    .await
                    .unwrap();
                }
            }
            assert!(
                !matches!(state.as_str(), "failed" | "interrupted" | "unavailable"),
                "unexpected state {state}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("real HE node did not finish");
    assert_eq!(responded.len(), 5);
    let leaked: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox WHERE stream='nodes' AND (data::text LIKE '%blinding%' OR data::text LIKE '%ciphertext%' OR data::text LIKE '%value%')").fetch_one(&pool).await.unwrap();
    assert_eq!(leaked, 0);
    stop.send(true).unwrap();
    node.await.unwrap().unwrap();

    // A separate OS process loses live numeric masks when killed.
    let signing_file = root.join("node-signing.key");
    fs::write(&signing_file, key.to_bytes()).unwrap();
    fs::set_permissions(&signing_file, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let mut database_url = url::Url::parse(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    database_url
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={node_schema}"));
    let launch = || {
        NodeProcess(
            Command::new(env!("CARGO_BIN_EXE_arbiter"))
                .arg("serve")
                .env("NODE_ID", id.to_string())
                .env("DEPLOYMENT_ID", deployment.to_string())
                .env("DATABASE_URL", database_url.as_str())
                .env("GATEWAY_URL", &url)
                .env("LOCAL_DEMO", "true")
                .env("LISTEN", "127.0.0.1:0")
                .env("ARTIFACT_ROOT", &root)
                .env("SIGNING_KEY_FILE", &signing_file)
                .env("SERVICE_TOKEN_FILE", &token)
                .env("RUST_LOG", "warn")
                .stdout(Stdio::null())
                .stderr(Stdio::from(
                    fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(root.join("node-process.log"))
                        .unwrap(),
                ))
                .spawn()
                .unwrap(),
        )
    };
    let mut process = launch();
    let interrupted = NewJob {
        id: Uuid::new_v4(),
        node_id: id,
        input: encrypted.input.clone(),
    };
    nodes::enqueue(&pool, interrupted.clone()).await.unwrap();
    status(&pool, interrupted.id, "waiting").await;
    process.0.kill().unwrap();
    process.0.wait().unwrap();
    drop(process);
    fs::remove_file(root.join(format!("{}.bin", interrupted.input.id))).unwrap();
    let restarted = launch();
    status(&pool, interrupted.id, "interrupted").await;
    drop(restarted);
    broker_stop.send(true).unwrap();
    worker.await.unwrap().unwrap();
    handle.stop(true).await;
    server_task.await.unwrap().unwrap();
    pool.close().await;
    for schema in [&market_schema, &node_schema] {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&admin)
            .await
            .unwrap();
    }
    admin.close().await;
    fs::remove_dir_all(root).unwrap();
}
