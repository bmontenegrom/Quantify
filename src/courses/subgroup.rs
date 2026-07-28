use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use super::group::{GroupSummary, LabGroup};
use crate::db::Practice;
use crate::users::AuthUser;

#[derive(Debug, Serialize, FromRow)]
pub struct PracticeSubgroup {
    pub id: String,
    pub course_id: String,
    pub practice_id: String,
    pub group_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PracticeTableAssignment {
    pub course_id: String,
    pub practice_id: String,
    pub group_id: String,
    pub user_id: String,
    pub table_number: i64,
}

#[derive(Debug, Serialize)]
pub struct SubgroupSummary {
    pub id: String,
    pub name: String,
    pub practice: Practice,
    pub group: GroupSummary,
    pub members: Vec<AuthUser>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSubgroup {
    pub practice_id: String,
    pub group_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SetPracticeTable {
    pub practice_id: String,
    pub table_number: i64,
}

/// Crea un subgrupo de práctica (combinación curso/práctica/grupo + nombre) y lo devuelve.
pub async fn create_subgroup(
    pool: &SqlitePool,
    course_id: &str,
    input: CreateSubgroup,
) -> anyhow::Result<PracticeSubgroup> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO practice_subgroups (id, course_id, practice_id, group_id, name, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(course_id)
    .bind(input.practice_id)
    .bind(input.group_id)
    .bind(input.name.trim())
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as::<_, PracticeSubgroup>(
        r#"
        SELECT id, course_id, practice_id, group_id, name
        FROM practice_subgroups
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Asigna (o reasigna) la mesa de trabajo de un estudiante para una práctica en un grupo.
/// Valida que la mesa esté en rango y que el estudiante pertenezca al grupo y la práctica
/// esté habilitada en el curso; devuelve `None` si algo no aplica.
pub async fn set_practice_table_assignment(
    pool: &SqlitePool,
    group_id: &str,
    user_id: &str,
    input: SetPracticeTable,
) -> anyhow::Result<Option<PracticeTableAssignment>> {
    let group = sqlx::query_as::<_, LabGroup>(
        "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;

    let Some(group) = group else {
        return Ok(None);
    };

    if input.table_number < 1 || input.table_number > group.table_count {
        return Ok(None);
    }

    let allowed: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT 1
        FROM group_members gm
        JOIN course_practices cp ON cp.course_id = ?3 AND cp.practice_id = ?4
        WHERE gm.group_id = ?1 AND gm.user_id = ?2
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .bind(&group.course_id)
    .bind(&input.practice_id)
    .fetch_optional(pool)
    .await?;

    if allowed.is_none() {
        return Ok(None);
    }

    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO practice_table_assignments (
            course_id, practice_id, group_id, user_id, table_number, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
        ON CONFLICT(practice_id, group_id, user_id) DO UPDATE SET
            table_number = excluded.table_number,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&group.course_id)
    .bind(&input.practice_id)
    .bind(group_id)
    .bind(user_id)
    .bind(input.table_number)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(Some(
        sqlx::query_as::<_, PracticeTableAssignment>(
            r#"
            SELECT course_id, practice_id, group_id, user_id, table_number
            FROM practice_table_assignments
            WHERE practice_id = ?1 AND group_id = ?2 AND user_id = ?3
            "#,
        )
        .bind(input.practice_id)
        .bind(group_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?,
    ))
}

/// Lista los subgrupos de práctica de un curso, resolviendo práctica, grupo y miembros de cada uno.
pub(super) async fn subgroups_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<SubgroupSummary>> {
    let subgroups = sqlx::query_as::<_, PracticeSubgroup>(
        r#"
        SELECT id, course_id, practice_id, group_id, name
        FROM practice_subgroups
        WHERE course_id = ?1
        ORDER BY practice_id, name
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?;

    let mut summaries = Vec::with_capacity(subgroups.len());
    for subgroup in subgroups {
        let practice = sqlx::query_as::<_, Practice>(
            "SELECT id, name, description, analysis_kind FROM practices WHERE id = ?1",
        )
        .bind(&subgroup.practice_id)
        .fetch_one(pool)
        .await?;

        let group_members = sqlx::query_as::<_, AuthUser>(
            r#"
            SELECT u.id, u.username, u.email, u.display_name, u.role
            FROM group_members gm
            JOIN users u ON u.id = gm.user_id
            WHERE gm.group_id = ?1
            ORDER BY u.display_name
            "#,
        )
        .bind(&subgroup.group_id)
        .fetch_all(pool)
        .await?;

        let group = sqlx::query_as::<_, LabGroup>(
            "SELECT id, course_id, name, table_count, group_type FROM lab_groups WHERE id = ?1",
        )
        .bind(&subgroup.group_id)
        .fetch_one(pool)
        .await?;

        let members = sqlx::query_as::<_, AuthUser>(
            r#"
            SELECT u.id, u.username, u.email, u.display_name, u.role
            FROM practice_subgroup_members sm
            JOIN users u ON u.id = sm.user_id
            WHERE sm.subgroup_id = ?1
            ORDER BY u.display_name
            "#,
        )
        .bind(&subgroup.id)
        .fetch_all(pool)
        .await?;

        summaries.push(SubgroupSummary {
            id: subgroup.id,
            name: subgroup.name,
            practice,
            group: GroupSummary {
                id: group.id,
                name: group.name,
                table_count: group.table_count,
                group_type: group.group_type,
                members: group_members,
            },
            members,
        });
    }

    Ok(summaries)
}

/// Lista las asignaciones de mesa por práctica/grupo/estudiante de un curso.
pub(super) async fn table_assignments_for_course(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<PracticeTableAssignment>> {
    Ok(sqlx::query_as::<_, PracticeTableAssignment>(
        r#"
        SELECT course_id, practice_id, group_id, user_id, table_number
        FROM practice_table_assignments
        WHERE course_id = ?1
        ORDER BY practice_id, group_id, table_number
        "#,
    )
    .bind(course_id)
    .fetch_all(pool)
    .await?)
}
