use crate::http::error::{ApiError, Result};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const SESSION_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub iat: u64,
    pub exp: u64,
    pub iss: String,
    pub aud: String,
}

pub fn issue(account: Uuid, secret: &[u8]) -> Result<String> {
    let now = now()?;
    let claims = Claims {
        sub: account,
        jti: Uuid::new_v4(),
        iat: now,
        exp: now + SESSION_SECONDS,
        iss: "void-market".to_owned(),
        aud: "void-market-web".to_owned(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|_| ApiError::Internal)
}

pub fn verify(token: &str, secret: &[u8]) -> Result<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&["void-market"]);
    validation.set_audience(&["void-market-web"]);
    validation.set_required_spec_claims(&["exp", "sub", "iss", "aud", "iat", "jti"]);
    validation.leeway = 0;
    let claims = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)
        .map_err(|_| ApiError::Unauthorized)?
        .claims;
    if claims.iat > now()? || claims.exp <= claims.iat || claims.exp - claims.iat > SESSION_SECONDS
    {
        return Err(ApiError::Unauthorized);
    }
    Ok(claims)
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| ApiError::Internal)
}
