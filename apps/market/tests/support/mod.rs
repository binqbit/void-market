use actix_web::{
    App,
    http::{
        Method, StatusCode,
        header::{self, HeaderMap},
    },
    test,
};
use market::{config::Config, database, http::router, state::AppState};
use serde_json::{Value, json};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{env, path::PathBuf, str::FromStr};
use url::Url;
use uuid::Uuid;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};

pub const ORIGIN: &str = "http://localhost:3000";

pub struct Fixture {
    pub state: AppState,
    admin: PgPool,
    schema: String,
    media: PathBuf,
}

pub struct Identity {
    pub address: Uuid,
    pub cookie: String,
    pub authenticator: WebauthnAuthenticator<SoftPasskey>,
}

impl Fixture {
    pub async fn new() -> Self {
        let url = env::var("MARKET_TEST_DATABASE_URL")
            .expect("Set MARKET_TEST_DATABASE_URL to an isolated PostgreSQL instance");
        let options = PgConnectOptions::from_str(&url).unwrap();
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(options.clone())
            .await
            .unwrap();
        // This identifier is generated here from a fixed prefix and UUID hex, never user input.
        let schema = format!("market_test_{}", Uuid::new_v4().simple());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
        let selected = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(6)
            .after_connect(move |connection, _| {
                let selected = selected.clone();
                Box::pin(async move {
                    sqlx::query("SELECT set_config('search_path', $1, false)")
                        .bind(selected)
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options.clone())
            .await
            .unwrap();
        database::MIGRATIONS.run(&pool).await.unwrap();

        let base = env::var_os("MARKET_TEST_MEDIA_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        let media = base.join(&schema);
        tokio::fs::create_dir_all(&media).await.unwrap();
        let config = Config {
            database: options,
            listen: "127.0.0.1:0".parse().unwrap(),
            origin: Url::parse(ORIGIN).unwrap(),
            rp_id: "localhost".to_owned(),
            secure_cookie: false,
            media_root: media.clone(),
            kafka_brokers: env::var("MARKET_TEST_KAFKA_BROKERS")
                .unwrap_or_else(|_| "127.0.0.1:1".to_owned()),
            event_topic: "test.voidmarket.assets.events.v1".to_owned(),
        };
        let state = AppState::new(pool, config).unwrap();
        Self {
            state,
            admin,
            schema,
            media,
        }
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        cookie: Option<&str>,
        body: Value,
    ) -> (StatusCode, HeaderMap, Value) {
        self.request_origin(method, path, cookie, body, ORIGIN)
            .await
    }

    pub async fn request_origin(
        &self,
        method: &str,
        path: &str,
        cookie: Option<&str>,
        body: Value,
        origin: &str,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut request = test::TestRequest::default()
            .method(Method::from_bytes(method.as_bytes()).unwrap())
            .uri(path)
            .insert_header((header::ORIGIN, origin))
            .insert_header((header::CONTENT_TYPE, "application/json"));
        if let Some(cookie) = cookie {
            request = request.insert_header((header::COOKIE, cookie));
        }
        let app =
            test::init_service(App::new().configure(router::configure(self.state.clone()))).await;
        let response = test::call_service(
            &app,
            request
                .set_payload(serde_json::to_vec(&body).unwrap())
                .to_request(),
        )
        .await;
        let status = response.status();
        let headers = response.headers().clone();
        let body = test::read_body(response).await;
        let json = if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body)
                .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&body)}))
        };
        (status, headers, json)
    }

    pub async fn register(&self, name: &str) -> Identity {
        let (status, _, options) = self
            .request(
                "POST",
                "/v1/auth/registration/options",
                None,
                json!({"name":name}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{options}");
        // A software authenticator exercises real signatures; hardware user presence is simulated.
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let credential = authenticator
            .do_registration(
                Url::parse(ORIGIN).unwrap(),
                serde_json::from_value(options["public_key"].clone()).unwrap(),
            )
            .unwrap();
        let (status, headers, account) = self
            .request(
                "POST",
                "/v1/auth/registration/finish",
                None,
                json!({"ceremony_id":options["ceremony_id"],"credential":credential}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{account}");
        Identity {
            address: serde_json::from_value(account["address"].clone()).unwrap(),
            cookie: cookie(&headers),
            authenticator,
        }
    }

    pub async fn login(&self, identity: &mut Identity) {
        let (_, _, options) = self
            .request(
                "POST",
                "/v1/auth/login/options",
                None,
                json!({"address":identity.address}),
            )
            .await;
        let credential = identity
            .authenticator
            .do_authentication(
                Url::parse(ORIGIN).unwrap(),
                serde_json::from_value(options["public_key"].clone()).unwrap(),
            )
            .unwrap();
        let (status, headers, body) = self
            .request(
                "POST",
                "/v1/auth/login/finish",
                None,
                json!({"ceremony_id":options["ceremony_id"],"credential":credential}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        identity.cookie = cookie(&headers);
    }

    pub async fn asset(&self, owner: &Identity) -> Uuid {
        let id = Uuid::new_v4();
        let (status, _, body) = self
            .request(
                "POST",
                "/v1/assets",
                Some(&owner.cookie),
                json!({"request_id":id,"name":"Registered asset"}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["owner_id"], json!(owner.address));
        assert!(body.get("creator_id").is_none());
        id
    }

    pub async fn cleanup(self) {
        self.state.pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await
        .unwrap();
        self.admin.close().await;
        tokio::fs::remove_dir_all(self.media).await.unwrap();
    }
}

fn cookie(headers: &HeaderMap) -> String {
    headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
