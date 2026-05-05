use crate::analysis::AnalysisResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::path::PathBuf;
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
