use crate::{
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::HttpRequest;
use arbiter_protocol::RequestSignature;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub struct Actor {
    pub public_key: String,
    pub request: RequestSignature,
    pub digest: Vec<u8>,
}

pub async fn authorize(
    state: &AppState,
    request: &HttpRequest,
    body: &[u8],
    registration: bool,
) -> Result<Actor> {
    let encoded = request
        .headers()
        .get("X-Node-Signature")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 2048)
        .ok_or(ApiError::Unauthorized)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| ApiError::Unauthorized)?;
    let signed: RequestSignature =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::Unauthorized)?;
    let token = request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.len() <= 256)
        .ok_or(ApiError::Unauthorized)?;
    let token_hash: Vec<u8> = Sha256::digest(token.as_bytes()).to_vec();
    let row: Option<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT
             deployment_id,
             public_key,
             instance_id
         FROM arbiter_nodes
         WHERE
             id = $1
             AND token_hash = $2
             AND enabled",
    )
    .bind(signed.node_id)
    .bind(token_hash)
    .fetch_optional(&state.pool)
    .await?;
    let (deployment, public, instance) = row.ok_or(ApiError::Unauthorized)?;
    if deployment != signed.deployment_id
        || (!registration && instance != Some(signed.instance_id))
        || (registration && instance.is_some_and(|id| id != signed.instance_id))
    {
        return Err(ApiError::Forbidden);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ApiError::Internal)?
        .as_millis();
    arbiter_protocol::verify(
        &public,
        &signed,
        request.method().as_str(),
        request
            .uri()
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or(""),
        body,
        now.try_into().map_err(|_| ApiError::Internal)?,
    )
    .map_err(|_| ApiError::Unauthorized)?;
    Ok(Actor {
        public_key: public,
        digest: Sha256::digest(&bytes).to_vec(),
        request: signed,
    })
}
