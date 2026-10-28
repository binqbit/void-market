use std::{env, process::ExitCode};
use tracing_subscriber::EnvFilter;

#[actix_web::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();
    let command = env::args().nth(1).unwrap_or_else(|| "serve".to_owned());
    if command == "--help" || command == "-h" {
        println!(
            "market <serve|worker|migrate|healthcheck>\nConfiguration: MARKET_DATABASE_URL or MARKET_DATABASE_* and MARKET_DATABASE_PASSWORD_FILE; MARKET_LISTEN; MARKET_ORIGIN; MARKET_RP_ID; MARKET_MEDIA_ROOT; MARKET_KAFKA_BROKERS; MARKET_EVENT_TOPIC"
        );
        return ExitCode::SUCCESS;
    }
    match market::startup::run(&command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %error, "market service stopped");
            ExitCode::FAILURE
        }
    }
}
