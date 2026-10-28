use super::{
    model::{Login, ResourceRequest, actor_id},
    repository, resources,
};
use crate::{
    auth::sessions::{self, Principal},
    http::error::{ApiError, Result},
    settlement,
    state::AppState,
};
use actix_web::{HttpRequest, HttpResponse, http::header, web};
use serde_json::{Value, json};

pub fn configure(config: &mut web::ServiceConfig) {
    config
        .route("/v1/demo", web::get().to(status))
        .route("/v1/demo/login", web::post().to(login))
        .route("/v1/demo/zec", web::post().to(zec))
        .route("/v1/demo/nft", web::post().to(nft))
        .route(
            "/v1/demo/images/{variant}",
            web::get().to(super::images::get),
        )
        .route("/v1/demo/nfts/{token}/metadata", web::get().to(metadata));
}

async fn status(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse> {
    let chains = state.chains.as_ref().ok_or(ApiError::NotFound)?;
    let actors: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object('account_id',d.account_id,'alias',d.alias,'name',a.name) FROM
         demo_accounts d JOIN accounts a ON a.id=d.account_id ORDER BY d.alias",
    )
    .fetch_all(&state.pool)
    .await?;
    let account = sessions::optional(&state, request.headers()).await?;
    let mut own = Value::Null;
    if let Some(account) = account
        && let Ok(actor) = repository::actor(&state.pool, account.0).await
    {
        let balance = chains
            .zcash
            .balance(actor.zcash_account.ok_or(ApiError::Unavailable)?)
            .await;
        let signing_ready: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM custody_keys WHERE account_id=$1 AND he_status='ready')",
        )
        .bind(account.0)
        .fetch_one(&state.pool)
        .await?;
        own = json!({"alias":actor.alias,"ethereum_address":actor.ethereum_address,"zcash_address":actor.zcash_address,
                "balance":balance.as_ref().ok().copied().map(crate::auctions::amount::format),"wallet_ready":balance.is_ok() && signing_ready});
    }
    let deployment = repository::deployment(&state.pool).await?;
    Ok(HttpResponse::Ok().json(json!({"mode":"localnet","actors":actors,"own":own,"ethereum_chain_id":31337,
        "collection":deployment.collection_address,"zcash_network":"Ironwood regtest","claim_amount":"10.00000000"})))
}

async fn login(
    state: web::Data<AppState>,
    input: web::Json<Login>,
    request: HttpRequest,
) -> Result<HttpResponse> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let id = actor_id(&input.actor).ok_or(ApiError::BadRequest("Choose Alice, Bob or Daniel"))?;
    repository::actor(&state.pool, id).await?;
    let _ = sessions::revoke(&state, request.headers()).await?;
    let mut tx = state.pool.begin().await?;
    let cookie = sessions::issue(
        &mut tx,
        id,
        state.config.secure_cookie,
        &state.config.jwt_secret,
    )
    .await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok()
        .insert_header((header::SET_COOKIE, cookie))
        .json(json!({"signed_in":true})))
}

async fn zec(
    state: web::Data<AppState>,
    account: Principal,
    input: web::Json<ResourceRequest>,
) -> Result<HttpResponse> {
    resources::request(&state, account.0, input.request_id, "airdrop").await?;
    Ok(HttpResponse::Accepted()
        .json(settlement::service::view(&state, account.0, input.request_id).await?))
}
async fn nft(
    state: web::Data<AppState>,
    account: Principal,
    input: web::Json<ResourceRequest>,
) -> Result<HttpResponse> {
    resources::request(&state, account.0, input.request_id, "mint").await?;
    Ok(HttpResponse::Accepted()
        .json(settlement::service::view(&state, account.0, input.request_id).await?))
}
async fn metadata(state: web::Data<AppState>, token: web::Path<String>) -> Result<HttpResponse> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let number = token.parse::<u128>().map_err(|_| ApiError::NotFound)?;
    if number.to_string() != *token {
        return Err(ApiError::NotFound);
    }
    let asset = crate::assets::repository::get(&state.pool, uuid::Uuid::from_u128(number)).await?;
    if asset.nft.is_none() {
        return Err(ApiError::NotFound);
    }
    let variant = asset
        .image
        .as_deref()
        .and_then(|image| image.strip_prefix("demo:"))
        .ok_or(ApiError::NotFound)?;
    Ok(HttpResponse::Ok().json(json!({
        "name": asset.name,
        "description": asset.description,
        "image": format!("{}api/v1/demo/images/{variant}", state.config.origin),
    })))
}
