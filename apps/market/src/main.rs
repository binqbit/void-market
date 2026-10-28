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
        println!(concat!(
            "market <serve|worker|migrate|healthcheck>\n",
            "Configuration:\n",
            "  DATABASE_URL or DATABASE_* and DATABASE_PASSWORD_FILE\n",
            "  LISTEN; ORIGIN; RP_ID; MEDIA_ROOT; KAFKA_BROKERS; EVENT_TOPIC\n",
            "  JWT_SECRET or JWT_SECRET_FILE\n",
            "  GOOGLE_CLIENT_ID; GOOGLE_CLIENT_SECRET or GOOGLE_CLIENT_SECRET_FILE"
        ));
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
