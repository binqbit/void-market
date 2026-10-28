use super::{model::AuctionRow, participation, repository};
use crate::{http::error::Result, state::AppState};
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn healthy(
    connection: &mut PgConnection,
    auction: &AuctionRow,
    now: i64,
) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM approval_runtime r JOIN arbiter_nodes n ON n.id=$1
         WHERE r.singleton AND r.healthy AND r.instance_id=$2
         AND $3-r.last_tick_ms BETWEEN 0 AND 5000 AND n.enabled AND n.heartbeat_until>now())
         AND ($4='internal' OR EXISTS(SELECT 1 FROM localnet_runtime WHERE singleton AND healthy AND $3-last_tick_ms BETWEEN 0 AND 5000))",
    )
    .bind(auction.node_id)
    .bind(auction.clock_epoch)
    .bind(now)
    .bind(&auction.execution)
    .fetch_one(connection)
    .await?)
}

/// One meter per participant/auction counts the union of ready obligations.
pub async fn meter(connection: &mut PgConnection, auction: Uuid, now: i64) -> Result<()> {
    let row = repository::lock(connection, auction).await?;
    let runtime: Option<Uuid> =
        sqlx::query_scalar("SELECT instance_id FROM approval_runtime WHERE singleton")
            .fetch_optional(&mut *connection)
            .await?;
    let available = healthy(connection, &row, now).await?
        && matches!(
            row.status.as_str(),
            "deposit_pending" | "matching" | "awaiting_completion"
        );
    let elapsed = row
        .last_tick_ms
        .and_then(|last| now.checked_sub(last))
        .filter(|elapsed| (0..=5000).contains(elapsed))
        .filter(|_| available && row.clock_epoch.is_some() && row.clock_epoch == runtime)
        .unwrap_or(0);
    let before: Vec<bool> = sqlx::query_scalar(
        "SELECT chargeable FROM auction_participants WHERE auction_id=$1 ORDER BY account_id",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE auction_participants SET remaining_ms=greatest(0,remaining_ms-$2)
         WHERE auction_id=$1 AND status='active' AND chargeable",
    )
    .bind(auction)
    .bind(elapsed)
    .execute(&mut *connection)
    .await?;
    update_charging(connection, auction, available).await?;
    let after: Vec<bool> = sqlx::query_scalar(
        "SELECT chargeable FROM auction_participants WHERE auction_id=$1 ORDER BY account_id",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    if before != after || (elapsed == 0 && before.iter().any(|active| *active)) {
        repository::touch(connection, auction).await?;
    }
    sqlx::query("UPDATE auctions SET clock_epoch=$2,last_tick_ms=$3 WHERE id=$1")
        .bind(auction)
        .bind(runtime)
        .bind(now)
        .execute(connection)
        .await?;
    Ok(())
}

async fn update_charging(
    connection: &mut PgConnection,
    auction: Uuid,
    healthy: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE auction_participants p SET chargeable=$2 AND p.status='active' AND p.remaining_ms>0
         AND (EXISTS (SELECT 1 FROM approval_messages m JOIN approval_sessions s ON s.id=m.session_id
         WHERE s.auction_id=p.auction_id AND s.status='active' AND m.account_id=p.account_id
         AND m.status='ready') OR EXISTS (SELECT 1 FROM auctions a WHERE a.id=p.auction_id
         AND a.status='awaiting_completion' AND a.seller_id=p.account_id
         AND (a.execution='internal' OR (a.payment_status='confirmed' AND a.custody_status='held')))
         OR EXISTS(SELECT 1 FROM chain_operations o WHERE o.auction_id=p.auction_id AND o.account_id=p.account_id
         AND (o.status='ready' OR (o.kind='payment' AND o.status='failed')))) WHERE p.auction_id=$1",
    )
    .bind(auction)
    .bind(healthy)
    .execute(connection)
    .await?;
    Ok(())
}

pub async fn refresh_charging(connection: &mut PgConnection, auction: Uuid) -> Result<()> {
    // Meter already established health; removing an obligation stops its clock immediately.
    let now = crate::messages::now_ms()?;
    meter(connection, auction, now).await
}

pub async fn enforce(connection: &mut PgConnection, auction: Uuid) -> Result<bool> {
    let expired: Vec<Uuid> = sqlx::query_scalar(
        "SELECT account_id FROM auction_participants WHERE auction_id=$1 AND status='active'
         AND remaining_ms=0 AND NOT EXISTS(SELECT 1 FROM auctions a WHERE a.id=$1 AND a.execution='localnet' AND a.status IN ('deposit_pending','awaiting_completion'))
         ORDER BY (account_id=(SELECT seller_id FROM auctions WHERE id=$1)) DESC, account_id",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    for account in &expired {
        participation::exclude(connection, auction, *account, "disqualified").await?;
    }
    Ok(!expired.is_empty())
}

pub async fn tick(state: &AppState, now: i64) -> Result<()> {
    let auctions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM auctions WHERE status IN ('deposit_pending','matching','awaiting_completion') ORDER BY id",
    )
    .fetch_all(&state.pool)
    .await?;
    for auction in auctions {
        let mut tx = state.pool.begin().await?;
        let row = repository::lock(&mut tx, auction).await?;
        if !matches!(
            row.status.as_str(),
            "deposit_pending" | "matching" | "awaiting_completion"
        ) {
            continue;
        }
        meter(&mut tx, auction, now).await?;
        enforce(&mut tx, auction).await?;
        tx.commit().await?;
    }
    Ok(())
}
