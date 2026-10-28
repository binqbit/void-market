use super::{auction, repository};
use crate::{
    auctions,
    http::error::{ApiError, Result},
    state::AppState,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentRequest {
    pub request_id: Uuid,
    pub generation: i64,
    pub amount: Option<String>,
}

pub async fn prepare(
    state: &AppState,
    account: Uuid,
    id: Uuid,
    input: PaymentRequest,
) -> Result<Uuid> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let value = input
        .amount
        .as_deref()
        .map(auctions::amount::parse)
        .transpose()?;
    let mut tx = state.pool.begin().await?;
    let row = auctions::lock(&mut tx, id).await?;
    if row.winner_id != Some(account) {
        return Err(ApiError::Forbidden);
    }
    if row.status != "awaiting_completion"
        || row.generation != input.generation
        || row.payment_status == "confirmed"
    {
        return Err(ApiError::Conflict(
            "This invoice is no longer awaiting payment",
        ));
    }
    let old: Option<(Uuid, Option<Uuid>)> =
        sqlx::query_as("SELECT account_id, auction_id FROM chain_operations WHERE id = $1")
            .bind(input.request_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some((owner, auction)) = old {
        if owner != account || auction != Some(id) {
            return Err(ApiError::Conflict(
                "Payment request belongs to another invoice",
            ));
        }
        let existing = repository::get(&state.pool, input.request_id).await?;
        match repository::payload(state, &existing)? {
            super::model::Payload::Zcash(plan) if plan.request_amount == value => {}
            _ => {
                return Err(ApiError::Conflict(
                    "This request ID has different payment details",
                ));
            }
        }
        return Ok(input.request_id);
    }
    let pending: Vec<(Uuid, Option<serde_json::Value>, Option<String>, String)> = sqlx::query_as(
        "SELECT
             id,
             approval,
             signed_transaction,
             status
         FROM chain_operations
         WHERE
             auction_id = $1
             AND kind = 'payment'
             AND status NOT IN (
                 'confirmed',
                 'cancelled'
             )",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for (operation, approval, signed, status) in pending {
        if status != "failed" && (approval.is_some() || signed.is_some()) {
            return Err(ApiError::Conflict(
                "Reconcile the existing payment before making another",
            ));
        }
        repository::status(&mut tx, operation, "cancelled", None).await?;
    }
    auction::create_payment(state, &mut tx, &row, input.request_id, value).await?;
    tx.commit().await?;
    Ok(input.request_id)
}
