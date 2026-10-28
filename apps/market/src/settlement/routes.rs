use super::{model::ApproveOperation, service};
use crate::{auth::sessions::Principal, http::error::Result, state::AppState};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub fn configure(config: &mut web::ServiceConfig) {
    config
        .route("/v1/me/transactions", web::get().to(list))
        .route("/v1/transactions/{id}", web::get().to(get))
        .route("/v1/transactions/{id}/approve", web::post().to(approve));
    config
        .route("/v1/assets/{id}/native-transfer", web::post().to(transfer))
        .route("/v1/transactions/{id}/cancel", web::post().to(cancel))
        .route("/v1/auctions/{id}/payments", web::post().to(payment));
}

async fn transfer(
    state: web::Data<AppState>,
    account: Principal,
    id: web::Path<Uuid>,
    input: web::Json<super::transfers::NativeTransfer>,
) -> Result<HttpResponse> {
    let operation = super::transfers::prepare(&state, account.0, *id, input.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(service::view(&state, account.0, operation).await?))
}
async fn cancel(
    state: web::Data<AppState>,
    account: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    super::transfers::cancel(&state, account.0, *id).await?;
    Ok(HttpResponse::Ok().json(service::view(&state, account.0, *id).await?))
}
async fn payment(
    state: web::Data<AppState>,
    account: Principal,
    id: web::Path<Uuid>,
    input: web::Json<super::payments::PaymentRequest>,
) -> Result<HttpResponse> {
    let operation = super::payments::prepare(&state, account.0, *id, input.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(service::view(&state, account.0, operation).await?))
}

async fn list(
    state: web::Data<AppState>,
    account: Principal,
    query: web::Query<crate::http::pagination::Page>,
) -> Result<HttpResponse> {
    let (limit, offset) = query.bounds()?;
    Ok(HttpResponse::Ok().json(service::list_page(&state, account.0, limit, offset).await?))
}
async fn get(
    state: web::Data<AppState>,
    account: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::view(&state, account.0, *id).await?))
}
async fn approve(
    state: web::Data<AppState>,
    account: Principal,
    id: web::Path<Uuid>,
    input: web::Json<ApproveOperation>,
) -> Result<HttpResponse> {
    service::approve(&state, account.0, *id, input.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(service::view(&state, account.0, *id).await?))
}
