use super::{
    amount, clock, inputs,
    model::{CompleteSale, PlaceBid},
    participation, repository,
};
use crate::{
    assets, events,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
    transfers,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use uuid::Uuid;

pub async fn bid(state: &AppState, buyer: Uuid, id: Uuid, input: PlaceBid) -> Result<()> {
    let value = amount::parse(&input.amount)?;
    if value == 0 {
        return Err(ApiError::BadRequest("Bid at least 0.00000001 ZEC"));
    }
    let original = repository::get(&state.pool, id).await?;
    if original.seller_id == buyer {
        return Err(ApiError::Forbidden);
    }
    let policy = URL_SAFE_NO_PAD
        .decode(&input.policy_hash)
        .map_err(|_| ApiError::BadRequest("Invalid auction policy"))?;
    if original.policy_hash.as_ref() != Some(&policy) {
        return Err(ApiError::Conflict(
            "Auction terms changed; review them again",
        ));
    }

    let mut tx = state.pool.begin().await?;
    let auction = repository::lock(&mut tx, id).await?;
    if existing_bid(state, &mut tx, id, buyer, input.request_id, value).await? {
        return Ok(());
    }
    admit(&auction, buyer, value, messages::now_ms()?)?;
    capacity(&mut tx, &auction).await?;
    tx.commit().await?;
    let frozen = inputs::encrypt(state, buyer, value, original.width as u8).await?;

    let mut tx = state.pool.begin().await?;
    let auction = repository::lock(&mut tx, id).await?;
    if existing_bid(state, &mut tx, id, buyer, input.request_id, value).await? {
        return Ok(());
    }
    let now = messages::now_ms()?;
    admit(&auction, buyer, value, now)?;
    capacity(&mut tx, &auction).await?;
    let sequence: i64 = sqlx::query_scalar(
        "INSERT INTO auction_participants(auction_id,account_id,id,input_sealed,frozen_input,created_at_ms)
         VALUES($1,$2,$3,$4,$5,$6) RETURNING admission_seq",
    )
    .bind(id)
    .bind(buyer)
    .bind(input.request_id)
    .bind(inputs::seal(state, id, buyer, value)?)
    .bind(json!(frozen))
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    let digest = inputs::digest(
        "AUCTION_BID_V1",
        &(id, input.request_id, sequence, policy, frozen),
    )?;
    let receipt = messages::sign_action(
        state,
        &mut tx,
        messages::ActionBinding {
            account: buyer,
            session: id,
            action: input.request_id,
            request: id,
            revision: sequence,
            digest: digest
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Internal)?,
            purpose: "place_bid",
            received_ms: now,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE auction_participants SET admission_receipt=$3 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(id)
    .bind(buyer)
    .bind(&receipt)
    .execute(&mut *tx)
    .await?;
    repository::touch(&mut tx, id).await?;
    events::record_node(
        &mut tx,
        "auction.bid_admitted",
        json!({"auction_id":id,"bid_id":input.request_id,"approval":receipt}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn existing_bid(
    state: &AppState,
    connection: &mut sqlx::PgConnection,
    auction: Uuid,
    buyer: Uuid,
    request: Uuid,
    value: u64,
) -> Result<bool> {
    let members = repository::participants(connection, auction).await?;
    if let Some(existing) = members.iter().find(|member| member.account_id == buyer) {
        if existing.id != request || inputs::amount(state, auction, existing)? != value {
            return Err(ApiError::Conflict(
                "Your admitted bid is immutable; you cannot bid again",
            ));
        }
        return Ok(true);
    }
    Ok(false)
}

fn admit(auction: &super::model::AuctionRow, buyer: Uuid, value: u64, now: i64) -> Result<()> {
    if auction.seller_id == buyer {
        return Err(ApiError::Forbidden);
    }
    if auction.status != "open" || auction.closes_at_ms <= now {
        return Err(ApiError::Conflict("This auction no longer accepts bids"));
    }
    let maximum = (1u64 << (auction.width - 1)) - 1;
    if value > maximum {
        return Err(ApiError::BadRequest(
            "Bid exceeds this auction's amount range",
        ));
    }
    Ok(())
}

pub async fn withdraw(state: &AppState, buyer: Uuid, id: Uuid) -> Result<Value> {
    let mut tx = state.pool.begin().await?;
    let auction = repository::lock(&mut tx, id).await?;
    if auction.seller_id == buyer {
        return Err(ApiError::Forbidden);
    }
    let members = repository::participants(&mut tx, id).await?;
    let own = members
        .iter()
        .find(|member| member.account_id == buyer)
        .ok_or(ApiError::NotFound)?;
    if own.status == "withdrawn" {
        return own.exit_receipt.clone().ok_or(ApiError::Internal);
    }
    if own.status != "active"
        || !matches!(
            auction.status.as_str(),
            "open" | "matching" | "awaiting_completion"
        )
    {
        return Err(ApiError::Conflict("Participation is no longer active"));
    }
    let now = messages::now_ms()?;
    clock::meter(&mut tx, id, now).await?;
    if clock::enforce(&mut tx, id).await? {
        tx.commit().await?;
        return Err(ApiError::Conflict(
            "An approval budget expired; refresh the auction",
        ));
    }
    let digest = auction.policy_hash.as_deref().ok_or(ApiError::Internal)?;
    let receipt = messages::sign_action(
        state,
        &mut tx,
        messages::ActionBinding {
            account: buyer,
            session: id,
            action: own.id,
            request: id,
            revision: auction.generation,
            digest: digest.try_into().map_err(|_| ApiError::Internal)?,
            purpose: "withdraw_bid",
            received_ms: now,
        },
    )
    .await?;
    participation::exclude(&mut tx, id, buyer, "withdrawn").await?;
    sqlx::query(
        "UPDATE auction_participants SET exit_receipt=$3 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(id)
    .bind(buyer)
    .bind(&receipt)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(receipt)
}

pub async fn complete(
    state: &AppState,
    seller: Uuid,
    id: Uuid,
    input: CompleteSale,
) -> Result<Value> {
    let original = repository::get(&state.pool, id).await?;
    if original.seller_id != seller {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.pool.begin().await?;
    let asset = assets::repository::lock(&mut tx, original.asset_id).await?;
    let auction = repository::lock(&mut tx, id).await?;
    if let Some(receipt) = &auction.completion_receipt {
        if auction.completion_request_id == Some(input.request_id)
            && receipt["revision"].as_i64() == Some(input.generation)
        {
            return Ok(receipt.clone());
        }
        return Err(ApiError::Conflict(
            "This sale already has a different completion receipt",
        ));
    }
    if auction.status != "awaiting_completion"
        || auction.completion_request_id != Some(input.request_id)
        || auction.generation != input.generation
    {
        return Err(ApiError::Conflict(
            "The sale request changed; review the current result",
        ));
    }
    if asset.owner_id != seller || asset.active_auction_id != Some(id) {
        return Err(ApiError::Conflict(
            "The auction no longer controls this asset",
        ));
    }
    let now = messages::now_ms()?;
    clock::meter(&mut tx, id, now).await?;
    if clock::enforce(&mut tx, id).await? {
        tx.commit().await?;
        return Err(ApiError::Conflict(
            "An approval budget expired; refresh the auction",
        ));
    }
    let winner = auction.winner_id.ok_or(ApiError::Internal)?;
    let award = auction.award_hash.as_ref().ok_or(ApiError::Internal)?;
    let transfer = Uuid::new_v4();
    let digest = inputs::digest(
        "AUCTION_DELIVERY_V1",
        &(
            id,
            auction.generation,
            award,
            asset.id,
            seller,
            winner,
            transfer,
            "payment_unverified",
        ),
    )?;
    let mut receipt = messages::sign_action(
        state,
        &mut tx,
        messages::ActionBinding {
            account: seller,
            session: id,
            action: input.request_id,
            request: id,
            revision: input.generation,
            digest: digest
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Internal)?,
            purpose: "complete_sale_payment_unverified",
            received_ms: now,
        },
    )
    .await?;
    receipt["transfer_id"] = json!(transfer);
    receipt["payment_status"] = json!("unverified");
    assets::repository::change_owner(&mut tx, asset.id, winner).await?;
    transfers::repository::insert(&mut tx, transfer, asset.id, seller, winner).await?;
    sqlx::query("UPDATE transfers SET auction_id=$2 WHERE id=$1")
        .bind(transfer)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE auctions SET status='completed',completion_receipt=$2,finished_at_ms=$3,
        revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(id)
    .bind(&receipt)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE auction_participants SET chargeable=FALSE WHERE auction_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    events::record(
        &mut tx,
        "AssetTransferred.v1",
        json!({"asset_id":asset.id,
        "sender_id":seller,"recipient_id":winner,"transfer_id":transfer,"auction_id":id}),
    )
    .await?;
    events::record_node(
        &mut tx,
        "auction.completed",
        json!({"auction_id":id,"transfer_id":transfer}),
    )
    .await?;
    tx.commit().await?;
    Ok(receipt)
}

async fn capacity(
    connection: &mut sqlx::PgConnection,
    auction: &super::model::AuctionRow,
) -> Result<()> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auction_participants WHERE auction_id=$1 AND account_id<>$2",
    )
    .bind(auction.id)
    .bind(auction.seller_id)
    .fetch_one(connection)
    .await?;
    if count >= i64::from(auction.buyer_limit) {
        return Err(ApiError::Conflict(
            "This auction has reached its lifetime buyer limit",
        ));
    }
    Ok(())
}
