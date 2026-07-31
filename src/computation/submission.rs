//! Alta y edición de entregas por formulario: valida, calcula el análisis (vía [`super::engines::analyze`])
//! y persiste la entrega + sus mediciones.

use super::engines::analyze;
use super::MeasurementInput;
use crate::db::{self, AuthUser};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

/// Cuerpo para crear una entrega por formulario.
#[derive(Debug, Deserialize)]
pub struct FormSubmissionInput {
    pub course_id: String,
    pub group_id: String,
    pub practice_id: String,
    pub measurements: Vec<MeasurementInput>,
    /// Metadatos de depuración por magnitud (bins del histograma + valores descartados).
    /// Se persiste tal cual para que el docente lo vea; opcional.
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    /// Mesa del informe compartido. Si no se envía, se resuelve desde las asignaciones
    /// del alumno. Para docentes/admin es opcional (puede entregar sin mesa asignada).
    #[serde(default)]
    pub table_number: Option<i64>,
    /// Resultado(s) final(es) que el alumno entrega junto con la medición (opcional, p. ej. `g`).
    /// Se valida y persiste igual que `POST /submissions/{id}/student-results`.
    #[serde(default)]
    pub student_results: Vec<crate::db::StudentResultInput>,
    /// Observaciones/comentarios libres del alumno sobre su entrega (opcional, cualquier práctica).
    #[serde(default)]
    pub student_comment: Option<String>,
}

