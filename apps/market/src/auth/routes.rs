use super::{
    model::{LoginFinish, LoginOptions, RegisterFinish, RegisterOptions},
    service, sessions,
};
use crate::{http::error::Result, state::AppState};
use actix_web::{HttpRequest, HttpResponse, http::header, web};

pub async fn register_options(
    state: web::Data<AppState>,
    input: web::Json<RegisterOptions>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::register_options(&state, input.into_inner()).await?))
}

pub async fn register_finish(
    state: web::Data<AppState>,
    input: web::Json<RegisterFinish>,
) -> Result<HttpResponse> {
    let (cookie, account) = service::register_finish(&state, input.into_inner()).await?;
    Ok(HttpResponse::Ok()
        .insert_header((header::SET_COOKIE, cookie))
        .json(account))
}

pub async fn login_options(
    state: web::Data<AppState>,
    input: web::Json<LoginOptions>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::login_options(&state, input.into_inner()).await?))
}

pub async fn login_finish(
    state: web::Data<AppState>,
    input: web::Json<LoginFinish>,
) -> Result<HttpResponse> {
    let (cookie, account) = service::login_finish(&state, input.into_inner()).await?;
    Ok(HttpResponse::Ok()
        .insert_header((header::SET_COOKIE, cookie))
        .json(account))
}

pub async fn logout(state: web::Data<AppState>, request: HttpRequest) -> Result<HttpResponse> {
    let cookie = sessions::revoke(&state, request.headers()).await?;
    Ok(HttpResponse::NoContent()
        .insert_header((header::SET_COOKIE, cookie))
        .finish())
}
