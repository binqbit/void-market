use crate::{database, state::AppState};
use actix_web::{HttpResponse, web};
use serde_json::json;
use std::sync::atomic::Ordering;

pub async fn live() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status":"ok"}))
}

pub async fn ready(state: web::Data<AppState>) -> HttpResponse {
    response(database::ready(&state.pool).await)
}

pub async fn worker_ready(state: web::Data<AppState>) -> HttpResponse {
    response(state.worker_ready.load(Ordering::Relaxed) && database::ready(&state.pool).await)
}

fn response(ready: bool) -> HttpResponse {
    if ready {
        HttpResponse::Ok().json(json!({"ready":true}))
    } else {
        HttpResponse::ServiceUnavailable().json(json!({"ready":false}))
    }
}
