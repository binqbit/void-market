use super::{
    inputs, matching,
    model::{AuctionRow, Participant},
    repository,
};
use crate::{
    http::error::{ApiError, Result},
    messages::{self, model::BoundCheck},
    state::AppState,
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::ContextRecord;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub(super) struct Check {
    pub(super) session_id: Uuid,
    kind: String,
    left_account: Uuid,
    right_account: Uuid,
    pub(super) result: Option<bool>,
    pub(super) result_ref: Option<Value>,
    pub(super) status: String,
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

impl Check {
    pub(super) fn matches(&self, slot: &matching::Slot) -> bool {
        self.kind == slot.kind && self.left_account == slot.left && self.right_account == slot.right
    }

    pub(super) fn decode_result(&self, state: &AppState) -> Result<bool> {
        let reference: ArtifactRef =
            serde_json::from_value(self.output.clone().ok_or(ApiError::Internal)?)
                .map_err(|_| ApiError::Internal)?;
        let context: ContextRecord =
            serde_json::from_value(self.check_context.clone().ok_or(ApiError::Internal)?)
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
}

pub(super) async fn load(
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<Vec<Check>> {
    Ok(sqlx::query_as(
        "SELECT
             c.session_id,
             c.kind,
             c.left_account,
             c.right_account,
             c.result,
             c.result_ref,
             s.status,
             j.output,
             s.check_context
         FROM auction_checks c
         JOIN approval_sessions s ON s.id = c.session_id
         LEFT JOIN arbiter_jobs j ON j.id = s.job_id
         JOIN auction_participants l
             ON l.auction_id = c.auction_id
                 AND l.account_id = c.left_account
         JOIN auction_participants r
             ON r.auction_id = c.auction_id
                 AND r.account_id = c.right_account
         WHERE
             c.auction_id = $1
             AND c.generation = $2
             AND l.status = 'active'
             AND r.status = 'active'
         ORDER BY c.slot",
    )
    .bind(auction.id)
    .bind(auction.generation.max(1))
    .fetch_all(&mut *connection)
    .await?)
}

pub(super) async fn store_result(
    connection: &mut PgConnection,
    check: &mut Check,
    value: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE auction_checks
         SET
             result = $2,
             result_ref = $3
         WHERE
             session_id = $1
             AND result IS NULL",
    )
    .bind(check.session_id)
    .bind(value)
    .bind(&check.output)
    .execute(&mut *connection)
    .await?;
    check.result = Some(value);
    check.result_ref = check.output.clone();
    Ok(())
}

pub(super) fn in_schedule_order<'a>(
    checks: &'a [Check],
    slots: &[matching::Slot],
) -> Result<Vec<&'a Check>> {
    slots
        .iter()
        .map(|slot| {
            checks
                .iter()
                .find(|check| check.matches(slot))
                .ok_or(ApiError::Internal)
        })
        .collect()
}

/// Enqueue every missing independent comparison; no waiting for a preceding slot.
pub(super) async fn schedule_missing(
    connection: &mut PgConnection,
    auction: &AuctionRow,
    members: &[Participant],
    slots: &[matching::Slot],
    checks: &[Check],
) -> Result<bool> {
    let mut scheduled = false;
    let mut position: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(slot), -1) + 1
         FROM auction_checks
         WHERE
             auction_id = $1
             AND generation = $2",
    )
    .bind(auction.id)
    .bind(auction.generation.max(1))
    .fetch_one(&mut *connection)
    .await?;
    for slot in slots {
        if !checks.iter().any(|check| check.matches(slot)) {
            schedule_check(connection, auction, members, slot, position).await?;
            position += 1;
            scheduled = true;
        }
    }
    Ok(scheduled)
}

async fn schedule_check(
    connection: &mut PgConnection,
    auction: &AuctionRow,
    members: &[Participant],
    slot: &matching::Slot,
    position: i32,
) -> Result<()> {
    let left = members
        .iter()
        .find(|member| member.account_id == slot.left)
        .ok_or(ApiError::Internal)?;
    let right = members
        .iter()
        .find(|member| member.account_id == slot.right)
        .ok_or(ApiError::Internal)?;
    let id = Uuid::new_v4();
    let scope = inputs::digest(
        "AUCTION_PAIR_V2",
        &(
            auction.id,
            &auction.policy_hash,
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
            generation: auction.generation.max(1),
            title: format!(
                "{} · {} check",
                auction.asset_snapshot["name"].as_str().unwrap_or("Auction"),
                if slot.kind == "reserve" {
                    "reserve"
                } else {
                    "bid"
                },
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
        "INSERT INTO auction_checks (
             session_id,
             auction_id,
             generation,
             slot,
             kind,
             left_account,
             right_account,
             round_root,
             precomputed
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)",
    )
    .bind(id)
    .bind(auction.id)
    .bind(auction.generation.max(1))
    .bind(position)
    .bind(slot.kind)
    .bind(slot.left)
    .bind(slot.right)
    .bind(&auction.round_root)
    .execute(&mut *connection)
    .await?;
    repository::touch(connection, auction.id).await
}
