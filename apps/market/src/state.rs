use crate::{
    auth::{ceremonies::Ceremonies, google::GoogleAuth},
    config::Config,
};
use sqlx::PgPool;
use std::sync::{Arc, atomic::AtomicBool};
use webauthn_rs::prelude::{Webauthn, WebauthnBuilder};

#[derive(Clone)]
pub struct AppState {
    pub chains: Option<Arc<crate::chains::Localnet>>,
    pub auction_preparations: Arc<tokio::sync::Semaphore>,
    pub message_streams: Arc<tokio::sync::Semaphore>,
    pub custody: Option<Arc<crate::custody::Custody>>,
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub webauthn: Arc<Webauthn>,
    pub ceremonies: Arc<Ceremonies>,
    pub worker_ready: Arc<AtomicBool>,
    pub node_ready: Arc<AtomicBool>,
    pub google: Arc<GoogleAuth>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        if config.jwt_secret.len() < 32 {
            return Err("JWT signing secret must contain at least 32 bytes".into());
        }
        let webauthn = WebauthnBuilder::new(&config.rp_id, &config.origin)?
            .rp_name("Void Market")
            .build()?;
        let custody = config
            .custody
            .as_ref()
            .map(crate::custody::Custody::new)
            .transpose()?;
        Ok(Self {
            chains: config
                .localnet
                .as_ref()
                .map(crate::chains::Localnet::new)
                .transpose()?
                .map(Arc::new),
            auction_preparations: Arc::new(tokio::sync::Semaphore::new(2)),
            message_streams: Arc::new(tokio::sync::Semaphore::new(64)),
            custody: custody.map(Arc::new),
            pool,
            config: Arc::new(config),
            webauthn: Arc::new(webauthn),
            ceremonies: Arc::new(Ceremonies::default()),
            worker_ready: Arc::new(AtomicBool::new(false)),
            node_ready: Arc::new(AtomicBool::new(false)),
            google: Arc::new(GoogleAuth::new()?),
        })
    }
}
