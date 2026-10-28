pub mod user;
pub mod balances;

use sqlx::PgPool;

pub struct Store {
    pub pool: PgPool,
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
