use super::{inputs, repository};
use crate::{
    events,
    http::error::{ApiError, Result},
    messages,
};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn cancel_checks(connection: &mut PgConnection, auction: Uuid) -> Result<()> {
    let sessions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval_sessions WHERE auction_id=$1 AND status IN ('initializing','active')
         ORDER BY id FOR UPDATE",
    )
    .bind(auction)
    .fetch_all(&mut *connection)
    .await?;
    for session in sessions {
        messages::interrupt_check(connection, session).await?;
    }
    Ok(())
}

pub async fn finish(
    connection: &mut PgConnection,
    auction: Uuid,
    status: &str,
    reason: &str,
    now: i64,
) -> Result<()> {
    cancel_checks(connection, auction).await?;
    sqlx::query(
        "UPDATE auctions SET status=$2,reason=$3,finished_at_ms=$4,
         completion_request_id=NULL,revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(auction)
    .bind(status)
    .bind(reason)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE auction_participants SET chargeable=FALSE WHERE auction_id=$1")
        .bind(auction)
        .execute(&mut *connection)
        .await?;
    events::record_node(
        connection,
        "auction.ended",
        json!({"auction_id":auction,"status":status,"reason":reason}),
    )
    .await
}

/// The caller holds the auction lock; this closes the generation before cancelling its work.
pub async fn exclude(
    connection: &mut PgConnection,
    auction: Uuid,
    account: Uuid,
    status: &str,
) -> Result<()> {
    let current = repository::lock(connection, auction).await?;
    if !matches!(
        current.status.as_str(),
        "open" | "matching" | "awaiting_completion"
    ) {
        return Ok(());
    }
    let changed = sqlx::query(
        "UPDATE auction_participants SET status=$3,chargeable=FALSE
         WHERE auction_id=$1 AND account_id=$2 AND status='active'",
    )
    .bind(auction)
    .bind(account)
    .bind(status)
    .execute(&mut *connection)
    .await?;
    if changed.rows_affected() == 0 {
        return Ok(());
    }
    sqlx::query(
        "UPDATE approval_members m SET status=$3,chargeable=FALSE
         FROM approval_sessions s WHERE m.session_id=s.id AND s.auction_id=$1 AND m.account_id=$2",
    )
    .bind(auction)
    .bind(account)
    .bind(status)
    .execute(&mut *connection)
    .await?;
    let now = messages::now_ms()?;
    if account == current.seller_id {
        return finish(
            connection,
            auction,
            "interrupted",
            "seller_approval_expired",
            now,
        )
        .await;
    }

    cancel_checks(connection, auction).await?;
    let members = repository::participants(connection, auction).await?;
    let survivors: Vec<_> = members
        .iter()
        .filter(|member| member.account_id != current.seller_id && member.status == "active")
        .collect();
    if current.status == "open" {
        repository::touch(connection, auction).await?;
    } else if survivors.is_empty() {
        finish(connection, auction, "no_sale", "no_remaining_buyers", now).await?;
    } else if matches!(current.status.as_str(), "matching" | "awaiting_completion") {
        if current.execution == "localnet" {
            sqlx::query("UPDATE chain_operations SET status='cancelled',revision=nextval('message_revision') WHERE auction_id=$1 AND status IN ('preparing','ready','failed') AND approval IS NULL")
                .bind(auction).execute(&mut *connection).await?;
            sqlx::query(
                "UPDATE auctions SET payment_invoice=NULL,payment_status='unverified' WHERE id=$1",
            )
            .bind(auction)
            .execute(&mut *connection)
            .await?;
        }
        let active: Vec<_> = survivors
            .iter()
            .map(|member| {
                (
                    member.account_id,
                    member.id,
                    member.admission_seq,
                    &member.frozen_input,
                )
            })
            .collect();
        let generation = current.generation + 1;
        let root = inputs::digest(
            "AUCTION_SURVIVORS_V1",
            &(
                auction,
                &current.input_root,
                generation,
                &current.round_root,
                account,
                status,
                active,
            ),
        )?;
        sqlx::query(
            "UPDATE auctions SET status='matching',generation=$2,round_root=$3,winner_id=NULL,
             completion_request_id=NULL,award_hash=NULL,revision=nextval('message_revision') WHERE id=$1",
        )
        .bind(auction)
        .bind(generation)
        .bind(root)
        .execute(&mut *connection)
        .await?;
    } else {
        return Err(ApiError::Conflict("Participation has already ended"));
    }
    sqlx::query("UPDATE auction_participants SET chargeable=FALSE WHERE auction_id=$1")
        .bind(auction)
        .execute(&mut *connection)
        .await?;
    events::record_node(
        connection,
        "auction.participant_excluded",
        json!({"auction_id":auction,"account_id":account,"reason":status}),
    )
    .await
}
