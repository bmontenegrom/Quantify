use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use rand_core::OsRng;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row, SqlitePool};
use std::{collections::HashMap, env, path::PathBuf, sync::Mutex};
use uuid::Uuid;

pub use crate::courses::*;
pub use crate::sessions::*;
pub use crate::submissions::*;
pub use crate::users::*;

/// Registro de intentos fallidos de login por email (para rate-limiting).
#[derive(Debug, Default)]
pub struct AttemptInfo {
    /// Número de intentos fallidos consecutivos desde el último éxito.
    pub count: u8,
    /// Si está seteado, no se permiten logins hasta este instante.
    pub blocked_until: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub upload_dir: PathBuf,
    /// Clave secreta para derivar los tokens CSRF (SHA-256 del token de sesión).
    /// Se lee de `APP_SECRET_KEY` o se genera aleatoriamente al arranque.
    pub secret_key: String,
    /// Si es `true`, la cookie de sesión incluye el flag `Secure` (requerido con TLS).
    /// Se activa con `APP_SECURE_COOKIES=true`.
    pub secure_cookies: bool,
    /// Mapa email → intentos de login fallidos (rate-limiting en memoria).
    pub login_attempts: std::sync::Arc<Mutex<HashMap<String, AttemptInfo>>>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Practice {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Tipo de análisis: `estadistico` o `regresion_lineal`. (El kind `relajacion_exponencial`
    /// se eliminó: τ se obtiene por medida directa y por desfasaje, ambas cubiertas.)
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
    /// Tolerancia máxima aceptable como diferencia porcentual |Δ%| entre el valor del alumno y
    /// el automático. `None` = sin veredicto. Configurable por el docente por mensurando.
    pub tolerance: Option<f64>,
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

    // Tolerancia porcentual configurable por el docente para el veredicto de comparación.
    add_column_if_missing(pool, "practice_results", "tolerance", "REAL").await?;

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
        // inalcanzable: params default + SaltString válido nunca fallan
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
pub(crate) fn digest_password(salt: &str, password: &str) -> String {
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

    const STUDENT: &str = "estudiante@quantify.local";
    const COURSE: &str = "fisica-experimental-i-2026";
    const GROUP: &str = "fisica-exp-i-grupo-1";

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
