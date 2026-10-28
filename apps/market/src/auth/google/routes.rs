use super::{FLOW_TTL, Flow, provider::response_json};
use crate::{
    auth::sessions,
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::{
    HttpRequest, HttpResponse,
    cookie::{Cookie, SameSite, time::Duration as CookieDuration},
    http::header,
    web,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{TryRngCore, rngs::OsRng};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Instant;
use url::Url;
use uuid::Uuid;

const STATE_COOKIE: &str = "market_google_state";

pub async fn config(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .json(json!({ "google": state.config.google.is_some() }))
}

pub async fn start(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse> {
    let config = state.config.google.as_ref().ok_or(ApiError::Unavailable)?;
    let id = random()?;
    let nonce = random()?;
    let verifier = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    {
        let mut flows = state.google.flows.lock().map_err(|_| ApiError::Internal)?;
        flows.retain(|_, flow| flow.expires > Instant::now());
        if let Some(previous) = request.cookie(STATE_COOKIE) {
            flows.remove(previous.value());
        }
        if flows.len() >= 1024 {
            return Err(ApiError::Unavailable);
        }
        flows.insert(
            id.clone(),
            Flow {
                nonce: nonce.clone(),
                verifier,
                expires: Instant::now() + FLOW_TTL,
            },
        );
    }
    let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|_| ApiError::Internal)?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", redirect_uri(&state)?.as_str()),
        ("response_type", "code"),
        ("scope", "openid email"),
        ("state", &id),
        ("nonce", &nonce),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("prompt", "select_account"),
    ]);
    Ok(HttpResponse::SeeOther()
        .insert_header((header::LOCATION, url.as_str()))
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .cookie(state_cookie(&id, state.config.secure_cookie, 300))
        .finish())
}

#[derive(Deserialize)]
pub struct Callback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

pub async fn callback(
    state: web::Data<AppState>,
    request: HttpRequest,
    query: web::Query<Callback>,
) -> HttpResponse {
    let result = finish(&state, &request, &query).await;
    let mut response = HttpResponse::SeeOther();
    response.insert_header((header::CACHE_CONTROL, "no-store"));
    response.insert_header(("Referrer-Policy", "no-referrer"));
    response.cookie(state_cookie("", state.config.secure_cookie, 0));
    match result {
        Ok(cookie) => {
            response
                .insert_header((header::LOCATION, "/account"))
                .append_header((header::SET_COOKIE, cookie));
        }
        Err(_) => {
            response.insert_header((header::LOCATION, "/login?error=google"));
        }
    }
    response.finish()
}

async fn finish(
    state: &AppState,
    request: &HttpRequest,
    query: &Callback,
) -> Result<header::HeaderValue> {
    let config = state.config.google.as_ref().ok_or(ApiError::Unavailable)?;
    let id = query.state.as_deref().ok_or(ApiError::Unauthorized)?;
    if request.cookie(STATE_COOKIE).as_ref().map(Cookie::value) != Some(id) {
        return Err(ApiError::Unauthorized);
    }
    let flow = state
        .google
        .flows
        .lock()
        .map_err(|_| ApiError::Internal)?
        .remove(id)
        .ok_or(ApiError::Unauthorized)?;
    if flow.expires <= Instant::now() || query.error.is_some() {
        return Err(ApiError::Unauthorized);
    }
    let code = query.code.as_deref().ok_or(ApiError::Unauthorized)?;
    let response = state
        .google
        .provider
        .client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("redirect_uri", &redirect_uri(state)?),
            ("grant_type", "authorization_code"),
            ("code_verifier", &flow.verifier),
        ])
        .send()
        .await
        .map_err(|_| ApiError::Unavailable)?;
    let tokens = response_json::<TokenResponse>(response).await?;
    let identity = state
        .google
        .provider
        .verify(&tokens.id_token, &config.client_id, &flow.nonce)
        .await?;

    let mut tx = state.pool.begin().await?;
    // A Google subject identifies the account. Matching email alone never links identities.
    let account: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts(id, name, google_subject, email) VALUES($1, $2, $3, lower($4))
         ON CONFLICT (google_subject) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind("New member")
    .bind(identity.sub)
    .bind(identity.email)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(custody) = &state.custody {
        custody.provision(&mut tx, account).await?;
    }
    let cookie = sessions::issue(
        &mut tx,
        account,
        state.config.secure_cookie,
        &state.config.jwt_secret,
    )
    .await?;
    tx.commit().await?;
    Ok(cookie)
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

fn redirect_uri(state: &AppState) -> Result<String> {
    state
        .config
        .origin
        .join("/api/v1/auth/google/callback")
        .map(|url| url.to_string())
        .map_err(|_| ApiError::Internal)
}

fn state_cookie(value: &str, secure: bool, seconds: i64) -> Cookie<'static> {
    Cookie::build(STATE_COOKIE, value.to_owned())
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::seconds(seconds))
        .finish()
}

fn random() -> Result<String> {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| ApiError::Internal)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
