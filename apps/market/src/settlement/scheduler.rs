use super::{auction, repository};
use crate::{
    auctions::{self, model::AuctionRow},
    demo,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

pub(crate) async fn advance(state: &AppState) -> Result<()> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM auctions WHERE execution='localnet' AND custody_status IN
         ('deposit_pending','held') ORDER BY created_at_ms",
    )
    .fetch_all(&state.pool)
    .await?;

    for id in ids {
        let mut tx = state.pool.begin().await?;
        let row = auctions::lock(&mut tx, id).await?;
        match row.status.as_str() {
            "no_sale" | "interrupted" | "cancelled" => {
                auction::return_asset(state, &mut tx, &row).await?;
            }
            "awaiting_completion" if row.payment_status == "confirmed" => {
                finish_paid_sale(state, &mut tx, &row).await?;
            }
            "awaiting_completion" => {
                advance_payment(state, &mut tx, &row).await?;
            }
            "deposit_pending" => {
                expire_deposit(state, &mut tx, &row).await?;
            }
            _ => {}
        }
        tx.commit().await?;
    }
    Ok(())
}

async fn remaining(connection: &mut PgConnection, auction: Uuid, account: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT remaining_ms FROM auction_participants WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(auction)
    .bind(account)
    .fetch_one(connection)
    .await?)
}

async fn finish_paid_sale(
    state: &AppState,
    connection: &mut PgConnection,
    row: &AuctionRow,
) -> Result<()> {
    if remaining(connection, row.id, row.seller_id).await? > 0 {
        return Ok(());
    }
    let approval = json!({
        "basis": "seller_timeout_consent",
        "publish_approval": row.publish_receipt,
        "generation": row.generation,
    });
    auction::deliver(state, connection, row, approval).await?;
    sqlx::query(
        "UPDATE auctions SET settlement_note='Seller timeout applied the accepted delivery terms' WHERE
         id=$1",
    )
    .bind(row.id)
    .execute(connection)
    .await?;
    Ok(())
}

async fn advance_payment(
    state: &AppState,
    connection: &mut PgConnection,
    row: &AuctionRow,
) -> Result<()> {
    let winner = row.winner_id.ok_or(ApiError::Internal)?;
    let in_flight: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM chain_operations WHERE auction_id=$1 AND kind='payment' AND status IN
         ('approved','building','signed','submitted','reconcile'))",
    )
    .bind(row.id)
    .fetch_one(&mut *connection)
    .await?;
    if in_flight {
        return Ok(());
    }
    if remaining(connection, row.id, winner).await? > 0 {
        return auction::ensure_payment(state, connection, row).await;
    }

    sqlx::query(
        "UPDATE auction_participants SET status='disqualified',chargeable=FALSE WHERE auction_id=$1 AND
         account_id=$2",
    )
    .bind(row.id)
    .bind(winner)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE chain_operations SET status='cancelled',revision=nextval('message_revision') WHERE
         auction_id=$1 AND kind='payment' AND status IN ('preparing','ready','failed')",
    )
    .bind(row.id)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE auctions SET status='interrupted',reason='buyer_payment_expired', settlement_note='Buyer
         failed to complete payment in time; transfers are not automatically refunded',
         finished_at_ms=$2,revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(row.id)
    .bind(messages::now_ms()?)
    .execute(&mut *connection)
    .await?;
    auction::return_asset(state, connection, row).await
}

async fn expire_deposit(
    state: &AppState,
    connection: &mut PgConnection,
    row: &AuctionRow,
) -> Result<()> {
    let pending: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id,status FROM chain_operations WHERE auction_id=$1 AND kind='deposit' ORDER BY
         created_at_ms DESC LIMIT 1",
    )
    .bind(row.id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((id, status)) = pending else {
        return Ok(());
    };
    let expired = remaining(connection, row.id, row.seller_id).await? <= 0;
    if status != "failed" && !(expired && matches!(status.as_str(), "ready" | "preparing")) {
        return Ok(());
    }

    let chains = state.chains.as_ref().ok_or(ApiError::Unavailable)?;
    let (collection, token) = auction::descriptor(state, &row.asset_snapshot["nft"]).await?;
    let owner = chains
        .ethereum
        .owner(&collection, &token, json!("latest"))
        .await?;
    let seller = demo::repository::actor(&state.pool, row.seller_id).await?;
    let custodian = auction::custodian(connection, row).await?;
    let custody = if owner == seller.ethereum_address {
        "none"
    } else if owner == custodian {
        "held"
    } else {
        "review"
    };

    repository::status(
        connection,
        id,
        "cancelled",
        Some("The NFT deposit did not complete"),
    )
    .await?;
    sqlx::query(
        "UPDATE auctions SET status='interrupted',custody_status=$2,reason='seller_deposit_expired',
         finished_at_ms=$3,revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(row.id)
    .bind(custody)
    .bind(messages::now_ms()?)
    .execute(&mut *connection)
    .await?;
    // If custody happened externally, retain the lock until the actual return is confirmed.
    if custody == "held" {
        let updated = auctions::lock(connection, row.id).await?;
        auction::return_asset(state, connection, &updated).await?;
    }
    Ok(())
}
