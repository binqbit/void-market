use super::repository as demo_repository;
use crate::{
    chains::{
        ethereum::{NftMovement, abi},
        zcash::memo::{MemoBinding, id_reference, reference},
    },
    http::error::{ApiError, Result},
    settlement::{
        model::{EvmPlan, Payload, PaymentPlan},
        repository::{self, NewOperation},
        service,
    },
    state::AppState,
};
use rand::{TryRngCore, rngs::OsRng};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub async fn request(state: &AppState, account: Uuid, id: Uuid, kind: &str) -> Result<()> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let actor = demo_repository::actor(&state.pool, account).await?;
    let existing: Option<(Uuid, String)> =
        sqlx::query_as("SELECT account_id, kind FROM chain_operations WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    if let Some((owner, original)) = existing {
        if owner != account || original != kind {
            return Err(ApiError::Conflict(
                "Request ID belongs to a different transaction",
            ));
        }
        return Ok(());
    }
    super::verify(state).await?;
    let deployment = demo_repository::deployment(&state.pool).await?;
    let payload = match kind {
        "mint" => {
            let collection = deployment.collection_address.ok_or(ApiError::Unavailable)?;
            let token = id.as_u128().to_string();
            let variant = (id.as_u128() % 6) as usize;
            let names = [
                "Event Horizon",
                "Quiet Signal",
                "Neon Cipher",
                "Deep Orbit",
                "Silent Relay",
                "Dark Matter",
            ];
            let uri = format!("{}api/v1/demo/nfts/{token}/metadata", state.config.origin);
            Payload::Ethereum(Box::new(EvmPlan {
                sender: deployment.minter_address,
                contract: collection.clone(),
                calldata: abi::mint(&actor.ethereum_address, &token, &uri)?,
                movement: NftMovement {
                    collection,
                    token_id: token,
                    from: "0x0000000000000000000000000000000000000000".into(),
                    to: actor.ethereum_address,
                },
                transaction: None,
                metadata: Some(json!({
                    "name": names[variant],
                    "description": "A disposable collectible minted on the Void Market Ethereum localnet.",
                    "variant": variant,
                    "image": format!("demo-{variant}.svg"),
                })),
            }))
        }
        "airdrop" => {
            let mut nonce = [0u8; 32];
            OsRng
                .try_fill_bytes(&mut nonce)
                .map_err(|_| ApiError::Internal)?;
            let binding = MemoBinding {
                obligation_kind: 2,
                intent_class: 1,
                network: reference("network", &deployment.zcash_genesis)?,
                context: id_reference("obligation_context_ref", account)?,
                obligation: Sha256::digest(id.as_bytes()).into(),
                invoice: id_reference("invoice_id", id)?,
                issue: Sha256::digest(nonce).into(),
                invoice_nonce: nonce,
                intent: id_reference("intent_id", id)?,
                intent_nonce: nonce,
                recipient: reference(
                    "recipient_registration",
                    actor
                        .zcash_address
                        .as_deref()
                        .ok_or(ApiError::Unavailable)?,
                )?,
                revision: 1,
                generation: 1,
            };
            Payload::Zcash(PaymentPlan {
                request_amount: None,
                signed_pczt: false,
                sender_account: deployment.faucet_account,
                sender_address: deployment.faucet_address,
                recipient_account: actor.zcash_account.ok_or(ApiError::Unavailable)?,
                recipient_address: actor.zcash_address.ok_or(ApiError::Unavailable)?,
                value: 1_000_000_000,
                memo: hex::encode(binding.encode()?),
                fee: None,
                expiry_height: None,
            })
        }
        _ => return Err(ApiError::BadRequest("Unknown demo resource")),
    };
    let mut tx = state.pool.begin().await?;
    let approval = service::sign(state, &mut tx, account, id, 0, &payload).await?;
    repository::insert(
        state,
        &mut tx,
        NewOperation {
            id,
            account,
            recipient: Some(account),
            auction: None,
            asset: None,
            kind,
            payload,
            approval: Some(approval),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
