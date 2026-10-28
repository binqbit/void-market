use crate::{
    artifacts::Artifacts,
    client::Gateway,
    crypto::{Assurance, CheckManifest, ComparisonEngine, Progress, Response},
    delivery::{deliver, interrupt},
    error::{Error, Result},
    repository::Repository,
};
use arbiter_protocol::{ArtifactRef, Completion, DemoRelease, Job, JobStatus};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    task::JoinHandle,
};
use uuid::Uuid;

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
    if *stop.borrow() || !alive.load(Ordering::Relaxed) {
        return interrupt(repository, gateway, job).await;
    }

    let permit = tokio::select! {
        permit = worker.semaphore.clone().acquire_owned() => permit.map_err(|_| Error::Ownership)?,
        _ = stop.changed() => return interrupt(repository, gateway, job).await,
    };
    let start = start_check(worker, job.input.clone(), permit)
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
        let accepted = deliver(repository, gateway, &result).await?;
        if accepted.status.terminal() {
            return Ok(());
        }

        let response = wait_for_response(gateway, job, &alive, &mut stop, deadline).await?;
        let Some(response) = response else {
            drop(running);
            return interrupt(repository, gateway, job).await;
        };

        consumed = Some(response.reference.id);
        let permit = tokio::select! {
            permit = worker.semaphore.clone().acquire_owned() => permit.map_err(|_| Error::Ownership)?,
            _ = stop.changed() => {
                drop(running);
                return interrupt(repository, gateway, job).await;
            },
        };
        let next = advance_check(worker, job, running, response, permit)
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

struct ApprovedResponse {
    reference: ArtifactRef,
    demo_release: Option<DemoRelease>,
}

async fn wait_for_response(
    gateway: &Gateway,
    job: &Job,
    alive: &AtomicBool,
    stop: &mut watch::Receiver<bool>,
    deadline: Option<Instant>,
) -> Result<Option<ApprovedResponse>> {
    loop {
        if *stop.borrow()
            || !alive.load(Ordering::Relaxed)
            || deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Ok(None);
        }
        let current = gateway.job(job.id).await?;
        if current.cancel_requested {
            return Ok(None);
        }
        if current.generation != job.generation {
            return Err(Error::Ownership);
        }
        if let Some(response) = current.response {
            return Ok(Some(ApprovedResponse {
                reference: response,
                demo_release: current.demo_release,
            }));
        }
        tokio::select! {
            _ = stop.changed() => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
        }
    }
}

fn start_check(
    worker: &Worker,
    input: ArtifactRef,
    permit: OwnedSemaphorePermit,
) -> JoinHandle<Result<(ComparisonEngine, Progress)>> {
    let store = worker.artifacts.clone();
    let allow_unverified_mechanics = worker.allow_unverified_mechanics;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let manifest: CheckManifest = store.record(&input)?;
        if matches!(manifest.assurance, Assurance::HonestRoleMechanics)
            && !allow_unverified_mechanics
        {
            return Err(Error::ProofUnavailable);
        }
        let (engine, progress) = ComparisonEngine::start(store, manifest)?;
        Ok::<_, Error>((engine, progress))
    })
}

fn advance_check(
    worker: &Worker,
    job: &Job,
    mut running: ComparisonEngine,
    response: ApprovedResponse,
    permit: OwnedSemaphorePermit,
) -> JoinHandle<Result<(ComparisonEngine, Progress)>> {
    let store = worker.artifacts.clone();
    let allow_unverified_mechanics = worker.allow_unverified_mechanics;
    let ApprovedResponse {
        reference: response,
        demo_release,
    } = response;
    let expected_input = job.input.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let response: Response = store.record(&response)?;
        let progress = if let Some(release) = demo_release {
            if !allow_unverified_mechanics || release.input != expected_input {
                return Err(Error::Response);
            }
            #[cfg(feature = "demo-mode")]
            {
                running.advance_after_demo_close(response, release.closed_at_ms)?
            }
            #[cfg(not(feature = "demo-mode"))]
            {
                return Err(Error::Response);
            }
        } else {
            running.advance(response)?
        };
        Ok::<_, Error>((running, progress))
    })
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
