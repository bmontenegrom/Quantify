use crate::courses::Course;
use crate::db::{clean_zero, next_position};
use crate::users::AuthUser;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

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

/// Crea un componente evaluable (pregunta/informe/parcial) en un curso, asignándole la
/// siguiente posición disponible, y lo devuelve.
pub async fn create_grade_component(
    pool: &SqlitePool,
    input: CreateGradeComponent,
) -> anyhow::Result<GradeComponent> {
    let id = Uuid::new_v4().to_string();
    let position = next_position(pool, "grade_components", "course_id", &input.course_id).await?;

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
    .bind(position)
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
