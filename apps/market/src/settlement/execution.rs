use super::{
    confirm,
    model::{Operation, Payload},
    repository,
};
use crate::{
    http::error::{ApiError, Result},
    messages,
    state::AppState,
};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

pub(crate) async fn step(state: &AppState, operation: Operation, instance: Uuid) -> Result<()> {
    match operation.status.as_str() {
        "preparing" => prepare(state, operation, instance).await,
        "approved" | "building" => sign(state, operation, instance).await,
        "signed" | "submitted" | "reconcile" => reconcile(state, operation, instance).await,
        _ => Ok(()),
    }
}

async fn current(
    connection: &mut PgConnection,
    operation: &Operation,
    instance: Uuid,
) -> Result<Operation> {
    let epoch: Uuid =
        sqlx::query_scalar("SELECT instance_id FROM localnet_runtime WHERE singleton")
            .fetch_one(&mut *connection)
            .await?;
    if epoch != instance {
        return Err(ApiError::Unavailable);
    }
    if let Some(auction) = operation.auction_id {
        crate::auctions::lock(connection, auction).await?;
    }
    repository::lock(connection, operation.id).await
}

async fn prepare(state: &AppState, operation: Operation, instance: Uuid) -> Result<()> {
    let chains = state.chains.as_ref().ok_or(ApiError::Unavailable)?;
    let mut payload = repository::payload(state, &operation)?;
    let mut pczt = None;
    match &mut payload {
        Payload::Ethereum(plan) => {
            if ethereum_busy(state, &operation, &plan.sender).await? {
                sqlx::query("UPDATE chain_operations SET updated_at_ms=$2 WHERE id=$1")
                    .bind(operation.id)
                    .bind(messages::now_ms()?)
                    .execute(&state.pool)
                    .await?;
                return Ok(());
            }
            plan.transaction = Some(
                chains
                    .ethereum
                    .prepare(&plan.sender, Some(&plan.contract), plan.calldata.clone())
                    .await?,
            );
        }
        Payload::Zcash(plan) => {
            if wallet_busy(state, &operation, plan.sender_account).await? {
                // Keep independently requested payments from selecting the same notes.
                sqlx::query("UPDATE chain_operations SET updated_at_ms=$2 WHERE id=$1")
                    .bind(operation.id)
                    .bind(messages::now_ms()?)
                    .execute(&state.pool)
                    .await?;
                return Ok(());
            }
            if chains.zcash.balance(plan.sender_account).await? <= plan.value {
                return Err(ApiError::Conflict(
                    "Insufficient spendable ZEC. Claim local funds and request a new payment review.",
                ));
            }
            let (encoded, fee) = chains
                .zcash
                .prepare(
                    &plan.sender_address,
                    &plan.recipient_address,
                    plan.value,
                    &plan.memo,
                )
                .await?;
            let inspected = chains
                .zcash
                .wallet
                .call("pczt_inspect", json!([encoded]))
                .await?;
            plan.fee = Some(fee);
            plan.expiry_height = inspected["expiry_height"].as_u64();
            pczt = Some(seal_blob(state, &operation, "pczt", encoded.as_bytes())?);
        }
    }
    let mut tx = state.pool.begin().await?;
    let latest = current(&mut tx, &operation, instance).await?;
    if latest.status != operation.status {
        return Ok(());
    }
    repository::save_payload(
        state,
        &mut tx,
        &operation,
        &payload,
        if operation.approval.is_some() {
            "approved"
        } else {
            "ready"
        },
    )
    .await?;
    sqlx::query("UPDATE chain_operations SET pczt_sealed=$2 WHERE id=$1")
        .bind(operation.id)
        .bind(pczt)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn sign(state: &AppState, operation: Operation, instance: Uuid) -> Result<()> {
    let chains = state.chains.as_ref().ok_or(ApiError::Unavailable)?;
    let mut payload = repository::payload(state, &operation)?;
    let (id, raw) = match &mut payload {
        Payload::Ethereum(plan) => {
            let transaction = plan.transaction.as_ref().ok_or(ApiError::Internal)?;
            let nonce = chains
                .ethereum
                .rpc
                .call(
                    "eth_getTransactionCount",
                    json!([transaction.from, "pending"]),
                )
                .await?;
            if nonce != transaction.nonce {
                return Err(ApiError::Conflict(
                    "Ethereum nonce changed. Review a fresh transaction.",
                ));
            }
            chains.ethereum.sign(transaction).await?
        }
        Payload::Zcash(plan) => {
            let pczt = operation.pczt_sealed.as_ref().ok_or(ApiError::Internal)?;
            let decoded = open_blob(state, &operation, "pczt", pczt)?;
            if !plan.signed_pczt {
                let signed = chains.zcash.prove_and_sign(&decoded).await?;
                plan.signed_pczt = true;
                let mut tx = state.pool.begin().await?;
                let latest = current(&mut tx, &operation, instance).await?;
                if latest.status != operation.status {
                    return Ok(());
                }
                repository::save_payload(state, &mut tx, &operation, &payload, "building").await?;
                sqlx::query("UPDATE chain_operations SET pczt_sealed=$2 WHERE id=$1")
                    .bind(operation.id)
                    .bind(seal_blob(state, &operation, "pczt", signed.as_bytes())?)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                return Ok(());
            }
            chains.zcash.extract(&decoded).await?
        }
    };
    let mut tx = state.pool.begin().await?;
    let latest = current(&mut tx, &operation, instance).await?;
    if latest.status != operation.status {
        return Ok(());
    }
    sqlx::query("UPDATE chain_operations SET
         signed_transaction=$2,transaction_id=$3,status='signed',revision=nextval('message_revision')
         WHERE id=$1")
        .bind(operation.id)
        .bind(raw)
        .bind(id)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn reconcile(state: &AppState, operation: Operation, instance: Uuid) -> Result<()> {
    let chains = state.chains.as_ref().ok_or(ApiError::Unavailable)?;
    let payload = repository::payload(state, &operation)?;
    let id = operation
        .transaction_id
        .as_deref()
        .ok_or(ApiError::Internal)?;
    let raw = operation
        .signed_transaction
        .as_deref()
        .ok_or(ApiError::Internal)?;
    let evidence = match &payload {
        Payload::Ethereum(plan) => {
            if let Some(receipt) = chains.ethereum.receipt(id).await? {
                if operation.kind == "withdrawal" {
                    chains
                        .ethereum
                        .verify_departure(&receipt, &plan.movement)
                        .await?;
                } else {
                    chains
                        .ethereum
                        .verify_movement(&receipt, &plan.movement)
                        .await?;
                }
                Some(receipt)
            } else {
                None
            }
        }
        Payload::Zcash(plan) => {
            chains
                .zcash
                .observe_payment(
                    id,
                    plan.recipient_account,
                    &plan.recipient_address,
                    plan.value,
                    &plan.memo,
                )
                .await?
        }
    };
    if let Some(evidence) = evidence {
        confirm::apply(state, &operation, &payload, evidence).await?;
        return Ok(());
    }
    if let Payload::Zcash(plan) = &payload
        && chains
            .zcash
            .expired_and_absent(id, plan.expiry_height)
            .await?
    {
        let mut tx = state.pool.begin().await?;
        current(&mut tx, &operation, instance).await?;
        repository::status(
            &mut tx, operation.id, "failed",
            Some(if operation.kind == "airdrop" {
                "The ZEC claim expired without confirmation. Request local ZEC again from Demo."
            } else {
                "The signed payment expired without confirmation. Prepare and review a fresh payment."
            }),
        ).await?;
        tx.commit().await?;
        return Ok(());
    }
    let mut tx = state.pool.begin().await?;
    let latest = current(&mut tx, &operation, instance).await?;
    if matches!(latest.status.as_str(), "confirmed" | "cancelled") {
        return Ok(());
    }
    repository::status(&mut tx, operation.id, "submitted", None).await?;
    tx.commit().await?;
    // The exact signed bytes are durable; a lost response never produces a new spend.
    match payload {
        Payload::Ethereum(_) => {
            let _ = chains.ethereum.broadcast(raw).await;
        }
        Payload::Zcash(_) => {
            let _ = chains
                .zcash
                .node
                .call("sendrawtransaction", json!([raw]))
                .await;
        }
    }
    Ok(())
}
async fn wallet_busy(state: &AppState, operation: &Operation, wallet: Uuid) -> Result<bool> {
    let active: Vec<Operation> = sqlx::query_as(
        "SELECT * FROM chain_operations WHERE id<>$1 AND status IN
         ('ready','approved','building','signed','submitted','reconcile')",
    )
    .bind(operation.id)
    .fetch_all(&state.pool)
    .await?;
    for pending in active {
        if let Payload::Zcash(plan) = repository::payload(state, &pending)?
            && plan.sender_account == wallet
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn ethereum_busy(state: &AppState, operation: &Operation, sender: &str) -> Result<bool> {
    let active: Vec<Operation> = sqlx::query_as(
        "SELECT * FROM chain_operations WHERE id<>$1 AND status IN
         ('ready','approved','building','signed','submitted','reconcile')",
    )
    .bind(operation.id)
    .fetch_all(&state.pool)
    .await?;
    for pending in active {
        if let Payload::Ethereum(plan) = repository::payload(state, &pending)?
            && plan.sender == sender
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn seal_blob(
    state: &AppState,
    operation: &Operation,
    purpose: &str,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .seal(
            operation.account_id,
            &format!("native-{purpose}:{}", operation.id),
            bytes,
        )
}
fn open_blob(
    state: &AppState,
    operation: &Operation,
    purpose: &str,
    bytes: &[u8],
) -> Result<String> {
    let bytes = state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .open(
            operation.account_id,
            &format!("native-{purpose}:{}", operation.id),
            bytes,
        )?;
    String::from_utf8(bytes.to_vec()).map_err(|_| ApiError::Internal)
}
