use sqlx::postgres::PgConnectOptions;
use std::{env, net::SocketAddr, path::PathBuf, str::FromStr};
use url::Url;

pub struct Config {
    pub database: PgConnectOptions,
    pub listen: SocketAddr,
    pub origin: Url,
    pub rp_id: String,
    pub secure_cookie: bool,
    pub media_root: PathBuf,
    pub kafka_brokers: String,
    pub event_topic: String,
    pub node_event_topic: String,
    pub jwt_secret: Vec<u8>,
    pub google: Option<GoogleConfig>,
}

pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let database = match env::var("DATABASE_URL") {
            Ok(url) => PgConnectOptions::from_str(&url)?,
            Err(_) => {
                let password_file = env::var("DATABASE_PASSWORD_FILE")?;
                let password = std::fs::read_to_string(password_file)?;
                PgConnectOptions::new()
                    .host(&setting("DATABASE_HOST", "127.0.0.1"))
                    .port(setting("DATABASE_PORT", "5432").parse()?)
                    .database(&setting("DATABASE_NAME", "market"))
                    .username(&setting("DATABASE_USER", "market"))
                    .password(password.trim_end())
            }
        };

        let origin = Url::parse(&setting("ORIGIN", "http://localhost:3000"))?;
        let local = matches!(origin.host_str(), Some("localhost" | "127.0.0.1"));
        if origin.scheme() != "https" && !(local && origin.scheme() == "http") {
            return Err("ORIGIN must use HTTPS or local HTTP".into());
        }
        if origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || !origin.username().is_empty()
            || origin.password().is_some()
        {
            return Err("ORIGIN must be an origin without a path or query".into());
        }

        let jwt_secret = secret("JWT_SECRET")?.ok_or("Set JWT_SECRET_FILE")?;
        if jwt_secret.len() < 32 {
            return Err("The JWT signing secret must contain at least 32 random bytes".into());
        }
        let google = match (
            env::var("GOOGLE_CLIENT_ID")
                .ok()
                .filter(|value| !value.is_empty()),
            secret("GOOGLE_CLIENT_SECRET")?.filter(|value| !value.is_empty()),
        ) {
            (Some(client_id), Some(client_secret))
                if !client_id.is_empty() && !client_secret.is_empty() =>
            {
                Some(GoogleConfig {
                    client_id,
                    client_secret,
                })
            }
            (None, None) => None,
            _ => {
                return Err(
                    "Configure both the Google client ID and client secret, or neither".into(),
                );
            }
        };

        Ok(Self {
            database,
            listen: setting("LISTEN", "127.0.0.1:8080").parse()?,
            rp_id: setting("RP_ID", "localhost"),
            secure_cookie: origin.scheme() == "https",
            origin,
            media_root: setting("MEDIA_ROOT", "./media").into(),
            kafka_brokers: setting("KAFKA_BROKERS", "127.0.0.1:9092"),
            event_topic: setting("EVENT_TOPIC", "dev.voidmarket.assets.events.v1"),
            node_event_topic: setting("NODE_EVENT_TOPIC", "dev.voidmarket.nodes.v1"),
            jwt_secret: jwt_secret.into_bytes(),
            google,
        })
    }
}

fn secret(name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Ok(path) = env::var(format!("{name}_FILE")) {
        return Ok(Some(std::fs::read_to_string(path)?.trim_end().to_owned()));
    }
    Ok(env::var(name).ok())
}

fn setting(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}
