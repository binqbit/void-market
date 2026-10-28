pub mod abi;
mod observation;
mod transactions;

use super::rpc::Rpc;
use crate::http::error::{ApiError, Result};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;

pub use observation::NftMovement;
pub use observation::transfer_from;
pub use transactions::EvmTransaction;

pub const CHAIN_ID: u64 = 31337;
pub const ARTIFACT: &str = include_str!("../../../../../contracts/demo-nft/artifacts/DemoNFT.json");

#[derive(Clone)]
pub struct Ethereum {
    pub rpc: Rpc,
}

impl Ethereum {
    pub fn new(url: Url) -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            rpc: Rpc::new(url, None)?,
        })
    }

    pub async fn identity(&self) -> Result<String> {
        let chain = self.rpc.call("eth_chainId", json!([])).await?;
        if abi::quantity(chain.as_str().ok_or(ApiError::Unavailable)?)? != u128::from(CHAIN_ID) {
            return Err(ApiError::Conflict(
                "Ethereum RPC is not the local chain 31337",
            ));
        }
        let block = self
            .rpc
            .call("eth_getBlockByNumber", json!(["0x0", false]))
            .await?;
        Ok(block["hash"]
            .as_str()
            .ok_or(ApiError::Unavailable)?
            .to_owned())
    }

    pub async fn accounts(&self) -> Result<Vec<String>> {
        let values = self.rpc.call("eth_accounts", json!([])).await?;
        values
            .as_array()
            .ok_or(ApiError::Unavailable)?
            .iter()
            .map(|value| abi::address(value.as_str().ok_or(ApiError::Unavailable)?))
            .collect()
    }

    pub async fn owner(&self, collection: &str, token: &str, block: Value) -> Result<String> {
        let result = self
            .rpc
            .call(
                "eth_call",
                json!([{"to":abi::address(collection)?,"data":abi::owner_of(token)?},block]),
            )
            .await?;
        abi::decoded_owner(result.as_str().ok_or(ApiError::Unavailable)?)
    }

    pub async fn code_hash(&self, contract: &str) -> Result<String> {
        let result = self
            .rpc
            .call("eth_getCode", json!([abi::address(contract)?, "latest"]))
            .await?;
        let code = hex::decode(
            result
                .as_str()
                .ok_or(ApiError::Unavailable)?
                .trim_start_matches("0x"),
        )
        .map_err(|_| ApiError::Unavailable)?;
        if code.is_empty() {
            return Err(ApiError::Conflict("Demo NFT contract is not deployed"));
        }
        Ok(hex::encode(Sha256::digest(code)))
    }
}
