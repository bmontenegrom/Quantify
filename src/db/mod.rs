use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use std::{collections::HashMap, env, path::PathBuf, sync::Mutex};
use uuid::Uuid;

pub use crate::courses::*;
pub use crate::sessions::*;
pub use crate::submissions::*;
pub use crate::users::*;

mod password;
mod schema;

#[allow(unused_imports)]
pub(crate) use password::{
    digest_password, hash_password, verify_password, VerifyResult, MIN_PASSWORD_LEN,
};
pub use schema::migrate;
pub(crate) use schema::{mark_migration_applied, migration_applied, DISPLAY_SYMBOL_TABLES};

pub const MAX_LOGIN_ATTEMPTS: u8 = 5;
pub const LOGIN_BLOCK_MINUTES: i64 = 15;

#[derive(Debug, Default)]
pub struct AttemptInfo {
    pub count: u8,
    pub blocked_until: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
}

impl AttemptInfo {
    /// Indica si el email está bloqueado en el instante `now`. Si el bloqueo ya
    ///
    /// ```
    /// use chrono::{Duration, Utc};
    /// use quantify::db::AttemptInfo;
    /// let now = Utc::now();
    /// let mut info = AttemptInfo::default();
    /// // Sin bloqueo: nunca está bloqueado.
    /// assert!(!info.is_blocked(now));
    /// // Bloqueo vigente.
    /// info.blocked_until = Some(now + Duration::minutes(15));
    /// info.count = 5;
    /// assert!(info.is_blocked(now));
    /// // Bloqueo expirado: se resetea y deja de estar bloqueado.
    /// assert!(!info.is_blocked(now + Duration::minutes(16)));
    /// assert_eq!(info.count, 0);
    /// ```
    pub fn is_blocked(&mut self, now: DateTime<Utc>) -> bool {
        match self.blocked_until {
            Some(until) if until > now => true,
            Some(_) => {
                self.blocked_until = None;
                self.count = 0;
                false
            }
            None => false,
        }
    }

    /// Registra un intento fallido en `now`; al alcanzar [`MAX_LOGIN_ATTEMPTS`] activa
    ///
    /// ```
    /// use chrono::Utc;
    /// use quantify::db::{AttemptInfo, MAX_LOGIN_ATTEMPTS};
    /// let now = Utc::now();
    /// let mut info = AttemptInfo::default();
    /// for _ in 0..MAX_LOGIN_ATTEMPTS - 1 {
    ///     info.register_failure(now);
    ///     assert!(info.blocked_until.is_none());
    /// }
    /// info.register_failure(now);
    /// assert!(info.blocked_until.is_some());
    /// ```
    pub fn register_failure(&mut self, now: DateTime<Utc>) {
        self.count = self.count.saturating_add(1);
        self.last_seen = Some(now);
        if self.count >= MAX_LOGIN_ATTEMPTS {
            self.blocked_until = Some(now + chrono::Duration::minutes(LOGIN_BLOCK_MINUTES));
        }
    }
}

