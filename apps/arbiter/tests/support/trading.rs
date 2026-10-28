use actix_web::{App, HttpServer, dev::ServerHandle};
use arbiter::{config::Config as NodeConfig, repository::MIGRATIONS, runtime};
use arbiter_protocol::{NodeProvision, SigningKey, public_key};
use market::{
    config::Config, custody::config::CustodyConfig, database, events, http::router, nodes,
    state::AppState,
};
use reqwest::{Client, Response};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgConnectOptions};
use std::{env, fs, net::TcpListener, path::PathBuf, str::FromStr, time::Duration};
use tokio::{sync::watch, task::JoinHandle};
use uuid::Uuid;

pub struct Trading {
    pub state: AppState,
    pub accounts: Vec<Uuid>,
    pub cookies: Vec<String>,
    pub url: String,
    pub client: Client,
    admin: PgPool,
    schemas: [String; 2],
    root: PathBuf,
    stop: watch::Sender<bool>,
    node: JoinHandle<arbiter::error::Result<()>>,
    worker: JoinHandle<()>,
    server: ServerHandle,
    http: JoinHandle<std::io::Result<()>>,
}

impl Trading {
    pub async fn new(count: usize) -> Self {
        let options = PgConnectOptions::from_str(&env::var("TEST_DATABASE_URL").unwrap()).unwrap();
        let admin = PgPool::connect_with(options.clone()).await.unwrap();
        let suffix = Uuid::new_v4().simple().to_string();
        let schemas = [
            format!("trading_market_{suffix}"),
            format!("trading_node_{suffix}"),
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
        let root = env::var_os("TEST_ARTIFACT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join(suffix);
        fs::create_dir_all(&root).unwrap();
        let state = AppState::new(
            pool.clone(),
            Config {
                localnet: None,
                custody: Some(CustodyConfig {
                    master_key: zeroize::Zeroizing::new([61; 32]),
                    artifacts: root.join("checks"),
                }),
                database: market_db,
                listen: "127.0.0.1:0".parse().unwrap(),
                origin: "http://localhost:3000".parse().unwrap(),
                rp_id: "localhost".into(),
                secure_cookie: false,
                media_root: root.clone(),
                kafka_brokers: env::var("TEST_KAFKA_BROKERS").unwrap(),
                event_topic: env::var("TEST_EVENT_TOPIC").unwrap(),
                node_event_topic: env::var("TEST_NODE_TOPIC").unwrap(),
                jwt_secret: vec![42; 32],
                google: None,
            },
        )
        .unwrap();
        let mut accounts = Vec::new();
        let mut cookies = Vec::new();
        for index in 0..count {
            let account = Uuid::new_v4();
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("INSERT INTO accounts(id,name,google_subject) VALUES($1,$2,$3)")
                .bind(account)
                .bind(format!("Trading owner {index}"))
                .bind(format!("fixture-{account}"))
                .execute(&mut *tx)
                .await
                .unwrap();
            state
                .custody
                .as_ref()
                .unwrap()
                .provision(&mut tx, account)
                .await
                .unwrap();
            let cookie =
                market::auth::sessions::issue(&mut tx, account, false, &state.config.jwt_secret)
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
            accounts.push(account);
            tx.commit().await.unwrap();
        }
        let node_id = Uuid::new_v4();
        let deployment = Uuid::new_v4();
        let key = SigningKey::from_bytes(&[23; 32]);
        let token = root.join("node.token");
        fs::write(&token, "fixture-trading-node-service-token-0123456789").unwrap();
        nodes::provision(
            &pool,
            NodeProvision {
                ethereum_address: None,
                node_id,
                deployment_id: deployment,
                name: "Trading fixture node".into(),
                public_key: public_key(&key),
                token_file: token.to_string_lossy().into(),
            },
        )
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let http_state = state.clone();
        let http =
            HttpServer::new(move || App::new().configure(router::configure(http_state.clone())))
                .workers(2)
                .disable_signals()
                .listen(listener)
                .unwrap()
                .run();
        let server = http.handle();
        let http = tokio::spawn(http);
        let (stop, signal) = watch::channel(false);
        let worker_state = state.clone();
        let worker_signal = signal.clone();
        let worker = tokio::spawn(async move {
            events::run(worker_state, worker_signal).await.unwrap();
        });
        let node = tokio::spawn(runtime::serve_until(
            NodeConfig {
                node_id,
                deployment_id: deployment,
                database: node_db,
                gateway: url.parse().unwrap(),
                gateway_ca: None,
                listen: "127.0.0.1:0".parse().unwrap(),
                artifacts: root.join("checks"),
                signing_key: key,
                token: "fixture-trading-node-service-token-0123456789".into(),
                capacity: 1,
                allow_unverified_mechanics: true,
            },
            signal,
        ));
        let fixture = Self {
            state,
            accounts,
            cookies,
            url,
            client: Client::new(),
            admin,
            schemas,
            root,
            stop,
            node,
            worker,
            server,
            http,
        };
        tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                let ready: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM custody_keys WHERE he_status='ready'")
                        .fetch_one(&fixture.state.pool)
                        .await
                        .unwrap();
                let node_ready: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM arbiter_nodes WHERE heartbeat_until>now())",
                )
                .fetch_one(&fixture.state.pool)
                .await
                .unwrap();
                if ready == count as i64 && node_ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("automatic account keys and node heartbeat must become ready");
        fixture
    }

    pub async fn post(&self, owner: usize, path: &str, body: Value) -> Response {
        self.client
            .post(format!("{}{path}", self.url))
            .header("Origin", "http://localhost:3000")
            .header("Cookie", &self.cookies[owner])
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get(&self, owner: Option<usize>, path: &str) -> Value {
        let mut request = self.client.get(format!("{}{path}", self.url));
        if let Some(owner) = owner {
            request = request.header("Cookie", &self.cookies[owner]);
        }
        request
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    pub async fn close(self) {
        self.stop.send(true).unwrap();
        self.node.await.unwrap().unwrap();
        self.worker.await.unwrap();
        self.server.stop(true).await;
        self.http.await.unwrap().unwrap();
        self.state.pool.close().await;
        for schema in &self.schemas {
            sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
                .execute(&self.admin)
                .await
                .unwrap();
        }
        self.admin.close().await;
        fs::remove_dir_all(self.root).unwrap();
    }
}
