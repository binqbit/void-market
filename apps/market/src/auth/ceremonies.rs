use crate::http::error::{ApiError, Result};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

pub enum Ceremony {
    Registration {
        account_id: Uuid,
        name: String,
        state: PasskeyRegistration,
    },
    Authentication {
        account_id: Uuid,
        state: PasskeyAuthentication,
        credential_snapshot: serde_json::Value,
    },
}

struct Pending {
    expires: Instant,
    ceremony: Ceremony,
}

/// Short-lived server state, consumed once. Restart requires a new ceremony.
#[derive(Default)]
pub struct Ceremonies {
    pending: Mutex<HashMap<Uuid, Pending>>,
}

impl Ceremonies {
    pub fn insert(&self, ceremony: Ceremony) -> Result<Uuid> {
        let mut pending = self.pending.lock().map_err(|_| ApiError::Internal)?;
        let now = Instant::now();
        pending.retain(|_, value| value.expires > now);
        if pending.len() >= 1024 {
            return Err(ApiError::Unavailable);
        }
        let id = Uuid::new_v4();
        pending.insert(
            id,
            Pending {
                expires: now + Duration::from_secs(300),
                ceremony,
            },
        );
        Ok(id)
    }

    pub fn take(&self, id: Uuid) -> Result<Ceremony> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| ApiError::Internal)?
            .remove(&id)
            .ok_or(ApiError::BadRequest("Unknown or consumed ceremony"))?;
        if pending.expires <= Instant::now() {
            return Err(ApiError::BadRequest("Ceremony expired"));
        }
        Ok(pending.ceremony)
    }
}
