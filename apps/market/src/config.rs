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
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let database = match env::var("MARKET_DATABASE_URL") {
            Ok(url) => PgConnectOptions::from_str(&url)?,
            Err(_) => {
                let password_file = env::var("MARKET_DATABASE_PASSWORD_FILE")?;
                let password = std::fs::read_to_string(password_file)?;
                PgConnectOptions::new()
                    .host(&setting("MARKET_DATABASE_HOST", "127.0.0.1"))
                    .port(setting("MARKET_DATABASE_PORT", "5432").parse()?)
                    .database(&setting("MARKET_DATABASE_NAME", "market"))
                    .username(&setting("MARKET_DATABASE_USER", "market"))
                    .password(password.trim_end())
            }
        };

        let origin = Url::parse(&setting("MARKET_ORIGIN", "http://localhost:3000"))?;
        let local = matches!(origin.host_str(), Some("localhost" | "127.0.0.1"));
        if origin.scheme() != "https" && !(local && origin.scheme() == "http") {
            return Err("MARKET_ORIGIN must use HTTPS or local HTTP".into());
        }
        if origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || !origin.username().is_empty()
            || origin.password().is_some()
        {
            return Err("MARKET_ORIGIN must be an origin without a path or query".into());
        }

        Ok(Self {
            database,
            listen: setting("MARKET_LISTEN", "127.0.0.1:8080").parse()?,
            rp_id: setting("MARKET_RP_ID", "localhost"),
            secure_cookie: origin.scheme() == "https",
            origin,
            media_root: setting("MARKET_MEDIA_ROOT", "./media").into(),
            kafka_brokers: setting("MARKET_KAFKA_BROKERS", "127.0.0.1:9092"),
            event_topic: setting("MARKET_EVENT_TOPIC", "dev.voidmarket.assets.events.v1"),
        })
    }
}

fn setting(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}
