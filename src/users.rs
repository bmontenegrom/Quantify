use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::db::{hash_password, verify_password, VerifyResult};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
}

/// Usuario autenticado con defaults de perfil (grupo y mesa por defecto).
/// Devuelto por `GET /api/auth/me` y `POST /api/auth/profile`.
#[derive(Debug, Serialize, FromRow)]
pub struct MeUser {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub default_group_id: Option<String>,
    pub default_table_number: Option<i64>,
}

/// Input para actualizar el perfil propio (nombre, email, y opcionalmente grupo/mesa por defecto).
#[derive(Debug, Deserialize)]
pub struct UpdateProfileInput {
    pub display_name: String,
    pub email: String,
    #[serde(default)]
    pub default_group_id: Option<String>,
    #[serde(default)]
    pub default_table_number: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPassword {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUser {
    pub email: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePassword {
    pub current_password: String,
    pub new_password: String,
}

/// Lista todos los usuarios ordenados por rol y nombre.
pub async fn users(pool: &SqlitePool) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users ORDER BY role, display_name",
    )
    .fetch_all(pool)
    .await?)
}

/// Lista los usuarios con rol `estudiante`, ordenados por nombre.
pub async fn students(pool: &SqlitePool) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users WHERE role = 'estudiante' ORDER BY display_name",
    )
    .fetch_all(pool)
    .await?)
}

