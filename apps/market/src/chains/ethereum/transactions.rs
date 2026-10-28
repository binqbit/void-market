use super::{Ethereum, abi};
use crate::http::error::{ApiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvmTransaction {
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub data: String,
    pub value: String,
    pub nonce: String,
    pub gas: String,
    pub gas_price: String,
    pub chain_id: String,
}

impl Ethereum {
    pub async fn prepare(
        &self,
        from: &str,
        to: Option<&str>,
        data: String,
    ) -> Result<EvmTransaction> {
        self.identity().await?;
        let from = abi::address(from)?;
        let to = to.map(abi::address).transpose()?;
        let nonce = self
            .rpc
            .call("eth_getTransactionCount", json!([from, "pending"]))
            .await?;
        let price = self.rpc.call("eth_gasPrice", json!([])).await?;
        let estimate = self
            .rpc
            .call(
                "eth_estimateGas",
                json!([{"from":from,"to":to,"data":data,"value":"0x0"}]),
            )
            .await?;
        let gas = abi::quantity(estimate.as_str().ok_or(ApiError::Unavailable)?)?
            .checked_mul(12)
            .and_then(|gas| gas.checked_div(10))
            .ok_or(ApiError::Internal)?;
        Ok(EvmTransaction {
            from,
            to,
            data,
            value: "0x0".into(),
            nonce: nonce.as_str().ok_or(ApiError::Unavailable)?.into(),
            gas: format!("0x{gas:x}"),
            gas_price: price.as_str().ok_or(ApiError::Unavailable)?.into(),
            chain_id: format!("0x{:x}", super::CHAIN_ID),
        })
    }

    pub async fn sign(&self, transaction: &EvmTransaction) -> Result<(String, String)> {
        self.identity().await?;
        let result = self
            .rpc
            .call("eth_signTransaction", json!([transaction]))
            .await?;
        let raw = result
            .as_str()
            .or_else(|| result["raw"].as_str())
            .ok_or(ApiError::Unavailable)?
            .to_owned();
        let id = abi::transaction_id(&raw)?;
        Ok((id, raw))
    }

    /// Signed bytes are already durable when this is called. Errors require receipt reconciliation.
    pub async fn broadcast(
        &self,
        raw: &str,
    ) -> std::result::Result<Value, crate::chains::rpc::RpcError> {
        self.rpc.call("eth_sendRawTransaction", json!([raw])).await
    }
}