/// Elimina del mapa de rate-limiting las entradas inactivas: sin bloqueo vigente y
///
/// ```
/// use std::collections::HashMap;
/// use chrono::{Duration, Utc};
/// use quantify::db::{purge_expired_attempts, AttemptInfo};
/// let now = Utc::now();
/// let mut map = HashMap::new();
/// // Entrada vieja e inactiva → se purga.
/// map.insert("viejo@x".to_string(), AttemptInfo {
///     count: 1,
///     blocked_until: None,
///     last_seen: Some(now - Duration::minutes(20)),
/// });
/// // Entrada con bloqueo vigente → se conserva.
/// map.insert("bloqueado@x".to_string(), AttemptInfo {
///     count: 5,
///     blocked_until: Some(now + Duration::minutes(10)),
///     last_seen: Some(now - Duration::minutes(1)),
/// });
/// purge_expired_attempts(&mut map, now);
/// assert!(!map.contains_key("viejo@x"));
/// assert!(map.contains_key("bloqueado@x"));
/// ```
pub fn purge_expired_attempts(map: &mut HashMap<String, AttemptInfo>, now: DateTime<Utc>) {
    let window = chrono::Duration::minutes(LOGIN_BLOCK_MINUTES);
    map.retain(|_, info| {
        let blocked = info.blocked_until.map(|until| until > now).unwrap_or(false);
        let recent = info
            .last_seen
            .map(|seen| now - seen < window)
            .unwrap_or(false);
        blocked || recent
    });
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub upload_dir: PathBuf,
    pub secret_key: String,
    pub secure_cookies: bool,
    pub login_attempts: std::sync::Arc<Mutex<HashMap<String, AttemptInfo>>>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Practice {
    pub id: String,
    pub name: String,
    pub description: String,
    pub analysis_kind: Option<String>,
    pub status: String,
}

pub(crate) const PRACTICE_COLS: &str = "id, name, description, analysis_kind, status";

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, FromRow)]
#[serde(default)]
pub struct PracticeQuantity {
    pub id: String,
    pub practice_id: String,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    #[serde(default = "default_true")]
    pub repeated: bool,
    pub quantity: Option<String>,
    pub position: i64,
    pub is_given: bool,
    pub replicas_per_point: Option<i64>,
    #[serde(default = "default_true")]
    pub per_point: bool,
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
    pub optional: bool,
    #[sqlx(default)]
    pub default_value: Option<f64>,
    #[sqlx(default)]
    pub slot_count: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, FromRow)]
