use super::{
    model::{EvmPlan, Payload, PaymentPlan},
    repository::{self, NewOperation},
};
use crate::{
    auctions::{self, model::AuctionRow},
    chains::{
        ethereum::{NftMovement, abi},
        zcash::memo::{MemoBinding, id_reference, reference},
    },
    demo,
    http::error::{ApiError, Result},
    state::AppState,
};
use rand::{TryRngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
struct Invoice {
    id: Uuid,
    generation: i64,
    nonce: [u8; 32],
    obligation: [u8; 32],
    issue: [u8; 32],
    recipient: [u8; 32],
}

pub(crate) async fn deposit(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<()> {
    let actor = demo::repository::actor(&state.pool, auction.seller_id).await?;
    let custodian = custodian(connection, auction).await?;
    let asset = crate::assets::repository::get(&state.pool, auction.asset_id).await?;
    let nft = asset.nft.ok_or(ApiError::Conflict(
        "Use a native local NFT for this auction",
    ))?;
    let (collection, token) = descriptor(state, &nft).await?;
    if state
        .chains
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .ethereum
        .owner(&collection, &token, json!("latest"))
        .await?
        != actor.ethereum_address
    {
        return Err(ApiError::Conflict(
            "Seller does not own the NFT on Ethereum",
        ));
    }
    let payload = movement(&actor.ethereum_address, &custodian, &collection, &token)?;
    repository::insert(
        state,
        connection,
        NewOperation {
            id: Uuid::new_v4(),
            account: auction.seller_id,
            recipient: None,
            auction: Some(auction.id),
            asset: Some(auction.asset_id),
            kind: "deposit",
            payload,
            approval: None,
        },
    )
    .await?;
    // Publication consent fixes return and timeout rules; the deposit itself still needs a separate approval.
    Ok(())
}

pub(crate) async fn custodian(
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<String> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT ethereum_address FROM arbiter_nodes WHERE id = $1")
            .bind(auction.node_id)
            .fetch_one(connection)
            .await?;
    abi::address(value.as_deref().ok_or(ApiError::Conflict(
        "Arbiter has no registered Ethereum custody address",
    ))?)
}

pub(crate) async fn descriptor(state: &AppState, nft: &Value) -> Result<(String, String)> {
    let deployment = demo::repository::deployment(&state.pool).await?;
    let collection = abi::address(nft["collection"].as_str().ok_or(ApiError::Internal)?)?;
    if nft["chain_id"] != 31337 || nft["genesis"] != deployment.ethereum_genesis {
        return Err(ApiError::Conflict(
            "NFT belongs to a different Ethereum network",
        ));
    }
    let token = nft["token_id"]
        .as_str()
        .ok_or(ApiError::Internal)?
        .to_owned();
    abi::owner_of(&token)?;
    Ok((collection, token))
}

fn movement(from: &str, to: &str, collection: &str, token: &str) -> Result<Payload> {
    Ok(Payload::Ethereum(Box::new(EvmPlan {
        sender: from.to_owned(),
        contract: collection.to_owned(),
        calldata: abi::transfer(from, to, token)?,
        movement: NftMovement {
            collection: collection.to_owned(),
            token_id: token.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
        },
        transaction: None,
        metadata: None,
    })))
}

pub(crate) async fn return_asset(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<()> {
    if auction.custody_status != "held" {
        return Ok(());
    }
    let custodian = custodian(connection, auction).await?;
    let seller = demo::repository::actor(&state.pool, auction.seller_id).await?;
    let (collection, token) = descriptor(state, &auction.asset_snapshot["nft"]).await?;
    repository::insert(
        state,
        connection,
        NewOperation {
            id: Uuid::new_v4(),
            account: auction.seller_id,
            recipient: Some(auction.seller_id),
            auction: Some(auction.id),
            asset: Some(auction.asset_id),
            kind: "return",
            payload: movement(&custodian, &seller.ethereum_address, &collection, &token)?,
            approval: auction.publish_receipt.clone(),
        },
    )
    .await?;
    sqlx::query(
        "UPDATE auctions
         SET
             custody_status = 'return_pending',
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn deliver(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
    approval: Value,
) -> Result<()> {
    if auction.payment_status != "confirmed" || auction.custody_status != "held" {
        return Err(ApiError::Conflict(
            "Delivery requires confirmed full payment and NFT custody",
        ));
    }
    let winner = auction.winner_id.ok_or(ApiError::Internal)?;
    let custodian = custodian(connection, auction).await?;
    let buyer = demo::repository::actor(&state.pool, winner).await?;
    let (collection, token) = descriptor(state, &auction.asset_snapshot["nft"]).await?;
    repository::insert(
        state,
        connection,
        NewOperation {
            id: auction.completion_request_id.ok_or(ApiError::Internal)?,
            account: auction.seller_id,
            recipient: Some(winner),
            auction: Some(auction.id),
            asset: Some(auction.asset_id),
            kind: "delivery",
            payload: movement(&custodian, &buyer.ethereum_address, &collection, &token)?,
            approval: Some(approval.clone()),
        },
    )
    .await?;
    sqlx::query(
        "UPDATE auctions
         SET
             custody_status = 'delivery_pending',
             completion_receipt = $2,
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .bind(approval)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn paid(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<u64> {
    let values: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT value_sealed
         FROM payment_credits
         WHERE auction_id = $1
         ORDER BY
             transaction_id,
             output_index",
    )
    .bind(auction.id)
    .fetch_all(connection)
    .await?;
    let owner = auction.winner_id.ok_or(ApiError::Internal)?;
    let mut total = 0u64;
    for value in values {
        let bytes = state
            .custody
            .as_ref()
            .ok_or(ApiError::Unavailable)?
            .vault
            .open(owner, &format!("payment-credit:{}", auction.id), &value)?;
        let value = u64::from_le_bytes(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Internal)?,
        );
        total = total.checked_add(value).ok_or(ApiError::Internal)?;
    }
    Ok(total)
}

pub(crate) async fn amount_due(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<u64> {
    let members = auctions::participants(connection, auction.id).await?;
    let buyer = members
        .iter()
        .find(|member| Some(member.account_id) == auction.winner_id)
        .ok_or(ApiError::Internal)?;
    auctions::participant_amount(state, auction.id, buyer)
}

pub(crate) async fn payment_started(connection: &mut PgConnection, id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT
             EXISTS
             (
                 SELECT 1
                 FROM chain_operations
                 WHERE
                     auction_id = $1
                     AND kind = 'payment'
                     AND approval IS NOT NULL
             )
             OR EXISTS
             (
                 SELECT 1
                 FROM payment_credits
                 WHERE auction_id = $1
             )",
    )
    .bind(id)
    .fetch_one(connection)
    .await?)
}

pub(crate) async fn ensure_payment(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
) -> Result<()> {
    if auction.status != "awaiting_completion" || auction.payment_status == "confirmed" {
        return Ok(());
    }
    let active: bool = sqlx::query_scalar(
        "SELECT COALESCE(
             (
                 SELECT status <> 'confirmed'
                 FROM chain_operations
                 WHERE
                     auction_id = $1
                     AND kind = 'payment'
                     AND status <> 'cancelled'
                 ORDER BY created_at_ms DESC
                 LIMIT 1
             ), FALSE
         )",
    )
    .bind(auction.id)
    .fetch_one(&mut *connection)
    .await?;
    if !active {
        create_payment(state, connection, auction, Uuid::new_v4(), None).await?;
    }
    Ok(())
}

pub(crate) async fn create_payment(
    state: &AppState,
    connection: &mut PgConnection,
    auction: &AuctionRow,
    id: Uuid,
    requested: Option<u64>,
) -> Result<()> {
    let buyer =
        demo::repository::actor(&state.pool, auction.winner_id.ok_or(ApiError::Internal)?).await?;
    let seller = demo::repository::actor(&state.pool, auction.seller_id).await?;
    let deployment = demo::repository::deployment(&state.pool).await?;
    let due = amount_due(state, connection, auction).await?;
    let credited = paid(state, connection, auction).await?;
    let remaining = due
        .checked_sub(credited)
        .filter(|remaining| *remaining > 0)
        .ok_or(ApiError::Conflict("No payment remains"))?;
    let value = requested.unwrap_or(remaining);
    if value == 0 || value > remaining {
        return Err(ApiError::BadRequest(
            "Pay a positive amount up to the remaining invoice total",
        ));
    }
    let recipient_address = seller.zcash_address.ok_or(ApiError::Unavailable)?;
    let vault = &state.custody.as_ref().ok_or(ApiError::Unavailable)?.vault;
    let invoice = if let Some(bytes) = &auction.payment_invoice {
        let bytes = vault.open(
            auction.seller_id,
            &format!("payment-invoice:{}", auction.id),
            bytes,
        )?;
        serde_json::from_slice::<Invoice>(&bytes).map_err(|_| ApiError::Internal)?
    } else {
        let mut nonce = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| ApiError::Internal)?;
        let recipient = reference("recipient_registration", &recipient_address)?;
        let mut encoded = minicbor::Encoder::new(Vec::new());
        encoded
            .array(7)
            .and_then(|e| e.str("LOCALNET_CUSTODIAL_OBLIGATION_V1"))
            .and_then(|e| e.bytes(auction.id.as_bytes()))
            .and_then(|e| e.bytes(auction.award_hash.as_deref().unwrap_or_default()))
            .and_then(|e| e.bytes(auction.policy_hash.as_deref().unwrap_or_default()))
            .and_then(|e| e.str(&deployment.zcash_genesis))
            .and_then(|e| e.bytes(&recipient))
            .and_then(|e| e.bytes(&nonce))
            .map_err(|_| ApiError::Internal)?;
        let obligation: [u8; 32] = Sha256::digest(encoded.into_writer()).into();
        let invoice = Invoice {
            id: Uuid::new_v4(),
            generation: auction.generation,
            nonce,
            obligation,
            issue: Sha256::digest(obligation).into(),
            recipient,
        };
        let sealed = vault.seal(
            auction.seller_id,
            &format!("payment-invoice:{}", auction.id),
            &serde_json::to_vec(&invoice).map_err(|_| ApiError::Internal)?,
        )?;
        sqlx::query("UPDATE auctions SET payment_invoice = $2 WHERE id = $1")
            .bind(auction.id)
            .bind(sealed)
            .execute(&mut *connection)
            .await?;
        invoice
    };
    if invoice.generation != auction.generation {
        return Err(ApiError::Conflict(
            "Invoice belongs to a different winning round",
        ));
    }
    let mut nonce = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| ApiError::Internal)?;
    let binding = MemoBinding {
        obligation_kind: 1,
        intent_class: if credited > 0 {
            3
        } else if value < remaining {
            2
        } else {
            1
        },
        network: reference("network", &deployment.zcash_genesis)?,
        context: id_reference("obligation_context_ref", auction.id)?,
        obligation: invoice.obligation,
        invoice: id_reference("invoice_id", invoice.id)?,
        issue: invoice.issue,
        invoice_nonce: invoice.nonce,
        intent: id_reference("intent_id", id)?,
        intent_nonce: nonce,
        recipient: invoice.recipient,
        revision: 1,
        generation: auction.generation as u64,
    };
    repository::insert(
        state,
        connection,
        NewOperation {
            id,
            account: buyer.account_id,
            recipient: Some(seller.account_id),
            auction: Some(auction.id),
            asset: Some(auction.asset_id),
            kind: "payment",
            payload: Payload::Zcash(PaymentPlan {
                request_amount: requested,
                signed_pczt: false,
                sender_account: buyer.zcash_account.ok_or(ApiError::Unavailable)?,
                sender_address: buyer.zcash_address.ok_or(ApiError::Unavailable)?,
                recipient_account: seller.zcash_account.ok_or(ApiError::Unavailable)?,
                recipient_address,
                value,
                memo: hex::encode(binding.encode()?),
                fee: None,
                expiry_height: None,
            }),
            approval: None,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE auctions
         SET
             payment_status = 'pending',
             revision = nextval('message_revision')
         WHERE id = $1",
    )
    .bind(auction.id)
    .execute(connection)
    .await?;
    Ok(())
}
