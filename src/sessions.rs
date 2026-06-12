use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::db::{verify_password, VerifyResult};
use crate::users::{AuthUser, MeUser};

#[derive(Debug, FromRow)]
struct UserWithPassword {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub password_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: Option<String>,
    pub username: Option<String>,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: MeUser,
}

/// Valida credenciales (email o username + contraseña) y, si son correctas, crea una
/// sesión de 12 h. Devuelve `Some((token, usuario))` o `None` si no coinciden.
pub async fn login(
    pool: &SqlitePool,
    request: LoginRequest,
) -> anyhow::Result<Option<(String, AuthUser)>> {
    let login = request
        .email
        .or(request.username)
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    let user = sqlx::query_as::<_, UserWithPassword>(
        r#"
        SELECT id, username, email, display_name, role, password_hash
        FROM users
        WHERE lower(email) = ?1 OR lower(username) = ?1
        "#,
    )
    .bind(login)
    .fetch_optional(pool)
    .await?;

    let Some(user) = user else {
        return Ok(None);
    };

    let new_hash = match verify_password(&request.password, &user.password_hash) {
        VerifyResult::Invalid => return Ok(None),
        VerifyResult::Valid => None,
        VerifyResult::ValidNeedsRehash(h) => Some(h),
    };

    if let Some(h) = new_hash {
        sqlx::query("UPDATE users SET password_hash = ?2 WHERE id = ?1")
            .bind(&user.id)
            .bind(h)
            .execute(pool)
            .await?;
    }

    let token = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires_at = now + Duration::hours(12);

    sqlx::query(
        r#"
        INSERT INTO sessions (token, user_id, created_at, expires_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(&token)
    .bind(&user.id)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(Some((
        token,
        AuthUser {
            id: user.id,
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            role: user.role,
        },
    )))
}

/// Resuelve el usuario asociado a un token de sesión vigente (no vencido).
/// Devuelve `None` si el token no existe o ya expiró.
pub async fn user_by_session(pool: &SqlitePool, token: &str) -> anyhow::Result<Option<AuthUser>> {
    let user = sqlx::query_as::<_, AuthUser>(
        r#"
        SELECT u.id, u.username, u.email, u.display_name, u.role
        FROM sessions s
        JOIN users u ON u.id = s.user_id
        WHERE s.token = ?1 AND s.expires_at > ?2
        "#,
    )
    .bind(token)
    .bind(Utc::now())
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

/// Elimina la sesión correspondiente al token (cierre de sesión). Es idempotente.
pub async fn logout(pool: &SqlitePool, token: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}
