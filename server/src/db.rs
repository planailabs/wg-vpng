//! Database connection.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn connect(url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await
        .unwrap_or_else(|e| panic!("failed to connect to database: {e}"))
}
