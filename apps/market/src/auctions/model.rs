use arbiter_protocol::ArtifactRef;
use check_workflow::messages::KeyMaterial;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const BUYER_LIMIT: usize = 3;
pub const APPROVAL_WINDOW_MS: i64 = 60 * 60 * 1000;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAuction {
    pub request_id: Uuid,
    pub asset_id: Uuid,
    pub reserve: String,
    pub closes_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditAuction {
    pub revision: i64,
    pub reserve: String,
    pub closes_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishAuction {
    pub revision: i64,
    pub consent: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceBid {
    pub request_id: Uuid,
    pub amount: String,
    pub policy_hash: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteSale {
    pub request_id: Uuid,
    pub generation: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenInput {
    pub owner: Uuid,
    pub material: KeyMaterial,
    pub ciphertext: ArtifactRef,
}

#[derive(sqlx::FromRow)]
pub(crate) struct AuctionRow {
    pub execution: String,
    pub custody_status: String,
    pub payment_status: String,
    pub settlement_note: Option<String>,
    pub payment_invoice: Option<Vec<u8>>,
    pub id: Uuid,
    pub asset_id: Uuid,
    pub seller_id: Uuid,
    pub node_id: Option<Uuid>,
    pub status: String,
    pub closes_at_ms: i64,
    pub width: i16,
    pub buyer_limit: i16,
    pub asset_snapshot: Value,
    pub creation_hash: Vec<u8>,
    pub policy_hash: Option<Vec<u8>>,
    pub publish_receipt: Option<Value>,
    pub input_root: Option<Vec<u8>>,
    pub round_root: Option<Vec<u8>>,
    pub generation: i64,
    pub winner_id: Option<Uuid>,
    pub completion_request_id: Option<Uuid>,
    pub completion_receipt: Option<Value>,
    pub award_hash: Option<Vec<u8>>,
    pub reason: Option<String>,
    pub clock_epoch: Option<Uuid>,
    pub last_tick_ms: Option<i64>,
    pub revision: i64,
    pub created_at_ms: i64,
    pub published_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
}

impl AuctionRow {
    pub fn bidding_deadline(&self) -> i64 {
        self.closes_at_ms.saturating_sub(APPROVAL_WINDOW_MS)
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct Participant {
    pub account_id: Uuid,
    pub id: Uuid,
    pub admission_seq: i64,
    pub input_sealed: Vec<u8>,
    pub frozen_input: Option<Value>,
    pub status: String,
    pub remaining_ms: i64,
    pub chargeable: bool,
    pub exit_receipt: Option<Value>,
    pub created_at_ms: i64,
}

#[derive(Serialize)]
pub struct SaleRequest {
    pub revision: i64,
    pub created_at_ms: i64,
    pub payment_status: String,
    pub auction_id: Uuid,
    pub request_id: Uuid,
    pub generation: i64,
    pub title: String,
    pub price: String,
    pub buyer: Value,
    pub remaining_ms: i64,
    pub timer_running: bool,
}

#[derive(Serialize)]
pub struct AuctionView {
    pub id: Uuid,
    pub asset: Value,
    pub seller: crate::accounts::model::Account,
    pub winner: Option<crate::accounts::model::Account>,
    pub status: String,
    pub reason: Option<String>,
    pub currency: &'static str,
    pub execution: &'static str,
    pub payment_status: String,
    pub custody_status: String,
    pub settlement_note: Option<String>,
    pub payment: Option<serde_json::Value>,
    pub wallet_ready: bool,
    pub closes_at_ms: i64,
    pub bidding_closes_at_ms: i64,
    pub created_at_ms: i64,
    pub required_from_ms: i64,
    pub published_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub revision: i64,
    pub policy_hash: Option<String>,
    pub buyer_limit: usize,
    pub admitted_buyers: i64,
    pub active_buyers: i64,
    pub generation: i64,
    pub checks_completed: i64,
    pub checks_total: i64,
    pub reserve: Option<String>,
    pub your_bid: Option<String>,
    pub agreed_price: Option<String>,
    pub participation: Option<Participation>,
    pub completion_request_id: Option<Uuid>,
    pub actions: Actions,
}

#[derive(Serialize)]
pub struct Participation {
    pub status: String,
    pub created_at_ms: i64,
    pub remaining_ms: i64,
    pub timer_running: bool,
}

#[derive(Serialize)]
pub struct Actions {
    pub edit: bool,
    pub publish: bool,
    pub cancel: bool,
    pub bid: bool,
    pub withdraw: bool,
    pub complete: bool,
    pub close_bidding: bool,
}
