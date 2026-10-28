use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Account {
    pub address: Uuid,
    pub name: String,
}
