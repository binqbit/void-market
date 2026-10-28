pub mod memo;
mod payments;

use super::{config::LocalnetConfig, rpc::Rpc};
use crate::http::error::{ApiError, Result};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Zcash {
    pub node: Rpc,
    pub wallet: Rpc,
}

impl Zcash {
    pub fn new(config: &LocalnetConfig) -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            node: Rpc::new(config.zcash_node.clone(), None)?,
            wallet: Rpc::new(
                config.zcash_wallet.clone(),
                Some(config.wallet_cookie.clone()),
            )?,
        })
    }

    pub async fn identity(&self) -> Result<String> {
        let info = self.node.call("getblockchaininfo", json!([])).await?;
        if info["chain"] != "test"
            || info["upgrades"]["37a5165b"]["activationheight"] != 2
            || info["upgrades"]["37a5165b"]["status"] != "active"
        {
            return Err(ApiError::Conflict(
                "Zcash RPC is not the configured Ironwood regtest",
            ));
        }
        Ok(self
            .node
            .call("getblockhash", json!([0]))
            .await?
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned())
    }

    pub async fn synchronized(&self) -> Result<bool> {
        let status = self.wallet.call("getwalletstatus", json!([])).await?;
        let Some(tip) = status.get("wallet_tip").filter(|tip| !tip.is_null()) else {
            return Ok(false);
        };

        if tip != &status["node_tip"] || status["locked"] != false {
            return Ok(false);
        }

        let height = self.node.call("getblockcount", json!([])).await?;
        Ok(tip["height"]
            .as_u64()
            .is_some_and(|value| Some(value) == height.as_u64()))
    }

    pub async fn balance(&self, account: uuid::Uuid) -> Result<u64> {
        if !self.synchronized().await? {
            return Err(ApiError::Unavailable);
        }
        let result = self
            .wallet
            .call("z_getbalanceforaccount", json!([account, 1]))
            .await?;
        match result["pools"].get("ironwood") {
            Some(pool) => pool["valueZat"].as_u64().ok_or(ApiError::Unavailable),
            None => Ok(0),
        }
    }

    pub async fn accounts(&self) -> Result<Vec<Value>> {
        let result = self.wallet.call("z_listaccounts", json!([])).await?;
        Ok(result.as_array().ok_or(ApiError::Unavailable)?.clone())
    }

    pub async fn ensure_account(&self, name: &str) -> Result<(uuid::Uuid, String)> {
        let accounts = self.accounts().await?;
        let matches: Vec<_> = accounts
            .iter()
            .filter(|account| account["name"] == name)
            .collect();
        if matches.len() > 1 {
            return Err(ApiError::Conflict(
                "Duplicate local wallet account names require reconciliation",
            ));
        }
        let account = if let Some(account) = matches.first() {
            account["account_uuid"]
                .as_str()
                .ok_or(ApiError::Unavailable)?
                .to_owned()
        } else {
            let seed = accounts
                .iter()
                .find(|account| account["name"] == "Void Market local faucet")
                .and_then(|account| account["seedfp"].as_str());
            let parameters = match seed {
                Some(seed) => json!([name, seed]),
                None => json!([name]),
            };
            self.wallet.call("z_getnewaccount", parameters).await?["account_uuid"]
                .as_str()
                .ok_or(ApiError::Unavailable)?
                .to_owned()
        };
        let id = uuid::Uuid::parse_str(&account).map_err(|_| ApiError::Unavailable)?;
        // Zallet prederives transparent gap addresses at low indices. Keep the shielded receiver separate.
        let result = self
            .wallet
            .call("z_getaddressforaccount", json!([id, ["orchard"], 1000000]))
            .await?;
        let receiver = result["address"]
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned();
        if !receiver.starts_with("uregtest1") {
            return Err(ApiError::Conflict("Wallet returned a non-regtest receiver"));
        }
        Ok((id, receiver))
    }
}
