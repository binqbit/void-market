use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Account {
    pub address: Uuid,
    pub name: String,
    pub username: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Profile {
    pub address: Uuid,
    pub name: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub is_demo: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProfile {
    pub name: String,
    pub username: String,
}
