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
        let token = abi::token(&movement.token_id)?;
        let collection = abi::address(&movement.collection)?;
        let event = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
        let from_topic = format!("0x{:0>64}", &expected_from[2..]);
        let to_topic = format!("0x{:0>64}", &expected_to[2..]);
        let token_topic = format!("0x{token}");
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

    pub async fn verify_departure(&self, receipt: &Value, movement: &NftMovement) -> Result<()> {
        let from = abi::address(&movement.from)?;
        let to = abi::address(&movement.to)?;
        let matched = transfer_from(receipt, &movement.collection, &movement.token_id, &to)?;
        if matched != from
            || self
                .owner(
                    &movement.collection,
                    &movement.token_id,
                    receipt["blockNumber"].clone(),
                )
                .await?
                == from
        {
            return Err(ApiError::Conflict(
                "NFT withdrawal evidence does not match this operation",
            ));
        }
        Ok(())
    }

    pub async fn supports_erc721(&self, collection: &str) -> Result<()> {
        for (interface, expected) in [("01ffc9a7", true), ("ffffffff", false), ("80ac58cd", true)] {
            let data = format!("0x01ffc9a7{interface}{}", "0".repeat(56));
            let result = self
                .rpc
                .call("eth_call", json!([{"to":collection,"data":data},"latest"]))
                .await?;
            let valid = if expected {
                format!("0x{:064x}", 1)
            } else {
                format!("0x{:064x}", 0)
            };
            if result != valid {
                return Err(ApiError::BadRequest(
                    "The NFT contract must support ERC-721",
                ));
            }
        }
        Ok(())
    }
}

pub fn transfer_from(receipt: &Value, collection: &str, token: &str, to: &str) -> Result<String> {
    let collection = abi::address(collection)?;
    let to = format!("0x{:0>64}", &abi::address(to)?[2..]);
    let token = format!("0x{}", abi::token(token)?);
    let logs = receipt["logs"].as_array().ok_or(ApiError::Unavailable)?;
    let mut matches = logs.iter().filter(|log| {
        log["address"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(&collection))
            && log["topics"]
                .as_array()
                .is_some_and(|topics| topics.len() == 4)
            && log["topics"][0]
                == "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
            && log["topics"][2]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(&to))
            && log["topics"][3]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(&token))
    });
    let entry = matches.next().ok_or(ApiError::Conflict(
        "No matching NFT deposit was found in this transaction",
    ))?;
    if matches.next().is_some() {
        return Err(ApiError::Conflict(
            "The transaction contains ambiguous NFT transfers",
        ));
    }
    let from = entry["topics"][1].as_str().ok_or(ApiError::Unavailable)?;
    if from == format!("0x{}", "0".repeat(64)) {
        Ok("0x0000000000000000000000000000000000000000".into())
    } else {
        abi::decoded_owner(from)
    }
}
