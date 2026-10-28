use super::Custody;
use crate::{
    http::error::{ApiError, Result},
    state::AppState,
};
use check_workflow::messages::KeyMaterial;
use sqlx::{PgConnection, PgPool};
use std::sync::Arc;
use tfhe_math::codec::{
    EncodingLimits, export_evaluation_key, export_public_key, import_evaluation_key,
    import_public_key,
};
use tfhe_math::secret_codec::{export_client_key, import_client_key};
use tfhe_math::{DecryptionContext, EncryptionContext, EvaluationContext, KeySet, MathParameters};
use uuid::Uuid;

pub struct OwnerKeys {
    pub decrypt: DecryptionContext,
    pub encrypt: EncryptionContext,
    pub evaluate: EvaluationContext,
}

pub async fn material(pool: &PgPool, account: Uuid) -> Result<KeyMaterial> {
    let value: serde_json::Value = sqlx::query_scalar(
        "SELECT he_material
         FROM custody_keys
         WHERE
             account_id = $1
             AND he_status = 'ready'",
    )
    .bind(account)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::Unavailable)?;
    serde_json::from_value(value).map_err(|_| ApiError::Internal)
}

pub fn load(
    custody: &Custody,
    account: Uuid,
    material: &KeyMaterial,
    secret: &[u8],
) -> Result<OwnerKeys> {
    let binding = material.binding().map_err(|_| ApiError::Internal)?;
    let limits = EncodingLimits::default();
    let client = custody.vault.open(account, "he-client-key", secret)?;
    let client = import_client_key(&client, binding).map_err(|_| ApiError::Unavailable)?;
    let public = import_public_key(
        &custody
            .artifacts
            .read(&material.public_key, tfhe_math::codec::PUBLIC_KEY_LIMIT)
            .map_err(|_| ApiError::Unavailable)?,
        binding,
        limits,
    )
    .map_err(|_| ApiError::Unavailable)?;
    let evaluate = import_evaluation_key(
        &custody
            .artifacts
            .read(
                &material.evaluation_key,
                tfhe_math::codec::EVALUATION_KEY_LIMIT,
            )
            .map_err(|_| ApiError::Unavailable)?,
        binding,
        limits,
    )
    .map_err(|_| ApiError::Unavailable)?;
    Ok(OwnerKeys {
        decrypt: DecryptionContext::new(client),
        encrypt: EncryptionContext::new(public),
        evaluate: EvaluationContext::new(evaluate),
    })
}

pub async fn generate_one(state: &AppState) -> Result<bool> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?.clone();
    let missing: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id
         FROM accounts
         WHERE
             NOT EXISTS
             (
                 SELECT 1
                 FROM custody_keys
                 WHERE account_id = accounts.id
             )
         LIMIT 20",
    )
    .fetch_all(&state.pool)
    .await?;
    for account in missing {
        let mut connection = state.pool.acquire().await?;
        custody.provision(&mut connection, account).await?;
    }
    let lease = Uuid::new_v4();
    let account: Option<Uuid> = sqlx::query_scalar(
        "UPDATE custody_keys
         SET
             he_status = 'building',
             lease_id = $1,
             lease_until = now() + interval '10 minutes'
         WHERE
             account_id = (
                 SELECT account_id
                 FROM custody_keys
                 WHERE
                     he_status = 'queued'
                     OR (
                         he_status = 'building'
                         AND lease_until < now()
                     )
                 ORDER BY account_id
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
         RETURNING account_id",
    )
    .bind(lease)
    .fetch_optional(&state.pool)
    .await?;
    let Some(account) = account else {
        return Ok(false);
    };
    let generated = tokio::task::spawn_blocking(move || generate(custody, account)).await;
    match generated {
        Ok(Ok((material, secret))) => {
            sqlx::query(
                "UPDATE custody_keys
                 SET
                     he_status = 'ready',
                     he_material = $3,
                     he_secret = $4,
                     lease_id = NULL,
                     lease_until = NULL
                 WHERE
                     account_id = $1
                     AND lease_id = $2",
            )
            .bind(account)
            .bind(lease)
            .bind(serde_json::json!(material))
            .bind(secret)
            .execute(&state.pool)
            .await?;
        }
        _ => {
            sqlx::query(
                "UPDATE custody_keys
                 SET
                     he_status = 'failed',
                     lease_id = NULL,
                     lease_until = NULL
                 WHERE
                     account_id = $1
                     AND lease_id = $2",
            )
            .bind(account)
            .bind(lease)
            .execute(&state.pool)
            .await?;
        }
    }
    Ok(true)
}

fn generate(custody: Arc<Custody>, account: Uuid) -> Result<(KeyMaterial, Vec<u8>)> {
    let keys = KeySet::generate(MathParameters).map_err(|_| ApiError::Unavailable)?;
    let binding = keys.binding();
    let public = export_public_key(keys.public_key(), EncodingLimits::default())
        .map_err(|_| ApiError::Unavailable)?;
    let evaluation = export_evaluation_key(keys.evaluation_key(), EncodingLimits::default())
        .map_err(|_| ApiError::Unavailable)?;
    let material = KeyMaterial {
        configuration: binding.config_fingerprint(),
        public_fingerprint: binding.public_key_fingerprint(),
        evaluation_fingerprint: binding.evaluation_key_fingerprint(),
        public_key: custody
            .artifacts
            .write(public.as_bytes())
            .map_err(|_| ApiError::Unavailable)?,
        evaluation_key: custody
            .artifacts
            .write(evaluation.as_bytes())
            .map_err(|_| ApiError::Unavailable)?,
    };
    let (client, _, _) = keys.into_parts();
    let bytes = export_client_key(&client).map_err(|_| ApiError::Unavailable)?;
    Ok((
        material,
        custody.vault.seal(account, "he-client-key", &bytes)?,
    ))
}

pub async fn secret(connection: &mut PgConnection, account: Uuid) -> Result<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT he_secret
         FROM custody_keys
         WHERE
             account_id = $1
             AND he_status = 'ready'",
    )
    .bind(account)
    .fetch_optional(connection)
    .await?
    .ok_or(ApiError::Unavailable)
}