/// Persiste las mediciones de una entrega en `submission_measurements`. Cubre los tres modos
/// (según `point_based`, que indica si la entrega es por puntos — regresión/curva):
/// - **estadístico** (`point_based = false`): réplicas de una magnitud → `point_index = 0`,
///   `replicate_index = réplica`.
/// - **estadístico con operadores** (`operator_replicas`): `operator_index = operador`,
///   `replicate_index = réplica` (point_index = 0).
/// - **por puntos sin réplicas** (`point_based = true`, `values`): un valor por punto →
///   `point_index = punto`, `replicate_index = 0`.
/// - **por puntos con réplicas** (`point_replicas`): `point_index = punto`, `replicate_index = réplica`.
///
/// Los índices explícitos (`operator_index`/`point_index`, en vez de meter todo en
/// `replicate_index`) permiten reconstruir la serie al editar agrupando por operador/punto. Los
/// datos de cátedra (`given_u`) guardan su único valor con la U en `value_u`.
async fn insert_measurements(
    conn: &mut sqlx::SqliteConnection,
    submission_id: &str,
    measurements: &[MeasurementInput],
    point_based: bool,
) -> anyhow::Result<()> {
    for measurement in measurements {
        // Filas (operator_index, point_index, replicate_index, value, value_u) según el modo.
        let rows: Vec<(i64, i64, i64, f64, Option<f64>)> =
            if let Some(operators) = &measurement.operator_replicas {
                operators
                    .iter()
                    .enumerate()
                    .flat_map(|(o, reps)| {
                        reps.iter()
                            .enumerate()
                            .map(move |(r, &v)| (o as i64, 0i64, r as i64, v, None))
                    })
                    .collect()
            } else if let Some(groups) = &measurement.point_replicas {
                groups
                    .iter()
                    .enumerate()
                    .flat_map(|(p, reps)| {
                        reps.iter()
                            .enumerate()
                            .map(move |(r, &v)| (0i64, p as i64, r as i64, v, None))
                    })
                    .collect()
            } else if measurement.given_u.is_some() {
                measurement
                    .values
                    .first()
                    .map(|&v| vec![(0i64, 0i64, 0i64, v, measurement.given_u)])
                    .unwrap_or_default()
            } else if point_based {
                // Un valor por punto: el índice va en point_index (replicate_index = 0).
                measurement
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (0i64, i as i64, 0i64, v, None))
                    .collect()
            } else {
                // Réplicas estadísticas: un solo punto (0), el índice va en replicate_index.
                measurement
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (0i64, 0i64, i as i64, v, None))
                    .collect()
            };
        for (operator_index, point_index, replicate_index, value, value_u) in rows {
            sqlx::query(
                "INSERT INTO submission_measurements \
                 (id, submission_id, quantity_id, instrument_id, scale_id, \
                  operator_index, point_index, replicate_index, value, value_u) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(submission_id)
            .bind(&measurement.quantity_id)
            .bind(measurement.instrument_id.as_deref())
            .bind(measurement.scale_id.as_deref())
            .bind(operator_index)
            .bind(point_index)
            .bind(replicate_index)
            .bind(value)
            .bind(value_u)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// `true` si la práctica analiza **por puntos** (regresión o curva). Determina el layout de
/// persistencia (el índice del punto va en `point_index`, no en `replicate_index`). Se deriva del
/// `analysis_kind` declarado en la práctica —**no** del resultado del cálculo— para que el formato
/// almacenado no dependa de que el ajuste haya producido salida con los datos cargados (p.ej. un
/// punto único o valores no finitos dejarían `regression`/`scatter` en `None` sin dejar de ser una
/// entrega por puntos).
async fn is_point_based_practice(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
) -> anyhow::Result<bool> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT analysis_kind FROM practices WHERE id = ?1")
            .bind(practice_id)
            .fetch_optional(pool)
            .await?;
    Ok(matches!(
        row.and_then(|r| r.0).as_deref(),
        Some("regresion_lineal") | Some("curva")
    ))
}

/// Crea una entrega por formulario: calcula el análisis, inserta la entrega y sus mediciones
/// en una transacción, y devuelve el detalle. El usuario ya fue validado por el handler.
pub async fn create_form_submission(
    pool: &sqlx::SqlitePool,
    user: &AuthUser,
    input: FormSubmissionInput,
) -> anyhow::Result<db::SubmissionDetail> {
    let is_teacher = matches!(user.role.as_str(), "docente" | "admin");

    // Resolver mesa: prioridad input > practice_table_assignments > user_default_tables.
    // Para docentes/admin la mesa es opcional (pueden entregar sin mesa asignada).
    let table_number = if let Some(t) = input.table_number {
        Some(t)
    } else if !is_teacher {
        db::resolve_user_table(pool, &user.id, &input.group_id, &input.practice_id).await?
    } else {
        None
    };

    // Para alumnos: la mesa es obligatoria.
    if !is_teacher && table_number.is_none() {
        anyhow::bail!(
            "No tenés una mesa asignada para esta práctica. \
             Pedile al docente que te asigne una mesa."
        );
    }

    // Si hay mesa asignada, verificar que no exista ya un informe para (práctica, grupo, mesa).
    if let Some(t) = table_number {
        // Validar rango de la mesa.
        let table_count: Option<(i64,)> =
            sqlx::query_as("SELECT table_count FROM lab_groups WHERE id = ?1")
                .bind(&input.group_id)
                .fetch_optional(pool)
                .await?;
        if let Some((count,)) = table_count {
            if t < 1 || t > count {
                anyhow::bail!("El número de mesa {t} no es válido para este grupo (1..={count})");
            }
        }

        if db::find_existing_report(pool, &input.practice_id, &input.group_id, t)
            .await?
            .is_some()
        {
            anyhow::bail!(
                "Ya existe un informe para la mesa {t} en esta práctica. \
                 Si sos parte de esa mesa, aceptá la invitación desde tus notificaciones."
            );
        }
    }

    validate_student_results(pool, &input.practice_id, &input.student_results).await?;

    let analysis = analyze(pool, &input.practice_id, &input.measurements).await?;
    let point_based = is_point_based_practice(pool, &input.practice_id).await?;
    let analysis_json = serde_json::to_string(&analysis)?;
    let meta_json = match &input.meta {
        Some(value) => Some(serde_json::to_string(value)?),
        None => None,
    };
    let student_comment = input
        .student_comment
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    // Inserta la entrega resolviendo nombres denormalizados (igual que la variante CSV).
    let inserted = sqlx::query(
        r#"
        INSERT INTO submissions (
            id, student_name, group_name, course, practice_id, file_name, csv_path,
            analysis_json, status, submitted_at, submitted_by_user_id, course_id, group_id,
            entry_mode, measurement_meta_json, table_number, student_comment
        )
        SELECT
            ?1,
            u.display_name,
            g.name,
            c.name,
            ?5,
            '(formulario)',
            '',
            ?6,
            'pendiente',
            ?7,
            u.id,
            c.id,
            g.id,
            'form',
            ?8,
            ?9,
            ?10
        FROM users u, lab_groups g, courses c
        WHERE u.id = ?2 AND g.id = ?3 AND c.id = ?4
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&input.group_id)
    .bind(&input.course_id)
    .bind(&input.practice_id)
    .bind(&analysis_json)
    .bind(now)
    .bind(&meta_json)
    .bind(table_number)
    .bind(student_comment)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // Captura la violación del índice único (carrera entre dos alumnos de la misma mesa).
        if e.to_string().contains("UNIQUE constraint failed") {
            anyhow::anyhow!(
                "Otro integrante ya creó el informe de esta mesa. \
                 Aceptá la invitación desde tus notificaciones."
            )
        } else {
            anyhow::Error::from(e)
        }
    })?;

    // El INSERT...SELECT no inserta nada si el curso/grupo (o usuario) no existe.
    if inserted.rows_affected() == 0 {
        anyhow::bail!("el curso o el grupo indicados no existen");
    }

    // Insertar al creador como owner del informe.
    sqlx::query(
        r#"
        INSERT INTO report_members (submission_id, user_id, role, status, invited_at, accepted_at)
        VALUES (?1, ?2, 'owner', 'accepted', ?3, ?3)
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    insert_measurements(&mut tx, &id, &input.measurements, point_based).await?;
    tx.commit().await?;

    if !input.student_results.is_empty() {
        db::save_student_results(pool, &id, &input.student_results).await?;
    }

    // Invitar a los demás alumnos de la mesa (fuera de la tx para no bloquear).
    if let Some(t) = table_number {
        let _ = db::invite_table_members(
            pool,
            &id,
            &input.group_id,
            &input.practice_id,
            t,
            &user.id,
            now,
        )
        .await;
    }

    db::submission_detail(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no se pudo leer la entrega recien creada"))
}

/// Valida que cada símbolo entregado por el alumno corresponda a un mensurando de la práctica.
/// Usada tanto al crear/editar una entrega por formulario como en `POST .../student-results`.
pub async fn validate_student_results(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
    results: &[db::StudentResultInput],
) -> anyhow::Result<()> {
    if results.is_empty() {
        return Ok(());
    }
    let definition = crate::practices::definition(pool, practice_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("practica no encontrada"))?;
    check_student_result_symbols(&definition, results).map_err(|e| anyhow::anyhow!(e))
}

/// Verifica que cada símbolo de `results` sea un mensurando comparable de la práctica: resultado
/// final, agregado `is_final`, o resultado por corrida `Re#k` (Motor E). Función **pura** (sin IO):
/// devuelve el mensaje amigable en `Err` para que cada caller elija el código HTTP (400 símbolo
/// inválido vs. 500/404 al resolver la definición).
pub fn check_student_result_symbols(
    definition: &crate::practices::PracticeDefinition,
    results: &[db::StudentResultInput],
) -> Result<(), String> {
    let valid: std::collections::HashSet<&str> = definition
        .results
        .iter()
        .map(|r| r.symbol.as_str())
        .chain(
            definition
                .aggregates
                .iter()
                .filter(|a| a.is_final)
                .map(|a| a.symbol.as_str()),
        )
        .collect();
    // Resultados por corrida (Motor E): el alumno carga su Reynolds de cada punto con el símbolo
    // compuesto "<base>#<indice>" (p. ej. "Re#0"). Se comparan por corrida contra el automático.
    let point_symbols: std::collections::HashSet<&str> = definition
        .point_results
        .iter()
        .map(|p| p.symbol.as_str())
        .collect();
    for result in results {
        let symbol = result.symbol.trim();
        if let Some((base, idx)) = symbol.split_once('#') {
            if point_symbols.contains(base) && idx.parse::<usize>().is_ok() {
                continue;
            }
        } else if valid.contains(symbol) {
            continue;
        }
        return Err(format!(
            "el simbolo \"{symbol}\" no es un mensurando de esta practica"
        ));
    }
    Ok(())
}

/// Reemplaza las lecturas y recalcula el análisis de una entrega por formulario existente
/// (edición dentro de la ventana permitida). No cambia `submitted_at` ni la práctica: la
/// validación de propiedad/ventana ocurre en la capa de rutas. Transaccional.
/// `student_results`: `None` = no tocar los cálculos del alumno ya guardados; `Some(vec)`
/// (incluso vacío) reemplaza por completo, igual que `POST .../student-results`.
pub async fn update_form_submission(
    pool: &sqlx::SqlitePool,
    submission_id: &str,
    practice_id: &str,
    measurements: &[MeasurementInput],
    meta: Option<&serde_json::Value>,
    student_results: Option<&[db::StudentResultInput]>,
    student_comment: Option<&str>,
) -> anyhow::Result<db::SubmissionDetail> {
    if let Some(results) = student_results {
        validate_student_results(pool, practice_id, results).await?;
    }

    let analysis = analyze(pool, practice_id, measurements).await?;
    let point_based = is_point_based_practice(pool, practice_id).await?;
    let analysis_json = serde_json::to_string(&analysis)?;
    let meta_json = match meta {
        Some(value) => Some(serde_json::to_string(value)?),
        None => None,
    };

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE submissions SET analysis_json = ?2, measurement_meta_json = ?3, \
         student_comment = ?4 WHERE id = ?1",
    )
    .bind(submission_id)
    .bind(&analysis_json)
    .bind(&meta_json)
    .bind(student_comment)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM submission_measurements WHERE submission_id = ?1")
        .bind(submission_id)
        .execute(&mut *tx)
        .await?;

    insert_measurements(&mut tx, submission_id, measurements, point_based).await?;
    tx.commit().await?;

    if let Some(results) = student_results {
        db::save_student_results(pool, submission_id, results).await?;
    }

    db::submission_detail(pool, submission_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no se pudo leer la entrega editada"))
}
