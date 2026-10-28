use super::{Ethereum, abi};
use crate::http::error::{ApiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NftMovement {
    pub collection: String,
    pub token_id: String,
    pub from: String,
    pub to: String,
}

impl Ethereum {
    pub async fn receipt(&self, id: &str) -> Result<Option<Value>> {
        let receipt = self
            .rpc
            .call("eth_getTransactionReceipt", json!([id]))
            .await?;
        if receipt.is_null() {
            return Ok(None);
        }
        let height = receipt["blockNumber"]
            .as_str()
            .ok_or(ApiError::Unavailable)?;
        let block = self
            .rpc
            .call("eth_getBlockByNumber", json!([height, false]))
            .await?;
        if block["hash"] != receipt["blockHash"] {
            return Err(ApiError::Conflict("Ethereum transaction was reorganized"));
        }
        if receipt["status"] != "0x1" {
            return Err(ApiError::Conflict("Ethereum transaction reverted"));
        }
        Ok(Some(receipt))
    }

    pub async fn verify_movement(&self, receipt: &Value, movement: &NftMovement) -> Result<()> {
        let expected_from = if movement.from == "0x0000000000000000000000000000000000000000" {
            movement.from.clone()
        } else {
            abi::address(&movement.from)?
        };
        let expected_to = abi::address(&movement.to)?;
        let token = movement
            .token_id
            .parse::<u128>()
            .map_err(|_| ApiError::Internal)?;
        let collection = abi::address(&movement.collection)?;
        let event = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
        let from_topic = format!("0x{:0>64}", &expected_from[2..]);
        let to_topic = format!("0x{:0>64}", &expected_to[2..]);
        let token_topic = format!("0x{token:064x}");
        let matched = receipt["logs"]
            .as_array()
            .ok_or(ApiError::Unavailable)?
            .iter()
            .any(|log| {
                log["address"]
                    .as_str()
                    .is_some_and(|address| address.eq_ignore_ascii_case(&collection))
                    && log["topics"] == json!([event, from_topic, to_topic, token_topic])
            });
        if !matched
            || self
                .owner(
                    &collection,
                    &movement.token_id,
                    receipt["blockNumber"].clone(),
                )
                .await?
                != expected_to
            || self
                .owner(&collection, &movement.token_id, json!("latest"))
                .await?
                != expected_to
        {
            return Err(ApiError::Conflict(
                "NFT transfer evidence does not match this operation",
            ));
        }
        Ok(())
    }
}
