use super::{error::ApiError, health};
use crate::{accounts, assets, auth, media, state::AppState, transfers};
use actix_cors::Cors;
use actix_web::{
    Error, ResponseError,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::{Method, header},
    middleware::{DefaultHeaders, Next, from_fn},
    web,
};

pub fn configure(state: AppState) -> impl FnOnce(&mut web::ServiceConfig) {
    move |config| {
        let origin = state.config.origin.origin().ascii_serialization();
        let cors = Cors::default()
            .allowed_origin(&origin)
            .supports_credentials()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![header::CONTENT_TYPE]);
        let json = web::JsonConfig::default()
            .limit(64 * 1024)
            .error_handler(|_, _| ApiError::BadRequest("Invalid JSON request").into());
        config.app_data(web::Data::new(state.clone()));
        config.configure(crate::nodes::configure);
        config.service(
            web::scope("")
                .configure(crate::demo::configure)
                .configure(crate::settlement::configure)
                .configure(crate::auctions::configure)
                .configure(crate::messages::configure)
                .app_data(web::Data::new(state))
                .app_data(json)
                .app_data(
                    web::QueryConfig::default().error_handler(|_, _| {
                        ApiError::BadRequest("Invalid query parameters").into()
                    }),
                )
                .wrap(from_fn(check_origin))
                .wrap(DefaultHeaders::new().add((header::CACHE_CONTROL, "no-store")))
                .wrap(cors)
                .route("/health/live", web::get().to(health::live))
                .route("/health/ready", web::get().to(health::ready))
                .route("/v1/auth/config", web::get().to(auth::google::config))
                .route("/v1/auth/google/start", web::get().to(auth::google::start))
                .route(
                    "/v1/auth/google/callback",
                    web::get().to(auth::google::callback),
                )
                .route(
                    "/v1/auth/registration/options",
                    web::post().to(auth::routes::register_options),
                )
                .route(
                    "/v1/auth/registration/finish",
                    web::post().to(auth::routes::register_finish),
                )
                .route(
                    "/v1/auth/login/options",
                    web::post().to(auth::routes::login_options),
                )
                .route(
                    "/v1/auth/login/finish",
                    web::post().to(auth::routes::login_finish),
                )
                .route("/v1/auth/logout", web::post().to(auth::routes::logout))
                .route("/v1/me", web::get().to(accounts::routes::me))
                .route("/v1/me/profile", web::post().to(accounts::routes::update))
                .route("/v1/accounts/{id}", web::get().to(accounts::routes::get))
                .route("/v1/assets", web::post().to(assets::routes::register))
                .route("/v1/assets/{id}", web::get().to(assets::routes::get))
                .route("/v1/me/assets", web::get().to(assets::routes::owned))
                .route(
                    "/v1/assets/{id}/transfers",
                    web::post().to(transfers::routes::create),
                )
                .route("/v1/transfers/{id}", web::get().to(transfers::routes::get))
                .route("/v1/me/transfers", web::get().to(transfers::routes::mine))
                .route("/v1/media", web::post().to(media::upload))
                .route("/v1/media/{name}", web::get().to(media::get)),
        );
    }
}

pub fn configure_worker(state: AppState) -> impl FnOnce(&mut web::ServiceConfig) {
    move |config| {
        config
            .app_data(web::Data::new(state))
            .route("/health/live", web::get().to(health::live))
            .route("/health/ready", web::get().to(health::worker_ready));
    }
}

async fn check_origin<B: MessageBody + 'static>(
    request: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error> {
    let method = request.method();
    if method != Method::GET && method != Method::HEAD && method != Method::OPTIONS {
        let actual = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        let expected = request
            .app_data::<web::Data<AppState>>()
            .map(|state| state.config.origin.origin().ascii_serialization());
        if actual != expected.as_deref() || expected.is_none() {
            return Ok(request
                .into_response(ApiError::Forbidden.error_response())
                .map_into_right_body());
        }
    }
    Ok(next.call(request).await?.map_into_left_body())
}
