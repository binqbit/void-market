use super::{clock, inputs, model::APPROVAL_WINDOW_MS, repository, worker};
use crate::{
    events,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use serde_json::json;
use uuid::Uuid;

pub async fn close(state: &AppState, seller: Uuid, id: Uuid) -> Result<()> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }

    let mut tx = state.pool.begin().await?;
    let auction = repository::lock(&mut tx, id).await?;
    if auction.execution != "localnet" {
        return Err(ApiError::NotFound);
    }
    if auction.seller_id != seller {
        return Err(ApiError::Forbidden);
    }
    // A repeated click or a lost response must not restart checks or their timers.
    let now = messages::now_ms()?;
    if auction.bidding_deadline() <= now
        && matches!(
            auction.status.as_str(),
            "open" | "matching" | "awaiting_completion" | "completed" | "no_sale" | "interrupted"
        )
    {
        return Ok(());
    }

    if auction.status != "open" {
        return Err(ApiError::Conflict("Bidding is no longer open"));
    }

    let digest = inputs::digest(
        "AUCTION_DEMO_CLOSE_V1",
        &(id, &auction.policy_hash, auction.closes_at_ms, now),
    )?;
    let approval = messages::sign_action(
        state,
        &mut tx,
        messages::ActionBinding {
            account: seller,
            session: id,
            action: id,
            request: id,
            revision: auction.revision,
            digest: digest
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Internal)?,
            purpose: "close_demo_bidding",
            received_ms: now,
        },
    )
    .await?;

    sqlx::query("UPDATE auctions SET closes_at_ms = $2 WHERE id = $1")
        .bind(id)
        .bind(now + APPROVAL_WINDOW_MS)
        .execute(&mut *tx)
        .await?;
    worker::freeze(&mut tx, &auction).await?;
    clock::meter(&mut tx, id, now).await?;
    events::record_node(
        &mut tx,
        "auction.bidding_closed_early",
        json!({"auction_id":id,"closed_at_ms":now,"previous_ends_at_ms":auction.closes_at_ms,
            "ends_at_ms":now + APPROVAL_WINDOW_MS,"approval":approval}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
