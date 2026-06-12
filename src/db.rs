use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::Utc;
use rand_core::OsRng;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row, SqlitePool};
use std::{env, path::PathBuf};
use uuid::Uuid;

pub use crate::courses::*;
pub use crate::sessions::*;
pub use crate::submissions::*;
pub use crate::users::*;

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
    add_column_if_missing(pool, "submissions", "entry_mode", "TEXT").await?;
    add_column_if_missing(
        pool,
        "submissions",
        "results_visible_to_student",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(pool, "submissions", "measurement_meta_json", "TEXT").await?;
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

    add_column_if_missing(
        pool,
        "practice_quantities",
        "is_given",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(pool, "submission_measurements", "value_u", "REAL").await?;

    add_column_if_missing(pool, "submissions", "table_number", "INTEGER").await?;
    add_column_if_missing(
        pool,
        "courses",
        "acceptance_window_hours",
        "REAL NOT NULL DEFAULT 4",
    )
    .await?;

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

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_submissions_report_unique
        ON submissions(practice_id, group_id, table_number)
        WHERE table_number IS NOT NULL
        "#,
    )
    .execute(pool)
    .await?;

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

/// Siembra las prácticas del primer bloque de Física 103 en la tabla `practices` (id, nombre,
/// descripción, tipo de análisis y, en regresión, las fórmulas de eje). Idempotente: usa
/// `ON CONFLICT(id) DO NOTHING`, así que no pisa ediciones del docente entre reinicios.
pub async fn seed_practices(pool: &SqlitePool) -> anyhow::Result<()> {
    let practices = [
        (
            "p1-estadistica",
            "Tratamiento Estadistico - Pendulo Simple",
            "Medicion del periodo T con replicas (cronometro), longitud L dada por catedra; incertidumbres tipo A y B, calculo indirecto de g = 4*pi^2*L/T^2.",
            "estadistico",
            None,
            None,
        ),
        (
            "p2-serie",
            "CC - Circuito en Serie",
            "Circuito en serie: R1, R2 y R3 en serie con RA (resistencia interna del amperimetro). I y caidas de tension por leyes de circuito.",
            "estadistico",
            None,
            None,
        ),
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

/// Resultado de verificar una contraseña contra su hash almacenado.
pub(crate) enum VerifyResult {
    /// Contraseña incorrecta.
    Invalid,
    /// Contraseña correcta; el hash ya está en formato Argon2.
    Valid,
    /// Contraseña correcta; el hash estaba en el formato SHA-256 legacy y debe actualizarse.
    ValidNeedsRehash(String),
}

/// Genera un hash Argon2id de la contraseña con salt aleatorio.
pub(crate) fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hash con params default y salt OsRng nunca falla")
        .to_string()
}

/// Verifica una contraseña contra su hash almacenado.
///
/// Soporta el formato legacy SHA-256 (`salt:hex`) y el nuevo formato Argon2id (`$argon2id$…`).
/// Si el hash es legacy y la contraseña es correcta, devuelve `ValidNeedsRehash` con el nuevo
/// hash para que el llamador pueda migrar el registro transparentemente.
pub(crate) fn verify_password(password: &str, stored_hash: &str) -> VerifyResult {
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
pub(crate) fn clean_zero(value: f64) -> f64 {
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
        sqlx::query(
            "UPDATE practices SET analysis_kind = 'regresion_lineal' WHERE id = 'p1-estadistica'",
        )
        .execute(&pool)
        .await
        .unwrap();
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

        let groups = groups_for_course(&pool, COURSE).await.unwrap();
        assert!(groups.iter().any(|g| g.name == "General"));
    }

    #[tokio::test]
    async fn add_and_remove_group_member() {
        let (pool, _dir) = seeded().await;
        let teacher = find_user(&pool, TEACHER).await;
        let student = find_user(&pool, STUDENT).await;

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

        assert!(
            user_can_submit(&pool, &teacher, COURSE, GROUP, "p1-estadistica")
                .await
                .unwrap()
        );
        assert!(
            user_can_submit(&pool, &student, COURSE, GROUP, "p1-estadistica")
                .await
                .unwrap()
        );
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
        assert!(reviewed.results_visible_to_student);

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

        assert!(student_results_for(&pool, &id).await.unwrap().is_empty());

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
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].symbol, "Q");
        assert!((saved[0].value - 11.0).abs() < 1e-12);
        assert_eq!(saved[0].u_expanded, Some(0.5));
        assert_eq!(saved[1].symbol, "R");
        assert_eq!(saved[1].u_expanded, None);

        let detail = submission_detail(&pool, &id).await.unwrap().unwrap();
        assert_eq!(detail.student_results.len(), 2);

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
        .bind(Option::<f64>::None)
        .bind(0.05_f64)
        .bind(Some(0.05_f64))
        .bind(Option::<f64>::None)
        .bind(Option::<f64>::None)
        .bind("apreciacion")
        .bind(Option::<f64>::None)
        .bind(Option::<f64>::None)
        .bind(Option::<f64>::None)
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
