use super::{model::UpdateProfile, repository};
use crate::{auth::sessions::Principal, http::error::Result, state::AppState};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub async fn me(state: web::Data<AppState>, actor: Principal) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::profile(&state.pool, actor.0).await?))
}

pub async fn update(
    state: web::Data<AppState>,
    actor: Principal,
    input: web::Json<UpdateProfile>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .json(repository::update_profile(&state.pool, actor.0, input.into_inner()).await?))
}

pub async fn get(state: web::Data<AppState>, id: web::Path<Uuid>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::get(&state.pool, id.into_inner()).await?))
}
