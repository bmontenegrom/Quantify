use super::ensure_lab_group_columns;
use sqlx::SqlitePool;

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
    // Observaciones/comentarios libres del alumno sobre su propia entrega (opcional, cualquier
    // practica). No se gatea: se ve siempre, igual que los resultados finales del alumno.
    add_column_if_missing(pool, "submissions", "student_comment", "TEXT").await?;
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
    // Fórmulas de eje (x, y) del ajuste lineal, para prácticas `regresion_lineal`.
    add_column_if_missing(pool, "practices", "x_formula", "TEXT").await?;
    add_column_if_missing(pool, "practices", "y_formula", "TEXT").await?;
    // Motor D (Fase 15): cantidad de operadores de una práctica estadística (cada uno carga su
    // propia serie de las magnitudes repetidas). NULL o ≤1 = sin operadores (comportamiento actual).
    add_column_if_missing(pool, "practices", "operator_count", "INTEGER").await?;

    // Motor B (Fase 15): una práctica `curva` define una o varias curvas sobre el mismo barrido,
    // cada una con su par de fórmulas de eje y su flag de eje x logarítmico (p. ej. dos en Filtros).
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

    // Motor C (Fase 15): magnitud intermedia por punto en regresión/curva. Su `formula` se evalúa
    // por réplica de cada punto y se promedia → un valor por punto, disponible como símbolo en las
    // fórmulas de eje (p. ej. Q = V/t por réplica, promediado, para graficar h/Q² vs 1/Q).
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

    // Motor E (Fase 15): magnitud derivada **por punto, post-ajuste** en regresión. Su `formula` se
    // evalúa en cada punto con las magnitudes/intermedias del punto + slope/intercept + los
    // mensurandos derivados, produciendo una columna por corrida (p. ej. el número de Reynolds).
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

    // Motor F (Fase 15): mensurando **agregado** escalar (un valor) en regresión, evaluado tras el
    // ajuste. Su `formula` puede usar escalares compartidos, slope/intercept, los mensurandos, los
    // agregados anteriores (encadenable), y los **valores de extremo** de cada magnitud por punto
    // (`X_first`, `X_first2`, `X_last`, `X_last2`). P. ej. Reynolds máx/mín con el primer/último par.
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
    // Réplicas esperadas por punto (solo magnitudes `repeated` en regresión/curva con grilla).
    add_column_if_missing(pool, "practice_quantities", "replicas_per_point", "INTEGER").await?;
    // Motor E (Fase 15): en regresión/curva, si `per_point` es true la magnitud se carga en la
    // tabla de la serie (un valor o réplicas por punto); si es false (o es `is_given`) es un escalar
    // compartido que se carga una sola vez. Default true = comportamiento previo (todo en la serie).
    add_column_if_missing(
        pool,
        "practice_quantities",
        "per_point",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    // Índice de punto en análisis por puntos (regresión/curva). En estadístico/CSV queda 0;
    // `replicate_index` pasa a ser la réplica dentro del punto.
    add_column_if_missing(
        pool,
        "submission_measurements",
        "point_index",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    // Índice de operador en el estadístico con operadores (Motor D). 0 si no hay operadores.
    add_column_if_missing(
        pool,
        "submission_measurements",
        "operator_index",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

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

    // Resultado central que el alumno debe entregar (valor ± U), p. ej. `g` en el péndulo.
    // Editable por el docente desde la UI; acá solo se backfillea el obvio de cada práctica
    // sembrada (no-op para instalaciones nuevas, que ya siembran el flag en `seed_definitions`).
    add_column_if_missing(
        pool,
        "practice_results",
        "is_final",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    for (practice_id, symbol) in [
        ("p1-estadistica", "g"),
        ("p3-relajacion", "tau_exp"),
        ("p3-relajacion-desfasaje", "tau"),
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

    // Igual que `practice_results.is_final`, pero para mensurandos agregados (Motor F): habilita
    // "Mis cálculos"/comparación alumno-vs-automático para agregados puntuales (p. ej. Re_max en
    // Fluidos II). El backfill para prácticas ya sembradas vive en `seed_fluidos2` (auto-curación
    // en cada boot), no acá.
    add_column_if_missing(
        pool,
        "practice_aggregates",
        "is_final",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    // `has_uncertainty` (magnitudes dadas y mensurandos): generaliza el antiguo hack de ocultar la
    // U de ciertos símbolos a mano en el frontend (`RESULTS_WITHOUT_U`). En `false`, la magnitud
    // dada no pide U al alumno (queda en 0, sin campo) y el mensurando se muestra sin ±U aunque el
    // valor propagado de fondo no sea exactamente cero (p. ej. `Q`, que sí usa la incertidumbre del
    // período). Default `true` = comportamiento previo (con incertidumbre).
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
    // Backfill: migra los símbolos que hoy dependen del hack `RESULTS_WITHOUT_U` del frontend.
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

    // Magnitud opcional (Motor... ninguno: entrega normal con campos opcionales, p. ej. operadores
    // 2 y 3 de p1-estadistica): si no tiene lecturas, no bloquea el envío del formulario.
    add_column_if_missing(
        pool,
        "practice_quantities",
        "optional",
        "INTEGER NOT NULL DEFAULT 0",
    )
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
