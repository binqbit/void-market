use super::{
    auction, execution,
    model::{Operation, Payload},
    repository,
};
use crate::{
    assets, auctions, events,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
    transfers,
};
use serde_json::{Value, json};

pub(crate) async fn apply(
    state: &AppState,
    operation: &Operation,
    payload: &Payload,
    evidence: Value,
) -> Result<()> {
    let mut tx = state.pool.begin().await?;
    if let Some(asset) = operation.asset_id {
        assets::repository::lock(&mut tx, asset).await?;
    }
    let parent = if let Some(id) = operation.auction_id {
        Some(auctions::lock(&mut tx, id).await?)
    } else {
        None
    };
    let current = repository::lock(&mut tx, operation.id).await?;
    if current.status == "confirmed" {
        return Ok(());
    }
    if current.transaction_id != operation.transaction_id {
        return Err(ApiError::Conflict("Native transaction identity changed"));
    }
    let txid = operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;
    let bytes = serde_json::to_vec(&evidence).map_err(|_| ApiError::Internal)?;
    sqlx::query("UPDATE chain_operations SET receipt_sealed=$2 WHERE id=$1")
        .bind(operation.id)
        .bind(execution::seal_blob(state, operation, "receipt", &bytes)?)
        .execute(&mut *tx)
        .await?;
    match (operation.kind.as_str(), payload) {
        ("mint", Payload::Ethereum(plan)) => {
            let metadata = plan.metadata.as_ref().ok_or(ApiError::Internal)?;
            let deployment = crate::demo::repository::deployment(&state.pool).await?;
            let nft = json!({"chain_id":31337,"genesis":deployment.ethereum_genesis,"collection":plan.movement.collection,
                "token_id":plan.movement.token_id,"mint_transaction":txid});
            sqlx::query("INSERT INTO assets(id,name,description,image,owner_id,nft) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(operation.id)
                .bind(metadata["name"].as_str().ok_or(ApiError::Internal)?)
                .bind(metadata["description"].as_str().ok_or(ApiError::Internal)?)
                .bind(format!("demo:{}",metadata["variant"].as_u64().ok_or(ApiError::Internal)?))
                .bind(operation.account_id)
                .bind(nft)
                .execute(&mut *tx).await?;
            sqlx::query("UPDATE chain_operations SET asset_id=$1 WHERE id=$1")
                .bind(operation.id)
                .execute(&mut *tx)
                .await?;
            events::record(&mut tx,"AssetRegistered.v1",json!({"asset_id":operation.id,"owner_id":operation.account_id,"transaction_id":txid})).await?;
        }
        ("deposit", Payload::Ethereum(_)) => {
            let auction = parent.as_ref().ok_or(ApiError::Internal)?;
            let open = auction.status == "deposit_pending"
                && auction.bidding_deadline() > messages::now_ms()?;
            sqlx::query("UPDATE auctions SET custody_status='held',status=CASE WHEN $2 THEN 'open' WHEN
         status='deposit_pending' THEN 'no_sale' ELSE status END,revision=nextval('message_revision')
         WHERE id=$1")
                .bind(auction.id)
                .bind(open)
                .execute(&mut *tx).await?;
        }
        ("return", Payload::Ethereum(_)) => {
            let auction = parent.as_ref().ok_or(ApiError::Internal)?;
            sqlx::query("UPDATE auctions SET custody_status='returned',revision=nextval('message_revision') WHERE id=$1")
                .bind(auction.id)
                .execute(&mut *tx).await?;
        }
        ("delivery" | "transfer", Payload::Ethereum(_)) => {
            let asset = operation.asset_id.ok_or(ApiError::Internal)?;
            let buyer = operation.recipient_id.ok_or(ApiError::Internal)?;
            let seller = parent
                .as_ref()
                .map(|auction| auction.seller_id)
                .unwrap_or(operation.account_id);
            assets::repository::change_owner(&mut tx, asset, buyer).await?;
            sqlx::query("UPDATE assets SET pending_transaction_id=NULL WHERE id=$1")
                .bind(asset)
                .execute(&mut *tx)
                .await?;
            transfers::repository::insert(&mut tx, operation.id, asset, seller, buyer).await?;
            if let Some(auction) = parent.as_ref() {
                if auction.payment_status != "confirmed" || auction.winner_id != Some(buyer) {
                    return Err(ApiError::Conflict(
                        "NFT delivery no longer matches the paid award",
                    ));
                }
                sqlx::query("UPDATE transfers SET auction_id=$2 WHERE id=$1")
                    .bind(operation.id)
                    .bind(auction.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE auctions SET
         status='completed',custody_status='delivered',finished_at_ms=$2,revision=nextval('message_revision')
         WHERE id=$1")
                    .bind(auction.id)
                    .bind(messages::now_ms()?)
                    .execute(&mut *tx).await?;
                sqlx::query("UPDATE auction_participants SET chargeable=FALSE WHERE auction_id=$1")
                    .bind(auction.id)
                    .execute(&mut *tx)
                    .await?;
                events::record_node(&mut tx,"auction.completed",json!({"auction_id":auction.id,"transfer_id":operation.id,"transaction_id":txid})).await?;
            }
            events::record(
                &mut tx,
                "AssetTransferred.v1",
                json!({
                    "asset_id": asset, "sender_id": seller, "recipient_id": buyer,
                    "transfer_id": operation.id, "transaction_id": txid,
                }),
            )
            .await?;
        }
        ("payment", Payload::Zcash(plan)) => {
            let auction = parent.as_ref().ok_or(ApiError::Internal)?;
            if auction.winner_id != Some(operation.account_id)
                || auction.status != "awaiting_completion"
            {
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
            sqlx::query("INSERT INTO
         payment_credits(auction_id,operation_id,transaction_id,output_index,value_sealed,block_hash,block_height)
         VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(auction.id)
                .bind(operation.id)
                .bind(txid)
                .bind(evidence["action"].as_i64().ok_or(ApiError::Internal)? as i32)
                .bind(value)
                .bind(evidence["block_hash"].as_str().ok_or(ApiError::Internal)?)
                .bind(evidence["height"].as_i64().ok_or(ApiError::Internal)?)
                .execute(&mut *tx).await?;
            let paid = auction::paid(state, &mut tx, auction).await?;
            let due = auction::amount_due(state, &mut tx, auction).await?;
            sqlx::query("UPDATE auctions SET payment_status=$2,settlement_note=$3,revision=nextval('message_revision')
         WHERE id=$1")
                .bind(auction.id)
                .bind(if paid>=due {"confirmed"}else{"pending"})
                .bind(if paid>=due {None}else{Some("A top-up is required to complete payment")})
                .execute(&mut *tx).await?;
        }
        ("airdrop", Payload::Zcash(_)) => {}
        _ => return Err(ApiError::Internal),
    }
    repository::status(&mut tx, operation.id, "confirmed", None).await?;
    tx.commit().await?;
    Ok(())
}
