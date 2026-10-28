use super::repository;
use crate::{auth::sessions::Principal, http::error::Result, state::AppState};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub async fn me(state: web::Data<AppState>, actor: Principal) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::get(&state.pool, actor.0).await?))
}

pub async fn get(state: web::Data<AppState>, id: web::Path<Uuid>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::get(&state.pool, id.into_inner()).await?))
}
