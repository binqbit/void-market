use super::{
    model::{ApproveOperation, OperationView, Payload},
    repository,
};
use crate::{
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn view(state: &AppState, account: Uuid, id: Uuid) -> Result<OperationView> {
    let operation = repository::get(&state.pool, id).await?;
    if operation.account_id != account && operation.recipient_id != Some(account) {
        return Err(ApiError::NotFound);
    }
    let payload = repository::payload(state, &operation)?;
    let (network, transaction, nft, amount, fee, recipient) = match payload {
        Payload::Ethereum(plan) => (
            "ethereum",
            plan.transaction,
            Some(plan.movement),
            None,
            None,
            None,
        ),
        Payload::Zcash(plan) => (
            "zcash",
            None,
            None,
            Some(crate::auctions::amount::format(plan.value)),
            plan.fee.map(crate::auctions::amount::format),
            Some(plan.recipient_address),
        ),
    };
    let (remaining_ms, timer_running) = if let Some(auction) = operation.auction_id {
        let row:Option<(i64,bool)>=sqlx::query_as(
            "SELECT p.remaining_ms,p.chargeable AND a.closes_at_ms<=$3 AND COALESCE(r.healthy AND r.last_tick_ms>$3-5000,FALSE) FROM
         auction_participants p JOIN auctions a ON a.id=p.auction_id
         LEFT JOIN localnet_runtime r ON r.singleton WHERE p.auction_id=$1 AND
         p.account_id=$2",
        )
        .bind(auction)
        .bind(account)
        .bind(messages::now_ms()?)
        .fetch_optional(&state.pool).await?;
        row.map(|(remaining, running)| (Some(remaining), running))
            .unwrap_or((None, false))
    } else {
        (None, false)
    };
    let title = match operation.kind.as_str() {
        "mint" => "Receive a demo NFT",
        "airdrop" => "Claim local ZEC",
        "deposit" => "Approve NFT custody",
        "return" => "Return NFT to seller",
        "delivery" => "Deliver NFT to buyer",
        "transfer" => "Transfer NFT",
        "payment" => "Pay the auction invoice",
        _ => return Err(ApiError::Internal),
    }
    .to_owned();
    let confirmation = if let Some(bytes) = &operation.receipt_sealed {
        let bytes = state
            .custody
            .as_ref()
            .ok_or(ApiError::Unavailable)?
            .vault
            .open(
                operation.account_id,
                &format!("native-receipt:{}", operation.id),
                bytes,
            )?;
        let evidence: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| ApiError::Internal)?;
        Some(if network == "ethereum" {
            serde_json::json!({"block_hash":evidence["blockHash"],"block_number":evidence["blockNumber"],"status":evidence["status"]})
        } else {
            evidence
        })
    } else {
        None
    };
    Ok(OperationView {
        id: operation.id,
        account_id: operation.account_id,
        auction_id: operation.auction_id,
        asset_id: operation.asset_id,
        kind: operation.kind,
        status: operation.status.clone(),
        network,
        title,
        transaction_id: operation.transaction_id,
        ethereum_transaction: transaction,
        nft,
        amount,
        fee,
        recipient,
        error: operation.error,
        revision: operation.revision,
        can_approve: operation.status == "ready"
            && operation.account_id == account
            && remaining_ms.is_none_or(|remaining| remaining > 0),
        remaining_ms,
        timer_running,
        created_at_ms: operation.created_at_ms,
        updated_at_ms: operation.updated_at_ms,
        confirmation,
    })
}

pub async fn list(state: &AppState, account: Uuid) -> Result<Vec<OperationView>> {
    if state.chains.is_none() {
        return Ok(Vec::new());
    }
    let ids:Vec<Uuid>=sqlx::query_scalar(
        "SELECT id FROM chain_operations WHERE account_id=$1 OR recipient_id=$1 ORDER BY created_at_ms
         DESC,id LIMIT 100",
    )
    .bind(account)
    .fetch_all(&state.pool).await?;
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        result.push(view(state, account, id).await?);
    }
    Ok(result)
}

pub async fn approve(
    state: &AppState,
    account: Uuid,
    id: Uuid,
    input: ApproveOperation,
) -> Result<()> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let original = repository::get(&state.pool, id).await?;
    if original.account_id != account {
        return Err(ApiError::NotFound);
    }
    let mut tx = state.pool.begin().await?;
    if let Some(auction) = original.auction_id {
        crate::auctions::lock(&mut tx, auction).await?;
        crate::auctions::meter(&mut tx, auction, messages::now_ms()?).await?;
    }
    let operation = repository::lock(&mut tx, id).await?;
    if let Some(approval) = &operation.approval {
        if approval["revision"].as_i64() == Some(input.revision) {
            return Ok(());
        }
        return Err(ApiError::Conflict(
            "This transaction has a different approval revision",
        ));
    }
    if operation.status != "ready" || operation.revision != input.revision {
        return Err(ApiError::Conflict(
            "The transaction changed; review it again",
        ));
    }
    if let Some(auction) = operation.auction_id {
        let (remaining,status):(i64,String)=sqlx::query_as("SELECT remaining_ms,status FROM auction_participants WHERE auction_id=$1 AND account_id=$2")
            .bind(auction)
            .bind(account)
            .fetch_one(&mut *tx).await?;
        if remaining <= 0 || status != "active" {
            return Err(ApiError::Conflict("The auction approval budget expired"));
        }
    }
    let receipt = sign(
        state,
        &mut tx,
        account,
        id,
        operation.revision,
        &repository::payload(state, &operation)?,
    )
    .await?;
    sqlx::query("UPDATE chain_operations SET
         approval=$2,status='approved',revision=nextval('message_revision'),updated_at_ms=$3 WHERE id=$1")
        .bind(id)
        .bind(receipt)
        .bind(messages::now_ms()?)
        .execute(&mut *tx).await?;
    if let Some(auction) = operation.auction_id {
        crate::auctions::refresh_charging(&mut tx, auction).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn sign(
    state: &AppState,
    connection: &mut PgConnection,
    account: Uuid,
    id: Uuid,
    revision: i64,
    payload: &Payload,
) -> Result<serde_json::Value> {
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let bytes = serde_json::to_vec(payload).map_err(|_| ApiError::Internal)?;
    let digest = custody
        .vault
        .fingerprint(account, "native-transaction-approval", &bytes);
    messages::sign_action(
        state,
        connection,
        messages::ActionBinding {
            account,
            session: id,
            action: id,
            request: id,
            revision,
            digest,
            purpose: "localnet_transaction",
            received_ms: messages::now_ms()?,
        },
    )
    .await
}
