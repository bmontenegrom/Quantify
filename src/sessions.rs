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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{digest_password, migrate, seed_academic, seed_practices, seed_users};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    const TEACHER: &str = "docente@quantify.local";

    async fn pool() -> (SqlitePool, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let url = format!("sqlite:{}", db_path.to_string_lossy());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seeded() -> (SqlitePool, TempDir) {
        let (pool, dir) = pool().await;
        seed_practices(&pool).await.unwrap();
        seed_users(&pool).await.unwrap();
        seed_academic(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn login_succeeds_with_email_and_username() {
        let (pool, _dir) = seeded().await;
        let by_email = login(
            &pool,
            LoginRequest {
                email: Some(TEACHER.into()),
                username: None,
                password: "docente123".into(),
            },
        )
        .await
        .unwrap();
        assert!(by_email.is_some());

        let by_username = login(
            &pool,
            LoginRequest {
                email: None,
                username: Some(TEACHER.into()),
                password: "docente123".into(),
            },
        )
        .await
        .unwrap();
        assert!(by_username.is_some());
    }

    #[tokio::test]
    async fn login_fails_with_wrong_password() {
        let (pool, _dir) = seeded().await;
        let result = login(
            &pool,
            LoginRequest {
                email: Some(TEACHER.into()),
                username: None,
                password: "incorrecta".into(),
            },
        )
        .await
        .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn login_migrates_legacy_sha256_hash_to_argon2() {
        let (pool, _dir) = pool().await;
        let salt = "test-salt-uuid";
        let legacy_hash = format!("{salt}:{}", digest_password(salt, "clave1234"));
        // Inserta un usuario con hash legacy SHA-256 directamente en la base.
        sqlx::query(
            "INSERT INTO users (id, username, email, display_name, role, password_hash, created_at)
             VALUES ('u1', 'legacy', 'legacy@test.local', 'Legacy', 'estudiante', ?1, '2024-01-01')",
        )
        .bind(&legacy_hash)
        .execute(&pool)
        .await
        .unwrap();

        let wrong = login(
            &pool,
            LoginRequest {
                email: Some("legacy@test.local".into()),
                username: None,
                password: "incorrecta".into(),
            },
        )
        .await
        .unwrap();
        assert!(
            wrong.is_none(),
            "login con contraseña incorrecta y hash legacy debe fallar"
        );
        let not_migrated: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = 'u1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            not_migrated, legacy_hash,
            "el hash NO debe modificarse tras un intento fallido"
        );

        let result = login(
            &pool,
            LoginRequest {
                email: Some("legacy@test.local".into()),
                username: None,
                password: "clave1234".into(),
            },
        )
        .await
        .unwrap();
        assert!(result.is_some(), "login con hash legacy debe tener éxito");

        let updated: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            updated.starts_with("$argon2"),
            "el hash debe actualizarse a Argon2id tras el login"
        );

        let result2 = login(
            &pool,
            LoginRequest {
                email: Some("legacy@test.local".into()),
                username: None,
                password: "clave1234".into(),
            },
        )
        .await
        .unwrap();
        assert!(
            result2.is_some(),
            "login con hash Argon2id debe tener éxito"
        );
    }

    #[tokio::test]
    async fn session_lookup_and_logout() {
        let (pool, _dir) = seeded().await;
        let (token, user) = login(
            &pool,
            LoginRequest {
                email: Some(TEACHER.into()),
                username: None,
                password: "docente123".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();

        let resolved = user_by_session(&pool, &token).await.unwrap().unwrap();
        assert_eq!(resolved.id, user.id);
        assert!(user_by_session(&pool, "token-inexistente")
            .await
            .unwrap()
            .is_none());

        logout(&pool, &token).await.unwrap();
        assert!(user_by_session(&pool, &token).await.unwrap().is_none());
    }
}
