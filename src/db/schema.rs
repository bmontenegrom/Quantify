use super::ensure_lab_group_columns;
use sqlx::SqlitePool;

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
    add_column_if_missing(pool, "submissions", "student_comment", "TEXT").await?;
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
    add_column_if_missing(pool, "practices", "operator_count", "INTEGER").await?;
    add_column_if_missing(
        pool,
        "practices",
        "status",
        "TEXT NOT NULL DEFAULT 'publicada'",
    )
    .await?;
    add_column_if_missing(pool, "practices", "created_by", "TEXT").await?;
    add_column_if_missing(pool, "practices", "created_at", "TEXT").await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_curves (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            position INTEGER NOT NULL DEFAULT 0,
            x_formula TEXT NOT NULL,
            y_formula TEXT NOT NULL,
            x_log INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_intermediates (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            position INTEGER NOT NULL DEFAULT 0,
            symbol TEXT NOT NULL,
            name TEXT NOT NULL,
            unit TEXT NOT NULL,
            formula TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_point_results (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            position INTEGER NOT NULL DEFAULT 0,
            symbol TEXT NOT NULL,
            name TEXT NOT NULL,
            unit TEXT NOT NULL,
            formula TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_aggregates (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            position INTEGER NOT NULL DEFAULT 0,
            symbol TEXT NOT NULL,
            name TEXT NOT NULL,
            unit TEXT NOT NULL,
            formula TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

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
    add_column_if_missing(pool, "practice_quantities", "replicas_per_point", "INTEGER").await?;
    add_column_if_missing(
        pool,
        "practice_quantities",
        "per_point",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    add_column_if_missing(
        pool,
        "submission_measurements",
        "point_index",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        pool,
        "submission_measurements",
        "operator_index",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    add_column_if_missing(
        pool,
        "instrument_scales",
        "u_cal_pct",
        "REAL NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        pool,
        "instrument_scales",
        "u_cal_fixed",
        "REAL NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(pool, "practice_quantities", "default_value", "REAL").await?;
    add_column_if_missing(pool, "practice_quantities", "slot_count", "INTEGER").await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_parts (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_sections (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            part_id TEXT REFERENCES practice_parts(id) ON DELETE SET NULL,
            title TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await?;
    add_column_if_missing(pool, "practice_parts", "analysis_kind", "TEXT").await?;
    for table in ["practice_parts", "practice_sections"] {
        add_column_if_missing(pool, table, "help_text", "TEXT NOT NULL DEFAULT ''").await?;
    }
    add_column_if_missing(
        pool,
        "practice_quantities",
        "removed",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_versions (
            id TEXT PRIMARY KEY,
            practice_id TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
            number INTEGER NOT NULL,
            definition_json TEXT NOT NULL,
            published_at TEXT NOT NULL,
            published_by TEXT,
            UNIQUE(practice_id, number)
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS practice_version_updates (
            id TEXT PRIMARY KEY,
            version_id TEXT NOT NULL REFERENCES practice_versions(id) ON DELETE CASCADE,
            updated_at TEXT NOT NULL,
            updated_by TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;
    add_column_if_missing(
        pool,
        "practices",
        "current_version_id",
        "TEXT REFERENCES practice_versions(id)",
    )
    .await?;
    add_column_if_missing(
        pool,
        "submissions",
        "practice_version_id",
        "TEXT REFERENCES practice_versions(id)",
    )
    .await?;
    for table in DISPLAY_SYMBOL_TABLES {
        add_column_if_missing(pool, table, "display_symbol", "TEXT NOT NULL DEFAULT ''").await?;
    }
    add_column_if_missing(
        pool,
        "practice_quantities",
        "label_mode",
        "TEXT NOT NULL DEFAULT 'nombre'",
    )
    .await?;
    add_column_if_missing(
        pool,
        "practice_quantities",
        "input_widget",
        "TEXT NOT NULL DEFAULT 'teclado'",
    )
    .await?;
    add_column_if_missing(
        pool,
        "practice_results",
        "compare_with",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    add_column_if_missing(
        pool,
        "practice_intermediates",
        "show_live",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        pool,
        "practice_intermediates",
        "on_means",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    for table in ["practice_quantities", "practice_results"] {
        add_column_if_missing(
            pool,
            table,
            "section_id",
            "TEXT REFERENCES practice_sections(id) ON DELETE SET NULL",
        )
        .await?;
    }
    add_column_if_missing(
        pool,
        "course_practices",
        "enabled",
        "BOOLEAN NOT NULL DEFAULT TRUE",
    )
    .await?;

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
        CREATE TABLE IF NOT EXISTS applied_migrations (
            name       TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        )
        "#,
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

    add_column_if_missing(pool, "practice_results", "tolerance", "REAL").await?;

    add_column_if_missing(
        pool,
        "practice_results",
        "is_final",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    add_column_if_missing(
        pool,
        "practice_aggregates",
        "is_final",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    add_column_if_missing(
        pool,
        "practice_quantities",
        "has_uncertainty",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    add_column_if_missing(
        pool,
        "practice_results",
        "has_uncertainty",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;

    add_column_if_missing(
        pool,
        "practice_quantities",
        "optional",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    backfill_result_flags(pool).await
}

const RESULT_FLAGS_BACKFILL: &str = "2026-11-backfill-is-final-y-has-uncertainty";

async fn backfill_result_flags(pool: &SqlitePool) -> anyhow::Result<()> {
    if migration_applied(pool, RESULT_FLAGS_BACKFILL).await? {
        return Ok(());
    }
    for (practice_id, symbol) in [
        ("p1-estadistica", "g"),
        ("p3-relajacion", "tau_exp"),
        ("fluidos-1", "mu"),
        ("viscosidad", "mu"),
        ("fluidos-2", "M_medio"),
    ] {
        sqlx::query(
            "UPDATE practice_results SET is_final = 1 WHERE practice_id = ?1 AND symbol = ?2",
        )
        .bind(practice_id)
        .bind(symbol)
        .execute(pool)
        .await?;
    }
    for (practice_id, symbol) in [
        ("p2-cc", "P_max_e"),
        ("p2-cc", "P_max_t"),
        ("p2-cc", "RP_max_e"),
        ("p2-cc", "RP_max_t"),
    ] {
        sqlx::query(
            "UPDATE practice_results SET has_uncertainty = 0 WHERE practice_id = ?1 AND symbol = ?2",
        )
        .bind(practice_id)
        .bind(symbol)
        .execute(pool)
        .await?;
    }
    mark_migration_applied(pool, RESULT_FLAGS_BACKFILL).await
}

pub(crate) async fn migration_applied(pool: &SqlitePool, name: &str) -> anyhow::Result<bool> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
            .bind(name)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

pub(crate) async fn mark_migration_applied(pool: &SqlitePool, name: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(name)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) const DISPLAY_SYMBOL_TABLES: [&str; 5] = [
    "practice_quantities",
    "practice_results",
    "practice_intermediates",
    "practice_point_results",
    "practice_aggregates",
];

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
