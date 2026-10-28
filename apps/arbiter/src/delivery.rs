use crate::{
    client::Gateway,
    error::{Error, Result},
    repository::Repository,
};
use arbiter_protocol::{Completion, Job, JobStatus};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub(crate) async fn deliver(
    repository: &Repository,
    gateway: &Gateway,
    result: &Completion,
) -> Result<Job> {
    let mut signed = result.clone();
    gateway.sign_completion(&mut signed)?;
    let result = &signed;
    // This durable exact result is retained before its first network transmission.
    repository.prepare(result).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        match gateway.complete(result).await {
            Ok(()) => match gateway.job(result.job_id).await {
                Ok(job) if job.status == result.status && job.output == result.output => {
                    repository.delivered(result).await?;
                    return Ok(job);
                }
                Ok(_) | Err(Error::Gateway) => {}
                Err(error) => return Err(error),
            },
            Err(Error::Gateway) => {}
            Err(error) => return Err(error),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(Error::Gateway)
}

pub(crate) async fn recover(repository: &Repository, gateway: &Gateway) -> Result<()> {
    for result in repository.pending().await? {
        let current = gateway.job(result.job_id).await?;
        if current.generation != result.generation {
            return Err(Error::Ownership);
        }
        if matches!(current.status, JobStatus::Running | JobStatus::Waiting) {
            gateway.renew(&current).await?;
        }
        deliver(repository, gateway, &result).await?;
    }
    let mut candidates = repository.unfinished().await?;
    for job in gateway.jobs().await? {
        if matches!(job.status, JobStatus::Running | JobStatus::Waiting)
            && !candidates.iter().any(|(id, _)| *id == job.id)
        {
            repository.started(&job).await?;
            candidates.push((job.id, job.generation));
        }
    }
    for (id, generation) in candidates {
        let current = gateway.job(id).await?;
        if current.status.terminal() {
            continue;
        }
        if current.generation != generation {
            return Err(Error::Ownership);
        }
        interrupt(repository, gateway, &current).await?;
    }
    Ok(())
}

pub(crate) async fn interrupt(repository: &Repository, gateway: &Gateway, job: &Job) -> Result<()> {
    let current = gateway.job(job.id).await?;
    if current.generation != job.generation {
        return Err(Error::Ownership);
    }
    if current.status.terminal() {
        return Ok(());
    }

    let result = Completion {
        deployment_id: Uuid::nil(),
        node_id: current.node_id,
        instance_id: current.instance_id.ok_or(Error::Ownership)?,
        signature: String::new(),
        id: Uuid::new_v4(),
        job_id: current.id,
        generation: current.generation,
        status: JobStatus::Interrupted,
        output: None,
        consumed_response: current.response.map(|value| value.id),
    };
    deliver(repository, gateway, &result).await?;
    Ok(())
}
