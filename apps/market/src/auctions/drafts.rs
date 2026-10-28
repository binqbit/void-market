use super::{
    amount, inputs,
    model::{AuctionRow, BUYER_LIMIT, CreateAuction, EditAuction, PublishAuction},
    repository,
};
use crate::{
    assets, events,
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use serde_json::json;
use uuid::Uuid;

fn closing_time(now: i64, close: i64) -> Result<()> {
    if close.saturating_sub(now) <= super::model::APPROVAL_WINDOW_MS
        || close.saturating_sub(now) > 30 * 24 * 60 * 60 * 1000
    {
        return Err(ApiError::BadRequest(
            "Choose an auction end more than one hour from now and within 30 days",
        ));
    }
    Ok(())
}

pub async fn create(state: &AppState, seller: Uuid, input: CreateAuction) -> Result<Uuid> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let reserve = amount::parse(&input.reserve)?;
    let bytes = serde_json::to_vec(&(1, input.asset_id, reserve, input.closes_at_ms))
        .map_err(|_| ApiError::Internal)?;
    let creation_hash = custody
        .vault
        .fingerprint(seller, "auction-create", &bytes)
        .to_vec();
    let mut tx = state.pool.begin().await?;
    let asset = assets::repository::lock(&mut tx, input.asset_id).await?;
    let existing: Option<AuctionRow> = sqlx::query_as("SELECT * FROM auctions WHERE id=$1")
        .bind(input.request_id)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(existing) = existing {
        if existing.seller_id != seller || existing.creation_hash != creation_hash {
            return Err(ApiError::Conflict(
                "Request ID has different auction details",
            ));
        }
        return Ok(existing.id);
    }
    if asset.owner_id != seller {
        return Err(ApiError::Forbidden);
    }
    if asset.active_auction_id.is_some() || asset.pending_transaction_id.is_some() {
        return Err(ApiError::Conflict(
            "This asset is already locked in an auction",
        ));
    }
    let execution = if asset.nft.is_some() {
        if state.chains.is_none() {
            return Err(ApiError::Unavailable);
        }
        crate::demo::repository::actor(&state.pool, seller).await?;
        "localnet"
    } else {
        if state.chains.is_some() {
            return Err(ApiError::Conflict(
                "Use Get NFT on the Demo page to obtain a native local NFT",
            ));
        }
        "internal"
    };
    let now = messages::now_ms()?;
    closing_time(now, input.closes_at_ms)?;
    custody.provision(&mut tx, seller).await?;
    let snapshot = json!({"id":asset.id,"name":asset.name,
        "description":asset.description,"image":asset.image,"nft":asset.nft});
    sqlx::query(
        "INSERT INTO auctions(id,asset_id,seller_id,closes_at_ms,asset_snapshot,
         creation_hash,created_at_ms,buyer_limit,execution) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(input.request_id)
    .bind(input.asset_id)
    .bind(seller)
    .bind(input.closes_at_ms)
    .bind(snapshot)
    .bind(creation_hash)
    .bind(now)
    .bind(BUYER_LIMIT as i16)
    .bind(execution)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO auction_participants(auction_id,account_id,id,input_sealed,created_at_ms)
         VALUES($1,$2,$1,$3,$4)",
    )
    .bind(input.request_id)
    .bind(seller)
    .bind(inputs::seal(state, input.request_id, seller, reserve)?)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(input.request_id)
}

