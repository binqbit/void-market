use crate::{config::Config, database, events, http::router, state::AppState};
use actix_web::{App, HttpServer, middleware::Logger};
use std::{error::Error, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::watch,
};

pub async fn run(command: &str) -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    if command == "healthcheck" {
        return healthcheck(config.listen.port()).await;
    }
    let pool = database::connect(config.database.clone()).await?;
    if command == "migrate" {
        database::MIGRATIONS.run(&pool).await?;
        return Ok(());
    }
    if !database::ready(&pool).await {
        return Err("Database migrations are missing or incompatible; run migrate first".into());
    }

    tokio::fs::create_dir_all(&config.media_root).await?;
    let state = AppState::new(pool, config)?;
    let worker_mode = command == "worker";
    if !worker_mode && command != "serve" {
        return Err("Expected serve, worker, migrate or healthcheck".into());
    }
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = if worker_mode {
        let worker_state = state.clone();
        Some(tokio::spawn(async move {
            events::run(worker_state, stop_rx).await
        }))
    } else {
        None
    };
    let http_state = state.clone();
    let server = HttpServer::new(move || {
        let state = http_state.clone();
        App::new().wrap(Logger::default()).configure(move |config| {
            if worker_mode {
                router::configure_worker(state)(config);
            } else {
                router::configure(state)(config);
            }
        })
    })
    .workers(2)
    .disable_signals()
    .shutdown_timeout(10)
    .bind(state.config.listen)?
    .run();
    let handle = server.handle();
    let signal_stop = stop_tx.clone();
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        let _ = signal_stop.send(true);
        handle.stop(true).await;
    });

    tracing::info!(command, address = %state.config.listen, "market service started");
    let result = server.await;
    let _ = stop_tx.send(true);
    signal.abort();
    if let Some(worker) = worker {
        worker.await??;
    }
    result?;
    state.pool.close().await;
    Ok(())
}

async fn healthcheck(port: u16) -> Result<(), Box<dyn Error>> {
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(
                b"GET /health/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await?;
        let mut buffer = [0; 64];
        let count = stream.read(&mut buffer).await?;
        Ok::<_, std::io::Error>(buffer[..count].starts_with(b"HTTP/1.1 200"))
    })
    .await??;
    if result {
        Ok(())
    } else {
        Err("Service is not ready".into())
    }
}

async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler must be installed");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}
