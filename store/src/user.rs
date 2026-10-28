use crate::Store;
use uuid::Uuid;
use chrono::Utc;

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
    DatabaseError(String),
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::UserExists => write!(f, "User already exists"),
            UserError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
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
            return Err(UserError::InvalidInput("Password must be at least 6 characters".to_string()));
        }

        let existing_user = sqlx::query!(
            r#"SELECT id FROM "User" WHERE email = $1"#,
            request.email
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        if existing_user.is_some() {
            return Err(UserError::UserExists);
        }

        let password_hash = bcrypt::hash(&request.password, bcrypt::DEFAULT_COST)
            .map_err(|e| UserError::DatabaseError(format!("Password hashing failed: {}", e)))?;

        let user_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        sqlx::query!(
            r#"INSERT INTO "User" (id, email, password, "createdAt", "updatedAt") VALUES ($1, $2, $3, $4, $5)"#,
            user_id,
            request.email,
            password_hash,
            now,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        let user = User {
            id: user_id,
            email: request.email,
            public_key: None,
            created_at: Some(now.to_rfc3339()),
        };

        Ok(user)
    }

    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>, UserError> {
        let row = sqlx::query!(
            r#"SELECT id, email, "publicKey", "createdAt" FROM "User" WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| User {
            id: r.id,
            email: r.email,
            public_key: r.publicKey,
            created_at: r.createdAt.map(|d| d.to_rfc3339()),
        }))
    }

    pub async fn set_public_key(&self, user_id: &str, public_key: &str) -> Result<(), UserError> {
        let now = Utc::now();
        sqlx::query!(
            r#"UPDATE "User" SET "publicKey" = $1, "updatedAt" = $2 WHERE id = $3"#,
            public_key,
            now,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}
