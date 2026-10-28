use crate::{
    artifacts::Artifacts,
    client::Gateway,
    config::Config,
    delivery::{interrupt, recover},
    error::{Error, Result},
    repository::Repository,
    worker::{Worker, execute},
};
use actix_web::{App, HttpResponse, HttpServer, web};
use arbiter_protocol::JobStatus;
use sqlx::PgPool;
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, watch},
    task::JoinSet,
};

struct Health {
    ready: Arc<AtomicBool>,
    pool: PgPool,
}

async fn live() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"live":true}))
}

async fn ready(state: web::Data<Health>) -> HttpResponse {
    let database = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query("SELECT 1").execute(&state.pool),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    let ready = database && state.ready.load(Ordering::Relaxed);
    let mut response = if ready {
        HttpResponse::Ok()
    } else {
        HttpResponse::ServiceUnavailable()
    };
    response.json(serde_json::json!({"ready":ready}))
}

pub async fn serve(config: Config) -> Result<()> {
    let (stop, signal) = watch::channel(false);
    let listener = tokio::spawn(async move {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
        let _ = stop.send(true);
    });
    let result = serve_until(config, signal).await;
    listener.abort();
    result
}

/// Runs the same node with an explicit supervisor shutdown signal.
pub async fn serve_until(config: Config, stop_rx: watch::Receiver<bool>) -> Result<()> {
    let repository = Repository::connect(&config).await?;
    let mut ownership = repository.pool.acquire().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 0))")
        .bind(config.node_id.to_string())
        .fetch_one(&mut *ownership)
        .await?;
    if !locked {
        return Err(Error::Ownership);
    }
    let instance = repository
        .identity(&arbiter_protocol::public_key(&config.signing_key))
        .await?;
    let gateway = Gateway::new(&config, instance)?;
    gateway
        .register(
            config.capacity,
            config.session_capacity,
            repository.sequence().await?,
        )
        .await?;
    let artifacts = Artifacts::open(&config.artifacts)?;
    let ready_flag = Arc::new(AtomicBool::new(false));
    let health = web::Data::new(Health {
        ready: ready_flag.clone(),
        pool: repository.pool.clone(),
    });
    let server = HttpServer::new(move || {
        App::new()
            .app_data(health.clone())
            .route("/health/live", web::get().to(live))
            .route("/health/ready", web::get().to(ready))
    })
    .workers(1)
    .disable_signals()
    .shutdown_timeout(5)
    .bind(config.listen)
    .map_err(|_| Error::Configuration)?
    .run();
    let handle = server.handle();
    let coordinator = coordinate(
        repository,
        gateway,
        artifacts,
        Capacity {
            computations: config.capacity,
            sessions: config.session_capacity,
        },
        config.allow_unverified_mechanics,
        ready_flag.clone(),
        stop_rx,
    );
    let server_task = tokio::spawn(server);
    let result = coordinator.await;
    ready_flag.store(false, Ordering::Relaxed);
    handle.stop(true).await;
    let _ = server_task.await;
    ownership.close().await?;
    result
}

struct Capacity {
    computations: usize,
    sessions: usize,
}

async fn coordinate(
    repository: Repository,
    gateway: Gateway,
    artifacts: Artifacts,
    capacity: Capacity,
    allow_unverified_mechanics: bool,
    ready: Arc<AtomicBool>,
    mut stop: watch::Receiver<bool>,
) -> Result<()> {
    recover(&repository, &gateway).await?;
    ready.store(true, Ordering::Relaxed);
    let semaphore = Arc::new(Semaphore::new(capacity.computations));
    let (work_stop, work_signal) = watch::channel(false);
    let mut workers = JoinSet::new();
    let mut active = HashSet::new();
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut control_lease_valid = true;
    let mut failure = None;

    'running: loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = heartbeat.tick() => {
                let sequence = match repository.sequence().await {
                    Ok(sequence) => sequence,
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                };
                let result = gateway.heartbeat(capacity.computations, capacity.sessions, sequence).await;
                control_lease_valid = result.is_ok();
                ready.store(control_lease_valid, Ordering::Relaxed);
            }
            _ = poll.tick() => {
                let jobs = match gateway.jobs().await {
                    Ok(jobs) => {
                        ready.store(control_lease_valid, Ordering::Relaxed);
                        jobs
                    }
                    Err(_) => {
                        ready.store(false, Ordering::Relaxed);
                        continue;
                    }
                };

                if !control_lease_valid {
                    continue;
                }

                // A pre-restart claim can arrive after startup recovery has finished.
                // Without live masks, this node must close that attempt explicitly.
                for abandoned in jobs.iter().filter(|job| {
                    matches!(job.status, JobStatus::Running | JobStatus::Waiting)
                        && !active.contains(&job.id)
                }) {
                    let result = async {
                        repository.started(abandoned).await?;
                        interrupt(&repository, &gateway, abandoned).await
                    }
                    .await;
                    if let Err(error) = result {
                        failure = Some(error);
                        break 'running;
                    }
                }

                for candidate in jobs.into_iter().filter(|job| job.status == JobStatus::Queued) {
                    if active.len() >= capacity.sessions {
                        break;
                    }
                    if active.contains(&candidate.id) {
                        continue;
                    }
                    let job = match gateway.claim(&candidate).await {
                        Ok(job) => job,
                        Err(_) => continue,
                    };
                    if let Err(error) = repository.started(&job).await {
                        failure = Some(error);
                        break 'running;
                    }

                    active.insert(job.id);
                    let worker = Worker {
                        gateway: gateway.clone(),
                        repository: repository.clone(),
                        artifacts: artifacts.clone(),
                        semaphore: semaphore.clone(),
                        allow_unverified_mechanics,
                    };
                    let stop = work_signal.clone();
                    workers.spawn(async move {
                        let id = job.id;
                        (id, execute(worker, job, stop).await)
                    });
                }
            }
            result = workers.join_next(), if !workers.is_empty() => {
                match result {
                    Some(Ok((id, Ok(())))) => {
                        active.remove(&id);
                    }
                    Some(Ok((id, Err(error)))) => {
                        active.remove(&id);
                        failure = Some(error);
                        break;
                    }
                    _ => {
                        failure = Some(Error::Ownership);
                        break;
                    }
                }
            }
        }
    }

    ready.store(false, Ordering::Relaxed);
    let _ = work_stop.send(true);
    while let Some(result) = workers.join_next().await {
        if let Ok((_, Err(error))) = result {
            failure = Some(error);
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
