use crate::{
    artifacts::Artifacts,
    client::Gateway,
    crypto::{Assurance, CheckManifest, ComparisonEngine, Progress, Response},
    delivery::{deliver, interrupt},
    error::{Error, Result},
    repository::Repository,
};
use arbiter_protocol::{ArtifactRef, Completion, Job, JobStatus};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use uuid::Uuid;

struct RunningCheck {
    engine: ComparisonEngine,
    _permit: OwnedSemaphorePermit,
}

pub(crate) struct Worker {
    pub repository: Repository,
    pub gateway: Gateway,
    pub artifacts: Artifacts,
    pub semaphore: Arc<Semaphore>,
    pub allow_unverified_mechanics: bool,
}

pub(crate) async fn execute(worker: Worker, job: Job, stop: watch::Receiver<bool>) -> Result<()> {
    let alive = Arc::new(AtomicBool::new(true));
    let lease_alive = alive.clone();
    let lease_gateway = worker.gateway.clone();
    let lease_job = job.clone();
    let lease = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(20));
        loop {
            interval.tick().await;
            if lease_gateway.renew(&lease_job).await.is_err() {
                lease_alive.store(false, Ordering::Relaxed);
                break;
            }
        }
    });
    let result = run_check(&worker, &job, alive, stop).await;
    lease.abort();
    result
}

async fn run_check(
    worker: &Worker,
    job: &Job,
    alive: Arc<AtomicBool>,
    mut stop: watch::Receiver<bool>,
) -> Result<()> {
    let repository = &worker.repository;
    let gateway = &worker.gateway;
    let artifacts = worker.artifacts.clone();
    let allow_unverified_mechanics = worker.allow_unverified_mechanics;
    if *stop.borrow() || !alive.load(Ordering::Relaxed) {
        return interrupt(repository, gateway, job).await;
    }
    let permit = worker
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Error::Ownership)?;
    let input = job.input.clone();
    let store = artifacts.clone();
    let start = tokio::task::spawn_blocking(move || {
        let manifest: CheckManifest = store.record(&input)?;
        if matches!(manifest.assurance, Assurance::HonestRoleMechanics)
            && !allow_unverified_mechanics
        {
            return Err(Error::ProofUnavailable);
        }
        let (engine, progress) = ComparisonEngine::start(store, manifest)?;
        Ok::<_, Error>((
            RunningCheck {
                engine,
                _permit: permit,
            },
            progress,
        ))
    })
    .await
    .map_err(|_| Error::Ownership)?;
    let (mut running, mut progress) = match start {
        Ok(result) => result,
        Err(error) => {
            let status = if matches!(error, Error::ProofUnavailable) {
                JobStatus::Unavailable
            } else {
                JobStatus::Failed
            };
            let result = completion(job, status, None, None);
            deliver(repository, gateway, &result).await?;
            return Ok(());
        }
    };
    // Human approval budgets are metered by the market; infrastructure waits do not spend them.
    let deadline = (!job.interactive).then(|| Instant::now() + Duration::from_secs(600));
    let mut consumed = None;
    loop {
        if *stop.borrow()
            || !alive.load(Ordering::Relaxed)
            || deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            drop(running);
            return interrupt(repository, gateway, job).await;
        }
        let (status, output) = match progress {
            Progress::Waiting(value) => (JobStatus::Waiting, value),
            Progress::Complete(value) => (JobStatus::Completed, value),
        };
        let result = completion(job, status, Some(output), consumed);
        deliver(repository, gateway, &result).await?;
        if status == JobStatus::Completed {
            return Ok(());
        }
        let response = loop {
            if *stop.borrow()
                || !alive.load(Ordering::Relaxed)
                || deadline.is_some_and(|deadline| Instant::now() >= deadline)
            {
                drop(running);
                return interrupt(repository, gateway, job).await;
            }
            let current = gateway.job(job.id).await?;
            if current.cancel_requested {
                drop(running);
                return interrupt(repository, gateway, job).await;
            }
            if current.generation != job.generation {
                return Err(Error::Ownership);
            }
            if let Some(response) = current.response {
                break response;
            }
            tokio::select! {
                _ = stop.changed() => {},
                _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            }
        };
        consumed = Some(response.id);
        let store = artifacts.clone();
        let next = tokio::task::spawn_blocking(move || {
            let response: Response = store.record(&response)?;
            let progress = running.engine.advance(response)?;
            Ok::<_, Error>((running, progress))
        })
        .await
        .map_err(|_| Error::Ownership)?;
        match next {
            Ok((state, next)) => {
                running = state;
                progress = next;
            }
            Err(_) => {
                deliver(
                    repository,
                    gateway,
                    &completion(job, JobStatus::Failed, None, consumed),
                )
                .await?;
                return Ok(());
            }
        }
    }
}

fn completion(
    job: &Job,
    status: JobStatus,
    output: Option<ArtifactRef>,
    consumed_response: Option<Uuid>,
) -> Completion {
    Completion {
        deployment_id: Uuid::nil(),
        node_id: job.node_id,
        instance_id: job.instance_id.unwrap_or(Uuid::nil()),
        signature: String::new(),
        id: Uuid::new_v4(),
        job_id: job.id,
        generation: job.generation,
        status,
        output,
        consumed_response,
    }
}
