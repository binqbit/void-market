use arbiter::{config::Config, error::Result, repository::Repository, runtime};
use std::{env, net::SocketAddr, path::Path, process::ExitCode, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[actix_web::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error=%error,"arbiter stopped");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let command = env::args().nth(1).unwrap_or_else(|| "serve".to_owned());
    if command == "--help" || command == "-h" {
        println!(concat!(
            "arbiter <serve|migrate|healthcheck|identity DIRECTORY>\n",
            "Set NODE_ID, DEPLOYMENT_ID, DATABASE_URL, GATEWAY_URL, ARTIFACT_ROOT, ",
            "SIGNING_KEY_FILE and SERVICE_TOKEN_FILE.\n",
            "LOCAL_DEMO=true explicitly enables honest-role mechanics and loopback HTTP."
        ));
        return Ok(());
    }
    if command == "healthcheck" {
        return healthcheck().await;
    }
    if command == "identity" {
        let directory = env::args()
            .nth(2)
            .ok_or(arbiter::error::Error::Configuration)?;
        let deployment =
            env::var("DEPLOYMENT_ID").map_err(|_| arbiter::error::Error::Configuration)?;
        let deployment =
            Uuid::parse_str(&deployment).map_err(|_| arbiter::error::Error::Configuration)?;
        let node = arbiter::identity::initialize(Path::new(&directory), deployment)?;
        println!("Node identity created: {node}. Provisioning record: {directory}/node.json");
        return Ok(());
    }
    let config = Config::from_env()?;
    match command.as_str() {
        "migrate" => {
            let repository = Repository::connect(&config).await?;
            arbiter::repository::MIGRATIONS
                .run(&repository.pool)
                .await
                .map_err(|_| arbiter::error::Error::Configuration)?;
            Ok(())
        }
        "serve" => runtime::serve(config).await,
        _ => Err(arbiter::error::Error::Configuration),
    }
}

async fn healthcheck() -> Result<()> {
    let address = env::var("LISTEN").unwrap_or_else(|_| "127.0.0.1:8090".to_owned());
    let address: SocketAddr = address
        .parse()
        .map_err(|_| arbiter::error::Error::Configuration)?;
    let ready = tokio::time::timeout(Duration::from_secs(2), async {
        let mut stream = TcpStream::connect(("127.0.0.1", address.port())).await?;
        stream
            .write_all(
                b"GET /health/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await?;
        let mut response = [0; 64];
        let count = stream.read(&mut response).await?;
        Ok::<_, std::io::Error>(response[..count].starts_with(b"HTTP/1.1 200"))
    })
    .await
    .map_err(|_| arbiter::error::Error::Gateway)?
    .map_err(|_| arbiter::error::Error::Gateway)?;
    if ready {
        Ok(())
    } else {
        Err(arbiter::error::Error::Gateway)
    }
}
