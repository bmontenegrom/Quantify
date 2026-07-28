use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::db::Practice;
use crate::users::AuthUser;

mod group;
mod subgroup;

pub use group::*;
pub use subgroup::*;

#[derive(Debug, Serialize, FromRow)]
pub struct Course {
    pub id: String,
    pub name: String,
    pub term: String,
    pub active: bool,
    pub submission_edit_hours: f64,
    pub acceptance_window_hours: f64,
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
pub struct SetCoursePractice {
    pub practice_id: String,
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
        practices: crate::db::practices(pool).await?,
        students: if matches!(user.role.as_str(), "docente" | "admin") {
            crate::users::students(pool).await?
        } else {
            Vec::new()
        },
        users: if matches!(user.role.as_str(), "docente" | "admin") {
            crate::users::users(pool).await?
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

/// Lista las prácticas habilitadas en un curso, ordenadas por nombre.
pub async fn practices_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<Practice>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrate, seed_academic, seed_practices, seed_users};
    use crate::users::{create_user, AuthUser, CreateUser};
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
}
