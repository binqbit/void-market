use super::{model::TransferAsset, repository, service};
use crate::{
    auth::sessions::Principal,
    http::{error::Result, pagination::Page},
    state::AppState,
};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub async fn create(
    state: web::Data<AppState>,
    actor: Principal,
    id: web::Path<Uuid>,
    input: web::Json<TransferAsset>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .json(service::transfer(&state, actor.0, id.into_inner(), input.into_inner()).await?))
}

pub async fn get(
    state: web::Data<AppState>,
    actor: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .json(repository::get_for_account(&state.pool, id.into_inner(), actor.0).await?))
}

pub async fn mine(
    state: web::Data<AppState>,
    actor: Principal,
    page: web::Query<Page>,
) -> Result<HttpResponse> {
    let (limit, offset) = page.bounds()?;
    Ok(HttpResponse::Ok()
        .json(repository::list_for_account(&state.pool, actor.0, limit, offset).await?))
}
