use crate::chains::ethereum::{EvmTransaction, NftMovement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "network",
    content = "plan",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Payload {
    Ethereum(Box<EvmPlan>),
    Zcash(PaymentPlan),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvmPlan {
    pub sender: String,
    pub contract: String,
    pub calldata: String,
    pub movement: NftMovement,
    pub transaction: Option<EvmTransaction>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentPlan {
    #[serde(default)]
    pub request_amount: Option<u64>,
    #[serde(default)]
    pub signed_pczt: bool,
    pub sender_account: Uuid,
    pub sender_address: String,
    pub recipient_account: Uuid,
    pub recipient_address: String,
    pub value: u64,
    pub memo: String,
    pub fee: Option<u64>,
    pub expiry_height: Option<u64>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct Operation {
    pub id: Uuid,
    pub account_id: Uuid,
    pub recipient_id: Option<Uuid>,
    pub auction_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub kind: String,
    pub status: String,
    pub payload_sealed: Vec<u8>,
    pub pczt_sealed: Option<Vec<u8>>,
    pub signed_transaction: Option<String>,
    pub transaction_id: Option<String>,
    pub receipt_sealed: Option<Vec<u8>>,
    pub approval: Option<Value>,
    pub error: Option<String>,
    pub revision: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproveOperation {
    pub revision: i64,
}

#[derive(Serialize)]
pub struct OperationView {
    pub id: Uuid,
    pub account_id: Uuid,
    pub auction_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub kind: String,
    pub status: String,
    pub network: &'static str,
    pub title: String,
    pub transaction_id: Option<String>,
    pub ethereum_transaction: Option<EvmTransaction>,
    pub nft: Option<NftMovement>,
    pub amount: Option<String>,
    pub fee: Option<String>,
    pub recipient: Option<String>,
    pub error: Option<String>,
    pub revision: i64,
    pub can_approve: bool,
    pub remaining_ms: Option<i64>,
    pub timer_running: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub confirmation: Option<Value>,
}
