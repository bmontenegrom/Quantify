use crate::analysis::AnalysisResult;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Duration, Utc};
use rand_core::OsRng;
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
    /// Tipo de análisis: `estadistico`, `regresion_lineal` o `relajacion_exponencial`.
    pub analysis_kind: Option<String>,
}

/// Magnitud medida directamente dentro de una práctica (variable de entrada).
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PracticeQuantity {
    pub id: String,
    pub practice_id: String,
    /// Símbolo corto usado en fórmulas: `l`, `a`, `T`, `V`, `i`.
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// `true` si admite varias réplicas (tipo A); `false` para medida única.
    pub repeated: bool,
    /// Magnitud física (para sugerir instrumentos compatibles).
    pub quantity: Option<String>,
    pub position: i64,
    /// `true` si es un dato dado (valor ± U entregado por la cátedra), no medido por el alumno.
    pub is_given: bool,
}

/// Mensurando derivado de una práctica (determinación indirecta).
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PracticeResult {
    pub id: String,
    pub practice_id: String,
    /// Símbolo del mensurando: `Q`, `g`, `tau`.
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// Expresión matemática en función de los símbolos de `practice_quantities`.
    pub formula: String,
    pub position: i64,
}

/// Instrumento de medida del catálogo de un curso. El `kind` (`analogico`/`digital`) es
/// la clasificación física; el modelo de incertidumbre concreto vive en cada escala.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Instrument {
    pub id: String,
    pub course_id: String,
    pub name: String,
    pub kind: String,
    /// Magnitud que mide (longitud, masa, tiempo, voltaje, corriente...).
    pub quantity: String,
    /// Unidad base del instrumento (m, kg, s, V, A...).
    pub unit: String,
}

