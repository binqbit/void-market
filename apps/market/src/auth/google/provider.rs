use crate::http::error::{ApiError, Result};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use reqwest::{Client, Response};
use serde::{Deserialize, de::DeserializeOwned};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

pub(super) struct GoogleProvider {
    pub(super) client: Client,
    keys: Mutex<Option<(Instant, JwkSet)>>,
}

impl GoogleProvider {
    pub(super) fn new() -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            keys: Mutex::new(None),
        })
    }
    pub(super) async fn verify(
        &self,
        token: &str,
        client_id: &str,
        nonce: &str,
    ) -> Result<GoogleClaims> {
        let header = decode_header(token).map_err(|_| ApiError::Unauthorized)?;
        if header.alg != Algorithm::RS256 {
            return Err(ApiError::Unauthorized);
        }
        let kid = header.kid.ok_or(ApiError::Unauthorized)?;
        let mut cached = self.keys.lock().await;
        let refresh = cached
            .as_ref()
            .is_none_or(|(expires, keys)| *expires <= Instant::now() || keys.find(&kid).is_none());
        if refresh {
            let response = self
                .client
                .get("https://www.googleapis.com/oauth2/v3/certs")
                .send()
                .await
                .map_err(|_| ApiError::Unavailable)?;
            let keys = response_json::<JwkSet>(response).await?;
            *cached = Some((Instant::now() + Duration::from_secs(3600), keys));
        }
        let keys = &cached.as_ref().ok_or(ApiError::Internal)?.1;
        verify_claims(token, keys, client_id, nonce)
    }
}

#[derive(Clone, Deserialize)]
pub struct GoogleClaims {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
    pub nonce: String,
    pub iat: u64,
    pub azp: Option<String>,
}

pub fn verify_claims(
    token: &str,
    keys: &JwkSet,
    client_id: &str,
    nonce: &str,
) -> Result<GoogleClaims> {
    let header = decode_header(token).map_err(|_| ApiError::Unauthorized)?;
    if header.alg != Algorithm::RS256 {
        return Err(ApiError::Unauthorized);
    }
    let kid = header.kid.ok_or(ApiError::Unauthorized)?;
    let key = keys.find(&kid).ok_or(ApiError::Unauthorized)?;
    let decoding = DecodingKey::from_jwk(key).map_err(|_| ApiError::Unauthorized)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat"]);
    validation.leeway = 30;
    let claims = decode::<GoogleClaims>(token, &decoding, &validation)
        .map_err(|_| ApiError::Unauthorized)?
        .claims;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ApiError::Internal)?
        .as_secs();
    if claims.nonce != nonce
        || !claims.email_verified
        || claims.sub.is_empty()
        || claims.sub.len() > 255
        || claims.email.len() > 254
        || !claims.email.contains('@')
        || claims.iat > now + 30
        || claims
            .azp
            .as_ref()
            .is_some_and(|authorized| authorized != client_id)
    {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}

pub(super) async fn response_json<T: DeserializeOwned>(mut response: Response) -> Result<T> {
    if !response.status().is_success() {
        return Err(ApiError::Unavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ApiError::Unavailable)? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err(ApiError::Unavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| ApiError::Unavailable)
}
