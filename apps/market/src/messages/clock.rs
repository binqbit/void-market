use crate::http::error::{ApiError, Result};
use sqlx::PgConnection;
use uuid::Uuid;

pub fn now_ms() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ApiError::Internal)?
        .as_millis()
        .try_into()
        .map_err(|_| ApiError::Internal)
}

/// Call with the session row locked. A stale worker clock never spends user time.
pub async fn meter(connection: &mut PgConnection, session: Uuid, now: i64) -> Result<()> {
    let auction: Option<Uuid> =
        sqlx::query_scalar("SELECT auction_id FROM approval_sessions WHERE id = $1")
            .bind(session)
            .fetch_one(&mut *connection)
            .await?;
    if let Some(auction) = auction {
        return crate::auctions::meter(connection, auction, now).await;
    }
    let (epoch, last, status, node): (Option<Uuid>, Option<i64>, String, Uuid) = sqlx::query_as(
        "SELECT
             clock_epoch,
             last_tick_ms,
             status,
             node_id
         FROM approval_sessions
         WHERE id = $1",
    )
    .bind(session)
    .fetch_one(&mut *connection)
    .await?;
    let clock: Option<(Uuid, i64, bool)> = sqlx::query_as(
        "SELECT
             instance_id,
             last_tick_ms,
             healthy
         FROM approval_runtime
         WHERE singleton",
    )
    .fetch_optional(&mut *connection)
    .await?;
    let node_ready: bool = sqlx::query_scalar(
        "SELECT
             COALESCE(
                 enabled
                 AND heartbeat_until > now(), FALSE
             )
         FROM arbiter_nodes
         WHERE id = $1",
    )
    .bind(node)
    .fetch_one(&mut *connection)
    .await?;
    let healthy = clock.is_some_and(|(_, tick, healthy)| {
        healthy && (0..=5_000).contains(&now.saturating_sub(tick))
    }) && node_ready
        && status == "active";
    let current_epoch = clock.map(|(id, _, _)| id);
    let elapsed = last
        .and_then(|last| now.checked_sub(last))
        .filter(|elapsed| (0..=5_000).contains(elapsed))
        .filter(|_| healthy && epoch.is_some() && epoch == current_epoch)
        .unwrap_or(0);

    let before: Vec<bool> = sqlx::query_scalar(
        "SELECT chargeable
         FROM approval_members
         WHERE session_id = $1
         ORDER BY account_id",
    )
    .bind(session)
    .fetch_all(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE approval_members
         SET remaining_ms = greatest(0, remaining_ms - $2)
         WHERE
             session_id = $1
             AND status = 'active'
             AND chargeable",
    )
    .bind(session)
    .bind(elapsed)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE approval_members m
         SET
             chargeable = $2
             AND m.status = 'active'
             AND m.remaining_ms > 0
             AND EXISTS
             (
                 SELECT 1
                 FROM approval_messages a
                 WHERE
                     a.session_id = m.session_id
                     AND a.account_id = m.account_id
                     AND a.status = 'ready'
             )
         WHERE m.session_id = $1",
    )
    .bind(session)
    .bind(healthy)
    .execute(&mut *connection)
    .await?;
    let after: Vec<bool> = sqlx::query_scalar(
        "SELECT chargeable
         FROM approval_members
         WHERE session_id = $1
         ORDER BY account_id",
    )
    .bind(session)
    .fetch_all(&mut *connection)
    .await?;
    if before != after || (elapsed == 0 && before.iter().any(|active| *active)) {
        sqlx::query(
            "UPDATE approval_sessions
             SET revision = nextval('message_revision')
             WHERE id = $1",
        )
        .bind(session)
        .execute(&mut *connection)
        .await?;
    }
    sqlx::query(
        "UPDATE approval_sessions
         SET
             clock_epoch = $2,
             last_tick_ms = $3
         WHERE id = $1",
    )
    .bind(session)
    .bind(current_epoch)
    .bind(now)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn refresh_charging(connection: &mut PgConnection, session: Uuid) -> Result<()> {
    let auction: Option<Uuid> =
        sqlx::query_scalar("SELECT auction_id FROM approval_sessions WHERE id = $1")
            .bind(session)
            .fetch_one(&mut *connection)
            .await?;
    if let Some(auction) = auction {
        return crate::auctions::refresh_charging(connection, auction).await;
    }
    sqlx::query(
        "UPDATE approval_members m
         SET
             chargeable = m.chargeable
             AND EXISTS
             (
                 SELECT 1
                 FROM approval_messages a
                 WHERE
                     a.session_id = m.session_id
                     AND a.account_id = m.account_id
                     AND a.status = 'ready'
             )
         WHERE m.session_id = $1",
    )
    .bind(session)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn terminate(
    connection: &mut PgConnection,
    session: Uuid,
    account: Uuid,
    status: &str,
) -> Result<()> {
    let auction: Option<Uuid> =
        sqlx::query_scalar("SELECT auction_id FROM approval_sessions WHERE id = $1")
            .bind(session)
            .fetch_one(&mut *connection)
            .await?;
    if let Some(auction) = auction {
        return crate::auctions::exclude(connection, auction, account, status).await;
    }
    sqlx::query(
        "UPDATE approval_members
         SET
             status = $3,
             chargeable = FALSE
         WHERE
             session_id = $1
             AND account_id = $2
             AND status = 'active'",
    )
    .bind(session)
    .bind(account)
    .bind(status)
    .execute(&mut *connection)
    .await?;
    interrupt(connection, session).await
}

/// Infrastructure failures interrupt work without penalizing either participant.
pub async fn interrupt(connection: &mut PgConnection, session: Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE approval_sessions
         SET
             status = 'interrupted',
             revision = nextval('message_revision')
         WHERE
             id = $1
             AND status IN (
                 'initializing',
                 'active'
             )",
    )
    .bind(session)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE approval_members SET chargeable = FALSE WHERE session_id = $1")
        .bind(session)
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "UPDATE approval_messages
         SET
             status = 'cancelled',
             revision = nextval('message_revision')
         WHERE
             session_id = $1
             AND status IN (
                 'ready',
                 'approved',
                 'queued',
                 'processing',
                 'response_pending',
                 'responded',
                 'blocked'
             )",
    )
    .bind(session)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE arbiter_jobs
         SET cancel_requested = TRUE
         WHERE
             id = (
                 SELECT job_id
                 FROM approval_sessions
                 WHERE id = $1
             )
             AND status NOT IN (
                 'completed',
                 'interrupted',
                 'failed',
                 'unavailable'
             )",
    )
    .bind(session)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn run(
    state: crate::state::AppState,
    instance: Uuid,
    ownership: &mut PgConnection,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            _ = interval.tick() => {}
        }
        let now = now_ms()?;
        let healthy = state
            .worker_ready
            .load(std::sync::atomic::Ordering::Relaxed);
        sqlx::query(
            "INSERT INTO approval_runtime (singleton, instance_id, last_tick_ms, healthy)
             VALUES (TRUE, $1, $2, $3) ON CONFLICT (singleton) DO
             UPDATE
                 SET
                     instance_id = $1,
                     last_tick_ms = $2,
                     healthy = $3",
        )
        .bind(instance)
        .bind(now)
        .bind(healthy)
        .execute(&mut *ownership)
        .await?;
        let sessions: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id
             FROM approval_sessions
             WHERE
                 status = 'active'
                 AND auction_id IS NULL
             ORDER BY id",
        )
        .fetch_all(&state.pool)
        .await?;
        for session in sessions {
            let mut tx = state.pool.begin().await?;
            super::repository::lock_session(&mut tx, session).await?;
            meter(&mut tx, session, now).await?;
            let expired: Vec<Uuid> = sqlx::query_scalar(
                "SELECT account_id
                 FROM approval_members
                 WHERE
                     session_id = $1
                     AND status = 'active'
                     AND remaining_ms = 0",
            )
            .bind(session)
            .fetch_all(&mut *tx)
            .await?;
            for account in expired {
                terminate(&mut tx, session, account, "disqualified").await?;
                crate::events::record_node(
                    &mut tx,
                    "message.expired",
                    serde_json::json!({"session_id":session,"account_id":account}),
                )
                .await?;
            }
            tx.commit().await?;
        }
        crate::auctions::tick(&state, now).await?;
    }
    sqlx::query("UPDATE approval_runtime SET healthy = FALSE WHERE instance_id = $1")
        .bind(instance)
        .execute(ownership)
        .await?;
    Ok(())
}
