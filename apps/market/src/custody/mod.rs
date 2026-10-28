pub mod config;
pub mod keys;
pub mod operations;
pub mod seal;

use crate::http::error::{ApiError, Result};
use arbiter_protocol::{SigningKey, public_key};
use check_workflow::artifacts::Artifacts;
use rand::{TryRngCore, rngs::OsRng};
use sqlx::PgConnection;
use uuid::Uuid;
use zeroize::Zeroizing;

pub struct Custody {
    pub vault: seal::Vault,
    pub artifacts: Artifacts,
}

impl Custody {
    pub fn new(config: &config::CustodyConfig) -> Result<Self> {
        Ok(Self {
            vault: seal::Vault::new(&config.master_key)?,
            artifacts: Artifacts::open(&config.artifacts).map_err(|_| ApiError::Unavailable)?,
        })
    }

    pub async fn provision(&self, connection: &mut PgConnection, account: Uuid) -> Result<()> {
        let mut seed = Zeroizing::new([0u8; 32]);
        OsRng
            .try_fill_bytes(&mut *seed)
            .map_err(|_| ApiError::Unavailable)?;
        let signer = SigningKey::from_bytes(&seed);
        let sealed = self.vault.seal(account, "signing-key", &*seed)?;
        sqlx::query(
            "INSERT INTO custody_keys(account_id, signing_key, signing_public_key) VALUES
             ($1, $2, $3) ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(account)
        .bind(sealed)
        .bind(public_key(&signer))
        .execute(connection)
        .await?;
        Ok(())
    }
}
