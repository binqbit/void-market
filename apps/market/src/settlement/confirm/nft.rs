use crate::{
    assets,
    auctions::model::AuctionRow,
    events,
    http::error::{ApiError, Result},
    messages,
    settlement::model::{EvmPlan, Operation},
    state::AppState,
    transfers,
};
use serde_json::json;
use sqlx::PgConnection;

pub(super) async fn mint(
    state: &AppState,
    connection: &mut PgConnection,
    operation: &Operation,
    plan: &EvmPlan,
) -> Result<()> {
    let txid = operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;

    let metadata = plan.metadata.as_ref().ok_or(ApiError::Internal)?;
    let deployment = crate::demo::repository::deployment(&state.pool).await?;
    let nft = json!({
        "chain_id": 31337,
        "genesis": deployment.ethereum_genesis,
        "collection": plan.movement.collection,
        "token_id": plan.movement.token_id,
        "mint_transaction": txid,
    });

    sqlx::query(
        "INSERT INTO assets (id, name, description, image, owner_id, nft)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(operation.id)
    .bind(metadata["name"].as_str().ok_or(ApiError::Internal)?)
    .bind(metadata["description"].as_str().ok_or(ApiError::Internal)?)
    .bind(format!(
        "demo:{}",
        metadata["variant"].as_u64().ok_or(ApiError::Internal)?
    ))
    .bind(operation.account_id)
    .bind(nft)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE chain_operations SET asset_id = $1 WHERE id = $1")
        .bind(operation.id)
        .execute(&mut *connection)
        .await?;
    events::record(
        connection,
        "AssetRegistered.v1",
        json!({
            "asset_id": operation.id,
            "owner_id": operation.account_id,
            "transaction_id": txid,
        }),
    )
    .await?;
    Ok(())
}

pub(super) async fn deposit(connection: &mut PgConnection, auction: &AuctionRow) -> Result<()> {
    let open =
        auction.status == "deposit_pending" && auction.bidding_deadline() > messages::now_ms()?;
    sqlx::query(
        "UPDATE auctions
         SET
             custody_status = 'held',
             status = CASE
                 WHEN $2 THEN 'open'
                 WHEN status = 'deposit_pending' THEN 'no_sale'
                 ELSE status
             END,
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .bind(open)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub(super) async fn returned(connection: &mut PgConnection, auction: &AuctionRow) -> Result<()> {
    sqlx::query(
        "UPDATE auctions
         SET
             custody_status = 'returned',
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub(super) async fn withdrawal(
    connection: &mut PgConnection,
    operation: &Operation,
    plan: &EvmPlan,
) -> Result<()> {
    let txid = operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;

    let asset = operation.asset_id.ok_or(ApiError::Internal)?;
    let updated = sqlx::query(
        "UPDATE assets
         SET
             owner_id = NULL,
             pending_transaction_id = NULL
         WHERE
             id = $1
             AND owner_id = $2
             AND pending_transaction_id = $3",
    )
    .bind(asset)
    .bind(operation.account_id)
    .bind(operation.id)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::Conflict(
            "NFT withdrawal no longer matches its owner",
        ));
    }
    events::record(
        connection,
        "AssetWithdrawn.v1",
        json!({
            "asset_id": asset,
            "owner_id": operation.account_id,
            "destination": plan.movement.to,
            "transaction_id": txid,
        }),
    )
    .await?;
    Ok(())
}

pub(super) async fn delivery(
    connection: &mut PgConnection,
    operation: &Operation,
    parent: Option<&AuctionRow>,
) -> Result<()> {
    let txid = operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;

    let asset = operation.asset_id.ok_or(ApiError::Internal)?;
    let buyer = operation.recipient_id.ok_or(ApiError::Internal)?;
    let seller = parent
        .map(|auction| auction.seller_id)
        .unwrap_or(operation.account_id);
    assets::repository::change_owner(connection, asset, buyer).await?;
    sqlx::query("UPDATE assets SET pending_transaction_id = NULL WHERE id = $1")
        .bind(asset)
        .execute(&mut *connection)
        .await?;
    transfers::repository::insert(connection, operation.id, asset, seller, buyer).await?;
    if let Some(auction) = parent {
        if auction.payment_status != "confirmed" || auction.winner_id != Some(buyer) {
            return Err(ApiError::Conflict(
                "NFT delivery no longer matches the paid award",
            ));
        }
        sqlx::query("UPDATE transfers SET auction_id = $2 WHERE id = $1")
            .bind(operation.id)
            .bind(auction.id)
            .execute(&mut *connection)
            .await?;
        sqlx::query(
            "UPDATE auctions
             SET
                 status = 'completed',
                 custody_status = 'delivered',
                 finished_at_ms = $2,
                 revision = nextval('message_revision')
             WHERE id = $1",
        )
        .bind(auction.id)
        .bind(messages::now_ms()?)
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "UPDATE auction_participants
             SET chargeable = FALSE
             WHERE auction_id = $1",
        )
        .bind(auction.id)
        .execute(&mut *connection)
        .await?;
        events::record_node(
            connection,
            "auction.completed",
            json!({
                "auction_id": auction.id,
                "transfer_id": operation.id,
                "transaction_id": txid,
            }),
        )
        .await?;
    }
    events::record(
        connection,
        "AssetTransferred.v1",
        json!({
            "asset_id": asset,
            "sender_id": seller,
            "recipient_id": buyer,
            "transfer_id": operation.id,
            "transaction_id": txid,
        }),
    )
    .await?;
    Ok(())
}
