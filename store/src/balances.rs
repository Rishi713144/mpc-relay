use crate::Store;
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Canonical asset key used for native SOL in the `Asset` table.
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const SOL_SYMBOL: &str = "SOL";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalanceView {
    pub balance: i64,
    #[serde(rename = "tokenMint")]
    pub token_mint: String,
    pub symbol: String,
    pub decimals: i32,
}

#[derive(Debug)]
pub enum BalanceError {
    DatabaseError(String),
    UnknownUser,
}

impl std::fmt::Display for BalanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalanceError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            BalanceError::UnknownUser => write!(f, "User not found"),
        }
    }
}

impl std::error::Error for BalanceError {}

impl Store {
    /// Make sure an `Asset` row exists and return its id.
    pub async fn ensure_asset(
        &self,
        mint_address: &str,
        symbol: Option<&str>,
        decimals: i32,
    ) -> Result<String, BalanceError> {
        if let Some(id) = sqlx::query_scalar::<_, String>(
            r#"SELECT id FROM "Asset" WHERE "mintAddress" = $1"#,
        )
        .bind(mint_address)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?
        {
            return Ok(id);
        }

        let asset_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            r#"INSERT INTO "Asset" (id, "mintAddress", decimals, symbol, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, NOW(), NOW())
               ON CONFLICT ("mintAddress") DO UPDATE
                 SET decimals = EXCLUDED.decimals,
                     symbol = COALESCE(EXCLUDED.symbol, "Asset".symbol),
                     "updatedAt" = NOW()"#,
        )
        .bind(&asset_id)
        .bind(mint_address)
        .bind(decimals)
        .bind(symbol)
        .execute(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        sqlx::query_scalar::<_, String>(r#"SELECT id FROM "Asset" WHERE "mintAddress" = $1"#)
            .bind(mint_address)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| BalanceError::DatabaseError(e.to_string()))
    }

    pub async fn upsert_balance(
        &self,
        user_id: &str,
        mint_address: &str,
        amount: i64,
        symbol: Option<&str>,
        decimals: i32,
    ) -> Result<(), BalanceError> {
        let user_exists = sqlx::query_scalar::<_, String>(r#"SELECT id FROM "User" WHERE id = $1"#)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        if user_exists.is_none() {
            return Err(BalanceError::UnknownUser);
        }

        let asset_id = self.ensure_asset(mint_address, symbol, decimals).await?;
        let balance_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"INSERT INTO "Balance" (id, amount, "createdAt", "updatedAt", "userId", "assetId")
               VALUES ($1, $2, NOW(), NOW(), $3, $4)
               ON CONFLICT ("userId", "assetId") DO UPDATE
                 SET amount = EXCLUDED.amount, "updatedAt" = NOW()"#,
        )
        .bind(&balance_id)
        .bind(amount)
        .bind(user_id)
        .bind(&asset_id)
        .execute(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_sol_balance(&self, user_id: &str) -> Result<i64, BalanceError> {
        let row = sqlx::query(
            r#"SELECT b.amount AS balance FROM "Balance" b
               JOIN "Asset" a ON a.id = b."assetId"
               WHERE b."userId" = $1 AND a."mintAddress" = $2
               LIMIT 1"#,
        )
        .bind(user_id)
        .bind(SOL_MINT)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| r.get::<i64, _>("balance")).unwrap_or(0))
    }

    pub async fn get_balance_for_mint(
        &self,
        user_id: &str,
        mint_address: &str,
    ) -> Result<i64, BalanceError> {
        let row = sqlx::query(
            r#"SELECT b.amount AS balance FROM "Balance" b
               JOIN "Asset" a ON a.id = b."assetId"
               WHERE b."userId" = $1 AND a."mintAddress" = $2
               LIMIT 1"#,
        )
        .bind(user_id)
        .bind(mint_address)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| r.get::<i64, _>("balance")).unwrap_or(0))
    }

    pub async fn get_token_balances(
        &self,
        user_id: &str,
    ) -> Result<Vec<TokenBalanceView>, BalanceError> {
        let rows = sqlx::query(
            r#"SELECT b.amount AS balance,
                      a."mintAddress" AS token_mint,
                      COALESCE(a.symbol, '') AS symbol,
                      a.decimals AS decimals
               FROM "Balance" b
               JOIN "Asset" a ON a.id = b."assetId"
               WHERE b."userId" = $1 AND a."mintAddress" <> $2
               ORDER BY a.symbol NULLS LAST"#,
        )
        .bind(user_id)
        .bind(SOL_MINT)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| TokenBalanceView {
                balance: r.get("balance"),
                token_mint: r.get("token_mint"),
                symbol: r.get("symbol"),
                decimals: r.get("decimals"),
            })
            .collect())
    }

    /// Look up a user by their on-chain address and return their balance row.
    pub async fn get_balance_by_pubkey(
        &self,
        pubkey: &str,
        mint_address: &str,
    ) -> Result<Option<(String, i64)>, BalanceError> {
        let row = sqlx::query(
            r#"SELECT u.id AS user_id, b.amount AS balance
               FROM "User" u
               JOIN "Balance" b ON b."userId" = u.id
               JOIN "Asset" a ON a.id = b."assetId"
               WHERE u."publicKey" = $1 AND a."mintAddress" = $2
               LIMIT 1"#,
        )
        .bind(pubkey)
        .bind(mint_address)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| (r.get::<String, _>("user_id"), r.get::<i64, _>("balance"))))
    }
}
