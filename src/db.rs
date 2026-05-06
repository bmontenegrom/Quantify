use crate::analysis::AnalysisResult;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row, SqlitePool};
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
    pub email: String,
    pub display_name: String,
    pub role: String,
}

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
    pub user: AuthUser,
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

#[derive(Debug, Serialize, FromRow)]
pub struct Course {
    pub id: String,
    pub name: String,
    pub term: String,
    pub active: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct LabGroup {
    pub id: String,
    pub course_id: String,
    pub name: String,
    pub table_count: i64,
    pub group_type: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PracticeSubgroup {
    pub id: String,
    pub course_id: String,
    pub practice_id: String,
    pub group_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PracticeTableAssignment {
    pub course_id: String,
    pub practice_id: String,
    pub group_id: String,
    pub user_id: String,
    pub table_number: i64,
}

#[derive(Debug, Serialize)]
pub struct CourseSummary {
    pub id: String,
    pub name: String,
    pub term: String,
    pub active: bool,
    pub members: Vec<AuthUser>,
    pub groups: Vec<GroupSummary>,
    pub practices: Vec<Practice>,
    pub subgroups: Vec<SubgroupSummary>,
    pub table_assignments: Vec<PracticeTableAssignment>,
}

#[derive(Debug, Serialize)]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub table_count: i64,
    pub group_type: String,
    pub members: Vec<AuthUser>,
}

#[derive(Debug, Serialize)]
pub struct SubgroupSummary {
    pub id: String,
    pub name: String,
    pub practice: Practice,
    pub group: GroupSummary,
    pub members: Vec<AuthUser>,
}

#[derive(Debug, Serialize)]
pub struct AcademicContext {
    pub courses: Vec<CourseSummary>,
    pub practices: Vec<Practice>,
    pub students: Vec<AuthUser>,
    pub users: Vec<AuthUser>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCourse {
    pub name: String,
    pub term: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCourse {
    pub name: String,
    pub term: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroup {
    pub name: String,
    pub table_count: Option<i64>,
    pub group_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroup {
    pub name: String,
    pub table_count: i64,
    pub group_type: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSubgroup {
    pub practice_id: String,
    pub group_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AddGroupMember {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SetPracticeTable {
    pub practice_id: String,
    pub table_number: i64,
}

#[derive(Debug, Deserialize)]
pub struct EnrollCourseMember {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SetCoursePractice {
    pub practice_id: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct GradeComponent {
    pub id: String,
    pub course_id: String,
    pub kind: String,
    pub name: String,
    pub max_points: f64,
    pub weight_points: f64,
    pub position: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct GradeScore {
    pub component_id: String,
    pub student_id: String,
    pub raw_points: f64,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GradeScoreDetail {
    pub component_id: String,
    pub kind: String,
    pub name: String,
    pub max_points: f64,
    pub weight_points: f64,
    pub raw_points: Option<f64>,
    pub normalized_points: f64,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StudentGradeSummary {
    pub student: AuthUser,
    pub scores: Vec<GradeScoreDetail>,
    pub totals_by_kind: Vec<GradeKindTotal>,
    pub total_points: f64,
    pub total_possible: f64,
}

#[derive(Debug, Serialize)]
pub struct GradeKindTotal {
    pub kind: String,
    pub points: f64,
    pub possible: f64,
}

#[derive(Debug, Serialize)]
pub struct CourseGradebook {
    pub course: Course,
    pub components: Vec<GradeComponent>,
    pub students: Vec<StudentGradeSummary>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGradeComponent {
    pub course_id: String,
    pub kind: String,
    pub name: String,
    pub max_points: f64,
    pub weight_points: f64,
}

#[derive(Debug, Deserialize)]
pub struct UpsertGradeScore {
    pub component_id: String,
    pub student_id: String,
    pub raw_points: f64,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SubmissionListItem {
    pub id: String,
    pub submitted_by_user_id: Option<String>,
    pub group_id: Option<String>,
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
    pub submitted_by_user_id: String,
    pub course_id: String,
    pub group_id: String,
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
        CREATE TABLE IF NOT EXISTS courses (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            term TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS lab_groups (
            id TEXT PRIMARY KEY,
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            table_count INTEGER NOT NULL DEFAULT 4,
            group_type TEXT NOT NULL DEFAULT 'regular',
            created_at TEXT NOT NULL,
            UNIQUE(course_id, name)
        )
        "#,
    )
    .execute(pool)
    .await?;
    ensure_lab_group_columns(pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS group_members (
            group_id TEXT NOT NULL REFERENCES lab_groups(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            PRIMARY KEY(group_id, user_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS course_members (
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            PRIMARY KEY(course_id, user_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS course_practices (
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            PRIMARY KEY(course_id, practice_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_subgroups (
            id TEXT PRIMARY KEY,
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            group_id TEXT NOT NULL REFERENCES lab_groups(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(course_id, practice_id, group_id, name)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_subgroup_members (
            subgroup_id TEXT NOT NULL REFERENCES practice_subgroups(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            PRIMARY KEY(subgroup_id, user_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_table_assignments (
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            group_id TEXT NOT NULL REFERENCES lab_groups(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            table_number INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(practice_id, group_id, user_id)
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

    add_column_if_missing(pool, "submissions", "submitted_by_user_id", "TEXT").await?;
    add_column_if_missing(pool, "submissions", "course_id", "TEXT").await?;
    add_column_if_missing(pool, "submissions", "group_id", "TEXT").await?;
    add_column_if_missing(pool, "users", "email", "TEXT").await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS grade_components (
            id TEXT PRIMARY KEY,
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('pregunta', 'informe', 'parcial')),
            name TEXT NOT NULL,
            max_points REAL NOT NULL,
            weight_points REAL NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS grade_scores (
            component_id TEXT NOT NULL REFERENCES grade_components(id) ON DELETE CASCADE,
            student_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            raw_points REAL NOT NULL,
            comment TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(component_id, student_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE users
        SET email = CASE username
            WHEN 'admin' THEN 'admin@quantify.local'
            WHEN 'docente' THEN 'docente@quantify.local'
            WHEN 'estudiante' THEN 'estudiante@quantify.local'
            ELSE username
        END
        WHERE email IS NULL OR email = ''
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_unique ON users(email)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    let pragma = format!("PRAGMA table_info({table})");
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&pragma).fetch_all(pool).await?;
    if rows.iter().any(|(_, name, _, _, _, _)| name == column) {
        return Ok(());
    }

    let alter = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    sqlx::query(&alter).execute(pool).await?;
    Ok(())
}

pub async fn seed_users(pool: &SqlitePool) -> anyhow::Result<()> {
    let users = [
        (
            "admin@quantify.local",
            "Administrador",
            "admin",
            env::var("SEED_ADMIN_PASSWORD").unwrap_or_else(|_| "admin123".into()),
        ),
        (
            "docente@quantify.local",
            "Docente de prueba",
            "docente",
            env::var("SEED_TEACHER_PASSWORD").unwrap_or_else(|_| "docente123".into()),
        ),
        (
            "estudiante@quantify.local",
            "Estudiante de prueba",
            "estudiante",
            env::var("SEED_STUDENT_PASSWORD").unwrap_or_else(|_| "estudiante123".into()),
        ),
    ];

    for (email, display_name, role, password) in users {
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?1")
            .bind(email)
            .fetch_optional(pool)
            .await?;

        if exists.is_none() {
            sqlx::query(
                r#"
                INSERT INTO users (id, username, email, display_name, role, password_hash, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(email)
            .bind(email)
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
            email: user.email,
            display_name: user.display_name,
            role: user.role,
        },
    )))
}

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

pub async fn logout(pool: &SqlitePool, token: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn users(pool: &SqlitePool) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users ORDER BY role, display_name",
    )
    .fetch_all(pool)
    .await?)
}

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

pub async fn change_password(
    pool: &SqlitePool,
    user_id: &str,
    input: ChangePassword,
) -> anyhow::Result<bool> {
    let user = sqlx::query_as::<_, UserWithPassword>(
        r#"
        SELECT id, username, email, display_name, role, password_hash
        FROM users
        WHERE id = ?1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some(user) = user else {
        return Ok(false);
    };

    if !verify_password(&input.current_password, &user.password_hash) {
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

async fn ensure_lab_group_columns(pool: &SqlitePool) -> anyhow::Result<()> {
    let rows = sqlx::query("PRAGMA table_info(lab_groups)")
        .fetch_all(pool)
        .await?;
    let columns: Vec<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    if !columns.iter().any(|column| column == "table_count") {
        sqlx::query("ALTER TABLE lab_groups ADD COLUMN table_count INTEGER NOT NULL DEFAULT 4")
            .execute(pool)
            .await?;
    }

    if !columns.iter().any(|column| column == "group_type") {
        sqlx::query("ALTER TABLE lab_groups ADD COLUMN group_type TEXT NOT NULL DEFAULT 'regular'")
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn seed_academic(pool: &SqlitePool) -> anyhow::Result<()> {
    let course_id = "fisica-experimental-i-2026";
    let group_id = "fisica-exp-i-grupo-1";

    sqlx::query(
        r#"
        INSERT INTO courses (id, name, term, active, created_at)
        VALUES (?1, 'Fisica Experimental I', '2026', 1, ?2)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(course_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO lab_groups (id, course_id, name, table_count, group_type, created_at)
        VALUES (?1, ?2, 'Grupo 1', 4, 'regular', ?3)
        ON CONFLICT(course_id, name) DO NOTHING
        "#,
    )
    .bind(group_id)
    .bind(course_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    if let Some((student_id,)) =
        sqlx::query_as::<_, (String,)>("SELECT id FROM users WHERE username = 'estudiante'")
            .fetch_optional(pool)
            .await?
            .or(sqlx::query_as::<_, (String,)>(
                "SELECT id FROM users WHERE email = 'estudiante@quantify.local'",
            )
            .fetch_optional(pool)
            .await?)
    {
        enroll_course_member(pool, course_id, &student_id).await?;

        sqlx::query(
            r#"
            INSERT INTO group_members (group_id, user_id, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(group_id, user_id) DO NOTHING
            "#,
        )
        .bind(group_id)
        .bind(student_id)
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    for practice in ["pendulo", "hooke", "caida-libre"] {
        sqlx::query(
            r#"
            INSERT INTO course_practices (course_id, practice_id, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(course_id, practice_id) DO NOTHING
            "#,
        )
        .bind(course_id)
        .bind(practice)
        .bind(Utc::now())
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

pub async fn students(pool: &SqlitePool) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users WHERE role = 'estudiante' ORDER BY display_name",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn academic_context(
    pool: &SqlitePool,
    user: &AuthUser,
) -> anyhow::Result<AcademicContext> {
    let courses = if matches!(user.role.as_str(), "docente" | "admin") {
        all_course_summaries(pool).await?
    } else {
        student_course_summaries(pool, &user.id).await?
    };

    Ok(AcademicContext {
        courses,
        practices: practices(pool).await?,
        students: if matches!(user.role.as_str(), "docente" | "admin") {
            students(pool).await?
        } else {
            Vec::new()
        },
        users: if matches!(user.role.as_str(), "docente" | "admin") {
            users(pool).await?
        } else {
            Vec::new()
        },
    })
}

pub async fn create_course(pool: &SqlitePool, input: CreateCourse) -> anyhow::Result<Course> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO courses (id, name, term, active, created_at)
        VALUES (?1, ?2, ?3, 1, ?4)
        "#,
    )
    .bind(&id)
    .bind(input.name.trim())
    .bind(input.term.trim())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(
        sqlx::query_as::<_, Course>("SELECT id, name, term, active FROM courses WHERE id = ?1")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn update_course(
    pool: &SqlitePool,
    course_id: &str,
    input: UpdateCourse,
) -> anyhow::Result<Option<Course>> {
    let result = sqlx::query(
        r#"
        UPDATE courses
        SET name = ?2,
            term = ?3
        WHERE id = ?1
        "#,
    )
    .bind(course_id)
    .bind(input.name.trim())
    .bind(input.term.trim())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(
        sqlx::query_as::<_, Course>("SELECT id, name, term, active FROM courses WHERE id = ?1")
            .bind(course_id)
            .fetch_one(pool)
            .await?,
    ))
}

pub async fn create_group(
    pool: &SqlitePool,
    course_id: &str,
    input: CreateGroup,
) -> anyhow::Result<LabGroup> {
    let id = Uuid::new_v4().to_string();
    let table_count = input.table_count.unwrap_or(4).clamp(1, 24);
    let group_type = normalize_group_type(input.group_type.as_deref());
    sqlx::query(
        r#"
        INSERT INTO lab_groups (id, course_id, name, table_count, group_type, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(course_id)
    .bind(input.name.trim())
    .bind(table_count)
    .bind(group_type)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as::<_, LabGroup>(
        "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

fn normalize_group_type(value: Option<&str>) -> &'static str {
    match value.unwrap_or("regular").trim() {
        "recuperacion" => "recuperacion",
        _ => "regular",
    }
}

pub async fn update_group(
    pool: &SqlitePool,
    group_id: &str,
    input: UpdateGroup,
) -> anyhow::Result<Option<LabGroup>> {
    let course: Option<(String,)> =
        sqlx::query_as("SELECT course_id FROM lab_groups WHERE id = ?1")
            .bind(group_id)
            .fetch_optional(pool)
            .await?;

    let Some((course_id,)) = course else {
        return Ok(None);
    };

    let result = sqlx::query(
        r#"
        UPDATE lab_groups
        SET name = ?2,
            table_count = ?3,
            group_type = ?4
        WHERE id = ?1
        "#,
    )
    .bind(group_id)
    .bind(input.name.trim())
    .bind(input.table_count.clamp(1, 24))
    .bind(normalize_group_type(Some(&input.group_type)))
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(
        sqlx::query_as::<_, LabGroup>(
            "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1 AND course_id = ?2",
        )
        .bind(group_id)
        .bind(course_id)
        .fetch_one(pool)
        .await?,
    ))
}

pub async fn create_subgroup(
    pool: &SqlitePool,
    course_id: &str,
    input: CreateSubgroup,
) -> anyhow::Result<PracticeSubgroup> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO practice_subgroups (id, course_id, practice_id, group_id, name, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(course_id)
    .bind(input.practice_id)
    .bind(input.group_id)
    .bind(input.name.trim())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as::<_, PracticeSubgroup>(
        r#"
        SELECT id, course_id, practice_id, group_id, name
        FROM practice_subgroups
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

pub async fn enroll_course_member(
    pool: &SqlitePool,
    course_id: &str,
    user_id: &str,
) -> anyhow::Result<()> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT user_id FROM course_members WHERE course_id = ?1 AND user_id = ?2")
            .bind(course_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    if existing.is_none() {
        sqlx::query(
            r#"
            INSERT INTO course_members (course_id, user_id, created_at)
            VALUES (?1, ?2, ?3)
            "#,
        )
        .bind(course_id)
        .bind(user_id)
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    let default_group_id = ensure_default_group(pool, course_id).await?;
    sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(group_id, user_id) DO NOTHING
        "#,
    )
    .bind(default_group_id)
    .bind(user_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}

async fn ensure_default_group(pool: &SqlitePool, course_id: &str) -> anyhow::Result<String> {
    let existing = sqlx::query_as::<_, LabGroup>(
        r#"
        SELECT id, course_id, name, table_count, group_type
        FROM lab_groups
        WHERE course_id = ?1 AND name = 'General'
        LIMIT 1
        "#,
    )
    .bind(course_id)
    .fetch_optional(pool)
    .await?;

    if let Some(group) = existing {
        return Ok(group.id);
    }

    let created = create_group(
        pool,
        course_id,
        CreateGroup {
            name: "General".into(),
            table_count: Some(4),
            group_type: Some("regular".into()),
        },
    )
    .await?;
    Ok(created.id)
}

pub async fn add_group_member(
    pool: &SqlitePool,
    group_id: &str,
    input: AddGroupMember,
) -> anyhow::Result<Option<()>> {
    let user = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE id = ?1 AND role = 'estudiante'",
    )
    .bind(input.user_id)
    .fetch_optional(pool)
    .await?;

    let Some((user_id,)) = user else {
        return Ok(None);
    };

    sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(group_id, user_id) DO NOTHING
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(Some(()))
}

pub async fn remove_group_member(
    pool: &SqlitePool,
    group_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM group_members
        WHERE group_id = ?1 AND user_id = ?2
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn set_practice_table_assignment(
    pool: &SqlitePool,
    group_id: &str,
    user_id: &str,
    input: SetPracticeTable,
) -> anyhow::Result<Option<PracticeTableAssignment>> {
    let group = sqlx::query_as::<_, LabGroup>(
        "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;

    let Some(group) = group else {
        return Ok(None);
    };

    if input.table_number < 1 || input.table_number > group.table_count {
        return Ok(None);
    }

    let allowed: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT 1
        FROM group_members gm
        JOIN course_practices cp ON cp.course_id = ?3 AND cp.practice_id = ?4
        WHERE gm.group_id = ?1 AND gm.user_id = ?2
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .bind(&group.course_id)
    .bind(&input.practice_id)
    .fetch_optional(pool)
    .await?;

    if allowed.is_none() {
        return Ok(None);
    }

    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO practice_table_assignments (
            course_id, practice_id, group_id, user_id, table_number, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
        ON CONFLICT(practice_id, group_id, user_id) DO UPDATE SET
            table_number = excluded.table_number,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&group.course_id)
    .bind(&input.practice_id)
    .bind(group_id)
    .bind(user_id)
    .bind(input.table_number)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(Some(
        sqlx::query_as::<_, PracticeTableAssignment>(
            r#"
            SELECT course_id, practice_id, group_id, user_id, table_number
            FROM practice_table_assignments
            WHERE practice_id = ?1 AND group_id = ?2 AND user_id = ?3
            "#,
        )
        .bind(input.practice_id)
        .bind(group_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?,
    ))
}

pub async fn add_course_member(
    pool: &SqlitePool,
    course_id: &str,
    input: EnrollCourseMember,
) -> anyhow::Result<Option<()>> {
    let user = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE id = ?1 AND role = 'estudiante'",
    )
    .bind(input.user_id)
    .fetch_optional(pool)
    .await?;

    let Some((user_id,)) = user else {
        return Ok(None);
    };

    enroll_course_member(pool, course_id, &user_id).await?;
    Ok(Some(()))
}

pub async fn enable_course_practice(
    pool: &SqlitePool,
    course_id: &str,
    input: SetCoursePractice,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO course_practices (course_id, practice_id, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(course_id, practice_id) DO NOTHING
        "#,
    )
    .bind(course_id)
    .bind(input.practice_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_can_submit(
    pool: &SqlitePool,
    user: &AuthUser,
    course_id: &str,
    group_id: &str,
    practice_id: &str,
) -> anyhow::Result<bool> {
    if matches!(user.role.as_str(), "docente" | "admin") {
        return Ok(true);
    }

    let allowed: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT 1
        FROM course_members cm
        JOIN lab_groups g ON g.course_id = cm.course_id
        JOIN course_practices cp ON cp.course_id = g.course_id
        JOIN group_members gm ON gm.group_id = g.id
        WHERE cm.user_id = ?1
          AND g.id = ?2
          AND g.course_id = ?3
          AND gm.user_id = ?1
          AND cp.practice_id = ?4
        "#,
    )
    .bind(&user.id)
    .bind(group_id)
    .bind(course_id)
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;

    Ok(allowed.is_some())
}

pub async fn create_grade_component(
    pool: &SqlitePool,
    input: CreateGradeComponent,
) -> anyhow::Result<GradeComponent> {
    let id = Uuid::new_v4().to_string();
    let position: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM grade_components WHERE course_id = ?1",
    )
    .bind(&input.course_id)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO grade_components (
            id, course_id, kind, name, max_points, weight_points, position, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&id)
    .bind(input.course_id)
    .bind(input.kind)
    .bind(input.name.trim())
    .bind(input.max_points)
    .bind(input.weight_points)
    .bind(position.0)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as::<_, GradeComponent>(
        r#"
        SELECT id, course_id, kind, name, max_points, weight_points, position
        FROM grade_components
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

pub async fn upsert_grade_score(pool: &SqlitePool, input: UpsertGradeScore) -> anyhow::Result<()> {
    let component: Option<(f64,)> =
        sqlx::query_as("SELECT max_points FROM grade_components WHERE id = ?1")
            .bind(&input.component_id)
            .fetch_optional(pool)
            .await?;

    let Some((max_points,)) = component else {
        return Err(anyhow::anyhow!("grade component not found"));
    };
    if input.raw_points > max_points {
        return Err(anyhow::anyhow!("raw points exceed component maximum"));
    }

    sqlx::query(
        r#"
        INSERT INTO grade_scores (component_id, student_id, raw_points, comment, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(component_id, student_id) DO UPDATE SET
            raw_points = excluded.raw_points,
            comment = excluded.comment,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(input.component_id)
    .bind(input.student_id)
    .bind(input.raw_points)
    .bind(input.comment)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn gradebook_for_user(
    pool: &SqlitePool,
    user: &AuthUser,
) -> anyhow::Result<Vec<CourseGradebook>> {
    let courses = if matches!(user.role.as_str(), "docente" | "admin") {
        sqlx::query_as::<_, Course>(
            "SELECT id, name, term, active FROM courses ORDER BY term DESC, name",
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Course>(
            r#"
            SELECT DISTINCT c.id, c.name, c.term, c.active
            FROM courses c
            JOIN lab_groups g ON g.course_id = c.id
            JOIN group_members gm ON gm.group_id = g.id
            WHERE gm.user_id = ?1 AND c.active = 1
            ORDER BY c.term DESC, c.name
            "#,
        )
        .bind(&user.id)
        .fetch_all(pool)
        .await?
    };

    let mut gradebooks = Vec::with_capacity(courses.len());
    for course in courses {
        let components = grade_components(pool, &course.id).await?;
        let students = if matches!(user.role.as_str(), "docente" | "admin") {
            students_for_course(pool, &course.id).await?
        } else {
            vec![user.clone()]
        };

        let mut summaries = Vec::with_capacity(students.len());
        for student in students {
            summaries.push(student_grade_summary(pool, student, &components).await?);
        }

        gradebooks.push(CourseGradebook {
            course,
            components,
            students: summaries,
        });
    }

    Ok(gradebooks)
}

async fn grade_components(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<GradeComponent>> {
    Ok(sqlx::query_as::<_, GradeComponent>(
        r#"
        SELECT id, course_id, kind, name, max_points, weight_points, position
        FROM grade_components
        WHERE course_id = ?1
        ORDER BY position, name
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?)
}

async fn students_for_course(pool: &SqlitePool, course_id: &str) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        r#"
        SELECT DISTINCT u.id, u.username, u.email, u.display_name, u.role
        FROM users u
        JOIN course_members cm ON cm.user_id = u.id
        WHERE cm.course_id = ?1 AND u.role = 'estudiante'
        ORDER BY u.display_name
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?)
}

async fn student_grade_summary(
    pool: &SqlitePool,
    student: AuthUser,
    components: &[GradeComponent],
) -> anyhow::Result<StudentGradeSummary> {
    let scores = sqlx::query_as::<_, GradeScore>(
        r#"
        SELECT component_id, student_id, raw_points, comment
        FROM grade_scores
        WHERE student_id = ?1
        "#,
    )
    .bind(&student.id)
    .fetch_all(pool)
    .await?;

    let mut details = Vec::with_capacity(components.len());
    for component in components {
        let score = scores
            .iter()
            .find(|score| score.component_id == component.id);
        let raw_points = score.map(|score| score.raw_points);
        let normalized_points = raw_points
            .map(|points| (points / component.max_points) * component.weight_points)
            .unwrap_or(0.0);

        details.push(GradeScoreDetail {
            component_id: component.id.clone(),
            kind: component.kind.clone(),
            name: component.name.clone(),
            max_points: component.max_points,
            weight_points: component.weight_points,
            raw_points,
            normalized_points: clean_zero(normalized_points),
            comment: score.and_then(|score| score.comment.clone()),
        });
    }

    let mut totals_by_kind = Vec::new();
    for kind in ["pregunta", "informe", "parcial"] {
        let points = details
            .iter()
            .filter(|detail| detail.kind == kind)
            .map(|detail| detail.normalized_points)
            .sum();
        let possible = components
            .iter()
            .filter(|component| component.kind == kind)
            .map(|component| component.weight_points)
            .sum();
        totals_by_kind.push(GradeKindTotal {
            kind: kind.to_string(),
            points: clean_zero(points),
            possible: clean_zero(possible),
        });
    }

    let total_points = clean_zero(details.iter().map(|detail| detail.normalized_points).sum());
    let total_possible = components
        .iter()
        .map(|component| component.weight_points)
        .sum();

    Ok(StudentGradeSummary {
        student,
        scores: details,
        totals_by_kind,
        total_points,
        total_possible: clean_zero(total_possible),
    })
}

async fn all_course_summaries(pool: &SqlitePool) -> anyhow::Result<Vec<CourseSummary>> {
    let courses = sqlx::query_as::<_, Course>(
        "SELECT id, name, term, active FROM courses ORDER BY term DESC, name",
    )
    .fetch_all(pool)
    .await?;
    course_summaries(pool, courses).await
}

async fn student_course_summaries(
    pool: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<CourseSummary>> {
    let courses = sqlx::query_as::<_, Course>(
        r#"
        SELECT DISTINCT c.id, c.name, c.term, c.active
        FROM courses c
        JOIN course_members cm ON cm.course_id = c.id
        WHERE cm.user_id = ?1 AND c.active = 1
        ORDER BY c.term DESC, c.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    course_summaries(pool, courses).await
}

async fn course_summaries(
    pool: &SqlitePool,
    courses: Vec<Course>,
) -> anyhow::Result<Vec<CourseSummary>> {
    let mut summaries = Vec::with_capacity(courses.len());
    for course in courses {
        let members = course_members_for_course(pool, &course.id).await?;
        let groups = groups_for_course(pool, &course.id).await?;
        let practices = practices_for_course(pool, &course.id).await?;
        let subgroups = subgroups_for_course(pool, &course.id).await?;
        let table_assignments = table_assignments_for_course(pool, &course.id).await?;
        summaries.push(CourseSummary {
            id: course.id,
            name: course.name,
            term: course.term,
            active: course.active,
            members,
            groups,
            practices,
            subgroups,
            table_assignments,
        });
    }
    Ok(summaries)
}

async fn course_members_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        r#"
        SELECT u.id, u.username, u.email, u.display_name, u.role
        FROM course_members cm
        JOIN users u ON u.id = cm.user_id
        WHERE cm.course_id = ?1 AND u.role = 'estudiante'
        ORDER BY u.display_name
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?)
}

async fn groups_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<GroupSummary>> {
    let groups = sqlx::query_as::<_, LabGroup>(
        "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE course_id = ?1 ORDER BY name",
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?;

    let mut summaries = Vec::with_capacity(groups.len());
    for group in groups {
        let members = sqlx::query_as::<_, AuthUser>(
            r#"
            SELECT u.id, u.username, u.email, u.display_name, u.role
            FROM group_members gm
            JOIN users u ON u.id = gm.user_id
            WHERE gm.group_id = ?1
            ORDER BY u.display_name
            "#,
        )
        .bind(&group.id)
        .fetch_all(pool)
        .await?;

        summaries.push(GroupSummary {
            id: group.id,
            name: group.name,
            table_count: group.table_count,
            group_type: group.group_type,
            members,
        });
    }

    Ok(summaries)
}

async fn subgroups_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<SubgroupSummary>> {
    let subgroups = sqlx::query_as::<_, PracticeSubgroup>(
        r#"
        SELECT id, course_id, practice_id, group_id, name
        FROM practice_subgroups
        WHERE course_id = ?1
        ORDER BY practice_id, name
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?;

    let mut summaries = Vec::with_capacity(subgroups.len());
    for subgroup in subgroups {
        let practice = sqlx::query_as::<_, Practice>(
            "SELECT id, name, description FROM practices WHERE id = ?1",
        )
        .bind(&subgroup.practice_id)
        .fetch_one(pool)
        .await?;

        let group_members = sqlx::query_as::<_, AuthUser>(
            r#"
            SELECT u.id, u.username, u.email, u.display_name, u.role
            FROM group_members gm
            JOIN users u ON u.id = gm.user_id
            WHERE gm.group_id = ?1
            ORDER BY u.display_name
            "#,
        )
        .bind(&subgroup.group_id)
        .fetch_all(pool)
        .await?;

        let group = sqlx::query_as::<_, LabGroup>(
            "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1",
        )
        .bind(&subgroup.group_id)
        .fetch_one(pool)
        .await?;

        let members = sqlx::query_as::<_, AuthUser>(
            r#"
            SELECT u.id, u.username, u.email, u.display_name, u.role
            FROM practice_subgroup_members sm
            JOIN users u ON u.id = sm.user_id
            WHERE sm.subgroup_id = ?1
            ORDER BY u.display_name
            "#,
        )
        .bind(&subgroup.id)
        .fetch_all(pool)
        .await?;

        summaries.push(SubgroupSummary {
            id: subgroup.id,
            name: subgroup.name,
            practice,
            group: GroupSummary {
                id: group.id,
                name: group.name,
                table_count: group.table_count,
                group_type: group.group_type,
                members: group_members,
            },
            members,
        });
    }

    Ok(summaries)
}

async fn table_assignments_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<PracticeTableAssignment>> {
    Ok(sqlx::query_as::<_, PracticeTableAssignment>(
        r#"
        SELECT course_id, practice_id, group_id, user_id, table_number
        FROM practice_table_assignments
        WHERE course_id = ?1
        ORDER BY practice_id, group_id, table_number
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?)
}

async fn practices_for_course(pool: &SqlitePool, course_id: &str) -> anyhow::Result<Vec<Practice>> {
    Ok(sqlx::query_as::<_, Practice>(
        r#"
        SELECT p.id, p.name, p.description
        FROM course_practices cp
        JOIN practices p ON p.id = cp.practice_id
        WHERE cp.course_id = ?1
        ORDER BY p.name
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?)
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
            csv_path, analysis_json, status, submitted_at,
            submitted_by_user_id, course_id, group_id
        )
        SELECT ?1, u.display_name, g.name, c.name, ?5, ?6, ?7, ?8,
            'pendiente', ?9, ?10, ?11, ?12
        FROM users u, lab_groups g, courses c
        WHERE u.id = ?10 AND g.id = ?12 AND c.id = ?11
        "#,
    )
    .bind(&id)
    .bind(&submission.submitted_by_user_id)
    .bind(&submission.group_id)
    .bind(&submission.course_id)
    .bind(&submission.practice_id)
    .bind(&submission.file_name)
    .bind(csv_path.to_string_lossy().to_string())
    .bind(analysis_json)
    .bind(submitted_at)
    .bind(&submission.submitted_by_user_id)
    .bind(&submission.course_id)
    .bind(&submission.group_id)
    .execute(pool)
    .await?;

    submission_detail(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created submission not found"))
}

pub async fn submission_list_for_user(
    pool: &SqlitePool,
    user: &AuthUser,
) -> anyhow::Result<Vec<SubmissionListItem>> {
    let mut query = String::from(
        r#"
        SELECT
            s.id,
            s.submitted_by_user_id,
            s.group_id,
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
        "#,
    );

    if !matches!(user.role.as_str(), "docente" | "admin") {
        query.push_str(" WHERE s.submitted_by_user_id = ?1");
    }

    query.push_str(" ORDER BY s.course, s.group_name, s.submitted_at DESC");

    let rows = if matches!(user.role.as_str(), "docente" | "admin") {
        sqlx::query_as::<_, SubmissionListItem>(&query)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, SubmissionListItem>(&query)
            .bind(&user.id)
            .fetch_all(pool)
            .await?
    };
    Ok(rows)
}

pub async fn submission_owner_id(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<String>> {
    let owner: Option<(Option<String>,)> =
        sqlx::query_as("SELECT submitted_by_user_id FROM submissions WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(owner.and_then(|(user_id,)| user_id))
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

fn clean_zero(value: f64) -> f64 {
    if value.abs() < 1e-9 {
        0.0
    } else {
        value
    }
}
