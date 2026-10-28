use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Asset {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub image: Option<String>,
    pub owner_id: Uuid,
    pub created_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAsset {
    pub request_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub image: Option<String>,
}
