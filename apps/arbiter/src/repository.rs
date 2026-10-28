use crate::{
    config::Config,
    error::{Error, Result},
};
use arbiter_protocol::{Completion, Job};
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use uuid::Uuid;

pub static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Repository {
    pub pool: PgPool,
    pub node: Uuid,
}

impl Repository {
    pub async fn connect(config: &Config) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect_with(config.database.clone())
            .await?;
        Ok(Self {
            pool,
            node: config.node_id,
        })
    }

    pub async fn identity(&self, public_key: &str) -> Result<Uuid> {
        sqlx::query(
            "INSERT INTO node_identity(node_id, instance_id, public_key)
             VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(self.node)
        .bind(Uuid::new_v4())
        .bind(public_key)
        .execute(&self.pool)
        .await?;
        let (instance, key): (Uuid, String) =
            sqlx::query_as("SELECT instance_id,public_key FROM node_identity WHERE node_id=$1")
                .bind(self.node)
                .fetch_one(&self.pool)
                .await?;
        if key != public_key {
            return Err(Error::Configuration);
        }
        Ok(instance)
    }

    pub async fn sequence(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "UPDATE node_identity
             SET heartbeat_sequence = heartbeat_sequence + 1
             WHERE node_id = $1
             RETURNING heartbeat_sequence",
        )
        .bind(self.node)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn started(&self, job: &Job) -> Result<()> {
        sqlx::query(
            "INSERT INTO node_attempts(node_id, job_id, generation, status)
             VALUES ($1, $2, $3, 'running')
             ON CONFLICT (node_id, job_id) DO UPDATE
             SET generation = EXCLUDED.generation, status = 'running', pending_result = NULL",
        )
        .bind(self.node)
        .bind(job.id)
        .bind(job.generation)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn prepare(&self, result: &Completion) -> Result<()> {
        let changed = sqlx::query(
            "UPDATE node_attempts
             SET status = 'result_pending', pending_result = $3
             WHERE node_id = $1
             AND job_id = $2
             AND generation = $4",
        )
        .bind(self.node)
        .bind(result.job_id)
        .bind(serde_json::json!(result))
        .bind(result.generation)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Ownership);
        }
        Ok(())
    }

    pub async fn delivered(&self, result: &Completion) -> Result<()> {
        sqlx::query(
            "UPDATE node_attempts SET status=$3,pending_result=NULL WHERE node_id=$1 AND job_id=$2",
        )
        .bind(self.node)
        .bind(result.job_id)
        .bind(result.status.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn pending(&self) -> Result<Vec<Completion>> {
        let values: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT pending_result FROM node_attempts
             WHERE node_id = $1 AND pending_result IS NOT NULL",
        )
        .bind(self.node)
        .fetch_all(&self.pool)
        .await?;
        values
            .into_iter()
            .map(|value| serde_json::from_value(value).map_err(|_| Error::Configuration))
            .collect()
    }

    pub async fn unfinished(&self) -> Result<Vec<(Uuid, i64)>> {
        Ok(sqlx::query_as(
            "SELECT job_id, generation FROM node_attempts
             WHERE node_id = $1 AND status IN ('running', 'waiting')",
        )
        .bind(self.node)
        .fetch_all(&self.pool)
        .await?)
    }
}
