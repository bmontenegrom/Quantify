use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::path::Path;
use uuid::Uuid;

use crate::analysis::AnalysisResult;
use crate::courses::Course;
use crate::db::clean_zero;
use crate::users::AuthUser;

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
        return Err(anyhow::anyhow!("el componente de nota no existe"));
    };
    if input.raw_points > max_points {
        return Err(anyhow::anyhow!(
            "el puntaje supera el maximo del componente"
        ));
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

/// Persiste una entrega: escribe el CSV en `upload_dir`, serializa el análisis y guarda
/// la fila resolviendo nombres denormalizados (estudiante, grupo, curso). Devuelve el detalle creado.
pub async fn create_submission(
    pool: &SqlitePool,
    upload_dir: &Path,
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
        .ok_or_else(|| anyhow::anyhow!("no se pudo leer la entrega recien creada"))
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
                return None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrate, seed_academic, seed_practices, seed_users};
    use crate::users::AuthUser;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    const TEACHER: &str = "docente@quantify.local";
    const STUDENT: &str = "estudiante@quantify.local";
    const COURSE: &str = "fisica-experimental-i-2026";
    const GROUP: &str = "fisica-exp-i-grupo-1";

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
        sqlx::query_as::<_, AuthUser>(
            "SELECT id, username, email, display_name, role FROM users WHERE email = ?1",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
    }

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
        // El docente habilitó la visibilidad.
        assert!(reviewed.results_visible_to_student);

        // Una revisión sin `results_visible` (None) no cambia la visibilidad ya habilitada.
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
}