pub async fn edit(state: &AppState, seller: Uuid, id: Uuid, input: EditAuction) -> Result<()> {
    let reserve = amount::parse(&input.reserve)?;
    let mut tx = state.pool.begin().await?;
    let auction = repository::lock(&mut tx, id).await?;
    owner_draft(&auction, seller)?;
    if auction.revision != input.revision {
        return Err(ApiError::Conflict(
            "The draft changed; refresh before saving",
        ));
    }
    closing_time(messages::now_ms()?, input.closes_at_ms)?;
    sqlx::query(
        "UPDATE auction_participants SET input_sealed=$2 WHERE auction_id=$1 AND account_id=$3",
    )
    .bind(id)
    .bind(inputs::seal(state, id, seller, reserve)?)
    .bind(seller)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE auctions SET closes_at_ms=$2,revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(id)
    .bind(input.closes_at_ms)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn cancel(state: &AppState, seller: Uuid, id: Uuid) -> Result<()> {
    let mut tx = state.pool.begin().await?;
    let auction = repository::lock(&mut tx, id).await?;
    if auction.seller_id != seller {
        return Err(ApiError::Forbidden);
    }
    if auction.status == "cancelled" {
        return Ok(());
    }
    owner_draft(&auction, seller)?;
    sqlx::query(
        "UPDATE auctions SET status='cancelled',finished_at_ms=$2,
        revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(id)
    .bind(messages::now_ms()?)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

fn owner_draft(auction: &AuctionRow, owner: Uuid) -> Result<()> {
    if auction.seller_id != owner {
        return Err(ApiError::Forbidden);
    }
    if auction.status != "draft" {
        return Err(ApiError::Conflict(
            "Published auction terms cannot be changed or cancelled",
        ));
    }
    Ok(())
}

pub async fn publish(
    state: &AppState,
    seller: Uuid,
    id: Uuid,
    input: PublishAuction,
) -> Result<()> {
    if !input.consent {
        return Err(ApiError::BadRequest(
            "Confirm the auction and asset-lock terms",
        ));
    }
    let original = repository::get(&state.pool, id).await?;
    if original.seller_id != seller {
        return Err(ApiError::Forbidden);
    }
    if original.status != "draft" {
        if original
            .publish_receipt
            .as_ref()
            .is_some_and(|receipt| receipt["revision"].as_i64() == Some(input.revision))
        {
            return Ok(());
        }
        return Err(ApiError::Conflict("This draft cannot be published"));
    }
    if original.revision != input.revision {
        return Err(ApiError::Conflict("The draft changed; review it again"));
    }
    let asset = assets::repository::get(&state.pool, original.asset_id).await?;
    if asset.owner_id != seller {
        return Err(ApiError::Forbidden);
    }
    if asset.active_auction_id.is_some() || asset.pending_transaction_id.is_some() {
        return Err(ApiError::Conflict(
            "This asset is already locked in another auction",
        ));
    }
    let mut connection = state.pool.acquire().await?;
    let members = repository::participants(&mut connection, id).await?;
    let owner = members
        .iter()
        .find(|member| member.account_id == seller)
        .ok_or(ApiError::Internal)?;
    let reserve = inputs::amount(state, id, owner)?;
    drop(connection);
    let frozen = inputs::encrypt(state, seller, reserve, original.width as u8).await?;

    let mut tx = state.pool.begin().await?;
    let asset = assets::repository::lock(&mut tx, original.asset_id).await?;
    let auction = repository::lock(&mut tx, id).await?;
    owner_draft(&auction, seller)?;
    if auction.revision != input.revision {
        return Err(ApiError::Conflict("The draft changed; review it again"));
    }
    if asset.owner_id != seller {
        return Err(ApiError::Forbidden);
    }
    if asset.active_auction_id.is_some() || asset.pending_transaction_id.is_some() {
        return Err(ApiError::Conflict(
            "This asset is already locked in another auction",
        ));
    }
    let now = messages::now_ms()?;
    closing_time(now, auction.closes_at_ms)?;
    let node: Option<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM arbiter_nodes n WHERE n.enabled AND n.heartbeat_until>now() AND ($1=FALSE OR n.ethereum_address IS NOT NULL)
         ORDER BY (SELECT count(*) FROM auctions a WHERE a.node_id=n.id
         AND a.status IN ('open','matching','awaiting_completion')),n.id LIMIT 1 FOR UPDATE OF n",
    )
    .bind(auction.execution == "localnet")
    .fetch_optional(&mut *tx)
    .await?;
    let node = node.ok_or(ApiError::Unavailable)?;
    let snapshot = json!({"id":asset.id,"name":asset.name,
        "description":asset.description,"image":asset.image,"nft":asset.nft});
    let native_terms = if auction.execution == "localnet" {
        let address: Option<String> =
            sqlx::query_scalar("SELECT ethereum_address FROM arbiter_nodes WHERE id=$1")
                .bind(node)
                .fetch_one(&mut *tx)
                .await?;
        json!({"custodian":address,"network":"ethereum-local-31337","payments":"ironwood-regtest",
            "seller_timeout":"accept_after_confirmed_full_payment","buyer_timeout":"breach_and_return",
            "confirmations":3,"partial_payments":"top_up_same_invoice"})
    } else {
        serde_json::Value::Null
    };
    let policy = inputs::digest(
        "AUCTION_POLICY_V4",
        &(
            id,
            &snapshot,
            node,
            auction.closes_at_ms,
            auction.width,
            auction.buyer_limit,
            "ZEC",
            "bid_ge_reserve",
            "earliest_admission",
            "early_masks_postclose_decryption",
            (
                "bids_close_before_auction_end",
                super::model::APPROVAL_WINDOW_MS,
            ),
            ("approval_budget_after_bidding_closes", 600_000),
            if auction.execution == "localnet" {
                "native_payment_then_seller_ack_or_timeout"
            } else {
                "seller_ack_payment_unverified"
            },
            &frozen,
            &native_terms,
        ),
    )?;
    let receipt = messages::sign_action(
        state,
        &mut tx,
        messages::ActionBinding {
            account: seller,
            session: id,
            action: id,
            request: id,
            revision: input.revision,
            digest: policy
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Internal)?,
            purpose: "publish_auction",
            received_ms: now,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE auction_participants SET frozen_input=$3 WHERE auction_id=$1 AND account_id=$2",
    )
    .bind(id)
    .bind(seller)
    .bind(json!(frozen))
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE auctions SET status=$7,custody_status=$8,node_id=$2,asset_snapshot=$3,policy_hash=$4,
         published_at_ms=$5,publish_receipt=$6,revision=nextval('message_revision') WHERE id=$1",
    )
    .bind(id)
    .bind(node)
    .bind(snapshot)
    .bind(policy)
    .bind(now)
    .bind(&receipt)
    .bind(if auction.execution == "localnet" { "deposit_pending" } else { "open" })
    .bind(if auction.execution == "localnet" { "deposit_pending" } else { "none" })
    .execute(&mut *tx)
    .await?;
    if auction.execution == "localnet" {
        let updated = repository::lock(&mut tx, id).await?;
        crate::settlement::auction::deposit(state, &mut tx, &updated).await?;
    }
    events::record_node(
        &mut tx,
        "auction.published",
        json!({"auction_id":id,"approval":receipt}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
