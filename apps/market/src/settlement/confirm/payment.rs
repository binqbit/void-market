use crate::{
    auctions::model::AuctionRow,
    http::error::{ApiError, Result},
    settlement::{
        auction,
        model::{Operation, PaymentPlan},
    },
    state::AppState,
};
use serde_json::Value;
use sqlx::PgConnection;

pub(super) async fn credit(
    state: &AppState,
    connection: &mut PgConnection,
    operation: &Operation,
    plan: &PaymentPlan,
    auction: &AuctionRow,
    evidence: &Value,
) -> Result<()> {
    let txid = operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;

    if auction.winner_id != Some(operation.account_id) || auction.status != "awaiting_completion" {
        return Err(ApiError::Conflict(
            "Payment does not belong to the current winner",
        ));
    }
    let value = state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .seal(
            operation.account_id,
            &format!("payment-credit:{}", auction.id),
            &plan.value.to_le_bytes(),
        )?;
    sqlx::query(
        "INSERT INTO payment_credits (
             auction_id,
             operation_id,
             transaction_id,
             output_index,
             value_sealed,
             block_hash,
             block_height
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(auction.id)
    .bind(operation.id)
    .bind(txid)
    .bind(evidence["action"].as_i64().ok_or(ApiError::Internal)? as i32)
    .bind(value)
    .bind(evidence["block_hash"].as_str().ok_or(ApiError::Internal)?)
    .bind(evidence["height"].as_i64().ok_or(ApiError::Internal)?)
    .execute(&mut *connection)
    .await?;

    let paid = auction::paid(state, connection, auction).await?;
    let due = auction::amount_due(state, connection, auction).await?;
    let (status, note) = if paid >= due {
        ("confirmed", None)
    } else {
        ("pending", Some("A top-up is required to complete payment"))
    };

    sqlx::query(
        "UPDATE auctions
         SET
             payment_status = $2,
             settlement_note = $3,
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .bind(status)
    .bind(note)
    .execute(&mut *connection)
    .await?;

    Ok(())
}
