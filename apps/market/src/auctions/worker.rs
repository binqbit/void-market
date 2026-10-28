use super::{
    checks::{self, Check},
    inputs, matching,
    model::AuctionRow,
    participation, repository,
};
use crate::{
    events,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn advance_one(state: &AppState) -> Result<bool> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id
         FROM auctions
         WHERE
             status IN (
                 'open',
                 'matching'
             )
         ORDER BY
             created_at_ms,
             id",
    )
    .fetch_all(&state.pool)
    .await?;
    for id in ids {
        let mut tx = state.pool.begin().await?;
        let auction = repository::lock(&mut tx, id).await?;
        let changed =
            if auction.status == "open" && auction.bidding_deadline() <= messages::now_ms()? {
                freeze(&mut tx, &auction).await?;
                true
            } else if matches!(auction.status.as_str(), "open" | "matching") {
                advance(state, &mut tx, &auction).await?
            } else {
                false
            };
        tx.commit().await?;
        if changed {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) async fn freeze(connection: &mut PgConnection, auction: &AuctionRow) -> Result<()> {
    let members = repository::participants(connection, auction.id).await?;
    let history: Vec<_> = members
        .iter()
        .map(|member| {
            (
                member.id,
                member.account_id,
                member.admission_seq,
                &member.frozen_input,
            )
        })
        .collect();
    let original = inputs::digest(
        "AUCTION_FREEZE_V1",
        &(auction.id, &auction.policy_hash, history),
    )?;
    let active: Vec<_> = members
        .iter()
        .filter(|member| member.status == "active")
        .map(|member| (member.id, member.account_id, &member.frozen_input))
        .collect();
    let root = inputs::digest("AUCTION_SURVIVORS_V1", &(auction.id, &original, 1, active))?;
    sqlx::query(
        "UPDATE auctions
         SET
             status = 'matching',
             generation = 1,
             input_root = $2,
             round_root = $3,
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .bind(original)
    .bind(&root)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE auction_checks
         SET round_root = $2
         WHERE
             auction_id = $1
             AND generation = 1",
    )
    .bind(auction.id)
    .bind(&root)
    .execute(&mut *connection)
    .await?;
    events::record_node(
        connection,
        "auction.frozen",
        json!({"auction_id":auction.id,"generation":1}),
    )
    .await
}

async fn advance(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<bool> {
    let members = repository::participants(connection, auction.id).await?;
    let buyers: Vec<Uuid> = members
        .iter()
        .filter(|member| member.account_id != auction.seller_id && member.status == "active")
        .map(|member| member.account_id)
        .collect();
    if buyers.is_empty() && auction.status == "open" {
        return Ok(false);
    }
    if buyers.is_empty() {
        participation::finish(
            connection,
            auction.id,
            "no_sale",
            "no_remaining_buyers",
            messages::now_ms()?,
        )
        .await?;
        return Ok(true);
    }

    let slots = matching::schedule(auction.seller_id, &buyers);
    let mut checks = checks::load(connection, auction).await?;
    let mut changed = false;
    for check in &mut checks {
        if !slots.iter().any(|slot| check.matches(slot)) {
            return interrupt(connection, auction.id, "inconsistent_transcript").await;
        }
        if check.status == "interrupted" {
            return interrupt(connection, auction.id, "comparison_interrupted").await;
        }
        if check.result.is_none() && check.status == "completed" {
            let value = match check.decode_result(state) {
                Ok(value) => value,
                Err(_) => {
                    return interrupt(connection, auction.id, "invalid_comparison_result").await;
                }
            };
            checks::store_result(connection, check, value).await?;
            changed = true;
        }
    }

    if changed {
        repository::touch(connection, auction.id).await?;
    }
    let scheduled =
        checks::schedule_missing(connection, auction, &members, &slots, &checks).await?;
    if auction.status == "matching" {
        messages::release_auction_responses(connection, auction.id).await?;
        if checks.len() == slots.len() && checks.iter().all(|check| check.result.is_some()) {
            let ordered = checks::in_schedule_order(&checks, &slots)?;
            award(connection, auction, &buyers, &ordered).await?;
            return Ok(true);
        }
    }
    Ok(changed || scheduled)
}

async fn award(
    connection: &mut PgConnection,
    auction: &AuctionRow,
    buyers: &[Uuid],
    checks: &[&Check],
) -> Result<()> {
    let values: Vec<bool> = checks
        .iter()
        .map(|check| check.result.ok_or(ApiError::Internal))
        .collect::<Result<_>>()?;
    let winner = match matching::winner(auction.seller_id, buyers, &values) {
        Ok(winner) => winner,
        Err(_) => {
            interrupt(connection, auction.id, "inconsistent_transcript").await?;
            return Ok(());
        }
    };
    if let Some(winner) = winner {
        let references: Vec<_> = checks
            .iter()
            .map(|check| (check.session_id, &check.result_ref))
            .collect();
        let award = inputs::digest(
            "AUCTION_AWARD_V1",
            &(
                auction.id,
                &auction.policy_hash,
                &auction.input_root,
                &auction.round_root,
                auction.generation,
                winner,
                references,
                "honest_role_mechanics",
            ),
        )?;
        sqlx::query(
            "UPDATE auctions
             SET
                 status = 'awaiting_completion',
                 winner_id = $2,
                 award_hash = $3,
                 completion_request_id = $4,
                 completion_requested_at_ms = $5,
                 revision = nextval('message_revision')
             WHERE id = $1",
        )
        .bind(auction.id)
        .bind(winner)
        .bind(award)
        .bind(Uuid::new_v4())
        .bind(messages::now_ms()?)
        .execute(&mut *connection)
        .await?;
        events::record_node(
            connection,
            "auction.awarded",
            json!({"auction_id":auction.id,
            "generation":auction.generation,"winner_id":winner}),
        )
        .await?;
    } else {
        participation::finish(
            connection,
            auction.id,
            "no_sale",
            "reserve_not_met",
            messages::now_ms()?,
        )
        .await?;
    }
    Ok(())
}

async fn interrupt(connection: &mut PgConnection, auction: Uuid, reason: &str) -> Result<bool> {
    participation::finish(
        connection,
        auction,
        "interrupted",
        reason,
        messages::now_ms()?,
    )
    .await?;
    Ok(true)
}
