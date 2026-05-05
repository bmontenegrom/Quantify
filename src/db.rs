use crate::analysis::AnalysisResult;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::{env, path::PathBuf};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub upload_dir: PathBuf,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Practice {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, FromRow)]
struct UserWithPassword {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub password_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: AuthUser,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SubmissionListItem {
    pub id: String,
    pub student_name: String,
    pub group_name: String,
    pub course: String,
    pub practice_id: String,
    pub practice_name: String,
    pub status: String,
    pub score: Option<f64>,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SubmissionRecord {
    pub id: String,
    pub student_name: String,
    pub group_name: String,
    pub course: String,
    pub practice_id: String,
    pub practice_name: String,
    pub file_name: String,
    pub csv_path: String,
    pub analysis_json: String,
    pub status: String,
    pub teacher_comment: Option<String>,
    pub score: Option<f64>,
    pub submitted_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct SubmissionDetail {
    pub id: String,
    pub student_name: String,
    pub group_name: String,
    pub course: String,
    pub practice_id: String,
    pub practice_name: String,
    pub file_name: String,
    pub analysis: AnalysisResult,
    pub status: String,
    pub teacher_comment: Option<String>,
    pub score: Option<f64>,
    pub submitted_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct NewSubmission {
    pub student_name: String,
    pub group_name: String,
    pub course: String,
    pub practice_id: String,
    pub file_name: String,
    pub csv_content: String,
    pub analysis: AnalysisResult,
}

#[derive(Debug, Deserialize)]
pub struct ReviewSubmission {
    pub status: String,
    pub teacher_comment: Option<String>,
    pub score: Option<f64>,
}

pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('estudiante', 'docente', 'admin')),
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practices (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS submissions (
            id TEXT PRIMARY KEY,
            student_name TEXT NOT NULL,
            group_name TEXT NOT NULL,
            course TEXT NOT NULL,
            practice_id TEXT NOT NULL REFERENCES practices(id),
            file_name TEXT NOT NULL,
            csv_path TEXT NOT NULL,
            analysis_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pendiente',
            teacher_comment TEXT,
            score REAL,
            submitted_at TEXT NOT NULL,
            reviewed_at TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn seed_users(pool: &SqlitePool) -> anyhow::Result<()> {
    let users = [
        (
            "admin",
            "Administrador",
            "admin",
            env::var("SEED_ADMIN_PASSWORD").unwrap_or_else(|_| "admin123".into()),
        ),
        (
            "docente",
            "Docente de prueba",
            "docente",
            env::var("SEED_TEACHER_PASSWORD").unwrap_or_else(|_| "docente123".into()),
        ),
        (
            "estudiante",
            "Estudiante de prueba",
            "estudiante",
            env::var("SEED_STUDENT_PASSWORD").unwrap_or_else(|_| "estudiante123".into()),
        ),
    ];

    for (username, display_name, role, password) in users {
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?1")
            .bind(username)
            .fetch_optional(pool)
            .await?;

        if exists.is_none() {
            sqlx::query(
                r#"
                INSERT INTO users (id, username, display_name, role, password_hash, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(username)
            .bind(display_name)
            .bind(role)
            .bind(hash_password(&password))
            .bind(Utc::now())
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

pub async fn login(
    pool: &SqlitePool,
    request: LoginRequest,
) -> anyhow::Result<Option<(String, AuthUser)>> {
    let user = sqlx::query_as::<_, UserWithPassword>(
        r#"
        SELECT id, username, display_name, role, password_hash
        FROM users
        WHERE username = ?1
        "#,
    )
    .bind(request.username.trim())
    .fetch_optional(pool)
    .await?;

    let Some(user) = user else {
        return Ok(None);
    };

    if !verify_password(&request.password, &user.password_hash) {
        return Ok(None);
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
            display_name: user.display_name,
            role: user.role,
        },
    )))
}

pub async fn user_by_session(pool: &SqlitePool, token: &str) -> anyhow::Result<Option<AuthUser>> {
    let user = sqlx::query_as::<_, AuthUser>(
        r#"
        SELECT u.id, u.username, u.display_name, u.role
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

pub async fn logout(pool: &SqlitePool, token: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn seed_practices(pool: &SqlitePool) -> anyhow::Result<()> {
    let practices = [
        (
            "pendulo",
            "Pendulo simple",
            "Medicion de periodos y estimacion de g mediante ajuste lineal.",
        ),
        (
            "hooke",
            "Ley de Hooke",
            "Relacion entre fuerza aplicada y elongacion del resorte.",
        ),
        (
            "caida-libre",
            "Caida libre",
            "Analisis de posicion, velocidad y aceleracion a partir de mediciones.",
        ),
    ];

    for (id, name, description) in practices {
        sqlx::query(
            r#"
            INSERT INTO practices (id, name, description)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn practices(pool: &SqlitePool) -> anyhow::Result<Vec<Practice>> {
    let rows =
        sqlx::query_as::<_, Practice>("SELECT id, name, description FROM practices ORDER BY name")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}

pub async fn create_submission(
    pool: &SqlitePool,
    upload_dir: &std::path::Path,
    submission: NewSubmission,
) -> anyhow::Result<SubmissionDetail> {
    let id = Uuid::new_v4().to_string();
    let submitted_at = Utc::now();
    let csv_path = upload_dir.join(format!("{id}.csv"));
    tokio::fs::write(&csv_path, submission.csv_content.as_bytes()).await?;
    let analysis_json = serde_json::to_string(&submission.analysis)?;

    sqlx::query(
        r#"
        INSERT INTO submissions (
            id, student_name, group_name, course, practice_id, file_name,
            csv_path, analysis_json, status, submitted_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pendiente', ?9)
        "#,
    )
    .bind(&id)
    .bind(&submission.student_name)
    .bind(&submission.group_name)
    .bind(&submission.course)
    .bind(&submission.practice_id)
    .bind(&submission.file_name)
    .bind(csv_path.to_string_lossy().to_string())
    .bind(analysis_json)
    .bind(submitted_at)
    .execute(pool)
    .await?;

    submission_detail(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created submission not found"))
}

pub async fn submission_list(pool: &SqlitePool) -> anyhow::Result<Vec<SubmissionListItem>> {
    let rows = sqlx::query_as::<_, SubmissionListItem>(
        r#"
        SELECT
            s.id,
            s.student_name,
            s.group_name,
            s.course,
            s.practice_id,
            p.name AS practice_name,
            s.status,
            s.score,
            s.submitted_at
        FROM submissions s
        JOIN practices p ON p.id = s.practice_id
        ORDER BY s.submitted_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn submission_detail(
    pool: &SqlitePool,
    id: &str,
) -> anyhow::Result<Option<SubmissionDetail>> {
    let row = sqlx::query_as::<_, SubmissionRecord>(
        r#"
        SELECT
            s.id,
            s.student_name,
            s.group_name,
            s.course,
            s.practice_id,
            p.name AS practice_name,
            s.file_name,
            s.csv_path,
            s.analysis_json,
            s.status,
            s.teacher_comment,
            s.score,
            s.submitted_at,
            s.reviewed_at
        FROM submissions s
        JOIN practices p ON p.id = s.practice_id
        WHERE s.id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        let analysis = serde_json::from_str(&row.analysis_json)?;
        Ok(SubmissionDetail {
            id: row.id,
            student_name: row.student_name,
            group_name: row.group_name,
            course: row.course,
            practice_id: row.practice_id,
            practice_name: row.practice_name,
            file_name: row.file_name,
            analysis,
            status: row.status,
            teacher_comment: row.teacher_comment,
            score: row.score,
            submitted_at: row.submitted_at,
            reviewed_at: row.reviewed_at,
        })
    })
    .transpose()
}

pub async fn update_review(
    pool: &SqlitePool,
    id: &str,
    review: ReviewSubmission,
) -> anyhow::Result<Option<SubmissionDetail>> {
    sqlx::query(
        r#"
        UPDATE submissions
        SET status = ?2,
            teacher_comment = ?3,
            score = ?4,
            reviewed_at = ?5
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(review.status)
    .bind(review.teacher_comment)
    .bind(review.score)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    submission_detail(pool, id).await
}

fn hash_password(password: &str) -> String {
    let salt = Uuid::new_v4().to_string();
    format!("{salt}:{}", digest_password(&salt, password))
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    let Some((salt, expected)) = password_hash.split_once(':') else {
        return false;
    };
    digest_password(salt, password) == expected
}

fn digest_password(salt: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}
