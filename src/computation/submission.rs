use super::engines::analyze;
use super::MeasurementInput;
use crate::db::{self, AuthUser};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct FormSubmissionInput {
    pub course_id: String,
    pub group_id: String,
    pub practice_id: String,
    pub measurements: Vec<MeasurementInput>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    #[serde(default)]
    pub table_number: Option<i64>,
    #[serde(default)]
    pub student_results: Vec<crate::db::StudentResultInput>,
    #[serde(default)]
    pub student_comment: Option<String>,
}

async fn insert_measurements(
    conn: &mut sqlx::SqliteConnection,
    submission_id: &str,
    measurements: &[MeasurementInput],
    point_based: bool,
) -> anyhow::Result<()> {
    for measurement in measurements {
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
                measurement
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (0i64, i as i64, 0i64, v, None))
                    .collect()
            } else {
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

pub async fn create_form_submission(
    pool: &sqlx::SqlitePool,
    user: &AuthUser,
    input: FormSubmissionInput,
) -> anyhow::Result<db::SubmissionDetail> {
    let is_teacher = matches!(user.role.as_str(), "docente" | "admin");

    let table_number = if let Some(t) = input.table_number {
        Some(t)
    } else if !is_teacher {
        db::resolve_user_table(pool, &user.id, &input.group_id, &input.practice_id).await?
    } else {
        None
    };

    if !is_teacher && table_number.is_none() {
        anyhow::bail!(
            "No tenés una mesa asignada para esta práctica. \
             Pedile al docente que te asigne una mesa."
        );
    }

    if let Some(t) = table_number {
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
        if e.to_string().contains("UNIQUE constraint failed") {
            anyhow::anyhow!(
                "Otro integrante ya creó el informe de esta mesa. \
                 Aceptá la invitación desde tus notificaciones."
            )
        } else {
            anyhow::Error::from(e)
        }
    })?;

    if inserted.rows_affected() == 0 {
        anyhow::bail!("el curso o el grupo indicados no existen");
    }

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

/// Acepta solo resultados del alumno que sean mensurandos o agregados finales, o `Re#k` por punto.
///
/// ```
/// use quantify::computation::check_student_result_symbols;
/// use quantify::db::{PracticeResult, StudentResultInput};
/// use quantify::practices::PracticeDefinition;
/// let g = PracticeResult {
///     id: "r".into(), practice_id: "p".into(), symbol: "g".into(), name: "g".into(),
///     unit: "m/s2".into(), formula: "L".into(), position: 0, tolerance: None,
///     is_final: true, has_uncertainty: true,
/// };
/// let def = PracticeDefinition {
///     practice_id: "p".into(), status: "publicada".into(), analysis_kind: None,
///     x_formula: None, y_formula: None, quantities: vec![], results: vec![g], curves: vec![],
///     operator_count: None, intermediates: vec![], point_results: vec![], aggregates: vec![],
///     parts: vec![], sections: vec![],
/// };
/// let result = |symbol: &str| StudentResultInput { symbol: symbol.into(), value: 9.8, u_expanded: None };
/// assert!(check_student_result_symbols(&def, &[result("g")]).is_ok());
/// assert!(check_student_result_symbols(&def, &[result("h")]).is_err());
/// ```
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
