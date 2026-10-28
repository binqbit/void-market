use super::tokens;
use crate::{
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::{
    FromRequest, HttpRequest,
    dev::Payload,
    http::header::{self, HeaderMap, HeaderValue},
    web,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::{future::Future, pin::Pin};
use uuid::Uuid;

pub const COOKIE_NAME: &str = "market_session";

pub struct Principal(pub Uuid);

impl FromRequest for Principal {
    type Error = ApiError;
    type Future = Pin<Box<dyn Future<Output = Result<Self>>>>;

    fn from_request(request: &HttpRequest, _: &mut Payload) -> Self::Future {
        let state = request.app_data::<web::Data<AppState>>().cloned();
        let headers = request.headers().clone();
        Box::pin(async move {
            let state = state.ok_or(ApiError::Internal)?;
            optional(&state, &headers)
                .await?
                .ok_or(ApiError::Unauthorized)
        })
    }
}

pub async fn optional(state: &AppState, headers: &HeaderMap) -> Result<Option<Principal>> {
    let Some(token) = token(headers) else {
        return Ok(None);
    };
    let claims = tokens::verify(&token, &state.config.jwt_secret)?;
    let account: Option<Uuid> = sqlx::query_scalar(
        "SELECT s.account_id
         FROM sessions s JOIN accounts a ON a.id=s.account_id
         WHERE s.token_hash = $1 AND s.expires_at > now() AND (NOT a.is_demo OR $2)",
    )
    .bind(token_hash(&token)?)
    .bind(state.chains.is_some())
    .fetch_optional(&state.pool)
    .await?;
    account
        .filter(|account| *account == claims.sub)
        .map(Principal)
        .map(Some)
        .ok_or(ApiError::Unauthorized)
}

pub async fn issue(
    connection: &mut PgConnection,
    account: Uuid,
    secure: bool,
    secret: &[u8],
) -> Result<HeaderValue> {
    let token = tokens::issue(account, secret)?;

    sqlx::query(
        "INSERT INTO sessions(token_hash, account_id, expires_at)
         VALUES ($1, $2, now() + interval '7 days')",
    )
    .bind(token_hash(&token)?)
    .bind(account)
    .execute(connection)
    .await?;
    cookie(&token, 604800, secure)
}

pub async fn revoke(state: &AppState, headers: &HeaderMap) -> Result<HeaderValue> {
    if let Some(token) = token(headers) {
        sqlx::query(
            "DELETE
         FROM sessions
         WHERE token_hash = $1",
        )
        .bind(token_hash(&token)?)
        .execute(&state.pool)
        .await?;
    }
    cookie("", 0, state.config.secure_cookie)
}

fn token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == COOKIE_NAME).then(|| value.to_owned())
        })
}

fn token_hash(token: &str) -> Result<Vec<u8>> {
    if token.is_empty() || token.len() > 4096 {
        return Err(ApiError::Unauthorized);
    }
    Ok(Sha256::digest(token.as_bytes()).to_vec())
}

fn cookie(token: &str, max_age: u32, secure: bool) -> Result<HeaderValue> {
    let suffix = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{suffix}"
    ))
    .map_err(|_| ApiError::Internal)
}
