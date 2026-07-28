use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::users::AuthUser;

#[derive(Debug, Serialize, FromRow)]
pub struct LabGroup {
    pub id: String,
    pub course_id: String,
    pub name: String,
    pub table_count: i64,
    pub group_type: String,
}

#[derive(Debug, Serialize)]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub table_count: i64,
    pub group_type: String,
    pub members: Vec<AuthUser>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroup {
    pub name: String,
    pub table_count: Option<i64>,
    pub group_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroup {
    pub name: String,
    pub table_count: i64,
    pub group_type: String,
}

#[derive(Debug, Deserialize)]
pub struct AddGroupMember {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct EnrollCourseMember {
    pub user_id: String,
}

/// Crea un grupo de laboratorio en un curso. `table_count` se acota a 1..=24 y
/// `group_type` se normaliza a `regular`/`recuperacion`.
pub async fn create_group(
    pool: &SqlitePool,
    course_id: &str,
    input: CreateGroup,
) -> anyhow::Result<LabGroup> {
    let id = Uuid::new_v4().to_string();
    let table_count = input.table_count.unwrap_or(4).clamp(1, 24);
    let group_type = normalize_group_type(input.group_type.as_deref());
    sqlx::query(
        r#"
        INSERT INTO lab_groups (id, course_id, name, table_count, group_type, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(course_id)
    .bind(input.name.trim())
    .bind(table_count)
    .bind(group_type)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as::<_, LabGroup>(
        "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Normaliza el tipo de grupo a uno de los valores válidos: `recuperacion` o `regular` (por defecto).
fn normalize_group_type(value: Option<&str>) -> &'static str {
    match value.unwrap_or("regular").trim() {
        "recuperacion" => "recuperacion",
        _ => "regular",
    }
}

/// Actualiza nombre, cantidad de mesas y tipo de un grupo. Devuelve `None` si no existe.
pub async fn update_group(
    pool: &SqlitePool,
    group_id: &str,
    input: UpdateGroup,
) -> anyhow::Result<Option<LabGroup>> {
    let course: Option<(String,)> =
        sqlx::query_as("SELECT course_id FROM lab_groups WHERE id = ?1")
            .bind(group_id)
            .fetch_optional(pool)
            .await?;

    let Some((course_id,)) = course else {
        return Ok(None);
    };

    let result = sqlx::query(
        r#"
        UPDATE lab_groups
        SET name = ?2,
            table_count = ?3,
            group_type = ?4
        WHERE id = ?1
        "#,
    )
    .bind(group_id)
    .bind(input.name.trim())
    .bind(input.table_count.clamp(1, 24))
    .bind(normalize_group_type(Some(&input.group_type)))
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(
        sqlx::query_as::<_, LabGroup>(
            "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1 AND course_id = ?2",
        )
        .bind(group_id)
        .bind(course_id)
        .fetch_one(pool)
        .await?,
    ))
}

/// Inscribe a un usuario en un curso (si no lo estaba) y lo agrega al grupo `General`
/// del curso, creándolo si hace falta. Idempotente.
pub async fn enroll_course_member(
    pool: &SqlitePool,
    course_id: &str,
    user_id: &str,
) -> anyhow::Result<()> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT user_id FROM course_members WHERE course_id = ?1 AND user_id = ?2")
            .bind(course_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    if existing.is_none() {
        sqlx::query(
            r#"
            INSERT INTO course_members (course_id, user_id, created_at)
            VALUES (?1, ?2, ?3)
            "#,
        )
        .bind(course_id)
        .bind(user_id)
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    let default_group_id = ensure_default_group(pool, course_id).await?;
    sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(group_id, user_id) DO NOTHING
        "#,
    )
    .bind(default_group_id)
    .bind(user_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}

/// Devuelve el id del grupo `General` del curso, creándolo si todavía no existe.
async fn ensure_default_group(pool: &SqlitePool, course_id: &str) -> anyhow::Result<String> {
    let existing = sqlx::query_as::<_, LabGroup>(
        r#"
        SELECT id, course_id, name, table_count, group_type
        FROM lab_groups
        WHERE course_id = ?1 AND name = 'General'
        LIMIT 1
        "#,
    )
    .bind(course_id)
    .fetch_optional(pool)
    .await?;

    if let Some(group) = existing {
        return Ok(group.id);
    }

    let created = create_group(
        pool,
        course_id,
        CreateGroup {
            name: "General".into(),
            table_count: Some(4),
            group_type: Some("regular".into()),
        },
    )
    .await?;
    Ok(created.id)
}

/// Agrega un estudiante a un grupo. Devuelve `None` si el usuario no existe o no es estudiante.
pub async fn add_group_member(
    pool: &SqlitePool,
    group_id: &str,
    input: AddGroupMember,
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

    sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, created_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(group_id, user_id) DO NOTHING
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(Some(()))
}

/// Quita a un estudiante de un grupo. Devuelve `true` si había una membresía y se eliminó.
pub async fn remove_group_member(
    pool: &SqlitePool,
    group_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM group_members
        WHERE group_id = ?1 AND user_id = ?2
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Lista los grupos de un curso, cada uno con sus estudiantes miembros.
pub async fn groups_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<GroupSummary>> {
    let groups = sqlx::query_as::<_, LabGroup>(
        "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE course_id = ?1 ORDER BY name",
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?;

    let mut summaries = Vec::with_capacity(groups.len());
    for group in groups {
        let members = sqlx::query_as::<_, AuthUser>(
            r#"
            SELECT u.id, u.username, u.email, u.display_name, u.role
            FROM group_members gm
            JOIN users u ON u.id = gm.user_id
            WHERE gm.group_id = ?1
            ORDER BY u.display_name
            "#,
        )
        .bind(&group.id)
        .fetch_all(pool)
        .await?;

        summaries.push(GroupSummary {
            id: group.id,
            name: group.name,
            table_count: group.table_count,
            group_type: group.group_type,
            members,
        });
    }

    Ok(summaries)
}
