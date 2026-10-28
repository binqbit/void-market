use super::{
    model::{Deployment, actor_id},
    repository,
};
use crate::{
    chains::ethereum::{ARTIFACT, abi},
    http::error::{ApiError, Result},
    state::AppState,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub async fn initialize(state: &AppState) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            match initialize_once(state).await {
                Ok(()) => return Ok(()),
                Err(ApiError::Unavailable) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(error) => return Err(error),
            }
        }
    })
    .await
    .map_err(|_| ApiError::Unavailable)?
}

async fn initialize_once(state: &AppState) -> Result<()> {
    let chains = state.chains.as_ref().ok_or(ApiError::NotFound)?;
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let mut ownership = state.pool.acquire().await?;
    ownership.close_on_drop();
    let acquired: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_lock(hashtextextended(current_schema(), 732617231))",
    )
    .fetch_one(&mut *ownership)
    .await?;
    if !acquired {
        return Err(ApiError::Conflict(
            "Localnet initialization is already running",
        ));
    }

    let ethereum_genesis = chains.ethereum.identity().await?;
    let zcash_genesis = chains.zcash.identity().await?;
    let accounts = chains.ethereum.accounts().await?;
    if accounts.len() < 5 {
        return Err(ApiError::Conflict(
            "Localnet requires a minter, three users and an arbiter address",
        ));
    }
    let artifact: Value = serde_json::from_str(ARTIFACT).map_err(|_| ApiError::Internal)?;
    let expected = artifact["deployed_bytecode"]
        .as_str()
        .ok_or(ApiError::Internal)?;
    let hash = hex::encode(Sha256::digest(
        hex::decode(expected.trim_start_matches("0x")).map_err(|_| ApiError::Internal)?,
    ));
    let (faucet_account, faucet_address) = chains
        .zcash
        .ensure_account("Void Market local faucet")
        .await?;
    sqlx::query(
        "INSERT INTO localnet_config (
             singleton,
             ethereum_genesis,
             collection_code_hash,
             zcash_genesis,
             minter_address,
             faucet_account,
             faucet_address
         )
         VALUES (TRUE, $1, $2, $3, $4, $5, $6) ON CONFLICT (singleton) DO NOTHING",
    )
    .bind(&ethereum_genesis)
    .bind(&hash)
    .bind(&zcash_genesis)
    .bind(&accounts[0])
    .bind(faucet_account)
    .bind(&faucet_address)
    .execute(&state.pool)
    .await?;
    let deployment = repository::deployment(&state.pool).await?;
    if deployment.ethereum_genesis != ethereum_genesis
        || deployment.zcash_genesis != zcash_genesis
        || deployment.collection_code_hash != hash
        || deployment.minter_address != accounts[0]
        || deployment.faucet_account != faucet_account
        || deployment.faucet_address != faucet_address
    {
        return Err(ApiError::Conflict(
            "Retained localnet identity does not match the running chains or wallet",
        ));
    }
    deploy_collection(state, &deployment, &artifact).await?;

    for (index, (alias, name)) in [("alice", "Alice"), ("bob", "Bob"), ("daniel", "Daniel")]
        .iter()
        .enumerate()
    {
        let id = actor_id(alias).ok_or(ApiError::Internal)?;
        let mut tx = state.pool.begin().await?;
        sqlx::query(
            "INSERT INTO accounts (id, name, username, is_demo)
             VALUES ($1, $2, $3, TRUE) ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(name)
        .bind(format!("demo_{alias}"))
        .execute(&mut *tx)
        .await?;
        let existing_demo: bool = sqlx::query_scalar("SELECT is_demo FROM accounts WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if !existing_demo {
            return Err(ApiError::Conflict(
                "Demo identity conflicts with a retained account",
            ));
        }
        custody.provision(&mut tx, id).await?;
        sqlx::query(
            "INSERT INTO demo_accounts (account_id, alias, ethereum_address)
             VALUES ($1, $2, $3) ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(id)
        .bind(alias)
        .bind(&accounts[index + 1])
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let actor = repository::actor(&state.pool, id).await?;
        if actor.ethereum_address != accounts[index + 1] {
            return Err(ApiError::Conflict("Demo Ethereum wallet mapping changed"));
        }
        let (wallet, address) = chains
            .zcash
            .ensure_account(&format!("Void Market demo {alias}"))
            .await?;
        if actor
            .zcash_account
            .is_some_and(|existing| existing != wallet)
            || actor
                .zcash_address
                .as_ref()
                .is_some_and(|existing| existing != &address)
        {
            return Err(ApiError::Conflict("Demo Zcash wallet mapping changed"));
        }
        sqlx::query(
            "UPDATE demo_accounts
             SET
                 zcash_account = $2,
                 zcash_address = $3
             WHERE account_id = $1",
        )
        .bind(id)
        .bind(wallet)
        .bind(address)
        .execute(&state.pool)
        .await?;
    }
    // Existing HE-only demo nodes receive their first chain registration once. Later changes fail.
    let nodes: Vec<(uuid::Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, ethereum_address FROM arbiter_nodes ORDER BY id")
            .fetch_all(&state.pool)
            .await?;
    let mut assigned: Vec<String> = nodes
        .iter()
        .filter_map(|(_, address)| address.clone())
        .collect();
    for (node, current) in &nodes {
        if let Some(current) = current {
            if !accounts[4..].contains(current) {
                return Err(ApiError::Conflict(
                    "Registered arbiter address is unavailable in this localnet",
                ));
            }
            continue;
        }
        let address = accounts[4..]
            .iter()
            .find(|address| !assigned.contains(address))
            .ok_or(ApiError::Conflict(
                "Configure a separate Ethereum address for every arbiter",
            ))?;
        sqlx::query(
            "UPDATE arbiter_nodes
             SET ethereum_address = $2
             WHERE
                 id = $1
                 AND ethereum_address IS NULL",
        )
        .bind(node)
        .bind(address)
        .execute(&state.pool)
        .await?;
        assigned.push(address.clone());
    }
    verify(state).await?;
    ownership.close().await?;
    Ok(())
}

async fn deploy_collection(
    state: &AppState,
    deployment: &Deployment,
    artifact: &Value,
) -> Result<()> {
    let chains = state.chains.as_ref().ok_or(ApiError::Unavailable)?;
    if let Some(collection) = &deployment.collection_address {
        if chains.ethereum.code_hash(collection).await? != deployment.collection_code_hash {
            return Err(ApiError::Conflict("Demo NFT runtime code changed"));
        }
        return Ok(());
    }
    let (id, raw) = match (
        &deployment.deployment_transaction,
        &deployment.signed_deployment,
    ) {
        (Some(id), Some(raw)) => (id.clone(), raw.clone()),
        (None, None) => {
            let bytecode = artifact["bytecode"]
                .as_str()
                .ok_or(ApiError::Internal)?
                .to_owned();
            let transaction = chains
                .ethereum
                .prepare(&deployment.minter_address, None, bytecode)
                .await?;
            let (id, raw) = chains.ethereum.sign(&transaction).await?;
            sqlx::query(
                "UPDATE localnet_config
                 SET
                     deployment_transaction = $1,
                     signed_deployment = $2
                 WHERE singleton",
            )
            .bind(&id)
            .bind(&raw)
            .execute(&state.pool)
            .await?;
            (id, raw)
        }
        _ => {
            return Err(ApiError::Conflict(
                "Incomplete retained deployment transaction",
            ));
        }
    };
    let _ = chains.ethereum.broadcast(&raw).await;
    let receipt = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Some(receipt) = chains.ethereum.receipt(&id).await? {
                break Ok::<_, ApiError>(receipt);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| ApiError::Unavailable)??;
    let contract = abi::address(
        receipt["contractAddress"]
            .as_str()
            .ok_or(ApiError::Unavailable)?,
    )?;
    if chains.ethereum.code_hash(&contract).await? != deployment.collection_code_hash {
        return Err(ApiError::Conflict(
            "Deployed NFT code does not match the pinned artifact",
        ));
    }
    sqlx::query("UPDATE localnet_config SET collection_address = $1 WHERE singleton")
        .bind(contract)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub async fn verify(state: &AppState) -> Result<()> {
    let chains = state.chains.as_ref().ok_or(ApiError::NotFound)?;
    let deployment = repository::deployment(&state.pool).await?;
    if chains.ethereum.identity().await? != deployment.ethereum_genesis
        || chains.zcash.identity().await? != deployment.zcash_genesis
    {
        return Err(ApiError::Conflict("Localnet chain identity changed"));
    }
    let collection = deployment
        .collection_address
        .as_ref()
        .ok_or(ApiError::Unavailable)?;
    if chains.ethereum.code_hash(collection).await? != deployment.collection_code_hash {
        return Err(ApiError::Conflict(
            "Local NFT contract changed or was reset",
        ));
    }
    Ok(())
}
