use axum::{
    extract::{State, Json},
    response::IntoResponse,
    http::header::SET_COOKIE,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use shared::User;
use crate::{AppState, auth, error::AppError};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AuthRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: User,
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    let password_hash = auth::hash_password(&payload.password)?;

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id, email, created_at"
    )
    .bind(&payload.email)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await?;

    let token = auth::new_session_token();
    sqlx::query(
        "INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, NOW() + INTERVAL '30 days')"
    )
    .bind(&token)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    let cookie = Cookie::build(("session", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    Ok((
        [(SET_COOKIE, cookie.to_string())],
        Json(AuthResponse { user }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as::<_, (uuid::Uuid, String, chrono::DateTime<chrono::Utc>, String)>(
        "SELECT id, email, created_at, password_hash FROM users WHERE email = $1"
    )
    .bind(&payload.email)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;

    if !auth::verify_password(&payload.password, &row.3)? {
        return Err(AppError::Unauthorized);
    }

    let token = auth::new_session_token();
    sqlx::query(
        "INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, NOW() + INTERVAL '30 days')"
    )
    .bind(&token)
    .bind(row.0)
    .execute(&state.db)
    .await?;

    let cookie = Cookie::build(("session", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    let user = User {
        id: row.0,
        email: row.1,
        created_at: row.2,
    };

    Ok((
        [(SET_COOKIE, cookie.to_string())],
        Json(AuthResponse { user }),
    ))
}
