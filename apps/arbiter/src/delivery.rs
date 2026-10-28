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
    // This durable exact result is retained before its first network transmission.
    repository.prepare(&signed).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        match gateway.complete(&signed).await {
            Ok(()) => match gateway.job(signed.job_id).await {
                Ok(job) if job.status == signed.status && job.output == signed.output => {
                    repository.delivered(&signed).await?;
                    return Ok(job);
                }
                Ok(_) | Err(Error::Gateway) => {}
                Err(error) => return Err(error),
            },
            Err(Error::Gateway) => {}
            Err(Error::Ownership) if signed.status != JobStatus::Interrupted => {
                let current = gateway.job(signed.job_id).await?;
                if !current.cancel_requested
                    || current.status.terminal()
                    || current.node_id != signed.node_id
                    || current.instance_id != Some(signed.instance_id)
                    || current.generation != signed.generation
                {
                    return Err(Error::Ownership);
                }

                // Cancellation can arrive while an HE operation is computing its result.
                // Acknowledge that job without stopping unrelated checks on the node.
                signed = interrupted_completion(&current)?;
                gateway.sign_completion(&mut signed)?;
                repository.prepare(&signed).await?;
            }
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

    let result = interrupted_completion(&current)?;
    deliver(repository, gateway, &result).await?;
    Ok(())
}

fn interrupted_completion(job: &Job) -> Result<Completion> {
    Ok(Completion {
        deployment_id: Uuid::nil(),
        node_id: job.node_id,
        instance_id: job.instance_id.ok_or(Error::Ownership)?,
        signature: String::new(),
        id: Uuid::new_v4(),
        job_id: job.id,
        generation: job.generation,
        status: JobStatus::Interrupted,
        output: None,
        consumed_response: job.response.as_ref().map(|value| value.id),
    })
}
