use super::Zcash;
use crate::{
    auctions::amount,
    http::error::{ApiError, Result},
};
use serde_json::{Number, Value, json};
use std::{str::FromStr, time::Duration};
use uuid::Uuid;

impl Zcash {
    pub async fn expired_and_absent(&self, id: &str, expiry: Option<u64>) -> Result<bool> {
        let Some(expiry) = expiry.filter(|height| *height > 0) else {
            return Ok(false);
        };
        let height = self
            .node
            .call("getblockcount", json!([]))
            .await?
            .as_u64()
            .ok_or(ApiError::Unavailable)?;
        if height <= expiry || !self.synchronized().await? {
            return Ok(false);
        }
        match self.node.call("getrawtransaction", json!([id, 1])).await {
            Err(crate::chains::rpc::RpcError::Rejected(-5)) => Ok(true),
            Err(error) => Err(error.into()),
            Ok(_) => Ok(false),
        }
    }

    pub async fn prepare(
        &self,
        sender: &str,
        recipient: &str,
        value: u64,
        memo: &str,
    ) -> Result<(String, u64)> {
        self.identity().await?;
        if !self.synchronized().await? {
            return Err(ApiError::Unavailable);
        }
        let decimal = Number::from_str(&amount::format(value)).map_err(|_| ApiError::Internal)?;
        let output = json!({"address": recipient, "amount": decimal, "memo": memo});
        let result = match self
            .wallet
            .call("pczt_create", json!([sender, [output], 1, "FullPrivacy"]))
            .await
        {
            Ok(result) => result,
            Err(crate::chains::rpc::RpcError::Rejected(-4 | -6)) => {
                return Err(ApiError::Conflict(
                    "The wallet cannot fund this payment and its fee. Claim local ZEC and prepare the payment again.",
                ));
            }
            Err(error) => return Err(error.into()),
        };
        let pczt = result["pczt"]
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned();
        let inspected = self.wallet.call("pczt_inspect", json!([pczt])).await?;
        let outputs = inspected["ironwood"]["outputs"]
            .as_array()
            .ok_or(ApiError::Unavailable)?;
        let recipients: Vec<_> = outputs
            .iter()
            .filter(|output| output.get("user_address").is_some())
            .collect();
        if inspected["tx_version"] != 6
            || inspected["consensus_branch_id"] != "37a5165b"
            || inspected["privacy_policy"] != "FullPrivacy"
            || recipients.len() != 1
            || recipients[0]["user_address"] != recipient
            || recipients[0]["value_zat"] != value
        {
            return Err(ApiError::Conflict(
                "Prepared shielded payment does not match the reviewed intent",
            ));
        }
        let fee = inspected["fee_zat"].as_u64().ok_or(ApiError::Unavailable)?;
        Ok((pczt, fee))
    }

    pub async fn prove_and_sign(&self, pczt: &str) -> Result<String> {
        let proved = self
            .wallet
            .call_with_timeout("pczt_prove", json!([pczt]), Duration::from_secs(180))
            .await?;
        let signed = self
            .wallet
            .call("pczt_sign", json!([proved["pczt"], "FullPrivacy", true]))
            .await?;
        for pool in [
            "unsigned_transparent",
            "unsigned_sapling",
            "unsigned_orchard",
            "unsigned_ironwood",
        ] {
            if !signed[pool]
                .as_array()
                .is_some_and(|items| items.is_empty())
            {
                return Err(ApiError::Conflict(
                    "The local wallet could not sign every payment input",
                ));
            }
        }
        Ok(signed["pczt"]
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned())
    }

    pub async fn extract(&self, pczt: &str) -> Result<(String, String)> {
        let result = self.wallet.call("pczt_extract", json!([pczt])).await?;
        let id = result["txid"]
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned();
        let raw = result["hex"]
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned();
        let decoded = self
            .wallet
            .call("decoderawtransaction", json!([raw]))
            .await?;
        if decoded["txid"] != id {
            // Keep the signed PCZT in the building stage until its identity is
            // trustworthy. Reconciliation needs validated transaction bytes.
            tracing::warn!("Zcash wallet returned inconsistent transaction identities");
            return Err(ApiError::Unavailable);
        }
        Ok((id, raw))
    }

    pub async fn observe_payment(
        &self,
        id: &str,
        recipient: Uuid,
        address: &str,
        value: u64,
        memo: &str,
    ) -> Result<Option<Value>> {
        let result = match self.node.call("getrawtransaction", json!([id, 1])).await {
            Ok(result) => result,
            Err(crate::chains::rpc::RpcError::Rejected(-5)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let confirmations = result["confirmations"].as_u64().unwrap_or(0);
        if confirmations < 3 {
            return Ok(None);
        }
        let height = result["height"].as_u64().ok_or(ApiError::Unavailable)?;
        let block = self.node.call("getblockhash", json!([height])).await?;
        if result["blockhash"] != block {
            return Err(ApiError::Conflict("Zcash payment was reorganized"));
        }
        if !self.synchronized().await? {
            return Ok(None);
        }
        let viewed = self.wallet.call("z_viewtransaction", json!([id])).await?;
        let matching: Vec<_> = viewed["outputs"]
            .as_array()
            .ok_or(ApiError::Unavailable)?
            .iter()
            .filter(|output| {
                output["pool"] == "ironwood"
                    && output["account_uuid"] == recipient.to_string()
                    && output["address"] == address
                    && output["walletInternal"] == false
                    && output["outgoing"] == false
                    && output["valueZat"] == value
                    && output["memo"] == memo
            })
            .collect();
        if viewed["status"] != "mined" || viewed["blockhash"] != block || matching.len() != 1 {
            return Err(ApiError::Conflict(
                "Confirmed payment does not match the exact receiver, invoice and value",
            ));
        }
        Ok(Some(
            json!({"txid":id,"block_hash":block,"height":height,"confirmations":confirmations,
            "action":matching[0]["action"],"pool":"ironwood","basis":"custodial_wallet_observation"}),
        ))
    }
}
