pub mod balances;
pub mod mpc;
pub mod user;

use sqlx::PgPool;

/// Thin wrapper around a Postgres pool.
///
/// The backend and the indexer share the main database. Each MPC node uses the
/// same type against its own database (which only contains `keyshares`).
pub struct Store {
    pub pool: PgPool,
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Connect using `DATABASE_URL`, failing with a readable error.
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self::new(pool))
    }
}
