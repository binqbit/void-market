use super::{
    model::{EvmPlan, Operation, Payload},
    repository::{self, NewOperation},
};
use crate::{
    assets::{self, model::Asset},
    chains::ethereum::{self, NftMovement, abi},
    demo, events,
    http::error::{ApiError, Result},
    state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Serialize)]
pub struct DepositAddress {
    address: String,
    chain_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositNft {
    pub request_id: Uuid,
    pub transaction_id: String,
    pub collection: String,
    pub token_id: String,
    pub name: Option<String>,
}

pub async fn address(state: &AppState, account: Uuid) -> Result<DepositAddress> {
    if state.chains.is_none() {
        return Err(ApiError::NotFound);
    }
    let actor = demo::repository::actor(&state.pool, account).await?;
    Ok(DepositAddress {
        address: actor.ethereum_address,
        chain_id: ethereum::CHAIN_ID,
    })
}

pub async fn receive(state: &AppState, account: Uuid, mut input: DepositNft) -> Result<Asset> {
    let chains = state.chains.as_ref().ok_or(ApiError::NotFound)?;
    let actor = demo::repository::actor(&state.pool, account).await?;
    input.transaction_id = abi::transaction_hash(input.transaction_id.trim())?;
    input.collection = abi::checked_address(input.collection.trim())?;
    abi::token(&input.token_id)?;
    input.name = input
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    if input
        .name
        .as_ref()
        .is_some_and(|name| name.chars().count() > 128)
    {
        return Err(ApiError::BadRequest("NFT name is too long"));
    }

    let deployment = demo::repository::deployment(&state.pool).await?;
    if chains.ethereum.identity().await? != deployment.ethereum_genesis {
        return Err(ApiError::Conflict(
            "Ethereum network does not match this platform",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let identity = format!(
        "{}:{}:{}",
        deployment.ethereum_genesis, input.collection, input.token_id
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,738291))")
        .bind(identity)
        .execute(&mut *tx)
        .await?;
    if let Some(asset) = replay(state, &mut tx, account, &input).await? {
        tx.commit().await?;
        return assets::repository::get(&state.pool, asset).await;
    }

    let receipt = chains
        .ethereum
        .receipt(&input.transaction_id)
        .await?
        .ok_or(ApiError::Conflict("The deposit has not been confirmed yet"))?;
    if receipt["transactionHash"]
        .as_str()
        .is_none_or(|id| !id.eq_ignore_ascii_case(&input.transaction_id))
    {
        return Err(ApiError::Conflict(
            "The deposit receipt does not match the transaction",
        ));
    }
    chains.ethereum.supports_erc721(&input.collection).await?;
    let movement = NftMovement {
        collection: input.collection.clone(),
        token_id: input.token_id.clone(),
        from: ethereum::transfer_from(
            &receipt,
            &input.collection,
            &input.token_id,
            &actor.ethereum_address,
        )?,
        to: actor.ethereum_address,
    };
    chains.ethereum.verify_movement(&receipt, &movement).await?;

    let asset = store_asset(&mut tx, account, &input, &deployment.ethereum_genesis).await?;
    record(state, &mut tx, account, asset, &input, movement, receipt).await?;
    tx.commit().await?;
    assets::repository::get(&state.pool, asset).await
}

async fn replay(
    state: &AppState,
    connection: &mut PgConnection,
    account: Uuid,
    input: &DepositNft,
) -> Result<Option<Uuid>> {
    let existing: Option<Operation> = sqlx::query_as("SELECT * FROM chain_operations WHERE id=$1")
        .bind(input.request_id)
        .fetch_optional(connection)
        .await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    if existing.account_id == account
        && existing.kind == "nft_deposit"
        && existing.transaction_id.as_deref() == Some(&input.transaction_id)
        && matches!(repository::payload(state, &existing)?, Payload::Ethereum(plan)
            if plan.movement.collection == input.collection && plan.movement.token_id == input.token_id
                && plan.metadata.as_ref().is_some_and(|metadata| metadata["requested_name"] == json!(input.name)))
    {
        return Ok(existing.asset_id);
    }
    Err(ApiError::Conflict("Deposit request has different details"))
}

async fn store_asset(
    connection: &mut PgConnection,
    account: Uuid,
    input: &DepositNft,
    genesis: &str,
) -> Result<Uuid> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM assets WHERE nft->>'genesis'=$1 AND lower(nft->>'collection')=$2 AND nft->>'token_id'=$3",
    ).bind(genesis).bind(&input.collection).bind(&input.token_id).fetch_optional(&mut *connection).await?;
    if let Some(id) = id {
        let asset = assets::repository::lock(connection, id).await?;
        if asset.owner_id.is_some()
            || asset.active_auction_id.is_some()
            || asset.pending_transaction_id.is_some()
        {
            return Err(ApiError::Conflict(
                "This NFT is already held on the platform",
            ));
        }
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM chain_operations WHERE kind='nft_deposit' AND asset_id=$1 AND transaction_id=$2)")
            .bind(id).bind(&input.transaction_id).fetch_one(&mut *connection).await?;
        if used {
            return Err(ApiError::Conflict(
                "This deposit transaction has already been recorded",
            ));
        }
        assets::repository::change_owner(connection, id, account).await?;
        return Ok(id);
    }

    let id = Uuid::new_v4();
    let name = input
        .name
        .clone()
        .unwrap_or_else(|| format!("NFT #{}", input.token_id));
    let nft = json!({"chain_id":ethereum::CHAIN_ID,"genesis":genesis,"collection":input.collection,"token_id":input.token_id});
    sqlx::query("INSERT INTO assets(id,name,description,owner_id,nft) VALUES($1,$2,'',$3,$4)")
        .bind(id)
        .bind(name)
        .bind(account)
        .bind(nft)
        .execute(connection)
        .await?;
    Ok(id)
}

async fn record(
    state: &AppState,
    connection: &mut PgConnection,
    account: Uuid,
    asset: Uuid,
    input: &DepositNft,
    movement: NftMovement,
    receipt: Value,
) -> Result<()> {
    repository::insert(
        state,
        connection,
        NewOperation {
            id: input.request_id,
            account,
            recipient: None,
            auction: None,
            asset: Some(asset),
            kind: "nft_deposit",
            payload: Payload::Ethereum(Box::new(EvmPlan {
                sender: movement.from.clone(),
                contract: movement.collection.clone(),
                calldata: String::new(),
                movement,
                transaction: None,
                metadata: Some(json!({"requested_name":input.name})),
            })),
            approval: None,
        },
    )
    .await?;
    let sealed = state
        .custody
        .as_ref()
        .ok_or(ApiError::Unavailable)?
        .vault
        .seal(
            account,
            &format!("native-receipt:{}", input.request_id),
            &serde_json::to_vec(&receipt).map_err(|_| ApiError::Internal)?,
        )?;
    sqlx::query("UPDATE chain_operations SET status='confirmed',transaction_id=$2,receipt_sealed=$3 WHERE id=$1")
        .bind(input.request_id).bind(&input.transaction_id).bind(sealed).execute(&mut *connection).await?;
    events::record(
        connection,
        "AssetDeposited.v1",
        json!({"asset_id":asset,"owner_id":account,"transaction_id":input.transaction_id}),
    )
    .await
}
