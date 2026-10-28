use crate::Store;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub email: String,
    pub public_key: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug)]
pub enum UserError {
    UserExists,
    InvalidInput(String),
    InvalidCredentials,
    DatabaseError(String),
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::UserExists => write!(f, "User already exists"),
            UserError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            UserError::InvalidCredentials => write!(f, "Invalid credentials"),
            UserError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for UserError {}

impl Store {
    pub async fn create_user(&self, request: CreateUserRequest) -> Result<User, UserError> {
        if !request.email.contains('@') {
            return Err(UserError::InvalidInput("Invalid email format".to_string()));
        }

        if request.password.len() < 6 {
            return Err(UserError::InvalidInput(
                "Password must be at least 6 characters".to_string(),
            ));
        }

        let existing = sqlx::query(r#"SELECT id FROM "User" WHERE email = $1"#)
            .bind(&request.email)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        if existing.is_some() {
            return Err(UserError::UserExists);
        }

        let password_hash = bcrypt::hash(&request.password, bcrypt::DEFAULT_COST)
            .map_err(|e| UserError::DatabaseError(format!("Password hashing failed: {e}")))?;

        let user_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"INSERT INTO "User" (id, email, password, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, NOW(), NOW())"#,
        )
        .bind(&user_id)
        .bind(&request.email)
        .bind(&password_hash)
        .execute(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(User {
            id: user_id,
            email: request.email,
            public_key: None,
            created_at: None,
        })
    }

    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, UserError> {
        let row = sqlx::query(
            r#"SELECT id, email, "publicKey" AS public_key, "createdAt"::text AS created_at
               FROM "User" WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            email: r.get("email"),
            public_key: r.get("public_key"),
            created_at: r.get("created_at"),
        }))
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, UserError> {
        let row = sqlx::query(
            r#"SELECT id, email, "publicKey" AS public_key, "createdAt"::text AS created_at
               FROM "User" WHERE email = $1"#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            email: r.get("email"),
            public_key: r.get("public_key"),
            created_at: r.get("created_at"),
        }))
    }

    /// Check the password hash for an existing user.
    pub async fn authenticate(
        &self,
        email: &str,
        password: &str,
    ) -> Result<User, UserError> {
        let row = sqlx::query(
            r#"SELECT id, email, password, "publicKey" AS public_key, "createdAt"::text AS created_at
               FROM "User" WHERE email = $1"#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?
        .ok_or(UserError::InvalidCredentials)?;

        let hash: String = row.get("password");
        let valid = bcrypt::verify(password, &hash)
            .map_err(|e| UserError::DatabaseError(format!("Password check failed: {e}")))?;

        if !valid {
            return Err(UserError::InvalidCredentials);
        }

        Ok(User {
            id: row.get("id"),
            email: row.get("email"),
            public_key: row.get("public_key"),
            created_at: row.get("created_at"),
        })
    }

    pub async fn set_public_key(&self, user_id: &str, public_key: &str) -> Result<(), UserError> {
        sqlx::query(r#"UPDATE "User" SET "publicKey" = $1, "updatedAt" = NOW() WHERE id = $2"#)
            .bind(public_key)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| UserError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Resolve a user from their on-chain address (indexer subscriptions).
    pub async fn get_user_by_public_key(&self, public_key: &str) -> Result<Option<User>, UserError> {
        let row = sqlx::query(
            r#"SELECT id, email, "publicKey" AS public_key, "createdAt"::text AS created_at
               FROM "User" WHERE "publicKey" = $1"#,
        )
        .bind(public_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            email: r.get("email"),
            public_key: r.get("public_key"),
            created_at: r.get("created_at"),
        }))
    }

    /// Users that have a wallet address; the indexer watches these.
    pub async fn list_users_with_public_key(&self) -> Result<Vec<User>, UserError> {
        let rows = sqlx::query(
            r#"SELECT id, email, "publicKey" AS public_key, "createdAt"::text AS created_at
               FROM "User" WHERE "publicKey" IS NOT NULL"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| User {
                id: r.get("id"),
                email: r.get("email"),
                public_key: r.get("public_key"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}
