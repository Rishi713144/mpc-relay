use crate::Store;
use serde::{Deserialize, Serialize};

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
}

impl std::fmt::Display for BalanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalanceError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for BalanceError {}

impl Store {
    pub async fn get_sol_balance(&self, user_id: &str) -> Result<i64, BalanceError> {
        // Try Balance table joined with Asset for native SOL? Or store SOL as asset; assignment says Balance has userId+assetId
        // Also maybe look up asset with mintAddress for SOL or store separately; but schema gives Balance. We'll also add fallback.
        let row = sqlx::query!(
            r#"SELECT b.amount as balance FROM "Balance" b
               JOIN "Asset" a ON a.id = b."assetId"
               JOIN "User" u ON u.id = b."userId"
               WHERE u.id = $1 AND (a."mintAddress" = 'So11111111111111111111111111111111111111112' OR a.symbol = 'SOL')
               LIMIT 1"#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| r.balance).unwrap_or(0))
    }

    pub async fn get_token_balances(&self, user_id: &str) -> Result<Vec<TokenBalanceView>, BalanceError> {
        let rows = sqlx::query!(
            r#"SELECT b.amount as balance, a."mintAddress" as token_mint, COALESCE(a.symbol, '') as symbol, a.decimals
               FROM "Balance" b
               JOIN "Asset" a ON a.id = b."assetId"
               JOIN "User" u ON u.id = b."userId"
               WHERE u.id = $1 AND (a."mintAddress" != 'So11111111111111111111111111111111111111112' AND COALESCE(a.symbol,'') != 'SOL')"#,
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        let balances = rows
            .into_iter()
            .map(|r| TokenBalanceView {
                balance: r.balance,
                token_mint: r.token_mint,
                symbol: r.symbol,
                decimals: r.decimals,
            })
            .collect();

        Ok(balances)
    }

    pub async fn upsert_balance(&self, user_id: &str, mint_address: &str, amount: i64, symbol: Option<&str>, decimals: i32) -> Result<(), BalanceError> {
        let user_exists = sqlx::query!(r#"SELECT id FROM "User" WHERE id = $1"#, user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;
        if user_exists.is_none() {
            return Err(BalanceError::DatabaseError("User not found".to_string()));
        }

        let asset_id = uuid::Uuid::new_v4().to_string();
        sqlx::query!(
            r#"INSERT INTO "Asset" (id, "mintAddress", decimals, symbol, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, NOW(), NOW())
               ON CONFLICT ("mintAddress") DO UPDATE SET "updatedAt" = NOW(), symbol = COALESCE(EXCLUDED.symbol, "Asset".symbol)"#,
            asset_id,
            mint_address,
            decimals,
            symbol,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        let asset_row = sqlx::query!(r#"SELECT id FROM "Asset" WHERE "mintAddress" = $1"#, mint_address)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        let balance_id = uuid::Uuid::new_v4().to_string();
        sqlx::query!(
            r#"INSERT INTO "Balance" (id, amount, "createdAt", "updatedAt", "userId", "assetId")
               VALUES ($1, $2, NOW(), NOW(), $3, $4)
               ON CONFLICT ("userId", "assetId") DO UPDATE SET amount = $2, "updatedAt" = NOW()"#,
            balance_id,
            amount,
            user_id,
            asset_row.id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| BalanceError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}
