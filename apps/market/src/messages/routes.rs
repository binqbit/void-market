use super::{model::Approve, repository};
use crate::{
    auth::sessions::{self, Principal},
    http::error::{ApiError, Result},
    state::AppState,
};
use actix_web::{HttpRequest, HttpResponse, web};
use futures_util::stream;
use std::time::Duration;
use uuid::Uuid;

pub fn configure(config: &mut web::ServiceConfig) {
    config
        .route("/v1/messages", web::get().to(list))
        .route("/v1/messages/events", web::get().to(events))
        .route("/v1/messages/{id}/approve", web::post().to(approve))
        .route(
            "/v1/approval-sessions/{id}/withdraw",
            web::post().to(withdraw),
        );
}

async fn list(state: web::Data<AppState>, principal: Principal) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::snapshot(&state, principal.0).await?))
}

async fn approve(
    state: web::Data<AppState>,
    principal: Principal,
    id: web::Path<Uuid>,
    body: web::Json<Approve>,
) -> Result<HttpResponse> {
    let receipt = repository::approve(&state, principal.0, *id, body.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(receipt))
}

async fn withdraw(
    state: web::Data<AppState>,
    principal: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(repository::withdraw(&state, principal.0, *id).await?))
}

async fn events(
    state: web::Data<AppState>,
    principal: Principal,
    request: HttpRequest,
) -> Result<HttpResponse> {
    let permit = state
        .message_streams
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::Unavailable)?;
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let initial = (
        state,
        principal.0,
        request.headers().clone(),
        permit,
        None,
        interval,
    );
    let events = stream::unfold(
        initial,
        |(state, account, headers, permit, previous, mut interval)| async move {
            interval.tick().await;
            let authorized = sessions::optional(&state, &headers)
                .await
                .ok()
                .flatten()
                .is_some_and(|principal| principal.0 == account);
            if !authorized {
                return None;
            }
            let revision = repository::revision(&state.pool, account).await.ok();
            let data = match revision {
                Some(revision) if previous != Some(revision) => format!(
                    "event: messages\nid: {revision}\ndata: {{\"revision\":{revision}}}\n\n"
                ),
                Some(_) => ": keep-alive\n\n".to_owned(),
                None => "event: unavailable\ndata: {}\n\n".to_owned(),
            };
            Some((
                Ok::<_, actix_web::Error>(web::Bytes::from(data)),
                (state, account, headers, permit, revision, interval),
            ))
        },
    );
    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(events))
}
