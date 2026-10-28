use super::{
    drafts,
    model::{CompleteSale, CreateAuction, EditAuction, PlaceBid, PublishAuction},
    repository, service,
};
use crate::{
    auth::sessions::{self, Principal},
    http::{error::Result, pagination::Page},
    state::AppState,
};
use actix_web::{HttpRequest, HttpResponse, web};
use uuid::Uuid;

#[cfg(feature = "demo-mode")]
use super::closing;

pub fn configure(config: &mut web::ServiceConfig) {
    config
        .route("/v1/auctions", web::get().to(list))
        .route("/v1/auctions", web::post().to(create))
        .route("/v1/me/auctions", web::get().to(mine))
        .route("/v1/auctions/{id}", web::get().to(get))
        .route("/v1/auctions/{id}/draft", web::post().to(edit))
        .route("/v1/auctions/{id}/publish", web::post().to(publish))
        .route("/v1/auctions/{id}/cancel", web::post().to(cancel))
        .route("/v1/auctions/{id}/bids", web::post().to(bid))
        .route("/v1/auctions/{id}/withdraw", web::post().to(withdraw))
        .route("/v1/auctions/{id}/complete", web::post().to(complete));

    #[cfg(feature = "demo-mode")]
    config.route(
        "/v1/auctions/{id}/close-bidding",
        web::post().to(close_bidding),
    );
}

async fn list(
    state: web::Data<AppState>,
    request: HttpRequest,
    page: web::Query<Page>,
) -> Result<HttpResponse> {
    let viewer = sessions::optional(&state, request.headers())
        .await?
        .map(|principal| principal.0);
    let (limit, offset) = page.bounds()?;
    Ok(HttpResponse::Ok().json(repository::list(&state, viewer, false, limit, offset).await?))
}

async fn mine(
    state: web::Data<AppState>,
    owner: Principal,
    page: web::Query<Page>,
) -> Result<HttpResponse> {
    let (limit, offset) = page.bounds()?;
    Ok(
        HttpResponse::Ok()
            .json(repository::list(&state, Some(owner.0), true, limit, offset).await?),
    )
}

async fn get(
    state: web::Data<AppState>,
    request: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let viewer = sessions::optional(&state, request.headers())
        .await?
        .map(|principal| principal.0);
    Ok(HttpResponse::Ok().json(repository::view(&state, *id, viewer).await?))
}

async fn create(
    state: web::Data<AppState>,
    owner: Principal,
    body: web::Json<CreateAuction>,
) -> Result<HttpResponse> {
    let id = drafts::create(&state, owner.0, body.into_inner()).await?;
    Ok(HttpResponse::Created().json(repository::view(&state, id, Some(owner.0)).await?))
}

async fn edit(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
    body: web::Json<EditAuction>,
) -> Result<HttpResponse> {
    drafts::edit(&state, owner.0, *id, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(repository::view(&state, *id, Some(owner.0)).await?))
}

async fn publish(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
    body: web::Json<PublishAuction>,
) -> Result<HttpResponse> {
    drafts::publish(&state, owner.0, *id, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(repository::view(&state, *id, Some(owner.0)).await?))
}

async fn cancel(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    drafts::cancel(&state, owner.0, *id).await?;
    Ok(HttpResponse::Ok().json(repository::view(&state, *id, Some(owner.0)).await?))
}

async fn bid(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
    body: web::Json<PlaceBid>,
) -> Result<HttpResponse> {
    service::bid(&state, owner.0, *id, body.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(repository::view(&state, *id, Some(owner.0)).await?))
}

#[cfg(feature = "demo-mode")]
async fn close_bidding(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    closing::close(&state, owner.0, *id).await?;
    Ok(HttpResponse::Ok().json(repository::view(&state, *id, Some(owner.0)).await?))
}

async fn withdraw(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::withdraw(&state, owner.0, *id).await?))
}

async fn complete(
    state: web::Data<AppState>,
    owner: Principal,
    id: web::Path<Uuid>,
    body: web::Json<CompleteSale>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::complete(&state, owner.0, *id, body.into_inner()).await?))
}
