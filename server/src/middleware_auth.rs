use axum::{
    async_trait,
    extract::{FromRequestParts, FromRef},
    http::request::Parts,
};
use axum_extra::extract::CookieJar;
use crate::AppState;
use shared::User;
use crate::error::AppError;

pub struct AuthUser(pub User);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let jar = CookieJar::from_request_parts(parts, state).await
            .map_err(|_| AppError::Internal("Failed to get cookies".into()))?;

        let token = jar.get("session")
            .map(|c| c.value().to_string())
            .ok_or(AppError::Unauthorized)?;

        let row = sqlx::query_as::<_, (uuid::Uuid, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT u.id, u.email, u.created_at FROM users u
             JOIN sessions s ON s.user_id = u.id
             WHERE s.token = $1 AND s.expires_at > NOW()"
        )
        .bind(token)
        .fetch_optional(&app_state.db)
        .await
        .map_err(|e| {
            eprintln!("Auth error: {:?}", e);
            AppError::Unauthorized
        })?
        .ok_or(AppError::Unauthorized)?;

        let user = User {
            id: row.0,
            email: row.1,
            created_at: row.2,
        };

        Ok(AuthUser(user))
    }
}
