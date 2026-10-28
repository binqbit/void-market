use crate::{auth::ceremonies::Ceremonies, config::Config};
use sqlx::PgPool;
use std::sync::{Arc, atomic::AtomicBool};
use webauthn_rs::prelude::{Webauthn, WebauthnBuilder};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub webauthn: Arc<Webauthn>,
    pub ceremonies: Arc<Ceremonies>,
    pub worker_ready: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let webauthn = WebauthnBuilder::new(&config.rp_id, &config.origin)?
            .rp_name("Void Market")
            .build()?;
        Ok(Self {
            pool,
            config: Arc::new(config),
            webauthn: Arc::new(webauthn),
            ceremonies: Arc::new(Ceremonies::default()),
            worker_ready: Arc::new(AtomicBool::new(false)),
        })
    }
}
