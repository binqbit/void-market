use crate::error::{Error, Result};
use arbiter_protocol::SigningKey;
use sqlx::postgres::PgConnectOptions;
use std::{
    env, fs, io::Read, net::SocketAddr, os::unix::fs::PermissionsExt, path::PathBuf, str::FromStr,
};
use url::Url;
use uuid::Uuid;

pub struct Config {
    pub node_id: Uuid,
    pub deployment_id: Uuid,
    pub database: PgConnectOptions,
    pub gateway: Url,
    pub gateway_ca: Option<Vec<u8>>,
    pub listen: SocketAddr,
    pub artifacts: PathBuf,
    pub signing_key: SigningKey,
    pub token: String,
    pub capacity: usize,
    pub allow_unverified_mechanics: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let gateway = Url::parse(&required("GATEWAY_URL")?).map_err(|_| Error::Configuration)?;
        let local = matches!(
            gateway.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
        let demo = env::var("LOCAL_DEMO").ok().as_deref() == Some("true");
        if gateway.scheme() != "https" && !(demo && local && gateway.scheme() == "http") {
            return Err(Error::Transport);
        }
        if gateway.path() != "/"
            || gateway.query().is_some()
            || gateway.fragment().is_some()
            || !gateway.username().is_empty()
            || gateway.password().is_some()
        {
            return Err(Error::Configuration);
        }
        let key = private_file(&required("SIGNING_KEY_FILE")?, 32)?;
        let key: [u8; 32] = key.try_into().map_err(|_| Error::Configuration)?;
        let token = String::from_utf8(private_file(&required("SERVICE_TOKEN_FILE")?, 256)?)
            .map_err(|_| Error::Configuration)?
            .trim()
            .to_owned();
        let database = match env::var("DATABASE_URL") {
            Ok(url) => PgConnectOptions::from_str(&url).map_err(|_| Error::Configuration)?,
            Err(_) => {
                let password =
                    String::from_utf8(private_file(&required("DATABASE_PASSWORD_FILE")?, 1024)?)
                        .map_err(|_| Error::Configuration)?;
                PgConnectOptions::new()
                    .host(&required("DATABASE_HOST")?)
                    .port(
                        env::var("DATABASE_PORT")
                            .unwrap_or_else(|_| "5432".to_owned())
                            .parse()
                            .map_err(|_| Error::Configuration)?,
                    )
                    .database(&required("DATABASE_NAME")?)
                    .username(&required("DATABASE_USER")?)
                    .password(password.trim())
            }
        };
        let gateway_ca = env::var("GATEWAY_CA_FILE")
            .ok()
            .map(fs::read)
            .transpose()
            .map_err(|_| Error::Configuration)?;
        let capacity = env::var("CAPACITY")
            .unwrap_or_else(|_| "1".to_owned())
            .parse::<usize>()
            .map_err(|_| Error::Configuration)?;
        if !(1..=4).contains(&capacity) || token.len() < 32 || token.len() > 256 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            node_id: Uuid::parse_str(&required("NODE_ID")?).map_err(|_| Error::Configuration)?,
            deployment_id: Uuid::parse_str(&required("DEPLOYMENT_ID")?)
                .map_err(|_| Error::Configuration)?,
            database,
            gateway_ca,
            gateway,
            listen: env::var("LISTEN")
                .unwrap_or_else(|_| "127.0.0.1:8090".to_owned())
                .parse()
                .map_err(|_| Error::Configuration)?,
            artifacts: required("ARTIFACT_ROOT")?.into(),
            signing_key: SigningKey::from_bytes(&key),
            token,
            capacity,
            allow_unverified_mechanics: demo,
        })
    }
}

fn private_file(path: &str, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|_| Error::Configuration)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 || metadata.len() > limit {
        return Err(Error::Configuration);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| Error::Configuration)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Configuration)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Configuration);
    }
    Ok(bytes)
}

fn required(name: &'static str) -> Result<String> {
    env::var(name).map_err(|_| Error::MissingSetting(name))
}
