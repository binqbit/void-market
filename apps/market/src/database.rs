use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};
use std::time::Duration;

pub static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

pub async fn connect(options: sqlx::postgres::PgConnectOptions) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
}

pub async fn ready(pool: &PgPool) -> bool {
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        sqlx::query_as::<_, (i64, Vec<u8>, bool)>(
            "SELECT
                 version,
                 checksum,
                 success
             FROM _sqlx_migrations
             ORDER BY version",
        )
        .fetch_all(pool),
    )
    .await;
    let Ok(Ok(applied)) = result else {
        return false;
    };
    let expected: Vec<_> = MIGRATIONS.iter().collect();
    applied.len() == expected.len()
        && applied
            .iter()
            .zip(expected)
            .all(|((version, checksum, success), migration)| {
                *success
                    && *version == migration.version
                    && checksum.as_slice() == migration.checksum.as_ref()
            })
}
