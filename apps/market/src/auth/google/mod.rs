mod provider;
mod routes;

pub use provider::verify_claims;
pub use routes::{callback, config, start};

use provider::GoogleProvider;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const FLOW_TTL: Duration = Duration::from_secs(300);

struct Flow {
    nonce: String,
    verifier: String,
    return_to: String,
    expires: Instant,
}

pub struct GoogleAuth {
    provider: GoogleProvider,
    flows: Mutex<HashMap<String, Flow>>,
}

impl GoogleAuth {
    pub fn new() -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            provider: GoogleProvider::new()?,
            flows: Mutex::new(HashMap::new()),
        })
    }
}