#[serde(default)]
pub struct PracticeResult {
    pub id: String,
    pub practice_id: String,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    pub position: i64,
    pub tolerance: Option<f64>,
    pub is_final: bool,
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Instrument {
    pub id: String,
    pub course_id: String,
    pub name: String,
    pub kind: String,
    pub quantity: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct InstrumentScale {
    pub id: String,
    pub instrument_id: String,
    pub label: String,
    pub full_scale: Option<f64>,
    pub step: f64,
    pub appreciation: Option<f64>,
    pub internal_res: Option<f64>,
    pub internal_res_u: Option<f64>,
    pub b_model: String,
    pub spec_pct_reading: Option<f64>,
    pub spec_step_coeff: Option<f64>,
    pub spec_fixed: Option<f64>,
    pub u_cal_pct: f64,
    pub u_cal_fixed: f64,
    pub unit: String,
    pub position: i64,
}

pub async fn next_position(
    pool: &SqlitePool,
    table: &str,
    scope_col: &str,
    scope_val: &str,
) -> anyhow::Result<i64> {
    let query =
        format!("SELECT COALESCE(MAX(position), 0) + 1 FROM {table} WHERE {scope_col} = ?1");
    let position: (i64,) = sqlx::query_as(&query)
        .bind(scope_val)
        .fetch_one(pool)
        .await?;
    Ok(position.0)
}

pub async fn run_startup_seeds(
    pool: &SqlitePool,
    demo_data: bool,
    admin_password: Option<String>,
) -> anyhow::Result<()> {
    seed_practices(pool).await?;
    seed_course(pool).await?;
    crate::instruments::seed_instruments(pool, TEST_COURSE_ID).await?;
    crate::practices::seed_definitions(pool).await?;
    seed_initial_accounts(pool, demo_data, admin_password).await
}

async fn seed_initial_accounts(
    pool: &SqlitePool,
    demo_data: bool,
    admin_password: Option<String>,
) -> anyhow::Result<()> {
    let (has_users,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(pool)
        .await?;
    if has_users {
        if demo_data {
            tracing::info!("SEED_DEMO_DATA se ignora: la base ya tiene usuarios");
        }
        return Ok(());
    }
    let admin = admin_account(demo_data, admin_password)?;
    let demo = demo_data.then(demo_accounts);
    let mut tx = pool.begin().await?;
    insert_accounts(&mut tx, &[admin]).await?;
    if let Some(accounts) = &demo {
        insert_accounts(&mut tx, accounts).await?;
        enroll_test_student(&mut tx).await?;
        insert_test_submissions(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

type SeedAccount = (&'static str, &'static str, &'static str, String);

pub fn env_password(variable: &str) -> Option<String> {
    env::var(variable)
        .ok()
        .filter(|password| !password.trim().is_empty())
}

fn development_password(variable: &str, development_default: &str) -> String {
    tracing::warn!(
        "{variable} no está definida: la cuenta inicial usa la contraseña de desarrollo"
    );
    development_default.to_string()
}

fn admin_account(demo_data: bool, password: Option<String>) -> anyhow::Result<SeedAccount> {
    let password = match password {
        Some(password) if password.len() < MIN_PASSWORD_LEN => {
            anyhow::bail!("SEED_ADMIN_PASSWORD debe tener al menos {MIN_PASSWORD_LEN} caracteres.")
        }
        Some(password) => password,
        None if demo_data => development_password("SEED_ADMIN_PASSWORD", "admin123"),
        None => anyhow::bail!(
            "Definí SEED_ADMIN_PASSWORD: es la contraseña del admin inicial y es obligatoria \
             en una base nueva."
        ),
    };
    Ok((
        "admin@quantify.local",
        "Administrador",
        "admin",
        hash_password(&password),
    ))
}

fn demo_account(
    email: &'static str,
    display_name: &'static str,
    role: &'static str,
    variable: &str,
    development_default: &str,
) -> SeedAccount {
    let password = env_password(variable)
        .unwrap_or_else(|| development_password(variable, development_default));
    (email, display_name, role, hash_password(&password))
}

fn demo_accounts() -> [SeedAccount; 2] {
    [
        demo_account(
            "docente@quantify.local",
            "Docente de prueba",
            "docente",
            "SEED_TEACHER_PASSWORD",
            "docente123",
        ),
        demo_account(
            "estudiante@quantify.local",
            "Estudiante de prueba",
            "estudiante",
            "SEED_STUDENT_PASSWORD",
            "estudiante123",
        ),
    ]
}

async fn insert_accounts(
    conn: &mut sqlx::SqliteConnection,
    accounts: &[SeedAccount],
) -> anyhow::Result<()> {
    for (email, display_name, role, password_hash) in accounts {
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, display_name, role, password_hash, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(*email)
        .bind(*email)
        .bind(*display_name)
        .bind(*role)
        .bind(password_hash)
        .bind(Utc::now())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
pub async fn seed_users(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut accounts = vec![admin_account(true, None)?];
    accounts.extend(demo_accounts());
    let mut conn = pool.acquire().await?;
    insert_accounts(&mut conn, &accounts).await
}

pub(crate) const P3_DESCRIPTION: &str = "Tiempo de relajacion tau de un circuito RC en una sola entrega: parte 1, medida directa del tiempo de semidescarga; parte 2, desfasaje con ajuste lineal de tg(phi) contra omega = 2*pi*f.";

pub async fn seed_practices(pool: &SqlitePool) -> anyhow::Result<()> {
    let practices = [
        (
            "p1-estadistica",
            "Tratamiento estadístico de Datos",
            "Pendulo simple con hasta 3 operadores independientes: cada uno mide su propia serie de periodos (T1/T2/T3) y determina su g; incertidumbres tipo A y B, calculo indirecto de g = 4*pi^2*L/T^2.",
            "estadistico",
            None,
            None,
        ),
        (
            "p2-cc",
            "Corriente continua",
            "Circuitos de corriente continua en una sola entrega: serie, paralelo (R1, R2, R3 compartidas) y curva de potencia P = I^2*R sobre carga variable.",
            "curva",
            None,
            None,
        ),
        (
            "p3-relajacion",
            "Relajacion Exponencial",
            P3_DESCRIPTION,
            "regresion_lineal",
            Some("2*pi*f"),
            Some("b / math::sqrt(a*a - b*b)"),
        ),
        (
            "fluidos-1",
            "Fluidos I",
            "Viscosidad del agua por la ley de Hagen-Poiseuille: ajuste lineal de h/Q^2 contra 1/Q (Q = caudal medio por altura).",
            "regresion_lineal",
            Some("1/Q"),
            Some("h/(Q*Q)"),
        ),
        (
            "viscosidad",
            "Viscosidad",
            "Viscosidad de la glicerina por la ley de Stokes: ajuste lineal de v_lim contra R^2 (v_lim = dx / tiempo medio por esfera).",
            "regresion_lineal",
            Some("R^2"),
            Some("dx/t"),
        ),
        (
            "fluidos-2",
            "Fluidos II",
            "Perdidas energeticas en la descarga de un recipiente por un capilar: ajuste lineal de t contra (sqrt(h_max) - sqrt(h)). M_medio sale de la pendiente; Reynolds max/min y M teorico son mensurandos agregados.",
            "regresion_lineal",
            Some("math::sqrt(h_max) - math::sqrt(h)"),
            Some("t"),
        ),
        (
            "filtros",
            "Filtros",
            "Barrido en frecuencia de un circuito RLC: respuesta en amplitud VR/Vg y desfasaje phi = asin(b/a) contra la frecuencia angular omega = 2*pi*f (eje log).",
            "curva",
            None,
            None,
        ),
        (
            "ca-rlc",
            "Corriente Alterna (RLC)",
            "Circuito RLC serie en corriente alterna: impedancia, corriente y tensiones teoricas vs experimentales por canal (R, C, L), y desfasaje phi = asin(b/a) por canal contra el teorico.",
            "estadistico",
            None,
            None,
        ),
        (
            "hidrostatica",
            "Hidrostatica y Tension Superficial",
            "Densidad de una pieza de goma por el principio de Arquimedes con balanza de Mohr (empuje y densidad por medida, promediados) y tension superficial de una solucion jabonosa por el metodo de Wilhelmy.",
            "estadistico",
            None,
            None,
        ),
    ];

    for (id, name, description, analysis_kind, x_formula, y_formula) in practices {
        sqlx::query(
            r#"
            INSERT INTO practices (id, name, description, analysis_kind, x_formula, y_formula)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO NOTHING
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

    for legacy_id in ["p2-serie", "p2-corriente-continua", "p2-potencia"] {
        let has_submissions: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE practice_id = ?1")
                .bind(legacy_id)
                .fetch_one(pool)
                .await?;
        if has_submissions.0 == 0 {
            sqlx::query("DELETE FROM practices WHERE id = ?1")
                .bind(legacy_id)
                .execute(pool)
                .await?;
        }
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

const TEST_COURSE_ID: &str = "fisica-experimental-i-2026";
const TEST_GROUP_ID: &str = "fisica-exp-i-grupo-1";

async fn seed_course(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO courses (id, name, term, active, created_at)
        VALUES (?1, 'Fisica Experimental I', '2026', 1, ?2)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(TEST_COURSE_ID)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO lab_groups (id, course_id, name, table_count, group_type, created_at)
        VALUES (?1, ?2, 'Grupo 1', 4, 'regular', ?3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(TEST_GROUP_ID)
    .bind(TEST_COURSE_ID)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    for practice in [
        "p1-estadistica",
        "p2-cc",
        "p3-relajacion",
        "fluidos-1",
        "viscosidad",
        "fluidos-2",
        "filtros",
        "ca-rlc",
        "hidrostatica",
    ] {
        sqlx::query(
            r#"
            INSERT INTO course_practices (course_id, practice_id, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(course_id, practice_id) DO NOTHING
            "#,
        )
        .bind(TEST_COURSE_ID)
        .bind(practice)
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
pub async fn seed_academic(pool: &SqlitePool) -> anyhow::Result<()> {
    seed_course(pool).await?;
    let mut conn = pool.acquire().await?;
    enroll_test_student(&mut conn).await
}

async fn test_student_id(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<Option<String>> {
    let student: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = 'estudiante@quantify.local'")
            .fetch_optional(&mut *conn)
            .await?;
    Ok(student.map(|(id,)| id))
}

async fn test_group_id(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<Option<String>> {
    let group: Option<(String,)> = sqlx::query_as("SELECT id FROM lab_groups WHERE id = ?1")
        .bind(TEST_GROUP_ID)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(group.map(|(id,)| id))
}

async fn enroll_test_student(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<()> {
    let Some(student_id) = test_student_id(conn).await? else {
        return Ok(());
    };
    enroll_in_course(conn, TEST_COURSE_ID, &student_id).await?;
    let Some(group_id) = test_group_id(conn).await? else {
        return Ok(());
    };
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
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[cfg(test)]
pub async fn seed_submissions(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut conn = pool.acquire().await?;
    insert_test_submissions(&mut conn).await
}

async fn insert_test_submissions(conn: &mut sqlx::SqliteConnection) -> anyhow::Result<()> {
    let (has_submissions,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM submissions)")
        .fetch_one(&mut *conn)
        .await?;
    if has_submissions {
        return Ok(());
    }
    let Some(student_id) = test_student_id(conn).await? else {
        return Ok(());
    };
    let Some(group_id) = test_group_id(conn).await? else {
        return Ok(());
    };
    let now = Utc::now();

    let submissions: &[(&str, &str, &str)] = &[
        ("p1-estadistica", "pendiente", ""),
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
                entry_mode, score, teacher_comment, reviewed_at, practice_version_id
            )
            SELECT
                ?1, u.display_name, g.name, c.name, ?5,
                '(formulario)', '', ?6, ?7, ?8, u.id, c.id, g.id,
                'form', ?9, ?10, ?11,
                (SELECT current_version_id FROM practices WHERE id = ?5)
            FROM users u, lab_groups g, courses c
            WHERE u.id = ?2 AND g.id = ?3 AND c.id = ?4
            "#,
        )
        .bind(&submission_id)
        .bind(&student_id)
        .bind(&group_id)
        .bind(TEST_COURSE_ID)
        .bind(practice_id)
        .bind(&analysis_json)
        .bind(status)
        .bind(now)
        .bind(score)
        .bind(comment)
        .bind(reviewed_at)
        .execute(&mut *conn)
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
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}

pub async fn practices(pool: &SqlitePool) -> anyhow::Result<Vec<Practice>> {
    let rows = sqlx::query_as::<_, Practice>(&format!(
        "SELECT {PRACTICE_COLS} FROM practices ORDER BY name"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn practices_for_role(pool: &SqlitePool, role: &str) -> anyhow::Result<Vec<Practice>> {
    let mut all = practices(pool).await?;
    if !matches!(role, "docente" | "admin") {
        all.retain(|p| p.status == "publicada");
    }
    Ok(all)
}

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
    use chrono::Duration;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    #[test]
    fn rate_limit_blocks_after_max_attempts() {
        let now = Utc::now();
        let mut info = AttemptInfo::default();
        for _ in 0..MAX_LOGIN_ATTEMPTS - 1 {
            info.register_failure(now);
            assert!(!info.is_blocked(now));
        }
        info.register_failure(now);
        assert!(info.is_blocked(now));
    }

    #[test]
    fn rate_limit_resets_after_block_expires() {
        let now = Utc::now();
        let mut info = AttemptInfo::default();
        for _ in 0..MAX_LOGIN_ATTEMPTS {
            info.register_failure(now);
        }
        assert!(info.is_blocked(now));
        let later = now + Duration::minutes(LOGIN_BLOCK_MINUTES + 1);
        assert!(!info.is_blocked(later));
        assert_eq!(info.count, 0);
        assert!(info.blocked_until.is_none());
        info.register_failure(later);
        assert!(!info.is_blocked(later));
    }

    #[test]
    fn purge_keeps_blocked_and_recent_drops_stale() {
        let now = Utc::now();
        let mut map = HashMap::new();
        map.insert(
            "stale@x".to_string(),
            AttemptInfo {
                count: 2,
                blocked_until: None,
                last_seen: Some(now - Duration::minutes(LOGIN_BLOCK_MINUTES + 5)),
            },
        );
        map.insert(
            "recent@x".to_string(),
            AttemptInfo {
                count: 1,
                blocked_until: None,
                last_seen: Some(now - Duration::minutes(1)),
            },
        );
        map.insert(
            "blocked@x".to_string(),
            AttemptInfo {
                count: MAX_LOGIN_ATTEMPTS,
                blocked_until: Some(now + Duration::minutes(5)),
                last_seen: Some(now - Duration::minutes(LOGIN_BLOCK_MINUTES + 5)),
            },
        );
        purge_expired_attempts(&mut map, now);
        assert!(!map.contains_key("stale@x"));
        assert!(map.contains_key("recent@x"));
        assert!(map.contains_key("blocked@x"));
    }

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
        migrate(&pool).await.unwrap();
    }

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        let (n,): (i64,) = sqlx::query_as(sql).fetch_one(pool).await.unwrap();
        n
    }

    #[tokio::test]
    async fn seed_users_creates_three_and_is_idempotent() {
        let (pool, _dir) = pool().await;
        seed_users(&pool).await.unwrap();
        seed_users(&pool).await.unwrap();
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM users").await, 3);
    }

    #[tokio::test]
    async fn an_interrupted_first_start_retries_all_the_test_data() {
        let (pool, _dir) = pool().await;
        let accounts = "SELECT COUNT(*) FROM users";
        let submissions = "SELECT COUNT(*) FROM submissions";
        sqlx::query(
            "CREATE TRIGGER stop_seed BEFORE INSERT ON report_members \
             BEGIN SELECT RAISE(ABORT, 'interrumpido'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(run_startup_seeds(&pool, true, None).await.is_err());
        assert_eq!(
            count(&pool, accounts).await,
            0,
            "no debían quedar cuentas a medias"
        );
        assert_eq!(
            count(&pool, submissions).await,
            0,
            "no debían quedar entregas a medias"
        );

        sqlx::query("DROP TRIGGER stop_seed")
            .execute(&pool)
            .await
            .unwrap();
        run_startup_seeds(&pool, true, None).await.unwrap();
        assert_eq!(count(&pool, accounts).await, 3);
        assert_eq!(count(&pool, submissions).await, 2);
    }

    #[tokio::test]
    async fn without_demo_data_a_new_base_only_has_the_admin() {
        let (pool, _dir) = pool().await;
        run_startup_seeds(&pool, false, Some("secreta123".into()))
            .await
            .unwrap();
        let users: Vec<(String, String)> = sqlx::query_as("SELECT email, password_hash FROM users")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].0, "admin@quantify.local");
        assert!(matches!(
            verify_password("secreta123", &users[0].1),
            VerifyResult::Valid
        ));
        for table in ["submissions", "course_members", "group_members"] {
            assert_eq!(
                count(&pool, &format!("SELECT COUNT(*) FROM {table}")).await,
                0,
                "{table} debía quedar vacía"
            );
        }
    }

    #[tokio::test]
    async fn without_demo_data_a_new_base_requires_the_admin_password() {
        let (pool, _dir) = pool().await;
        assert!(run_startup_seeds(&pool, false, None).await.is_err());
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM users").await, 0);
    }

    #[tokio::test]
    async fn a_short_admin_password_is_rejected() {
        let (pool, _dir) = pool().await;
        assert!(run_startup_seeds(&pool, false, Some("corta".into()))
            .await
            .is_err());
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM users").await, 0);
    }

    #[tokio::test]
    async fn renaming_the_seeded_group_does_not_break_startup() {
        let (pool, _dir) = pool().await;
        run_startup_seeds(&pool, true, None).await.unwrap();
        sqlx::query("UPDATE lab_groups SET name = 'Mesa A' WHERE name = 'Grupo 1'")
            .execute(&pool)
            .await
            .unwrap();
        for sql in [
            "DELETE FROM submissions",
            "DELETE FROM group_members",
            "DELETE FROM users",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        run_startup_seeds(&pool, true, None).await.unwrap();
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM users").await, 3);
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM group_members m JOIN lab_groups g ON g.id = m.group_id WHERE g.name = 'Mesa A'"
            )
            .await,
            1,
            "el estudiante debía quedar en el grupo renombrado"
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM submissions WHERE group_name = 'Mesa A'"
            )
            .await,
            2
        );
    }

    #[tokio::test]
    async fn restarting_does_not_bring_back_test_accounts_or_submissions() {
        let (pool, _dir) = pool().await;
        run_startup_seeds(&pool, true, None).await.unwrap();
        let submissions = "SELECT COUNT(*) FROM submissions";
        let teacher = "SELECT COUNT(*) FROM users WHERE email = 'docente@quantify.local'";
        let enrollments = "SELECT COUNT(*) FROM group_members m \
             JOIN users u ON u.id = m.user_id WHERE u.email = 'estudiante@quantify.local'";
        assert!(
            count(&pool, submissions).await > 0,
            "la base nueva trae entregas de prueba"
        );
        assert_eq!(count(&pool, teacher).await, 1);
        assert!(
            count(&pool, enrollments).await > 0,
            "la base nueva inscribe al estudiante"
        );

        for sql in [
            "DELETE FROM submissions",
            "DELETE FROM group_members",
            "DELETE FROM users WHERE email = 'docente@quantify.local'",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        migrate(&pool).await.unwrap();
        run_startup_seeds(&pool, true, None).await.unwrap();

        assert_eq!(
            count(&pool, submissions).await,
            0,
            "volvieron las entregas de prueba"
        );
        assert_eq!(count(&pool, teacher).await, 0, "volvió la cuenta de prueba");
        assert_eq!(
            count(&pool, enrollments).await,
            0,
            "volvió a inscribirse el estudiante"
        );
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
        assert!(ids.contains(&"p2-cc".to_string()));
        assert!(ids.contains(&"p3-relajacion".to_string()));
        assert!(!ids.contains(&"p3-relajacion-desfasaje".to_string()));
        assert!(!ids.contains(&"p2-serie".to_string()));
        assert!(!ids.contains(&"p2-corriente-continua".to_string()));
        assert!(!ids.contains(&"p2-potencia".to_string()));
    }

    #[tokio::test]
    async fn seed_practices_does_not_clobber_edits() {
        let (pool, _dir) = pool().await;
        seed_practices(&pool).await.unwrap();
        sqlx::query(
            "UPDATE practices SET analysis_kind = 'regresion_lineal', name = 'Pendulo (editado)' \
             WHERE id = 'p1-estadistica'",
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
        assert_eq!(p.name, "Pendulo (editado)");
        assert_eq!(p.status, "publicada");
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
            user_can_submit(&pool, &student, COURSE, GROUP, "p3-relajacion")
                .await
                .unwrap()
        );
        assert_eq!(practices_for_course(&pool, COURSE).await.unwrap().len(), 9);

        assert!(disable_course_practice(&pool, COURSE, "p1-estadistica")
            .await
            .unwrap());
        seed_academic(&pool).await.unwrap();
        assert_eq!(practices_for_course(&pool, COURSE).await.unwrap().len(), 8);
        assert!(
            !user_can_submit(&pool, &student, COURSE, GROUP, "p1-estadistica")
                .await
                .unwrap()
        );
        enable_course_practice(
            &pool,
            COURSE,
            SetCoursePractice {
                practice_id: "p1-estadistica".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(practices_for_course(&pool, COURSE).await.unwrap().len(), 9);
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

        let scale = crate::instruments::find_scale(&pool, "scale-1")
            .await
            .unwrap()
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
