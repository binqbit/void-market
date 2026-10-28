use actix_web::{App, HttpServer};
use arbiter::{config::Config as NodeConfig, repository::MIGRATIONS, runtime};
use arbiter_protocol::{NodeProvision, SigningKey, public_key};
use check_workflow::messages::Predicate;
use market::custody::config::CustodyConfig;
use market::messages::model::{ParticipantInput, SessionInput};
use market::{config::Config, database, events, http::router, messages, nodes, state::AppState};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgConnectOptions};
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::TcpListener,
    path::PathBuf,
    str::FromStr,
    time::Duration,
};
use tokio::sync::watch;
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires PostgreSQL, Kafka and fresh TEST_NODE_TOPIC; performs real HE operations"]
async fn manual_user_approvals_complete_the_real_custodial_check() {
    let options = PgConnectOptions::from_str(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    let admin = PgPool::connect_with(options.clone()).await.unwrap();
    let suffix = Uuid::new_v4().simple().to_string();
    let schemas = [
        format!("messages_market_{suffix}"),
        format!("messages_node_{suffix}"),
    ];
    for schema in &schemas {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
    }
    let market_db = options
        .clone()
        .options([("search_path", schemas[0].clone())]);
    let node_db = options.options([("search_path", schemas[1].clone())]);
    let pool = PgPool::connect_with(market_db.clone()).await.unwrap();
    database::MIGRATIONS.run(&pool).await.unwrap();
    let local = PgPool::connect_with(node_db.clone()).await.unwrap();
    MIGRATIONS.run(&local).await.unwrap();
    local.close().await;
    let root = std::env::var_os("TEST_ARTIFACT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(&suffix);
    fs::create_dir_all(&root).unwrap();
    let state = AppState::new(
        pool.clone(),
        Config {
            localnet: None,
            custody: Some(CustodyConfig {
                master_key: zeroize::Zeroizing::new([55; 32]),
                artifacts: root.join("artifacts"),
            }),
            database: market_db,
            listen: "127.0.0.1:0".parse().unwrap(),
            origin: "http://localhost:3000".parse().unwrap(),
            rp_id: "localhost".into(),
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
    let owners = [Uuid::new_v4(), Uuid::new_v4()];
    let mut cookies = Vec::new();
    for (i, account) in owners.iter().enumerate() {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO accounts(id,name,google_subject) VALUES($1,$2,$3)")
            .bind(account)
            .bind(format!("Test owner {i}"))
            .bind(format!("test-only-{account}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        state
            .custody
            .as_ref()
            .unwrap()
            .provision(&mut tx, *account)
            .await
            .unwrap();
        let cookie =
            market::auth::sessions::issue(&mut tx, *account, false, &state.config.jwt_secret)
                .await
                .unwrap();
        cookies.push(
            cookie
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned(),
        );
        tx.commit().await.unwrap();
    }
    let node = Uuid::new_v4();
    let key = SigningKey::from_bytes(&[19; 32]);
    let token = root.join("service.token");
    fs::write(&token, "test-only-node-service-token-0123456789").unwrap();
    let deployment = Uuid::new_v4();
    nodes::provision(
        &pool,
        NodeProvision {
            ethereum_address: None,
            deployment_id: deployment,
            node_id: node,
            name: "Test node".into(),
            public_key: public_key(&key),
            token_file: token.to_string_lossy().into(),
        },
    )
    .await
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let http_state = state.clone();
    let server =
        HttpServer::new(move || App::new().configure(router::configure(http_state.clone())))
            .workers(2)
            .disable_signals()
            .listen(listener)
            .unwrap()
            .run();
    let server_handle = server.handle();
    let server_task = tokio::spawn(server);
    let (stop, signal) = watch::channel(false);
    let (worker_stop, worker_signal) = watch::channel(false);
    let mut worker = tokio::spawn(events::run(state.clone(), worker_signal));
    let node_task = tokio::spawn(runtime::serve_until(
        NodeConfig {
            node_id: node,
            deployment_id: deployment,
            database: node_db,
            gateway: url.parse().unwrap(),
            gateway_ca: None,
            listen: "127.0.0.1:0".parse().unwrap(),
            artifacts: root.join("artifacts"),
            signing_key: key,
            token: "test-only-node-service-token-0123456789".into(),
            capacity: 1,
            allow_unverified_mechanics: true,
        },
        signal,
    ));
    let session = Uuid::new_v4();
    messages::create_session(
        &state,
        SessionInput {
            id: session,
            title: "Two owners, one private check".into(),
            node_id: node,
            comparison: Predicate::Greater,
            width: 8,
            participants: [
                ParticipantInput {
                    account_id: owners[0],
                    value: 42,
                },
                ParticipantInput {
                    account_id: owners[1],
                    value: 17,
                },
            ],
        },
    )
    .await
    .unwrap();
    let client = reqwest::Client::new();
    let mut stream = client
        .get(format!("{url}/v1/messages/events"))
        .header("Cookie", &cookies[0])
        .send()
        .await
        .unwrap();
    assert!(
        String::from_utf8_lossy(&stream.chunk().await.unwrap().unwrap())
            .contains("event: messages")
    );
    drop(stream);
    let mut approved = HashSet::new();
    let mut restarted = false;
    let mut previous_budgets = HashMap::new();
    tokio::time::timeout(Duration::from_secs(360), async {
        loop {
            let status: String =
                sqlx::query_scalar("SELECT status FROM approval_sessions WHERE id=$1")
                    .bind(session)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_ne!(
                status, "interrupted",
                "custodial session unexpectedly interrupted"
            );
            if status == "completed" {
                break;
            }

            if !restarted && approved.len() >= 3 {
                let durable: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM approval_messages WHERE session_id=$1
                     AND kind='mask_contribution' AND private_state IS NOT NULL",
                )
                .bind(session)
                .fetch_one(&pool)
                .await
                .unwrap();
                if durable == 2 {
                    worker_stop.send(true).unwrap();
                    (&mut worker).await.unwrap().unwrap();
                    worker_stop.send_replace(false);
                    worker = tokio::spawn(events::run(state.clone(), worker_stop.subscribe()));
                    restarted = true;
                }
            }

            for (index, cookie) in cookies.iter().enumerate() {
                let snapshot: Value = client
                    .get(format!("{url}/v1/messages"))
                    .header("Cookie", cookie)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                for message in snapshot["messages"].as_array().unwrap() {
                    assert_ne!(message["status"], "blocked");
                    if message["status"] != "ready"
                        || !approved.insert(message["id"].as_str().unwrap().to_owned())
                    {
                        continue;
                    }

                    let budget: i64 = sqlx::query_scalar(
                        "SELECT remaining_ms FROM approval_members
                         WHERE session_id=$1 AND account_id=$2",
                    )
                    .bind(session)
                    .bind(owners[index])
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                    if let Some(previous) = previous_budgets.insert(index, budget) {
                        assert!(
                            budget <= previous,
                            "a new action reset the cumulative budget"
                        );
                    }

                    let path = format!(
                        "{url}/v1/messages/{}/approve",
                        message["id"].as_str().unwrap()
                    );
                    let body =
                        json!({"request_id":message["request_id"],"revision":message["revision"]});
                    let first = approve(&client, &path, cookie, &body).await;
                    let retry = approve(&client, &path, cookie, &body).await;
                    assert_eq!(
                        first, retry,
                        "retry must reuse the original signed approval"
                    );
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the custodial approval workflow did not complete");
    assert_eq!(approved.len(), 7);
    assert!(
        restarted,
        "the persistent owner checkpoints were exercised across a worker restart"
    );
    let complete: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_messages WHERE session_id=$1 AND status='completed'",
    )
    .bind(session)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(complete, 7);
    let output: Value = sqlx::query_scalar("SELECT output FROM arbiter_jobs WHERE id=$1")
        .bind(session)
        .fetch_one(&pool)
        .await
        .unwrap();
    let output = serde_json::from_value(output).unwrap();
    let decoded: Value = state
        .custody
        .as_ref()
        .unwrap()
        .artifacts
        .record(&output)
        .unwrap();
    assert_eq!(decoded["value"], true);
    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM outbox WHERE data::text LIKE '%signing_key%' OR data::text
         LIKE '%blinding%' OR data::text LIKE '%ciphertext%' OR data::text LIKE '%value%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(leaked, 0);
    stop.send(true).unwrap();
    worker_stop.send(true).unwrap();
    node_task.await.unwrap().unwrap();
    worker.await.unwrap().unwrap();
    server_handle.stop(true).await;
    server_task.await.unwrap().unwrap();
    pool.close().await;
    for schema in &schemas {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&admin)
            .await
            .unwrap();
    }
    admin.close().await;
    fs::remove_dir_all(root).unwrap();
}

async fn approve(client: &reqwest::Client, path: &str, cookie: &str, body: &Value) -> Value {
    client
        .post(path)
        .header("Cookie", cookie)
        .header("Origin", "http://localhost:3000")
        .json(body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
