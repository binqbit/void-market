use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    NumberTransfer,
    MaskContribution,
    AnswerCommitment,
    AnswerReveal,
    MaskOpening,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NumberTransfer => "number_transfer",
            Self::MaskContribution => "mask_contribution",
            Self::AnswerCommitment => "answer_commitment",
            Self::AnswerReveal => "answer_reveal",
            Self::MaskOpening => "mask_opening",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approve {
    pub request_id: Uuid,
    pub revision: i64,
}

#[derive(Serialize)]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub session_title: String,
    pub auction_id: Option<Uuid>,
    pub can_withdraw: bool,
    pub request_id: Uuid,
    pub revision: i64,
    pub stage: ActionKind,
    pub step: u8,
    pub required_from_ms: Option<i64>,
    pub closes_at_ms: Option<i64>,
    pub status: String,
    pub participant_status: String,
    pub session_status: String,
    pub remaining_ms: i64,
    pub timer_running: bool,
    pub approval: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub custody_mode: &'static str,
    pub revision: i64,
    pub server_time_ms: i64,
    pub pending: usize,
    pub messages: Vec<Message>,
    pub sessions: Vec<SessionSummary>,
    pub sales: Vec<crate::auctions::model::SaleRequest>,
    pub transactions: Vec<crate::settlement::model::OperationView>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionInput {
    pub id: Uuid,
    pub title: String,
    pub node_id: Uuid,
    pub comparison: check_workflow::messages::Predicate,
    pub width: u8,
    pub participants: [ParticipantInput; 2],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantInput {
    pub account_id: Uuid,
    pub value: u64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SessionSummary {
    pub id: Uuid,
    pub title: String,
    pub auction_id: Option<Uuid>,
    pub can_withdraw: bool,
    pub status: String,
    pub participation_status: String,
    pub remaining_ms: i64,
    pub timer_running: bool,
    pub waiting_until_ms: Option<i64>,
}

pub(crate) struct BoundCheck {
    pub id: Uuid,
    pub auction: Uuid,
    pub generation: i64,
    pub title: String,
    pub node: Uuid,
    pub width: u8,
    pub predicate: check_workflow::messages::Predicate,
    pub scope: Vec<u8>,
    pub inputs: [crate::auctions::model::FrozenInput; 2],
}
