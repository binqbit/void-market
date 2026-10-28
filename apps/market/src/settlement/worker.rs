use super::{
    execution,
    model::{Operation, Payload},
    repository,
};
use crate::{
    demo,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

pub async fn run(state: AppState, mut stop: watch::Receiver<bool>) -> Result<()> {
    if state.chains.is_none() {
        return Ok(());
    }
    let mut ownership = state.pool.acquire().await?;
    ownership.close_on_drop();
    let owned: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_lock(hashtextextended(current_schema(),732617232))",
    )
    .fetch_one(&mut *ownership)
    .await?;
    if !owned {
        return Err(ApiError::Unavailable);
    }
    let instance = Uuid::new_v4();
    sqlx::query("INSERT INTO localnet_runtime(singleton,instance_id,last_tick_ms,healthy) VALUES(TRUE,$1,$2,FALSE)
         ON CONFLICT(singleton) DO UPDATE SET instance_id=$1,last_tick_ms=$2,healthy=FALSE")
        .bind(instance)
        .bind(messages::now_ms()?)
        .execute(&state.pool).await?;
    loop {
        if *stop.borrow() {
            break;
        }
        sqlx::query("SELECT 1").execute(&mut *ownership).await?;
        let healthy = demo::verify(&state).await.is_ok()
            && state
                .chains
                .as_ref()
                .ok_or(ApiError::Internal)?
                .zcash
                .synchronized()
                .await
                .unwrap_or(false);
        sqlx::query("UPDATE localnet_runtime SET last_tick_ms=$2,healthy=$3 WHERE singleton AND instance_id=$1")
            .bind(instance)
            .bind(messages::now_ms()?)
            .bind(healthy)
            .execute(&state.pool).await?;
        if healthy {
            super::scheduler::advance(&state).await?;
            if let Some(operation) = next(&state).await? {
                let id = operation.id;
                if let Err(error) = execution::step(&state, operation, instance).await {
                    match error {
                        ApiError::Unavailable => {
                            sqlx::query(
                                "UPDATE localnet_runtime SET healthy=FALSE WHERE instance_id=$1",
                            )
                            .bind(instance)
                            .execute(&state.pool)
                            .await?;
                        }
                        ApiError::Conflict(message) | ApiError::BadRequest(message) => {
                            let mut tx = state.pool.begin().await?;
                            let op = repository::get(&state.pool, id).await?;
                            if let Some(auction) = op.auction_id {
                                crate::auctions::lock(&mut tx, auction).await?;
                            }
                            let latest = repository::lock(&mut tx, id).await?;
                            if !matches!(latest.status.as_str(), "cancelled" | "confirmed") {
                                let payload = repository::payload(&state, &latest)?;
                                let possibly_signed = latest.signed_transaction.is_some()
                                    || matches!(payload, Payload::Zcash(plan) if plan.signed_pczt);
                                let status = if possibly_signed
                                    && message != "Ethereum transaction reverted"
                                {
                                    "reconcile"
                                } else {
                                    "failed"
                                };
                                repository::status(&mut tx, id, status, Some(message)).await?;
                            }
                            tx.commit().await?;
                        }
                        error => return Err(error),
                    }
                }
            }
        }
        tokio::select! {
            _=stop.changed()=>break,
            _=tokio::time::sleep(Duration::from_millis(250))=>{}
        }
    }
    sqlx::query("UPDATE localnet_runtime SET healthy=FALSE WHERE instance_id=$1")
        .bind(instance)
        .execute(&state.pool)
        .await?;
    ownership.close().await?;
    Ok(())
}

async fn next(state: &AppState) -> Result<Option<Operation>> {
    Ok(sqlx::query_as(
        "SELECT o.* FROM chain_operations o WHERE o.status IN
         ('preparing','approved','building','signed','submitted','reconcile') AND (o.status<>'preparing'
         OR NOT EXISTS(SELECT 1 FROM chain_operations busy WHERE busy.account_id=o.account_id AND
         busy.status='ready')) ORDER BY o.updated_at_ms,o.created_at_ms LIMIT 1",
    )
    .fetch_optional(&state.pool).await?)
}