/// Crea un usuario nuevo (email normalizado a minúsculas, contraseña hasheada) y lo devuelve.
pub async fn create_user(pool: &SqlitePool, input: CreateUser) -> anyhow::Result<AuthUser> {
    let id = Uuid::new_v4().to_string();
    let email = input.email.trim().to_lowercase();
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, display_name, role, password_hash, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(&id)
    .bind(&email)
    .bind(&email)
    .bind(input.display_name.trim())
    .bind(input.role.trim())
    .bind(hash_password(&input.password))
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Restablece (sobrescribe) la contraseña de un usuario por id, como acción docente/admin.
/// Devuelve `true` si el usuario existía y se actualizó.
pub async fn reset_password(
    pool: &SqlitePool,
    user_id: &str,
    input: ResetPassword,
) -> anyhow::Result<bool> {
    let result = sqlx::query("UPDATE users SET password_hash = ?2 WHERE id = ?1")
        .bind(user_id)
        .bind(hash_password(&input.password))
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Actualiza email (= username), nombre y rol de un usuario. Devuelve `None` si no existe.
pub async fn update_user(
    pool: &SqlitePool,
    user_id: &str,
    input: UpdateUser,
) -> anyhow::Result<Option<AuthUser>> {
    let email = input.email.trim().to_lowercase();
    let display_name = input.display_name.trim().to_string();
    let role = input.role.trim().to_string();

    let result = sqlx::query(
        r#"
        UPDATE users
        SET username = ?2,
            email = ?2,
            display_name = ?3,
            role = ?4
        WHERE id = ?1
        "#,
    )
    .bind(user_id)
    .bind(&email)
    .bind(&display_name)
    .bind(&role)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(
        sqlx::query_as::<_, AuthUser>(
            "SELECT id, username, email, display_name, role FROM users WHERE id = ?1",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?,
    ))
}

/// Cambia la contraseña del propio usuario validando la actual. Si tiene éxito invalida
/// todas sus sesiones. Devuelve `false` si el usuario no existe o la contraseña actual no coincide.
pub async fn change_password(
    pool: &SqlitePool,
    user_id: &str,
    input: ChangePassword,
) -> anyhow::Result<bool> {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    let Some(stored_hash) = stored else {
        return Ok(false);
    };

    if matches!(
        verify_password(&input.current_password, &stored_hash),
        VerifyResult::Invalid
    ) {
        return Ok(false);
    }

    sqlx::query("UPDATE users SET password_hash = ?2 WHERE id = ?1")
        .bind(user_id)
        .bind(hash_password(&input.new_password))
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM sessions WHERE user_id = ?1")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(true)
}

/// Devuelve el usuario con sus defaults de perfil (grupo y mesa por defecto).
pub async fn me_user(pool: &SqlitePool, user_id: &str) -> anyhow::Result<Option<MeUser>> {
    Ok(sqlx::query_as::<_, MeUser>(
        r#"
        SELECT u.id, u.username, u.email, u.display_name, u.role,
               u.default_group_id,
               udt.table_number AS default_table_number
        FROM users u
        LEFT JOIN user_default_tables udt
            ON udt.user_id = u.id AND udt.group_id = u.default_group_id
        WHERE u.id = ?1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

/// Actualiza el grupo por defecto del usuario en su perfil.
pub async fn set_user_default_group(
    pool: &SqlitePool,
    user_id: &str,
    group_id: &str,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE users SET default_group_id = ?1 WHERE id = ?2")
        .bind(group_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Guarda o actualiza la mesa por defecto del alumno para un grupo. Valida que el alumno
/// pertenezca al grupo y que la mesa esté en rango (1..=table_count).
/// Devuelve `None` si el grupo no existe o el alumno no pertenece a él.
pub async fn set_user_default_table(
    pool: &SqlitePool,
    user_id: &str,
    group_id: &str,
    table_number: i64,
) -> anyhow::Result<Option<()>> {
    let group: Option<(i64,)> = sqlx::query_as("SELECT table_count FROM lab_groups WHERE id = ?1")
        .bind(group_id)
        .fetch_optional(pool)
        .await?;

    let Some((table_count,)) = group else {
        return Ok(None);
    };

    if table_number < 1 || table_number > table_count {
        return Ok(None);
    }

    let is_member: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM group_members WHERE group_id = ?1 AND user_id = ?2")
            .bind(group_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    if is_member.is_none() {
        return Ok(None);
    }

    sqlx::query(
        r#"
        INSERT INTO user_default_tables (user_id, group_id, table_number, updated_at)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(user_id, group_id) DO UPDATE SET
            table_number = excluded.table_number,
            updated_at   = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(group_id)
    .bind(table_number)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(Some(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrate, seed_academic, seed_practices, seed_users};
    use crate::sessions::{login, user_by_session, LoginRequest};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    const TEACHER: &str = "docente@quantify.local";
    const STUDENT: &str = "estudiante@quantify.local";

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

    async fn find_user(pool: &SqlitePool, email: &str) -> AuthUser {
        sqlx::query_as::<_, AuthUser>(
            "SELECT id, username, email, display_name, role FROM users WHERE email = ?1",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn create_user_lowercases_email() {
        let (pool, _dir) = pool().await;
        let u = create_user(
            &pool,
            CreateUser {
                email: "  UPPER@TEST.LOCAL  ".into(),
                display_name: "Upper Test".into(),
                role: "estudiante".into(),
                password: "pw".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(u.email, "upper@test.local");
        assert_eq!(u.username, "upper@test.local");
    }

    #[tokio::test]
    async fn reset_password_changes_login() {
        let (pool, _dir) = seeded().await;
        let user = find_user(&pool, TEACHER).await;
        let ok = reset_password(
            &pool,
            &user.id,
            ResetPassword {
                password: "nuevaclave".into(),
            },
        )
        .await
        .unwrap();
        assert!(ok);
        let session = login(
            &pool,
            LoginRequest {
                email: Some(TEACHER.into()),
                username: None,
                password: "nuevaclave".into(),
            },
        )
        .await
        .unwrap();
        assert!(session.is_some());

        let missing = reset_password(
            &pool,
            "id-inexistente",
            ResetPassword {
                password: "x12345678".into(),
            },
        )
        .await
        .unwrap();
        assert!(
            !missing,
            "reset sobre un usuario inexistente debe devolver false"
        );
    }

    #[tokio::test]
    async fn update_user_changes_fields_and_handles_missing() {
        let (pool, _dir) = seeded().await;
        let user = find_user(&pool, TEACHER).await;
        let updated = update_user(
            &pool,
            &user.id,
            UpdateUser {
                email: "nuevo@test.local".into(),
                display_name: "Nuevo Nombre".into(),
                role: "estudiante".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.email, "nuevo@test.local");
        assert_eq!(updated.display_name, "Nuevo Nombre");
        assert_eq!(updated.role, "estudiante");

        let not_found = update_user(
            &pool,
            "id-inexistente",
            UpdateUser {
                email: "x@x.com".into(),
                display_name: "X".into(),
                role: "estudiante".into(),
            },
        )
        .await
        .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn change_password_validates_current_and_clears_sessions() {
        let (pool, _dir) = seeded().await;
        let user = find_user(&pool, TEACHER).await;

        let (token, _) = login(
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

        let wrong = change_password(
            &pool,
            &user.id,
            ChangePassword {
                current_password: "incorrecta".into(),
                new_password: "nueva".into(),
            },
        )
        .await
        .unwrap();
        assert!(!wrong);
        assert!(
            user_by_session(&pool, &token).await.unwrap().is_some(),
            "sesión debe seguir activa tras intento fallido"
        );

        let ok = change_password(
            &pool,
            &user.id,
            ChangePassword {
                current_password: "docente123".into(),
                new_password: "clave_nueva".into(),
            },
        )
        .await
        .unwrap();
        assert!(ok);
        assert!(
            user_by_session(&pool, &token).await.unwrap().is_none(),
            "sesión debe invalidarse tras cambio de contraseña"
        );
    }

    #[tokio::test]
    async fn students_lists_only_estudiantes() {
        let (pool, _dir) = seeded().await;
        let all = users(&pool).await.unwrap();
        let studs = students(&pool).await.unwrap();
        let expected: Vec<_> = all
            .iter()
            .filter(|u| u.role == "estudiante")
            .map(|u| u.id.clone())
            .collect();
        let got: Vec<_> = studs.iter().map(|u| u.id.clone()).collect();
        assert_eq!(
            got, expected,
            "students() debe devolver exactamente los usuarios con rol 'estudiante'"
        );
        assert!(studs.iter().all(|u| u.role == "estudiante"));
        assert!(
            all.len() > studs.len(),
            "debe haber al menos un usuario no-estudiante en el seed"
        );
        assert_eq!(studs.len(), 1);
        assert_eq!(studs[0].email, STUDENT);
    }
}
