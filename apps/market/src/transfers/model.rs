use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferAsset {
    pub request_id: Uuid,
    pub recipient_address: Uuid,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Transfer {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub created_at: String,
}
