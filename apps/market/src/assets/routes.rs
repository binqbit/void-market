use super::{model::RegisterAsset, repository, service};
use crate::{
    auth::sessions::Principal,
    http::{error::Result, pagination::Page},
    state::AppState,
};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub async fn register(
    state: web::Data<AppState>,
    actor: Principal,
    input: web::Json<RegisterAsset>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Created().json(service::register(&state, actor.0, input.into_inner()).await?))
}

pub async fn get(state: web::Data<AppState>, id: web::Path<Uuid>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::get(&state.pool, id.into_inner()).await?))
}

pub async fn owned(
    state: web::Data<AppState>,
    actor: Principal,
    page: web::Query<Page>,
) -> Result<HttpResponse> {
    let (limit, offset) = page.bounds()?;
    Ok(HttpResponse::Ok().json(repository::owned(&state.pool, actor.0, limit, offset).await?))
}
