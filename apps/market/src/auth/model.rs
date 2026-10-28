use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterOptions {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginOptions {
    pub address: Uuid,
}

#[derive(Serialize)]
pub struct Options {
    pub ceremony_id: Uuid,
    pub address: Uuid,
    pub public_key: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterFinish {
    pub ceremony_id: Uuid,
    pub credential: RegisterPublicKeyCredential,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginFinish {
    pub ceremony_id: Uuid,
    pub credential: PublicKeyCredential,
}
