use super::{
    inputs, matching,
    model::{AuctionRow, Participant},
    participation, repository,
};
use crate::{
    events,
    http::error::{ApiError, Result},
    messages::{self, model::BoundCheck},
    state::AppState,
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::ContextRecord;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct Check {
    session_id: Uuid,
    slot: i32,
    kind: String,
    left_account: Uuid,
    right_account: Uuid,
    result: Option<bool>,
    result_ref: Option<Value>,
    status: String,
    output: Option<Value>,
    check_context: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MechanicalResult {
    schema_version: u8,
    context: [u8; 32],
    value: bool,
    assurance: String,
    proof_status: String,
    final_audit: String,
}

pub async fn advance_one(state: &AppState) -> Result<bool> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM auctions WHERE (status='open' AND closes_at_ms<=$1) OR status='matching'
         ORDER BY created_at_ms,id",
    )
    .bind(messages::now_ms()?)
    .fetch_all(&state.pool)
    .await?;
    for id in ids {
        let mut tx = state.pool.begin().await?;
        let auction = repository::lock(&mut tx, id).await?;
        let changed = if auction.status == "open" && auction.closes_at_ms <= messages::now_ms()? {
            freeze(&mut tx, &auction).await?;
            true
        } else if auction.status == "matching" {
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

async fn freeze(connection: &mut PgConnection, auction: &AuctionRow) -> Result<()> {
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
        "UPDATE auctions SET status='matching',generation=1,input_root=$2,round_root=$3,
        revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(auction.id)
    .bind(original)
    .bind(root)
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
    let mut checks: Vec<Check> = sqlx::query_as(
        "SELECT c.session_id,c.slot,c.kind,c.left_account,c.right_account,c.result,c.result_ref,
         s.status,j.output,s.check_context FROM auction_checks c JOIN approval_sessions s ON s.id=c.session_id
         LEFT JOIN arbiter_jobs j ON j.id=s.job_id WHERE c.auction_id=$1 AND c.generation=$2 ORDER BY c.slot",
    )
    .bind(auction.id)
    .bind(auction.generation)
    .fetch_all(&mut *connection)
    .await?;
    let mut changed = false;
    for (position, check) in checks.iter_mut().enumerate() {
        let Some(slot) = slots.get(position) else {
            return interrupt(connection, auction.id, "inconsistent_transcript").await;
        };
        if check.slot != position as i32
            || check.kind != slot.kind
            || check.left_account != slot.left
            || check.right_account != slot.right
        {
            return interrupt(connection, auction.id, "inconsistent_transcript").await;
        }
        if check.status == "interrupted" {
            return interrupt(connection, auction.id, "comparison_interrupted").await;
        }
        if check.result.is_none() && check.status == "completed" {
            let value = match result(state, check) {
                Ok(value) => value,
                Err(_) => {
                    return interrupt(connection, auction.id, "invalid_comparison_result").await;
                }
            };
            sqlx::query(
                "UPDATE auction_checks SET result=$2,result_ref=$3 WHERE session_id=$1 AND result IS NULL",
            )
            .bind(check.session_id)
            .bind(value)
            .bind(&check.output)
            .execute(&mut *connection)
            .await?;
            check.result = Some(value);
            check.result_ref = check.output.clone();
            changed = true;
        }
    }

    if changed {
        repository::touch(connection, auction.id).await?;
    }
    if checks.iter().any(|check| check.result.is_none()) {
        return Ok(changed);
    }

    if checks.len() < slots.len() {
        schedule_check(connection, auction, &members, &slots, checks.len()).await?;
    } else {
        award(connection, auction, &buyers, &checks).await?;
    }
    Ok(true)
}

async fn schedule_check(
    connection: &mut PgConnection,
    auction: &AuctionRow,
    members: &[Participant],
    slots: &[matching::Slot],
    position: usize,
) -> Result<()> {
    let slot = &slots[position];
    let left = members
        .iter()
        .find(|member| member.account_id == slot.left)
        .ok_or(ApiError::Internal)?;
    let right = members
        .iter()
        .find(|member| member.account_id == slot.right)
        .ok_or(ApiError::Internal)?;
    let id = Uuid::new_v4();
    let root = auction.round_root.as_ref().ok_or(ApiError::Internal)?;
    let scope = inputs::digest(
        "AUCTION_CHECK_V1",
        &(
            auction.id,
            &auction.policy_hash,
            &auction.input_root,
            root,
            auction.generation,
            position,
            slot.kind,
            left.id,
            right.id,
            &left.frozen_input,
            &right.frozen_input,
        ),
    )?;
    messages::create_bound_check(
        connection,
        BoundCheck {
            id,
            auction: auction.id,
            generation: auction.generation,
            title: format!(
                "{} · {} check {} of {}",
                auction.asset_snapshot["name"].as_str().unwrap_or("Auction"),
                if slot.kind == "reserve" {
                    "reserve"
                } else {
                    "bid"
                },
                position + 1,
                slots.len()
            ),
            node: auction.node_id.ok_or(ApiError::Internal)?,
            width: auction.width as u8,
            predicate: slot.predicate,
            scope,
            inputs: [
                serde_json::from_value(left.frozen_input.clone().ok_or(ApiError::Internal)?)
                    .map_err(|_| ApiError::Internal)?,
                serde_json::from_value(right.frozen_input.clone().ok_or(ApiError::Internal)?)
                    .map_err(|_| ApiError::Internal)?,
            ],
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO auction_checks(session_id,auction_id,generation,slot,kind,left_account,
            right_account,round_root) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(auction.id)
    .bind(auction.generation)
    .bind(position as i32)
    .bind(slot.kind)
    .bind(slot.left)
    .bind(slot.right)
    .bind(root)
    .execute(&mut *connection)
    .await?;
    repository::touch(connection, auction.id).await
}

async fn award(
    connection: &mut PgConnection,
    auction: &AuctionRow,
    buyers: &[Uuid],
    checks: &[Check],
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
            "UPDATE auctions SET status='awaiting_completion',winner_id=$2,award_hash=$3,
            completion_request_id=$4,revision=nextval('message_revision') WHERE id=$1",
        )
        .bind(auction.id)
        .bind(winner)
        .bind(award)
        .bind(Uuid::new_v4())
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

fn result(state: &AppState, check: &Check) -> Result<bool> {
    let reference: ArtifactRef =
        serde_json::from_value(check.output.clone().ok_or(ApiError::Internal)?)
            .map_err(|_| ApiError::Internal)?;
    let context: ContextRecord =
        serde_json::from_value(check.check_context.clone().ok_or(ApiError::Internal)?)
            .map_err(|_| ApiError::Internal)?;
    let record: MechanicalResult = state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .artifacts
        .record(&reference)
        .map_err(|_| ApiError::Unavailable)?;
    if record.schema_version != 1
        || record.context != context.restore().map_err(|_| ApiError::Internal)?.id()
        || record.assurance != "honest_role_mechanics"
        || record.proof_status != "unavailable"
        || record.final_audit != "not_released"
    {
        return Err(ApiError::Conflict("Invalid comparison result"));
    }
    Ok(record.value)
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
