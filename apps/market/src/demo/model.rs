use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Actor {
    pub account_id: Uuid,
    pub alias: String,
    pub ethereum_address: String,
    pub zcash_account: Option<Uuid>,
    pub zcash_address: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct Deployment {
    pub ethereum_genesis: String,
    pub collection_address: Option<String>,
    pub collection_code_hash: String,
    pub deployment_transaction: Option<String>,
    pub signed_deployment: Option<String>,
    pub zcash_genesis: String,
    pub minter_address: String,
    pub faucet_account: Uuid,
    pub faucet_address: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    pub actor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequest {
    pub request_id: Uuid,
}

pub fn actor_id(alias: &str) -> Option<Uuid> {
    match alias {
        "alice" => Some(Uuid::from_u128(0xded00000000040008000000000000001)),
        "bob" => Some(Uuid::from_u128(0xded00000000040008000000000000002)),
        "daniel" => Some(Uuid::from_u128(0xded00000000040008000000000000003)),
        _ => None,
    }
}