/// Escala de un instrumento, con los datos necesarios para calcular la incertidumbre tipo B.
/// `b_model` selecciona la fórmula: `resolucion` (`step`), `apreciacion` (`appreciation`) o
/// `fabricante` (`spec_pct_reading`/`spec_step_coeff`/`spec_fixed`). Ver [`crate::uncertainty`].
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct InstrumentScale {
    pub id: String,
    pub instrument_id: String,
    pub label: String,
    /// Fondo de escala (valor máximo); opcional.
    pub full_scale: Option<f64>,
    /// Resolución (digital), menor división (analógico) o VOLTS/DIV (osciloscopio).
    pub step: f64,
    /// Apreciación efectiva del operador (analógico); por defecto se usa `step`.
    pub appreciation: Option<f64>,
    /// Resistencia interna de la escala (P2; ohm), opcional.
    pub internal_res: Option<f64>,
    /// Incertidumbre de la resistencia interna (p. ej. ±10 en 1002±10), opcional.
    pub internal_res_u: Option<f64>,
    /// Modelo de incertidumbre tipo B: `resolucion`, `apreciacion` o `fabricante`.
    pub b_model: String,
    /// Fabricante: porcentaje del valor leído (p. ej. 3.0 = 3 %).
    pub spec_pct_reading: Option<f64>,
    /// Fabricante: coeficiente que multiplica `step` (5 = "5 dgt"; 0.1 osciloscopio).
    pub spec_step_coeff: Option<f64>,
    /// Fabricante: término fijo en unidad base (p. ej. 0.001 V = 1 mV).
    pub spec_fixed: Option<f64>,
    pub unit: String,
    pub position: i64,
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

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: MeUser,
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

#[derive(Debug, Serialize, FromRow)]
pub struct Course {
    pub id: String,
    pub name: String,
    pub term: String,
    pub active: bool,
    pub submission_edit_hours: f64,
    pub acceptance_window_hours: f64,
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
    pub submission_edit_hours: f64,
    pub acceptance_window_hours: f64,
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
    /// Horas de edición de entregas (opcional; si viene, se acota a 0..=72).
    #[serde(default)]
    pub submission_edit_hours: Option<f64>,
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

/// Miembro de un informe compartido por mesa (owner o miembro aceptado/pendiente).
#[derive(Debug, Serialize)]
pub struct ReportMember {
    pub user_id: String,
    pub display_name: String,
    /// `owner` o `member`.
    pub role: String,
    /// `pending`, `accepted` o `expired`.
    pub status: String,
    pub accepted_at: Option<DateTime<Utc>>,
}

/// Invitación pendiente de aceptar para un alumno (informe creado por otro de la misma mesa).
#[derive(Debug, Serialize)]
pub struct PendingInvitation {
    pub submission_id: String,
    pub practice_name: String,
    pub course: String,
    pub group_name: String,
    pub table_number: Option<i64>,
    pub owner_name: String,
    pub invited_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Resultado de intentar aceptar una invitación.
#[derive(Debug)]
pub enum AcceptOutcome {
    Accepted,
    NotInvited,
    Expired,
    AlreadyAccepted,
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
    pub entry_mode: String,
    pub table_number: Option<i64>,
    pub member_count: i64,
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
    pub entry_mode: String,
    pub results_visible_to_student: bool,
    pub status: String,
    pub teacher_comment: Option<String>,
    pub score: Option<f64>,
    pub submitted_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub measurement_meta_json: Option<String>,
    pub course_id: Option<String>,
    pub submission_edit_hours: f64,
    pub table_number: Option<i64>,
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
    /// Modo de carga: `"csv"` (legacy) o `"form"` (lecturas crudas con cálculo de incertidumbres).
    pub entry_mode: String,
    /// Si el docente habilitó que el estudiante vea el cálculo automático de esta entrega.
    pub results_visible_to_student: bool,
    /// Resultado calculado, crudo como JSON: para `csv` es un `AnalysisResult`; para `form`
    /// es un `computation::FormAnalysis`. El cliente decide cómo renderizarlo según `entry_mode`.
    /// Se devuelve `null` a un estudiante mientras `results_visible_to_student` sea `false`.
    pub analysis: serde_json::Value,
    /// Mensurandos finales calculados por el estudiante (a mano), para comparar con el cálculo
    /// automático. No se gatea: el estudiante ve los suyos siempre y el docente también.
    pub student_results: Vec<StudentResult>,
    pub status: String,
    pub teacher_comment: Option<String>,
    pub score: Option<f64>,
    pub submitted_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    /// Metadatos de depuración por magnitud (bins + valores descartados), si la entrega los trae.
    pub measurement_meta: Option<serde_json::Value>,
    /// Instante hasta el que el alumno puede editar (submitted_at + horas del curso).
    pub editable_until: Option<DateTime<Utc>>,
    /// True si la ventana sigue abierta y la entrega es editable (estado pendiente, no visible).
    /// El endpoint de edición igual valida la propiedad del alumno.
    pub can_edit: bool,
    /// Lecturas crudas (sólo entregas por formulario), para prefillear el form al editar.
    pub measurements: Vec<SubmissionMeasurement>,
    /// Número de mesa del informe compartido (NULL en entregas legacy/CSV sin mesa asignada).
    pub table_number: Option<i64>,
    /// Miembros del informe (owner + miembros aceptados/pendientes).
    pub members: Vec<ReportMember>,
}

/// Un mensurando final calculado por el estudiante (valor ± U), por símbolo.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StudentResult {
    pub symbol: String,
    pub value: f64,
    pub u_expanded: Option<f64>,
}

/// Una lectura cruda persistida de una entrega por formulario (para prefill al editar).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SubmissionMeasurement {
    pub quantity_id: String,
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
    pub replicate_index: i64,
    pub value: f64,
    pub value_u: Option<f64>,
}

/// Una fila del cuerpo para guardar los cálculos del estudiante.
#[derive(Debug, Deserialize)]
pub struct StudentResultInput {
    pub symbol: String,
    pub value: f64,
    pub u_expanded: Option<f64>,
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
    /// Si se incluye, actualiza la visibilidad del cálculo para el estudiante.
    pub results_visible: Option<bool>,
}

/// Crea las tablas si no existen y aplica las migraciones idempotentes de columnas.
/// Es seguro ejecutarla en cada arranque: usa `CREATE TABLE IF NOT EXISTS` y
/// `add_column_if_missing`, por lo que no destruye datos existentes.
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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS submission_measurements (
            id              TEXT PRIMARY KEY,
            submission_id   TEXT NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
            quantity_id     TEXT NOT NULL REFERENCES practice_quantities(id),
            instrument_id   TEXT REFERENCES instruments(id),
            scale_id        TEXT REFERENCES instrument_scales(id),
            replicate_index INTEGER NOT NULL DEFAULT 0,
            value           REAL NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Mensurandos finales calculados por el estudiante (a mano), para comparar con el cálculo
    // automático. Uno por símbolo de mensurando; `u_expanded` opcional (puede no calcular U).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS submission_student_results (
            id            TEXT PRIMARY KEY,
            submission_id TEXT NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
            symbol        TEXT NOT NULL,
            value         REAL NOT NULL,
            u_expanded    REAL,
            created_at    TEXT NOT NULL,
            UNIQUE(submission_id, symbol)
        )
        "#,
    )
    .execute(pool)
    .await?;

    add_column_if_missing(pool, "submissions", "submitted_by_user_id", "TEXT").await?;
    add_column_if_missing(pool, "submissions", "course_id", "TEXT").await?;
    add_column_if_missing(pool, "submissions", "group_id", "TEXT").await?;
    // Modo de carga de la entrega: 'csv' (legacy) o 'form' (lecturas crudas). NULL = csv.
    add_column_if_missing(pool, "submissions", "entry_mode", "TEXT").await?;
    // Visibilidad del calculo automatico para el estudiante (la habilita el docente). 0 = oculto.
    add_column_if_missing(
        pool,
        "submissions",
        "results_visible_to_student",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    // Metadatos de depuración por magnitud (JSON): nº de bins del histograma y valores
    // descartados por el alumno. Visible para el docente. NULL en entregas sin depuración.
    add_column_if_missing(pool, "submissions", "measurement_meta_json", "TEXT").await?;
    // Horas durante las que el alumno puede editar su entrega (desde submitted_at). Default 4h.
    add_column_if_missing(
        pool,
        "courses",
        "submission_edit_hours",
        "REAL NOT NULL DEFAULT 4",
    )
    .await?;
    add_column_if_missing(pool, "users", "email", "TEXT").await?;
    add_column_if_missing(pool, "users", "default_group_id", "TEXT").await?;
    add_column_if_missing(pool, "practices", "analysis_kind", "TEXT").await?;
    // Fórmulas de eje (x, y) por punto, solo para prácticas `regresion_lineal`.
    add_column_if_missing(pool, "practices", "x_formula", "TEXT").await?;
    add_column_if_missing(pool, "practices", "y_formula", "TEXT").await?;

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
        CREATE TABLE IF NOT EXISTS instruments (
            id TEXT PRIMARY KEY,
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('analogico', 'digital')),
            quantity TEXT NOT NULL,
            unit TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS instrument_scales (
            id TEXT PRIMARY KEY,
            instrument_id TEXT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
            label TEXT NOT NULL,
            full_scale REAL,
            step REAL NOT NULL,
            appreciation REAL,
            internal_res REAL,
            internal_res_u REAL,
            b_model TEXT NOT NULL DEFAULT 'resolucion'
                CHECK(b_model IN ('resolucion', 'apreciacion', 'fabricante')),
            spec_pct_reading REAL,
            spec_step_coeff REAL,
            spec_fixed REAL,
            unit TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_quantities (
            id          TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            symbol      TEXT NOT NULL,
            name        TEXT NOT NULL,
            unit        TEXT NOT NULL,
            repeated    INTEGER NOT NULL DEFAULT 1,
            quantity    TEXT,
            position    INTEGER NOT NULL DEFAULT 0,
            UNIQUE(practice_id, symbol)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_results (
            id          TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            symbol      TEXT NOT NULL,
            name        TEXT NOT NULL,
            unit        TEXT NOT NULL,
            formula     TEXT NOT NULL,
            position    INTEGER NOT NULL DEFAULT 0,
            UNIQUE(practice_id, symbol)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Magnitud dada por la cátedra (valor ± U directo, sin instrumento ni réplicas).
    add_column_if_missing(
        pool,
        "practice_quantities",
        "is_given",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    // Incertidumbre expandida U del dato aportado por el alumno.
    add_column_if_missing(pool, "submission_measurements", "value_u", "REAL").await?;

    // Número de mesa del informe compartido (NULL en entregas legacy/CSV).
    add_column_if_missing(pool, "submissions", "table_number", "INTEGER").await?;
    // Ventana de aceptación de invitaciones (horas). Default 4. Acotada a 0..=72.
    add_column_if_missing(
        pool,
        "courses",
        "acceptance_window_hours",
        "REAL NOT NULL DEFAULT 4",
    )
    .await?;

    // Membresía de un informe compartido por mesa. Owner: role='owner', status='accepted'.
    // Los demás miembros de la mesa reciben una invitación (status='pending') al crear el informe.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS report_members (
            submission_id TEXT NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
            user_id       TEXT NOT NULL REFERENCES users(id)       ON DELETE CASCADE,
            role          TEXT NOT NULL CHECK(role   IN ('owner', 'member')),
            status        TEXT NOT NULL CHECK(status IN ('pending', 'accepted', 'expired')),
            invited_at    TEXT NOT NULL,
            accepted_at   TEXT,
            PRIMARY KEY(submission_id, user_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_report_members_user ON report_members(user_id, status)",
    )
    .execute(pool)
    .await?;

    // Mesa por defecto del alumno por grupo (pre-rellena el formulario; puede variar por práctica).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_default_tables (
            user_id      TEXT    NOT NULL REFERENCES users(id)      ON DELETE CASCADE,
            group_id     TEXT    NOT NULL REFERENCES lab_groups(id) ON DELETE CASCADE,
            table_number INTEGER NOT NULL,
            updated_at   TEXT    NOT NULL,
            PRIMARY KEY(user_id, group_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Candado: un único informe por (práctica, grupo, mesa). Solo para entregas con mesa asignada.
    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_submissions_report_unique
        ON submissions(practice_id, group_id, table_number)
        WHERE table_number IS NOT NULL
        "#,
    )
    .execute(pool)
    .await?;

    // Backfill: cada entrega existente pasa a ser un informe de 1 miembro (owner accepted).
    // Idempotente: solo inserta si no existe ya la fila en report_members.
    sqlx::query(
        r#"
        INSERT INTO report_members (submission_id, user_id, role, status, invited_at, accepted_at)
        SELECT s.id, s.submitted_by_user_id, 'owner', 'accepted', s.submitted_at, s.submitted_at
        FROM submissions s
        WHERE s.submitted_by_user_id IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM report_members rm
              WHERE rm.submission_id = s.id AND rm.user_id = s.submitted_by_user_id
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

/// Agrega `column` (con la `definition` dada) a `table` solo si todavía no existe,
/// inspeccionando `PRAGMA table_info`. Permite evolucionar el esquema sin migraciones destructivas.
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

/// Inserta los usuarios iniciales de desarrollo (admin, docente, estudiante) si no existen.
/// Las contraseñas salen de las variables `SEED_*_PASSWORD` o usan valores por defecto.
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

/// Lista todos los usuarios ordenados por rol y nombre.
pub async fn users(pool: &SqlitePool) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users ORDER BY role, display_name",
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

    if matches!(
        verify_password(&input.current_password, &user.password_hash),
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

/// Siembra las prácticas del primer bloque de Física 103 en la tabla `practices` (id, nombre,
/// descripción, tipo de análisis y, en regresión, las fórmulas de eje). Idempotente: usa
/// `ON CONFLICT(id) DO NOTHING`, así que no pisa ediciones del docente entre reinicios.
pub async fn seed_practices(pool: &SqlitePool) -> anyhow::Result<()> {
    // Practicas reales del primer bloque de Fisica 103. Las columnas `x_formula`/`y_formula`
    // solo se usan en el camino `regresion_lineal`; en las estadisticas van en `None`.
    let practices = [
        (
            "p1-estadistica",
            "Tratamiento Estadistico - Pendulo Simple",
            "Medicion del periodo T con replicas (cronometro), longitud L dada por catedra; incertidumbres tipo A y B, calculo indirecto de g = 4*pi^2*L/T^2.",
            "estadistico",
            None,
            None,
        ),
        // P2-serie: R1, R2, R3 en serie con RA; I = Vg/(R1+R2+R3+RA) y V=I*R en cada resistencia.
        (
            "p2-serie",
            "CC - Circuito en Serie",
            "Circuito en serie: R1, R2 y R3 en serie con RA (resistencia interna del amperimetro). I y caidas de tension por leyes de circuito.",
            "estadistico",
            None,
            None,
        ),
        // P2-paralelo: R2 y R3 en paralelo con el circuito serie. Req y I calculados.
        (
            "p2-corriente-continua",
            "CC - Circuito en Paralelo",
            "Circuito mixto: R2 y R3 en paralelo, en serie con R1 y RA. Req e I calculados por leyes de circuito.",
            "estadistico",
            None,
            None,
        ),
        (
            "p3-relajacion",
            "Relajacion Exponencial",
            "Determinacion del tiempo de relajacion tau de un circuito RC (parte 1: medida directa).",
            "estadistico",
            None,
            None,
        ),
        // P3-parte2 — desfasaje por figura de Lissajous: se ajusta tg(phi) vs omega y la
        // pendiente del ajuste es RC = tau. El alumno carga f, a y b por punto; las formulas
        // de eje derivan x = 2*pi*f y y = b/sqrt(a^2 - b^2).
        (
            "p3-relajacion-desfasaje",
            "Relajacion Exponencial - Desfasaje",
            "Determinacion de tau = RC por desfasaje (parte 2): ajuste lineal de tg(phi) contra omega = 2*pi*f.",
            "regresion_lineal",
            Some("2*pi*f"),
            Some("b / math::sqrt(a*a - b*b)"),
        ),
    ];

    for (id, name, description, analysis_kind, x_formula, y_formula) in practices {
        // Actualiza nombre y descripción en conflicto para corregir errores de texto entre
        // versiones, pero preserva los campos editables por el docente (analysis_kind,
        // x_formula, y_formula) para no pisar cambios hechos desde la UI.
        sqlx::query(
            r#"
            INSERT INTO practices (id, name, description, analysis_kind, x_formula, y_formula)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(analysis_kind)
        .bind(x_formula)
        .bind(y_formula)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Asegura que `lab_groups` tenga las columnas `table_count` y `group_type`
/// (migración para bases creadas antes de introducir mesas y tipo de grupo).
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

/// Crea el curso, grupo y estudiante de prueba, lo inscribe y habilita las prácticas P1/P2/P3.
/// Idempotente vía `ON CONFLICT DO NOTHING`.
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

    for practice in [
        "p1-estadistica",
        "p2-serie",
        "p2-corriente-continua",
        "p3-relajacion",
        "p3-relajacion-desfasaje",
    ] {
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

/// Siembra entregas de prueba para el estudiante de seed, una por práctica habilitada.
/// Idempotente: no inserta si el estudiante ya tiene entregas.
pub async fn seed_submissions(pool: &SqlitePool) -> anyhow::Result<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM submissions")
        .fetch_one(pool)
        .await?;
    if count.0 > 0 {
        return Ok(());
    }

    let student = sqlx::query_as::<_, (String, String)>(
        "SELECT id, display_name FROM users WHERE email = 'estudiante@quantify.local'",
    )
    .fetch_optional(pool)
    .await?;

    let Some((student_id, _)) = student else {
        return Ok(());
    };

    let course_id = "fisica-experimental-i-2026";
    let group_row = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM lab_groups WHERE course_id = ?1 AND name = 'Grupo 1'",
    )
    .bind(course_id)
    .fetch_optional(pool)
    .await?;

    let Some((group_id,)) = group_row else {
        return Ok(());
    };

    let now = Utc::now();

    // Una entrega por práctica con datos realistas.
    let submissions: &[(&str, &str, &str)] = &[
        ("p1-estadistica", "pendiente", ""),
        (
            "p2-serie",
            "aprobada",
            "Buena medición. Todos los valores dentro del rango esperado.",
        ),
        (
            "p2-corriente-continua",
            "observada",
            "Revisar la medición de R3: la caída de tensión parece alta.",
        ),
        ("p3-relajacion", "pendiente", ""),
    ];

    for (practice_id, status, teacher_comment) in submissions {
        let analysis_json = serde_json::json!({
            "quantities": [],
            "derived": [],
            "warnings": ["Entrega de prueba generada por seed — no contiene mediciones reales."]
        })
        .to_string();

        let score: Option<f64> = if *status == "aprobada" {
            Some(8.5)
        } else {
            None
        };
        let reviewed_at = if *status != "pendiente" {
            Some(now)
        } else {
            None
        };
        let comment = if teacher_comment.is_empty() {
            None
        } else {
            Some(*teacher_comment)
        };

        let submission_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO submissions (
                id, student_name, group_name, course, practice_id, file_name, csv_path,
                analysis_json, status, submitted_at, submitted_by_user_id, course_id, group_id,
                entry_mode, score, teacher_comment, reviewed_at
            )
            SELECT
                ?1, u.display_name, g.name, c.name, ?5,
                '(formulario)', '', ?6, ?7, ?8, u.id, c.id, g.id,
                'form', ?9, ?10, ?11
            FROM users u, lab_groups g, courses c
            WHERE u.id = ?2 AND g.id = ?3 AND c.id = ?4
            "#,
        )
        .bind(&submission_id)
        .bind(&student_id)
        .bind(&group_id)
        .bind(course_id)
        .bind(practice_id)
        .bind(&analysis_json)
        .bind(status)
        .bind(now)
        .bind(score)
        .bind(comment)
        .bind(reviewed_at)
        .execute(pool)
        .await?;

        // Insertar al alumno como owner del informe sembrado.
        sqlx::query(
            r#"
            INSERT INTO report_members (submission_id, user_id, role, status, invited_at, accepted_at)
            VALUES (?1, ?2, 'owner', 'accepted', ?3, ?3)
            ON CONFLICT(submission_id, user_id) DO NOTHING
            "#,
        )
        .bind(&submission_id)
        .bind(&student_id)
        .bind(now)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Lista el catálogo completo de prácticas ordenado por nombre.
pub async fn practices(pool: &SqlitePool) -> anyhow::Result<Vec<Practice>> {
    let rows = sqlx::query_as::<_, Practice>(
        "SELECT id, name, description, analysis_kind FROM practices ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Lista los usuarios con rol `estudiante`, ordenados por nombre.
pub async fn students(pool: &SqlitePool) -> anyhow::Result<Vec<AuthUser>> {
    Ok(sqlx::query_as::<_, AuthUser>(
        "SELECT id, username, email, display_name, role FROM users WHERE role = 'estudiante' ORDER BY display_name",
    )
    .fetch_all(pool)
    .await?)
}

/// Arma el contexto académico que ve un usuario: docentes/admin ven todos los cursos,
/// estudiantes solo los suyos. Incluye prácticas y, para docentes, listas de estudiantes y usuarios.
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

/// Crea un curso activo (nombre + período) y lo devuelve.
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

    Ok(sqlx::query_as::<_, Course>(
        "SELECT id, name, term, active, submission_edit_hours, acceptance_window_hours FROM courses WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Actualiza nombre y período de un curso. Devuelve `None` si el curso no existe.
pub async fn update_course(
    pool: &SqlitePool,
    course_id: &str,
    input: UpdateCourse,
) -> anyhow::Result<Option<Course>> {
    let edit_hours = input.submission_edit_hours.map(|h| h.clamp(0.0, 72.0));
    let result = sqlx::query(
        r#"
        UPDATE courses
        SET name = ?2,
            term = ?3,
            submission_edit_hours = COALESCE(?4, submission_edit_hours)
        WHERE id = ?1
        "#,
    )
    .bind(course_id)
    .bind(input.name.trim())
    .bind(input.term.trim())
    .bind(edit_hours)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(
        sqlx::query_as::<_, Course>(
            "SELECT id, name, term, active, submission_edit_hours, acceptance_window_hours FROM courses WHERE id = ?1",
        )
        .bind(course_id)
        .fetch_one(pool)
        .await?,
    ))
}

/// Crea un grupo de laboratorio en un curso. `table_count` se acota a 1..=24 y
/// `group_type` se normaliza a `regular`/`recuperacion`.
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

/// Normaliza el tipo de grupo a uno de los valores válidos: `recuperacion` o `regular` (por defecto).
fn normalize_group_type(value: Option<&str>) -> &'static str {
    match value.unwrap_or("regular").trim() {
        "recuperacion" => "recuperacion",
        _ => "regular",
    }
}

/// Actualiza nombre, cantidad de mesas y tipo de un grupo. Devuelve `None` si no existe.
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

/// Crea un subgrupo de práctica (combinación curso/práctica/grupo + nombre) y lo devuelve.
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

/// Inscribe a un usuario en un curso (si no lo estaba) y lo agrega al grupo `General`
/// del curso, creándolo si hace falta. Idempotente.
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

/// Devuelve el id del grupo `General` del curso, creándolo si todavía no existe.
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

/// Agrega un estudiante a un grupo. Devuelve `None` si el usuario no existe o no es estudiante.
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

/// Quita a un estudiante de un grupo. Devuelve `true` si había una membresía y se eliminó.
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

/// Asigna (o reasigna) la mesa de trabajo de un estudiante para una práctica en un grupo.
/// Valida que la mesa esté en rango y que el estudiante pertenezca al grupo y la práctica
/// esté habilitada en el curso; devuelve `None` si algo no aplica.
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

/// Inscribe a un estudiante en un curso (vía `enroll_course_member`).
/// Devuelve `None` si el usuario no existe o no es estudiante.
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

/// Habilita una práctica en un curso. Idempotente (`ON CONFLICT DO NOTHING`).
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

/// Indica si un usuario puede entregar en (curso, grupo, práctica). Docentes/admin siempre
/// pueden; un estudiante puede solo si está inscripto, pertenece al grupo y la práctica está habilitada.
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

/// Crea un componente evaluable (pregunta/informe/parcial) en un curso, asignándole la
/// siguiente posición disponible, y lo devuelve.
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

/// Inserta o actualiza el puntaje crudo de un estudiante en un componente.
/// Devuelve error si el componente no existe o si el puntaje supera el máximo del componente.
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

/// Construye la libreta de calificaciones por curso. Docentes/admin ven todos los cursos y
/// todos los estudiantes; un estudiante ve solo sus cursos activos y su propio resumen.
pub async fn gradebook_for_user(
    pool: &SqlitePool,
    user: &AuthUser,
) -> anyhow::Result<Vec<CourseGradebook>> {
    let courses = if matches!(user.role.as_str(), "docente" | "admin") {
        sqlx::query_as::<_, Course>(
            "SELECT id, name, term, active, submission_edit_hours, acceptance_window_hours FROM courses ORDER BY term DESC, name",
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Course>(
            r#"
            SELECT DISTINCT c.id, c.name, c.term, c.active, c.submission_edit_hours, c.acceptance_window_hours
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

/// Lista los componentes evaluables de un curso, ordenados por posición y nombre.
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

/// Lista los estudiantes inscriptos (vía `course_members`) en un curso, sin duplicados.
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

/// Calcula el resumen de notas de un estudiante: normaliza cada puntaje
/// (`crudo/sobre*valor`), agrega subtotales por tipo y el total normalizado.
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

/// Resúmenes de todos los cursos (vista docente/admin), ordenados por período y nombre.
async fn all_course_summaries(pool: &SqlitePool) -> anyhow::Result<Vec<CourseSummary>> {
    let courses = sqlx::query_as::<_, Course>(
        "SELECT id, name, term, active, submission_edit_hours, acceptance_window_hours FROM courses ORDER BY term DESC, name",
    )
    .fetch_all(pool)
    .await?;
    course_summaries(pool, courses).await
}

/// Resúmenes de los cursos activos en los que está inscripto un estudiante.
async fn student_course_summaries(
    pool: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<CourseSummary>> {
    let courses = sqlx::query_as::<_, Course>(
        r#"
        SELECT DISTINCT c.id, c.name, c.term, c.active, c.submission_edit_hours, c.acceptance_window_hours
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

/// Enriquece una lista de cursos con sus miembros, grupos, prácticas, subgrupos y mesas.
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
            submission_edit_hours: course.submission_edit_hours,
            acceptance_window_hours: course.acceptance_window_hours,
            members,
            groups,
            practices,
            subgroups,
            table_assignments,
        });
    }
    Ok(summaries)
}

/// Lista los estudiantes miembros de un curso (para el resumen de curso).
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

/// Lista los grupos de un curso, cada uno con sus estudiantes miembros.
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

/// Lista los subgrupos de práctica de un curso, resolviendo práctica, grupo y miembros de cada uno.
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
            "SELECT id, name, description, analysis_kind FROM practices WHERE id = ?1",
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

/// Lista las asignaciones de mesa por práctica/grupo/estudiante de un curso.
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

/// Lista las prácticas habilitadas en un curso, ordenadas por nombre.
async fn practices_for_course(pool: &SqlitePool, course_id: &str) -> anyhow::Result<Vec<Practice>> {
    Ok(sqlx::query_as::<_, Practice>(
        r#"
        SELECT p.id, p.name, p.description, p.analysis_kind
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

/// Persiste una entrega: escribe el CSV en `upload_dir`, serializa el análisis y guarda
/// la fila resolviendo nombres denormalizados (estudiante, grupo, curso). Devuelve el detalle creado.
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

    sqlx::query(
        "INSERT INTO report_members (submission_id, user_id, role, status, invited_at, accepted_at) \
         VALUES (?1, ?2, 'owner', 'accepted', ?3, ?3)",
    )
    .bind(&id)
    .bind(&submission.submitted_by_user_id)
    .bind(submitted_at)
    .execute(pool)
    .await?;

    submission_detail(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created submission not found"))
}

/// Lista entregas: docentes/admin ven todas; un estudiante ve solo los informes donde es miembro aceptado.
pub async fn submission_list_for_user(
    pool: &SqlitePool,
    user: &AuthUser,
) -> anyhow::Result<Vec<SubmissionListItem>> {
    // Subquery que cuenta miembros aceptados por informe.
    let member_count_sq = "(SELECT COUNT(*) FROM report_members rm \
                            WHERE rm.submission_id = s.id AND rm.status = 'accepted')";

    let is_teacher = matches!(user.role.as_str(), "docente" | "admin");

    let query = if is_teacher {
        format!(
            r#"
            SELECT
                s.id, s.submitted_by_user_id, s.group_id,
                s.student_name, s.group_name, s.course,
                s.practice_id, p.name AS practice_name,
                s.status, s.score, s.submitted_at,
                COALESCE(s.entry_mode, 'csv') AS entry_mode,
                s.table_number,
                MAX({member_count_sq}, 1) AS member_count
            FROM submissions s
            JOIN practices p ON p.id = s.practice_id
            ORDER BY s.course, s.group_name, s.submitted_at DESC
            "#
        )
    } else {
        format!(
            r#"
            SELECT
                s.id, s.submitted_by_user_id, s.group_id,
                s.student_name, s.group_name, s.course,
                s.practice_id, p.name AS practice_name,
                s.status, s.score, s.submitted_at,
                COALESCE(s.entry_mode, 'csv') AS entry_mode,
                s.table_number,
                MAX({member_count_sq}, 1) AS member_count
            FROM submissions s
            JOIN practices p ON p.id = s.practice_id
            JOIN report_members rm_me ON rm_me.submission_id = s.id
                AND rm_me.user_id = ?1 AND rm_me.status = 'accepted'
            ORDER BY s.course, s.group_name, s.submitted_at DESC
            "#
        )
    };

    let rows = if is_teacher {
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

/// Devuelve el id del usuario que realizó una entrega (para control de acceso), o `None`.
pub async fn submission_owner_id(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<String>> {
    let owner: Option<(Option<String>,)> =
        sqlx::query_as("SELECT submitted_by_user_id FROM submissions WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(owner.and_then(|(user_id,)| user_id))
}

/// Devuelve `true` si el usuario es miembro aceptado (o owner) del informe dado.
/// Docentes/admin siempre acceden desde la capa de rutas; esta fn es para alumnos.
pub async fn is_accepted_member(
    pool: &SqlitePool,
    submission_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM report_members WHERE submission_id = ?1 AND user_id = ?2 AND status = 'accepted'",
    )
    .bind(submission_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Resuelve la mesa del alumno para una práctica y grupo: prioridad `practice_table_assignments`
/// → `user_default_tables`. Devuelve `None` si no tiene mesa asignada en ninguna fuente.
pub async fn resolve_user_table(
    pool: &SqlitePool,
    user_id: &str,
    group_id: &str,
    practice_id: &str,
) -> anyhow::Result<Option<i64>> {
    let row: Option<(Option<i64>,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(pta.table_number, udt.table_number)
        FROM group_members gm
        LEFT JOIN practice_table_assignments pta
            ON pta.group_id = gm.group_id AND pta.user_id = gm.user_id
            AND pta.practice_id = ?3
        LEFT JOIN user_default_tables udt
            ON udt.user_id = gm.user_id AND udt.group_id = gm.group_id
        WHERE gm.group_id = ?2 AND gm.user_id = ?1
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(group_id)
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(t,)| t))
}

/// Busca un informe existente para (práctica, grupo, mesa). Devuelve el `submission_id` o `None`.
pub async fn find_existing_report(
    pool: &SqlitePool,
    practice_id: &str,
    group_id: &str,
    table_number: i64,
) -> anyhow::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM submissions WHERE practice_id = ?1 AND group_id = ?2 AND table_number = ?3",
    )
    .bind(practice_id)
    .bind(group_id)
    .bind(table_number)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Lista los miembros de un informe ordenados por role (owner primero) y nombre.
pub async fn report_members_for(
    pool: &SqlitePool,
    submission_id: &str,
) -> anyhow::Result<Vec<ReportMember>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        user_id: String,
        display_name: String,
        role: String,
        status: String,
        accepted_at: Option<DateTime<Utc>>,
    }
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT rm.user_id, u.display_name, rm.role, rm.status, rm.accepted_at
        FROM report_members rm
        JOIN users u ON u.id = rm.user_id
        WHERE rm.submission_id = ?1
        ORDER BY CASE rm.role WHEN 'owner' THEN 0 ELSE 1 END, u.display_name
        "#,
    )
    .bind(submission_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ReportMember {
            user_id: r.user_id,
            display_name: r.display_name,
            role: r.role,
            status: r.status,
            accepted_at: r.accepted_at,
        })
        .collect())
}

/// Inserta entradas `pending` en `report_members` para los alumnos del mismo grupo
/// que tienen la misma mesa resuelta para la práctica (excluyendo al owner).
/// No falla si algún alumno ya tiene entrada (idempotente por `INSERT OR IGNORE`).
pub async fn invite_table_members(
    pool: &SqlitePool,
    submission_id: &str,
    group_id: &str,
    practice_id: &str,
    table_number: i64,
    owner_user_id: &str,
    invited_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    // Alumnos del mismo grupo con la misma mesa resuelta (pta primero, udt como fallback).
    let candidates: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT gm.user_id
        FROM group_members gm
        JOIN users u ON u.id = gm.user_id AND u.role = 'estudiante'
        LEFT JOIN practice_table_assignments pta
            ON pta.group_id = gm.group_id AND pta.user_id = gm.user_id
            AND pta.practice_id = ?2
        LEFT JOIN user_default_tables udt
            ON udt.user_id = gm.user_id AND udt.group_id = gm.group_id
        WHERE gm.group_id = ?1
          AND gm.user_id != ?3
          AND COALESCE(pta.table_number, udt.table_number) = ?4
        "#,
    )
    .bind(group_id)
    .bind(practice_id)
    .bind(owner_user_id)
    .bind(table_number)
    .fetch_all(pool)
    .await?;

    for (user_id,) in candidates {
        sqlx::query(
            r#"
            INSERT INTO report_members (submission_id, user_id, role, status, invited_at)
            VALUES (?1, ?2, 'member', 'pending', ?3)
            ON CONFLICT(submission_id, user_id) DO NOTHING
            "#,
        )
        .bind(submission_id)
        .bind(user_id)
        .bind(invited_at)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Devuelve las invitaciones vigentes (pendientes y aún dentro de la ventana) de un alumno.
pub async fn pending_invitations_for(
    pool: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<PendingInvitation>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        submission_id: String,
        practice_name: String,
        course: String,
        group_name: String,
        table_number: Option<i64>,
        owner_name: String,
        invited_at: DateTime<Utc>,
        submitted_at: DateTime<Utc>,
        acceptance_window_hours: f64,
    }
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT
            s.id AS submission_id,
            p.name AS practice_name,
            s.course,
            s.group_name,
            s.table_number,
            owner_u.display_name AS owner_name,
            rm.invited_at,
            s.submitted_at,
            COALESCE(c.acceptance_window_hours, 4.0) AS acceptance_window_hours
        FROM report_members rm
        JOIN submissions s ON s.id = rm.submission_id
        JOIN practices p ON p.id = s.practice_id
        LEFT JOIN courses c ON c.id = s.course_id
        JOIN report_members rm_owner ON rm_owner.submission_id = s.id AND rm_owner.role = 'owner'
        JOIN users owner_u ON owner_u.id = rm_owner.user_id
        WHERE rm.user_id = ?1 AND rm.status = 'pending'
        ORDER BY rm.invited_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let now = Utc::now();
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let expires_at = r.submitted_at
                + chrono::Duration::milliseconds((r.acceptance_window_hours * 3_600_000.0) as i64);
            if now >= expires_at {
                return None; // expirada — no mostrar
            }
            Some(PendingInvitation {
                submission_id: r.submission_id,
                practice_name: r.practice_name,
                course: r.course,
                group_name: r.group_name,
                table_number: r.table_number,
                owner_name: r.owner_name,
                invited_at: r.invited_at,
                expires_at,
            })
        })
        .collect())
}

/// Acepta una invitación pendiente dentro de la ventana del curso. Devuelve el resultado.
pub async fn accept_report_invitation(
    pool: &SqlitePool,
    submission_id: &str,
    user_id: &str,
) -> anyhow::Result<AcceptOutcome> {
    // Leer la entrada del alumno y la ventana de aceptación.
    #[derive(sqlx::FromRow)]
    struct Row {
        status: String,
        submitted_at: DateTime<Utc>,
        acceptance_window_hours: f64,
    }
    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT rm.status, s.submitted_at,
               COALESCE(c.acceptance_window_hours, 4.0) AS acceptance_window_hours
        FROM report_members rm
        JOIN submissions s ON s.id = rm.submission_id
        LEFT JOIN courses c ON c.id = s.course_id
        WHERE rm.submission_id = ?1 AND rm.user_id = ?2
        "#,
    )
    .bind(submission_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(AcceptOutcome::NotInvited);
    };

    if row.status == "accepted" {
        return Ok(AcceptOutcome::AlreadyAccepted);
    }

    let expires_at = row.submitted_at
        + chrono::Duration::milliseconds((row.acceptance_window_hours * 3_600_000.0) as i64);
    if Utc::now() >= expires_at {
        return Ok(AcceptOutcome::Expired);
    }

    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE report_members
        SET status = 'accepted', accepted_at = ?3
        WHERE submission_id = ?1 AND user_id = ?2 AND status = 'pending'
        "#,
    )
    .bind(submission_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AcceptOutcome::Accepted)
}

/// Agrega un miembro a un informe (uso docente). Si `force_accept` es `true`, el miembro
/// entra directamente como `accepted` sin pasar por la ventana de invitación.
pub async fn add_report_member(
    pool: &SqlitePool,
    submission_id: &str,
    user_id: &str,
    force_accept: bool,
) -> anyhow::Result<bool> {
    // Verificar que el usuario existe y es estudiante.
    let exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM users WHERE id = ?1 AND role = 'estudiante'")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    if exists.is_none() {
        return Ok(false);
    }

    let now = Utc::now();
    let status = if force_accept { "accepted" } else { "pending" };
    let accepted_at: Option<DateTime<Utc>> = if force_accept { Some(now) } else { None };

    sqlx::query(
        r#"
        INSERT INTO report_members (submission_id, user_id, role, status, invited_at, accepted_at)
        VALUES (?1, ?2, 'member', ?3, ?4, ?5)
        ON CONFLICT(submission_id, user_id) DO UPDATE SET
            status = excluded.status,
            accepted_at = excluded.accepted_at
        "#,
    )
    .bind(submission_id)
    .bind(user_id)
    .bind(status)
    .bind(now)
    .bind(accepted_at)
    .execute(pool)
    .await?;

    Ok(true)
}

/// Quita un miembro de un informe (uso docente). Si se quita al owner, el primer miembro
/// aceptado restante pasa a ser el nuevo owner. Devuelve `false` si el usuario no era miembro.
pub async fn remove_report_member(
    pool: &SqlitePool,
    submission_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let result =
        sqlx::query("DELETE FROM report_members WHERE submission_id = ?1 AND user_id = ?2")
            .bind(submission_id)
            .bind(user_id)
            .execute(pool)
            .await?;

    if result.rows_affected() == 0 {
        return Ok(false);
    }

    // Si ya no queda ningún owner, promover al primer miembro aceptado.
    let has_owner: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM report_members WHERE submission_id = ?1 AND role = 'owner'")
            .bind(submission_id)
            .fetch_optional(pool)
            .await?;

    if has_owner.is_none() {
        // Promover al primero aceptado (orden por accepted_at).
        sqlx::query(
            r#"
            UPDATE report_members
            SET role = 'owner'
            WHERE submission_id = ?1 AND status = 'accepted'
              AND rowid = (
                  SELECT rowid FROM report_members
                  WHERE submission_id = ?1 AND status = 'accepted'
                  ORDER BY accepted_at ASC LIMIT 1
              )
            "#,
        )
        .bind(submission_id)
        .execute(pool)
        .await?;
    }

    Ok(true)
}

/// Guarda o actualiza la mesa por defecto del alumno para un grupo. Valida que el alumno
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

/// Actualiza el grupo y/o mesa de un informe (uso docente). Valida que la nueva combinación
/// no colisione con otro informe existente. Devuelve `None` si la entrega no existe.
pub async fn update_report_meta(
    pool: &SqlitePool,
    submission_id: &str,
    group_id: Option<&str>,
    table_number: Option<i64>,
) -> anyhow::Result<Option<SubmissionDetail>> {
    let current: Option<(String, Option<String>, Option<i64>)> =
        sqlx::query_as("SELECT practice_id, group_id, table_number FROM submissions WHERE id = ?1")
            .bind(submission_id)
            .fetch_optional(pool)
            .await?;

    let Some((practice_id, current_group, current_table)) = current else {
        return Ok(None);
    };

    let new_group = group_id.unwrap_or(current_group.as_deref().unwrap_or(""));
    let new_table = table_number.or(current_table);

    // Verificar que no colisione con otro informe existente en el nuevo (práctica, grupo, mesa).
    if let Some(t) = new_table {
        let collision: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM submissions WHERE practice_id = ?1 AND group_id = ?2 AND table_number = ?3 AND id != ?4",
        )
        .bind(&practice_id)
        .bind(new_group)
        .bind(t)
        .bind(submission_id)
        .fetch_optional(pool)
        .await?;

        if collision.is_some() {
            anyhow::bail!("Ya existe un informe para esa combinación de grupo y mesa");
        }
    }

    sqlx::query(
        r#"
        UPDATE submissions
        SET group_id = COALESCE(?2, group_id),
            table_number = ?3
        WHERE id = ?1
        "#,
    )
    .bind(submission_id)
    .bind(group_id)
    .bind(new_table)
    .execute(pool)
    .await?;

    submission_detail(pool, submission_id).await
}

/// Recupera el detalle completo de una entrega (incluye el análisis deserializado).
/// Devuelve `None` si no existe.
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
            COALESCE(s.entry_mode, 'csv') AS entry_mode,
            COALESCE(s.results_visible_to_student, 0) AS results_visible_to_student,
            s.status,
            s.teacher_comment,
            s.score,
            s.submitted_at,
            s.reviewed_at,
            s.measurement_meta_json,
            s.course_id,
            COALESCE(c.submission_edit_hours, 4) AS submission_edit_hours,
            s.table_number
        FROM submissions s
        JOIN practices p ON p.id = s.practice_id
        LEFT JOIN courses c ON c.id = s.course_id
        WHERE s.id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let analysis = serde_json::from_str(&row.analysis_json)?;
    let measurement_meta = match &row.measurement_meta_json {
        Some(json) => serde_json::from_str(json).ok(),
        None => None,
    };
    let student_results = student_results_for(pool, &row.id).await?;
    let measurements = measurements_for(pool, &row.id).await?;
    let members = report_members_for(pool, &row.id).await?;
    // Ventana de edición: submitted_at + horas del curso. Editable si sigue abierta, la entrega
    // está pendiente y el cálculo aún no es visible (la propiedad la valida el endpoint).
    let editable_until = row.submitted_at
        + chrono::Duration::milliseconds((row.submission_edit_hours * 3_600_000.0) as i64);
    let can_edit = row.entry_mode == "form"
        && row.status == "pendiente"
        && !row.results_visible_to_student
        && Utc::now() < editable_until;
    Ok(Some(SubmissionDetail {
        id: row.id,
        student_name: row.student_name,
        group_name: row.group_name,
        course: row.course,
        practice_id: row.practice_id,
        practice_name: row.practice_name,
        file_name: row.file_name,
        entry_mode: row.entry_mode,
        results_visible_to_student: row.results_visible_to_student,
        analysis,
        student_results,
        status: row.status,
        teacher_comment: row.teacher_comment,
        score: row.score,
        submitted_at: row.submitted_at,
        reviewed_at: row.reviewed_at,
        measurement_meta,
        editable_until: Some(editable_until),
        can_edit,
        measurements,
        table_number: row.table_number,
        members,
    }))
}

/// Lecturas crudas persistidas de una entrega (ordenadas por magnitud y réplica), para prefill.
pub async fn measurements_for(
    pool: &SqlitePool,
    submission_id: &str,
) -> anyhow::Result<Vec<SubmissionMeasurement>> {
    Ok(sqlx::query_as::<_, SubmissionMeasurement>(
        "SELECT quantity_id, instrument_id, scale_id, replicate_index, value, value_u \
         FROM submission_measurements WHERE submission_id = ?1 \
         ORDER BY quantity_id, replicate_index",
    )
    .bind(submission_id)
    .fetch_all(pool)
    .await?)
}

/// Devuelve los mensurandos calculados por el estudiante para una entrega (ordenados por símbolo).
pub async fn student_results_for(
    pool: &SqlitePool,
    submission_id: &str,
) -> anyhow::Result<Vec<StudentResult>> {
    Ok(sqlx::query_as::<_, StudentResult>(
        "SELECT symbol, value, u_expanded FROM submission_student_results \
         WHERE submission_id = ?1 ORDER BY symbol",
    )
    .bind(submission_id)
    .fetch_all(pool)
    .await?)
}

/// Reemplaza por completo los cálculos del estudiante de una entrega (borra los previos e
/// inserta los nuevos), en una transacción. Ignora filas con valor no finito.
pub async fn save_student_results(
    pool: &SqlitePool,
    submission_id: &str,
    results: &[StudentResultInput],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM submission_student_results WHERE submission_id = ?1")
        .bind(submission_id)
        .execute(&mut *tx)
        .await?;
    for input in results {
        if !input.value.is_finite() {
            continue;
        }
        let u = input.u_expanded.filter(|u| u.is_finite());
        // `ON CONFLICT` hace que, si el payload trae el mismo símbolo repetido, gane el último
        // (en vez de violar el UNIQUE y abortar la transacción).
        sqlx::query(
            "INSERT INTO submission_student_results \
             (id, submission_id, symbol, value, u_expanded, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(submission_id, symbol) DO UPDATE SET \
             value = excluded.value, u_expanded = excluded.u_expanded",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(submission_id)
        .bind(input.symbol.trim())
        .bind(input.value)
        .bind(u)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Registra la revisión docente de una entrega (estado, comentario, nota y fecha) y
/// devuelve el detalle actualizado.
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
            reviewed_at = ?5,
            results_visible_to_student = COALESCE(?6, results_visible_to_student)
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(review.status)
    .bind(review.teacher_comment)
    .bind(review.score)
    .bind(Utc::now())
    .bind(review.results_visible)
    .execute(pool)
    .await?;

    submission_detail(pool, id).await
}

/// Resultado de verificar una contraseña contra su hash almacenado.
enum VerifyResult {
    /// Contraseña incorrecta.
    Invalid,
    /// Contraseña correcta; el hash ya está en formato Argon2.
    Valid,
    /// Contraseña correcta; el hash estaba en el formato SHA-256 legacy y debe actualizarse.
    ValidNeedsRehash(String),
}

/// Genera un hash Argon2id de la contraseña con salt aleatorio.
fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        // inalcanzable: params default + SaltString válido nunca fallan
        .expect("argon2 hash con params default y salt OsRng nunca falla")
        .to_string()
}

/// Verifica una contraseña contra su hash almacenado.
///
/// Soporta el formato legacy SHA-256 (`salt:hex`) y el nuevo formato Argon2id (`$argon2id$…`).
/// Si el hash es legacy y la contraseña es correcta, devuelve `ValidNeedsRehash` con el nuevo
/// hash para que el llamador pueda migrar el registro transparentemente.
fn verify_password(password: &str, stored_hash: &str) -> VerifyResult {
    if stored_hash.starts_with("$argon2") {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return VerifyResult::Invalid;
        };
        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
        {
            VerifyResult::Valid
        } else {
            VerifyResult::Invalid
        }
    } else {
        // Formato legacy SHA-256: `salt:hex`
        let Some((salt, expected)) = stored_hash.split_once(':') else {
            return VerifyResult::Invalid;
        };
        if digest_password(salt, password) == expected {
            VerifyResult::ValidNeedsRehash(hash_password(password))
        } else {
            VerifyResult::Invalid
        }
    }
}

/// Calcula el digest SHA-256 hexadecimal de `salt:password` (solo para verificar hashes legacy).
fn digest_password(salt: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Colapsa a `0.0` los valores con magnitud despreciable (< 1e-9) para evitar mostrar
/// "ceros sucios" por error de punto flotante.
fn clean_zero(value: f64) -> f64 {
    if value.abs() < 1e-9 {
        0.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Crea un pool sobre una base SQLite temporal con el esquema ya migrado.
    /// Devuelve también el `TempDir` para mantenerlo vivo mientras dure el test.
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

    /// Igual que `pool` pero con las semillas aplicadas (prácticas, usuarios y curso de prueba).
    async fn seeded() -> (SqlitePool, TempDir) {
        let (pool, dir) = pool().await;
        seed_practices(&pool).await.unwrap();
        seed_users(&pool).await.unwrap();
        seed_academic(&pool).await.unwrap();
        (pool, dir)
    }

    /// Busca un usuario sembrado por email.
    async fn find_user(pool: &SqlitePool, email: &str) -> AuthUser {
        users(pool)
            .await
            .unwrap()
            .into_iter()
            .find(|u| u.email == email)
            .unwrap()
    }

    const TEACHER: &str = "docente@quantify.local";
    const STUDENT: &str = "estudiante@quantify.local";
    const COURSE: &str = "fisica-experimental-i-2026";
    const GROUP: &str = "fisica-exp-i-grupo-1";

    /// Crea una entrega de prueba (vía CSV) del estudiante dado y devuelve su id.
    async fn make_submission(pool: &SqlitePool, dir: &std::path::Path, student_id: &str) -> String {
        let analysis = crate::analysis::analyze_csv("x,y\n1,2\n2,4\n3,6\n").unwrap();
        create_submission(
            pool,
            dir,
            NewSubmission {
                submitted_by_user_id: student_id.to_string(),
                course_id: COURSE.into(),
                group_id: GROUP.into(),
                practice_id: "p1-estadistica".into(),
                file_name: "medidas.csv".into(),
                csv_content: "x,y\n1,2\n2,4\n3,6\n".into(),
                analysis,
            },
        )
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let (pool, _dir) = pool().await;
        // Volver a migrar no debe fallar (cubre add_column_if_missing y ensure_lab_group_columns).
        migrate(&pool).await.unwrap();
    }

    #[tokio::test]
    async fn seed_users_creates_three_and_is_idempotent() {
        let (pool, _dir) = pool().await;
        seed_users(&pool).await.unwrap();
        seed_users(&pool).await.unwrap();
        assert_eq!(users(&pool).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn seed_practices_loads_p1_p2_p3() {
        let (pool, _dir) = pool().await;
        seed_practices(&pool).await.unwrap();
        let ids: Vec<String> = practices(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert!(ids.contains(&"p1-estadistica".to_string()));
        assert!(ids.contains(&"p2-serie".to_string()));
        assert!(ids.contains(&"p2-corriente-continua".to_string()));
        assert!(ids.contains(&"p3-relajacion".to_string()));
        assert!(ids.contains(&"p3-relajacion-desfasaje".to_string()));
    }

    #[tokio::test]
    async fn seed_practices_does_not_clobber_edits() {
        let (pool, _dir) = pool().await;
        seed_practices(&pool).await.unwrap();
        // El docente edita el tipo de análisis de una práctica.
        sqlx::query(
            "UPDATE practices SET analysis_kind = 'regresion_lineal' WHERE id = 'p1-estadistica'",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Un reinicio re-corre el seed: NO debe pisar la edición.
        seed_practices(&pool).await.unwrap();
        let p = practices(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.id == "p1-estadistica")
            .unwrap();
        assert_eq!(p.analysis_kind.as_deref(), Some("regresion_lineal"));
    }

    #[tokio::test]
    async fn seed_academic_enrolls_student_and_enables_practices() {
        let (pool, _dir) = seeded().await;
        let student = find_user(&pool, STUDENT).await;
        assert!(
            user_can_submit(&pool, &student, COURSE, GROUP, "p1-estadistica")
                .await
                .unwrap()
        );
        assert!(
            user_can_submit(&pool, &student, COURSE, GROUP, "p3-relajacion-desfasaje")
                .await
                .unwrap()
        );
        assert_eq!(practices_for_course(&pool, COURSE).await.unwrap().len(), 5);
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
        // Inserta un usuario con hash legacy SHA-256 directamente en la base.
        let salt = "test-salt-uuid";
        let legacy_hash = format!("{salt}:{}", digest_password(salt, "clave1234"));
        sqlx::query(
            "INSERT INTO users (id, username, email, display_name, role, password_hash, created_at)
             VALUES ('u1', 'legacy', 'legacy@test.local', 'Legacy', 'estudiante', ?1, '2024-01-01')",
        )
        .bind(&legacy_hash)
        .execute(&pool)
        .await
        .unwrap();

        // Contraseña incorrecta contra hash legacy debe fallar (sin migrar).
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

        // El login debe tener éxito con el hash legacy.
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

        // El hash debe haberse migrado a Argon2id.
        let updated: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            updated.starts_with("$argon2"),
            "el hash debe actualizarse a Argon2id tras el login"
        );

        // Con el nuevo hash Argon2id, el login debe seguir funcionando.
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

    #[tokio::test]
    async fn create_user_lowercases_email() {
        let (pool, _dir) = pool().await;
        let created = create_user(
            &pool,
            CreateUser {
                email: "Nuevo@Facultad.Edu".into(),
                display_name: "Nuevo".into(),
                role: "estudiante".into(),
                password: "clave1234".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(created.email, "nuevo@facultad.edu");
    }

    #[tokio::test]
    async fn reset_password_changes_login() {
        let (pool, _dir) = seeded().await;
        let teacher = find_user(&pool, TEACHER).await;
        assert!(reset_password(
            &pool,
            &teacher.id,
            ResetPassword {
                password: "otraclave".into()
            }
        )
        .await
        .unwrap());
        assert!(login(
            &pool,
            LoginRequest {
                email: Some(TEACHER.into()),
                username: None,
                password: "otraclave".into()
            },
        )
        .await
        .unwrap()
        .is_some());
        assert!(!reset_password(
            &pool,
            "id-inexistente",
            ResetPassword {
                password: "x12345678".into()
            }
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn update_user_changes_fields_and_handles_missing() {
        let (pool, _dir) = seeded().await;
        let student = find_user(&pool, STUDENT).await;
        let updated = update_user(
            &pool,
            &student.id,
            UpdateUser {
                email: "ESTU2@fq.edu".into(),
                display_name: "Estu Dos".into(),
                role: "estudiante".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.email, "estu2@fq.edu");
        assert_eq!(updated.display_name, "Estu Dos");

        let missing = update_user(
            &pool,
            "id-inexistente",
            UpdateUser {
                email: "x@y.com".into(),
                display_name: "X".into(),
                role: "estudiante".into(),
            },
        )
        .await
        .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn change_password_validates_current_and_clears_sessions() {
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

        // Contraseña actual incorrecta -> false.
        assert!(!change_password(
            &pool,
            &user.id,
            ChangePassword {
                current_password: "mala".into(),
                new_password: "nuevaclave".into()
            },
        )
        .await
        .unwrap());

        // Correcta -> true y se invalidan las sesiones.
        assert!(change_password(
            &pool,
            &user.id,
            ChangePassword {
                current_password: "docente123".into(),
                new_password: "nuevaclave".into()
            },
        )
        .await
        .unwrap());
        assert!(user_by_session(&pool, &token).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn students_lists_only_estudiantes() {
        let (pool, _dir) = seeded().await;
        let students = students(&pool).await.unwrap();
        assert!(students.iter().all(|u| u.role == "estudiante"));
        assert!(students.iter().any(|u| u.email == STUDENT));
    }

    #[tokio::test]
    async fn academic_context_differs_by_role() {
        let (pool, _dir) = seeded().await;
        let teacher = find_user(&pool, TEACHER).await;
        let student = find_user(&pool, STUDENT).await;

        let teacher_ctx = academic_context(&pool, &teacher).await.unwrap();
        assert!(!teacher_ctx.courses.is_empty());
        assert!(!teacher_ctx.students.is_empty());
        assert_eq!(teacher_ctx.users.len(), 3);

        let student_ctx = academic_context(&pool, &student).await.unwrap();
        assert!(!student_ctx.courses.is_empty());
        assert!(student_ctx.students.is_empty());
        assert!(student_ctx.users.is_empty());
    }

    #[tokio::test]
    async fn create_and_update_course() {
        let (pool, _dir) = pool().await;
        let course = create_course(
            &pool,
            CreateCourse {
                name: "Curso".into(),
                term: "2026".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(course.name, "Curso");

        let updated = update_course(
            &pool,
            &course.id,
            UpdateCourse {
                name: "Curso B".into(),
                term: "2027".into(),
                submission_edit_hours: Some(6.0),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.name, "Curso B");
        assert_eq!(updated.term, "2027");
        assert_eq!(updated.submission_edit_hours, 6.0);

        assert!(update_course(
            &pool,
            "x",
            UpdateCourse {
                name: "n".into(),
                term: "t".into(),
                submission_edit_hours: None,
            }
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn create_group_clamps_and_normalizes() {
        let (pool, _dir) = pool().await;
        let course = create_course(
            &pool,
            CreateCourse {
                name: "C".into(),
                term: "2026".into(),
            },
        )
        .await
        .unwrap();
        // table_count fuera de rango se acota a 24; tipo inválido -> regular.
        let group = create_group(
            &pool,
            &course.id,
            CreateGroup {
                name: "G".into(),
                table_count: Some(999),
                group_type: Some("raro".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(group.table_count, 24);
        assert_eq!(group.group_type, "regular");

        let recup = create_group(
            &pool,
            &course.id,
            CreateGroup {
                name: "GR".into(),
                table_count: None,
                group_type: Some("recuperacion".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(recup.table_count, 4);
        assert_eq!(recup.group_type, "recuperacion");
    }

    #[tokio::test]
    async fn update_group_changes_and_handles_missing() {
        let (pool, _dir) = seeded().await;
        let updated = update_group(
            &pool,
            GROUP,
            UpdateGroup {
                name: "Grupo Uno".into(),
                table_count: 6,
                group_type: "regular".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.name, "Grupo Uno");
        assert_eq!(updated.table_count, 6);

        assert!(update_group(
            &pool,
            "x",
            UpdateGroup {
                name: "n".into(),
                table_count: 2,
                group_type: "regular".into()
            }
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn create_subgroup_persists() {
        let (pool, _dir) = seeded().await;
        let subgroup = create_subgroup(
            &pool,
            COURSE,
            CreateSubgroup {
                practice_id: "p1-estadistica".into(),
                group_id: GROUP.into(),
                name: "Sub A".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(subgroup.name, "Sub A");
        assert_eq!(subgroup.practice_id, "p1-estadistica");
    }

    #[tokio::test]
    async fn enroll_course_member_is_idempotent_and_creates_general_group() {
        let (pool, _dir) = seeded().await;
        let new_student = create_user(
            &pool,
            CreateUser {
                email: "otro@fq.edu".into(),
                display_name: "Otro".into(),
                role: "estudiante".into(),
                password: "clave1234".into(),
            },
        )
        .await
        .unwrap();
        enroll_course_member(&pool, COURSE, &new_student.id)
            .await
            .unwrap();
        enroll_course_member(&pool, COURSE, &new_student.id)
            .await
            .unwrap();

        // Debe existir el grupo "General" creado por ensure_default_group.
        let groups = groups_for_course(&pool, COURSE).await.unwrap();
        assert!(groups.iter().any(|g| g.name == "General"));
    }

    #[tokio::test]
    async fn add_and_remove_group_member() {
        let (pool, _dir) = seeded().await;
        let teacher = find_user(&pool, TEACHER).await;
        let student = find_user(&pool, STUDENT).await;

        // Un docente no es estudiante: add_group_member devuelve None.
        assert!(add_group_member(
            &pool,
            GROUP,
            AddGroupMember {
                user_id: teacher.id.clone()
            }
        )
        .await
        .unwrap()
        .is_none());

        // El estudiante ya es miembro; agregarlo de nuevo es idempotente y devuelve Some.
        assert!(add_group_member(
            &pool,
            GROUP,
            AddGroupMember {
                user_id: student.id.clone()
            }
        )
        .await
        .unwrap()
        .is_some());

        assert!(remove_group_member(&pool, GROUP, &student.id)
            .await
            .unwrap());
        assert!(!remove_group_member(&pool, GROUP, &student.id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn set_practice_table_validates_membership_and_range() {
        let (pool, _dir) = seeded().await;
        let student = find_user(&pool, STUDENT).await;

        let ok = set_practice_table_assignment(
            &pool,
            GROUP,
            &student.id,
            SetPracticeTable {
                practice_id: "p1-estadistica".into(),
                table_number: 2,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(ok.table_number, 2);

        // Mesa fuera de rango (table_count = 4).
        assert!(set_practice_table_assignment(
            &pool,
            GROUP,
            &student.id,
            SetPracticeTable {
                practice_id: "p1-estadistica".into(),
                table_number: 99
            },
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn add_course_member_requires_student() {
        let (pool, _dir) = seeded().await;
        let teacher = find_user(&pool, TEACHER).await;
        let new_student = create_user(
            &pool,
            CreateUser {
                email: "tercero@fq.edu".into(),
                display_name: "Tercero".into(),
                role: "estudiante".into(),
                password: "clave1234".into(),
            },
        )
        .await
        .unwrap();

        assert!(add_course_member(
            &pool,
            COURSE,
            EnrollCourseMember {
                user_id: new_student.id
            }
        )
        .await
        .unwrap()
        .is_some());
        assert!(add_course_member(
            &pool,
            COURSE,
            EnrollCourseMember {
                user_id: teacher.id
            }
        )
        .await
        .unwrap()
        .is_none());
    }

    #[tokio::test]
    async fn enable_course_practice_is_idempotent() {
        let (pool, _dir) = seeded().await;
        let course = create_course(
            &pool,
            CreateCourse {
                name: "Vacio".into(),
                term: "2026".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            practices_for_course(&pool, &course.id).await.unwrap().len(),
            0
        );
        enable_course_practice(
            &pool,
            &course.id,
            SetCoursePractice {
                practice_id: "p1-estadistica".into(),
            },
        )
        .await
        .unwrap();
        enable_course_practice(
            &pool,
            &course.id,
            SetCoursePractice {
                practice_id: "p1-estadistica".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            practices_for_course(&pool, &course.id).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn user_can_submit_rules() {
        let (pool, _dir) = seeded().await;
        let teacher = find_user(&pool, TEACHER).await;
        let student = find_user(&pool, STUDENT).await;

        // Docente siempre puede.
        assert!(
            user_can_submit(&pool, &teacher, COURSE, GROUP, "p1-estadistica")
                .await
                .unwrap()
        );
        // Estudiante con curso/grupo/práctica válidos.
        assert!(
            user_can_submit(&pool, &student, COURSE, GROUP, "p1-estadistica")
                .await
                .unwrap()
        );
        // Práctica no habilitada / inexistente -> false.
        assert!(
            !user_can_submit(&pool, &student, COURSE, GROUP, "p9-inexistente")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn grade_component_position_increments() {
        let (pool, _dir) = seeded().await;
        let c1 = create_grade_component(
            &pool,
            CreateGradeComponent {
                course_id: COURSE.into(),
                kind: "pregunta".into(),
                name: "P1".into(),
                max_points: 10.0,
                weight_points: 5.0,
            },
        )
        .await
        .unwrap();
        let c2 = create_grade_component(
            &pool,
            CreateGradeComponent {
                course_id: COURSE.into(),
                kind: "informe".into(),
                name: "I1".into(),
                max_points: 20.0,
                weight_points: 10.0,
            },
        )
        .await
        .unwrap();
        assert_eq!(c1.position, 1);
        assert_eq!(c2.position, 2);
    }

    #[tokio::test]
    async fn upsert_grade_score_normalizes_and_rejects_over_max() {
        let (pool, _dir) = seeded().await;
        let student = find_user(&pool, STUDENT).await;
        let component = create_grade_component(
            &pool,
            CreateGradeComponent {
                course_id: COURSE.into(),
                kind: "pregunta".into(),
                name: "P1".into(),
                max_points: 10.0,
                weight_points: 5.0,
            },
        )
        .await
        .unwrap();

        upsert_grade_score(
            &pool,
            UpsertGradeScore {
                component_id: component.id.clone(),
                student_id: student.id.clone(),
                raw_points: 8.0,
                comment: None,
            },
        )
        .await
        .unwrap();

        // Supera el máximo -> error.
        assert!(upsert_grade_score(
            &pool,
            UpsertGradeScore {
                component_id: component.id.clone(),
                student_id: student.id.clone(),
                raw_points: 11.0,
                comment: None
            },
        )
        .await
        .is_err());

        // Componente inexistente -> error.
        assert!(upsert_grade_score(
            &pool,
            UpsertGradeScore {
                component_id: "no-existe".into(),
                student_id: student.id.clone(),
                raw_points: 1.0,
                comment: None
            },
        )
        .await
        .is_err());

        let teacher = find_user(&pool, TEACHER).await;
        let books = gradebook_for_user(&pool, &teacher).await.unwrap();
        let course_book = books.into_iter().find(|b| b.course.id == COURSE).unwrap();
        let summary = course_book
            .students
            .into_iter()
            .find(|s| s.student.id == student.id)
            .unwrap();
        // 8/10 * 5 = 4.0 normalizado.
        assert!((summary.total_points - 4.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn submission_lifecycle() {
        let (pool, dir) = seeded().await;
        let student = find_user(&pool, STUDENT).await;
        let analysis = crate::analysis::analyze_csv("x,y\n1,2\n2,4\n3,6\n").unwrap();

        let created = create_submission(
            &pool,
            dir.path(),
            NewSubmission {
                submitted_by_user_id: student.id.clone(),
                course_id: COURSE.into(),
                group_id: GROUP.into(),
                practice_id: "p1-estadistica".into(),
                file_name: "medidas.csv".into(),
                csv_content: "x,y\n1,2\n2,4\n3,6\n".into(),
                analysis,
            },
        )
        .await
        .unwrap();
        assert_eq!(created.status, "pendiente");

        // El docente ve la entrega; el estudiante también la suya.
        let teacher = find_user(&pool, TEACHER).await;
        assert_eq!(
            submission_list_for_user(&pool, &teacher)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            submission_list_for_user(&pool, &student)
                .await
                .unwrap()
                .len(),
            1
        );

        assert_eq!(
            submission_owner_id(&pool, &created.id)
                .await
                .unwrap()
                .as_deref(),
            Some(student.id.as_str())
        );
        let detail = submission_detail(&pool, &created.id)
            .await
            .unwrap()
            .unwrap();
        // Por defecto el calculo no es visible para el estudiante.
        assert!(!detail.results_visible_to_student);

        let reviewed = update_review(
            &pool,
            &created.id,
            ReviewSubmission {
                status: "aprobada".into(),
                teacher_comment: Some("ok".into()),
                score: Some(10.0),
                results_visible: Some(true),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(reviewed.status, "aprobada");
        assert_eq!(reviewed.score, Some(10.0));
        // El docente habilito la visibilidad.
        assert!(reviewed.results_visible_to_student);

        // Una revision sin `results_visible` (None) no cambia la visibilidad ya habilitada.
        let again = update_review(
            &pool,
            &created.id,
            ReviewSubmission {
                status: "observada".into(),
                teacher_comment: None,
                score: None,
                results_visible: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert!(again.results_visible_to_student);
    }

    #[tokio::test]
    async fn student_results_save_read_and_replace() {
        let (pool, dir) = seeded().await;
        let student = find_user(&pool, STUDENT).await;
        let id = make_submission(&pool, dir.path(), &student.id).await;

        // Sin cálculos al inicio.
        assert!(student_results_for(&pool, &id).await.unwrap().is_empty());

        // Guarda dos mensurandos (uno con U, otro sin); la fila con valor NaN se ignora.
        save_student_results(
            &pool,
            &id,
            &[
                StudentResultInput {
                    symbol: "Q".into(),
                    value: 11.0,
                    u_expanded: Some(0.5),
                },
                StudentResultInput {
                    symbol: "R".into(),
                    value: 3.0,
                    u_expanded: None,
                },
                StudentResultInput {
                    symbol: "bad".into(),
                    value: f64::NAN,
                    u_expanded: None,
                },
            ],
        )
        .await
        .unwrap();

        let saved = student_results_for(&pool, &id).await.unwrap();
        assert_eq!(saved.len(), 2); // NaN ignorado
        assert_eq!(saved[0].symbol, "Q"); // ordenado por símbolo
        assert!((saved[0].value - 11.0).abs() < 1e-12);
        assert_eq!(saved[0].u_expanded, Some(0.5));
        assert_eq!(saved[1].symbol, "R");
        assert_eq!(saved[1].u_expanded, None);

        // Aparecen en el detalle de la entrega.
        let detail = submission_detail(&pool, &id).await.unwrap().unwrap();
        assert_eq!(detail.student_results.len(), 2);

        // Replace-all: un nuevo set reemplaza por completo al anterior.
        save_student_results(
            &pool,
            &id,
            &[StudentResultInput {
                symbol: "Q".into(),
                value: 12.0,
                u_expanded: Some(0.7),
            }],
        )
        .await
        .unwrap();
        let replaced = student_results_for(&pool, &id).await.unwrap();
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].symbol, "Q");
        assert!((replaced[0].value - 12.0).abs() < 1e-12);

        // Símbolo repetido en el mismo payload: gana el último (sin violar el UNIQUE).
        save_student_results(
            &pool,
            &id,
            &[
                StudentResultInput {
                    symbol: "Q".into(),
                    value: 1.0,
                    u_expanded: None,
                },
                StudentResultInput {
                    symbol: "Q".into(),
                    value: 2.0,
                    u_expanded: Some(0.1),
                },
            ],
        )
        .await
        .unwrap();
        let deduped = student_results_for(&pool, &id).await.unwrap();
        assert_eq!(deduped.len(), 1);
        assert!((deduped[0].value - 2.0).abs() < 1e-12);
        assert_eq!(deduped[0].u_expanded, Some(0.1));
    }

    #[tokio::test]
    async fn student_results_cascade_on_submission_delete() {
        let (pool, dir) = seeded().await;
        // El pool de test no fuerza FKs por defecto; las activamos para ver el ON DELETE CASCADE.
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        let student = find_user(&pool, STUDENT).await;
        let id = make_submission(&pool, dir.path(), &student.id).await;
        save_student_results(
            &pool,
            &id,
            &[StudentResultInput {
                symbol: "Q".into(),
                value: 1.0,
                u_expanded: None,
            }],
        )
        .await
        .unwrap();
        assert_eq!(student_results_for(&pool, &id).await.unwrap().len(), 1);

        sqlx::query("DELETE FROM submissions WHERE id = ?1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(student_results_for(&pool, &id).await.unwrap().is_empty());
    }

    #[test]
    fn clean_zero_collapses_tiny_values() {
        assert_eq!(clean_zero(1e-12), 0.0);
        assert_eq!(clean_zero(2.5), 2.5);
    }

    #[tokio::test]
    async fn instruments_schema_roundtrip() {
        let (pool, _dir) = seeded().await;
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO instruments (id, course_id, name, kind, quantity, unit, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind("inst-1")
        .bind(COURSE)
        .bind("Calibre")
        .bind("analogico")
        .bind("longitud")
        .bind("mm")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO instrument_scales (id, instrument_id, label, full_scale, step, \
             appreciation, internal_res, internal_res_u, b_model, spec_pct_reading, \
             spec_step_coeff, spec_fixed, unit, position, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )
        .bind("scale-1")
        .bind("inst-1")
        .bind("0-150 mm")
        .bind(Option::<f64>::None) // full_scale
        .bind(0.05_f64) // step
        .bind(Some(0.05_f64)) // appreciation
        .bind(Option::<f64>::None) // internal_res
        .bind(Option::<f64>::None) // internal_res_u
        .bind("apreciacion")
        .bind(Option::<f64>::None) // spec_pct_reading
        .bind(Option::<f64>::None) // spec_step_coeff
        .bind(Option::<f64>::None) // spec_fixed
        .bind("mm")
        .bind(0_i64)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let inst = sqlx::query_as::<_, Instrument>(
            "SELECT id, course_id, name, kind, quantity, unit FROM instruments WHERE id = ?1",
        )
        .bind("inst-1")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inst.kind, "analogico");
        assert_eq!(inst.quantity, "longitud");
        assert_eq!(inst.course_id, COURSE);

        let scale = sqlx::query_as::<_, InstrumentScale>(
            "SELECT id, instrument_id, label, full_scale, step, appreciation, internal_res, \
             internal_res_u, b_model, spec_pct_reading, spec_step_coeff, spec_fixed, unit, \
             position FROM instrument_scales WHERE id = ?1",
        )
        .bind("scale-1")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(scale.b_model, "apreciacion");
        assert_eq!(scale.step, 0.05);
        assert_eq!(scale.appreciation, Some(0.05));
        assert!(scale.full_scale.is_none());
        assert_eq!(scale.position, 0);
    }

    #[tokio::test]
    async fn instrument_scale_rejects_invalid_b_model() {
        let (pool, _dir) = seeded().await;
        let now = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO instruments (id, course_id, name, kind, quantity, unit, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind("inst-2")
        .bind(COURSE)
        .bind("X")
        .bind("digital")
        .bind("tiempo")
        .bind("s")
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        // El CHECK de b_model debe rechazar un valor fuera del conjunto permitido.
        let result = sqlx::query(
            "INSERT INTO instrument_scales (id, instrument_id, label, step, b_model, unit, position, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind("scale-2")
        .bind("inst-2")
        .bind("escala")
        .bind(0.1_f64)
        .bind("inventado")
        .bind("s")
        .bind(0_i64)
        .bind(now)
        .execute(&pool)
        .await;
        assert!(result.is_err());
    }
}
