use super::{amount, inputs, model::Participant};
use crate::{http::error::Result, messages, state::AppState};
use serde::Serialize;
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Serialize)]
pub struct BiddingActivity {
    stats: BidStats,
    bids: Vec<BidView>,
    has_more: bool,
    server_time_ms: i64,
}

#[derive(Serialize)]
struct BidStats {
    bids_placed: i64,
    active_bids: i64,
    completed_purchases: i64,
    pending_approvals: i64,
}

#[derive(Default, Serialize, sqlx::FromRow)]
struct ApprovalProgress {
    requested: i64,
    approved: i64,
    pending: i64,
}

#[derive(sqlx::FromRow)]
struct ApprovalRow {
    auction_id: Uuid,
    #[sqlx(flatten)]
    progress: ApprovalProgress,
}

#[derive(sqlx::FromRow)]
struct BidRow {
    auction_id: Uuid,
    asset_name: String,
    auction_status: String,
    is_winner: bool,
    closes_at_ms: i64,
    #[sqlx(flatten)]
    participant: Participant,
}

#[derive(Serialize)]
struct BidView {
    auction_id: Uuid,
    asset_name: String,
    amount: String,
    auction_status: String,
    participation_status: String,
    is_winner: bool,
    created_at_ms: i64,
    closes_at_ms: i64,
    bidding_closes_at_ms: i64,
    approvals: ApprovalProgress,
}

pub async fn list(
    state: &AppState,
    account: Uuid,
    limit: i64,
    offset: i64,
) -> Result<BiddingActivity> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;

    let (bids_placed, active_bids, completed_purchases): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),
             count(*) FILTER (WHERE p.status='active' AND (a.status IN ('open','matching')
                 OR (a.status='awaiting_completion' AND a.winner_id=$1))),
             count(*) FILTER (WHERE a.status='completed' AND a.winner_id=$1)
         FROM auction_participants p JOIN auctions a ON a.id=p.auction_id
         WHERE p.account_id=$1 AND a.seller_id<>$1",
    )
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    let mut progress = approval_progress(&mut tx, account).await?;
    let pending_approvals = progress.values().map(|value| value.pending).sum();

    let rows: Vec<BidRow> = sqlx::query_as(
        "SELECT p.*, a.asset_snapshot->>'name' AS asset_name,
             a.status AS auction_status, COALESCE(a.winner_id=$1,FALSE) AS is_winner,
             a.closes_at_ms
         FROM auction_participants p JOIN auctions a ON a.id=p.auction_id
         WHERE p.account_id=$1 AND a.seller_id<>$1
         ORDER BY (p.status='active' AND (a.status IN ('open','matching')
             OR (a.status='awaiting_completion' AND a.winner_id=$1))) DESC,
             p.created_at_ms DESC, p.id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(account)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    let mut bids = Vec::with_capacity(rows.len());
    for row in rows {
        let value = inputs::amount(state, row.auction_id, &row.participant)?;
        bids.push(BidView {
            auction_id: row.auction_id,
            asset_name: row.asset_name,
            amount: amount::format(value),
            auction_status: row.auction_status,
            participation_status: row.participant.status,
            is_winner: row.is_winner,
            created_at_ms: row.participant.created_at_ms,
            closes_at_ms: row.closes_at_ms,
            bidding_closes_at_ms: row
                .closes_at_ms
                .saturating_sub(super::model::APPROVAL_WINDOW_MS),
            approvals: progress.remove(&row.auction_id).unwrap_or_default(),
        });
    }
    tx.commit().await?;

    Ok(BiddingActivity {
        has_more: offset + (bids.len() as i64) < bids_placed,
        bids,
        stats: BidStats {
            bids_placed,
            active_bids,
            completed_purchases,
            pending_approvals,
        },
        server_time_ms: messages::now_ms()?,
    })
}

async fn approval_progress(
    connection: &mut PgConnection,
    account: Uuid,
) -> Result<HashMap<Uuid, ApprovalProgress>> {
    let rows: Vec<ApprovalRow> = sqlx::query_as(
        "WITH requests AS (
             SELECT s.auction_id, m.approval IS NOT NULL AS approved,
                 (m.status='ready' AND s.status='active' AND member.status='active') AS actionable
             FROM approval_messages m
             JOIN approval_sessions s ON s.id=m.session_id
             JOIN approval_members member ON member.session_id=s.id AND member.account_id=m.account_id
             WHERE m.account_id=$1 AND m.status<>'cancelled' AND s.auction_id IS NOT NULL
             UNION ALL
             SELECT auction_id, approval IS NOT NULL, status='ready'
             FROM chain_operations
             WHERE account_id=$1 AND kind='payment'
                 AND status NOT IN ('preparing','failed','cancelled') AND auction_id IS NOT NULL
         )
         SELECT r.auction_id, count(*) AS requested,
             count(*) FILTER (WHERE r.approved) AS approved,
             count(*) FILTER (WHERE r.actionable AND NOT r.approved AND p.status='active'
                 AND p.remaining_ms>0 AND (a.status IN ('open','matching')
                     OR (a.status='awaiting_completion' AND a.winner_id=$1))) AS pending
         FROM requests r
         JOIN auctions a ON a.id=r.auction_id
         JOIN auction_participants p ON p.auction_id=a.id AND p.account_id=$1
         WHERE a.seller_id<>$1 GROUP BY r.auction_id",
    )
    .bind(account)
    .fetch_all(connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.auction_id, row.progress))
        .collect())
}
