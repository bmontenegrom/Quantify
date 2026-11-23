use super::{PartInput, PracticePart, PracticeSection, SectionInput};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct PartRow {
    #[sqlx(flatten)]
    part: PracticePart,
    config_json: String,
}

pub async fn parts_for(
    conn: &mut SqliteConnection,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticePart>> {
    let rows = sqlx::query_as::<_, PartRow>(
        "SELECT id, practice_id, title, position, analysis_kind, help_text, config_json          FROM practice_parts WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PracticePart {
            determinations: super::PracticeConfig::parse(&row.config_json).determinations,
            ..row.part
        })
        .collect())
}

pub async fn sections_for(
    conn: &mut SqliteConnection,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeSection>> {
    let mut sections = sqlx::query_as::<_, PracticeSection>(
        "SELECT id, practice_id, part_id, title, position, help_text FROM practice_sections \
         WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(&mut *conn)
    .await?;
    for (table, is_quantity) in [("practice_quantities", true), ("practice_results", false)] {
        let members: Vec<(String, String)> = sqlx::query_as(&format!(
            "SELECT section_id, symbol FROM {table} \
             WHERE practice_id = ?1 AND section_id IS NOT NULL ORDER BY position, symbol"
        ))
        .bind(practice_id)
        .fetch_all(&mut *conn)
        .await?;
        for (section_id, symbol) in members {
            if let Some(section) = sections.iter_mut().find(|s| s.id == section_id) {
                if is_quantity {
                    section.quantities.push(symbol);
                } else {
                    section.results.push(symbol);
                }
            }
        }
    }
    Ok(sections)
}

pub async fn create_part(
    pool: &SqlitePool,
    practice_id: &str,
    input: &PartInput,
) -> anyhow::Result<PracticePart> {
    let help_text = input.help_text.as_deref().unwrap_or_default().trim();
    let id = Uuid::new_v4().to_string();
    let mut tx = crate::db::begin_write(pool).await?;
    let position =
        crate::db::next_position(&mut tx, "practice_parts", "practice_id", practice_id).await?;
    let analysis_kind: Option<String> = sqlx::query_scalar(
        "SELECT CASE WHEN COUNT(analysis_kind) > 0 THEN 'estadistico' END \
         FROM practice_parts WHERE practice_id = ?1",
    )
    .bind(practice_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO practice_parts \
         (id, practice_id, title, position, help_text, analysis_kind) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.title.trim())
    .bind(position)
    .bind(help_text)
    .bind(&analysis_kind)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(PracticePart {
        id,
        practice_id: practice_id.to_string(),
        title: input.title.trim().to_string(),
        position,
        analysis_kind,
        help_text: help_text.to_string(),
        effective_kind: "estadistico".into(),
        determinations: None,
    })
}

pub async fn save_part_kinds(
    pool: &SqlitePool,
    definition: &super::PracticeDefinition,
) -> anyhow::Result<()> {
    let mut tx = crate::db::begin_write(pool).await?;
    write_kinds(&mut tx, definition).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_part_saving_kinds(
    pool: &SqlitePool,
    part_id: &str,
    after: &super::PracticeDefinition,
) -> anyhow::Result<bool> {
    let mut tx = crate::db::begin_write(pool).await?;
    let deleted = sqlx::query("DELETE FROM practice_parts WHERE id = ?1 AND practice_id = ?2")
        .bind(part_id)
        .bind(&after.practice_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Ok(false);
    }
    write_kinds(&mut tx, after).await?;
    tx.commit().await?;
    Ok(true)
}

async fn write_kinds(
    conn: &mut sqlx::SqliteConnection,
    definition: &super::PracticeDefinition,
) -> anyhow::Result<()> {
    for part in &definition.parts {
        sqlx::query(
            "UPDATE practice_parts SET analysis_kind = ?1 WHERE id = ?2 AND practice_id = ?3",
        )
        .bind(&part.analysis_kind)
        .bind(&part.id)
        .bind(&definition.practice_id)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query("UPDATE practices SET analysis_kind = ?1 WHERE id = ?2")
        .bind(&definition.analysis_kind)
        .bind(&definition.practice_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn update_part(
    pool: &SqlitePool,
    practice_id: &str,
    part_id: &str,
    input: &PartInput,
) -> anyhow::Result<bool> {
    let changed = sqlx::query(
        "UPDATE practice_parts SET title = ?3, help_text = COALESCE(?4, help_text) WHERE id = ?2 AND practice_id = ?1",
    )
    .bind(practice_id)
    .bind(part_id)
    .bind(input.title.trim())
    .bind(input.help_text.as_deref().map(str::trim))
    .execute(pool)
    .await?
    .rows_affected();
    Ok(changed > 0)
}

async fn belongs(
    pool: &SqlitePool,
    table: &str,
    id: &str,
    practice_id: &str,
) -> anyhow::Result<bool> {
    let found: Option<(i64,)> = sqlx::query_as(&format!(
        "SELECT 1 FROM {table} WHERE id = ?1 AND practice_id = ?2"
    ))
    .bind(id)
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

async fn part_belongs(
    pool: &SqlitePool,
    practice_id: &str,
    part_id: Option<&str>,
) -> anyhow::Result<bool> {
    match part_id {
        Some(part_id) => belongs(pool, "practice_parts", part_id, practice_id).await,
        None => Ok(true),
    }
}

pub async fn create_section(
    pool: &SqlitePool,
    practice_id: &str,
    input: &SectionInput,
) -> anyhow::Result<Option<PracticeSection>> {
    let help_text = input.help_text.as_deref().unwrap_or_default().trim();
    let part_id = input.part_id.as_deref().filter(|p| !p.is_empty());
    if !part_belongs(pool, practice_id, part_id).await? {
        return Ok(None);
    }
    let id = Uuid::new_v4().to_string();
    let mut tx = crate::db::begin_write(pool).await?;
    let position =
        crate::db::next_position(&mut tx, "practice_sections", "practice_id", practice_id).await?;
    sqlx::query(
        "INSERT INTO practice_sections (id, practice_id, part_id, title, position, help_text) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(part_id)
    .bind(input.title.trim())
    .bind(position)
    .bind(help_text)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(PracticeSection {
        id,
        practice_id: practice_id.to_string(),
        part_id: part_id.map(str::to_string),
        title: input.title.trim().to_string(),
        position,
        help_text: help_text.to_string(),
        quantities: vec![],
        results: vec![],
    }))
}

pub async fn update_section(
    pool: &SqlitePool,
    practice_id: &str,
    section_id: &str,
    input: &SectionInput,
) -> anyhow::Result<Option<bool>> {
    let part_id = input.part_id.as_deref().filter(|p| !p.is_empty());
    if !part_belongs(pool, practice_id, part_id).await? {
        return Ok(None);
    }
    let changed = sqlx::query(
        "UPDATE practice_sections SET title = ?3, part_id = ?4, help_text = COALESCE(?5, help_text) \
         WHERE id = ?2 AND practice_id = ?1",
    )
    .bind(practice_id)
    .bind(section_id)
    .bind(input.title.trim())
    .bind(part_id)
    .bind(input.help_text.as_deref().map(str::trim))
    .execute(pool)
    .await?
    .rows_affected();
    Ok(Some(changed > 0))
}

pub async fn delete_section(
    pool: &SqlitePool,
    practice_id: &str,
    section_id: &str,
) -> anyhow::Result<bool> {
    let deleted = sqlx::query("DELETE FROM practice_sections WHERE id = ?2 AND practice_id = ?1")
        .bind(practice_id)
        .bind(section_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(deleted > 0)
}

#[derive(Debug, Clone, Copy)]
pub enum SectionMember {
    Quantity,
    Result,
}

pub async fn set_member_section(
    pool: &SqlitePool,
    practice_id: &str,
    member: SectionMember,
    member_id: &str,
    section_id: Option<&str>,
) -> anyhow::Result<bool> {
    if let Some(section_id) = section_id {
        if !belongs(pool, "practice_sections", section_id, practice_id).await? {
            return Ok(false);
        }
    }
    let (table, live) = match member {
        SectionMember::Quantity => ("practice_quantities", "AND removed = 0"),
        SectionMember::Result => ("practice_results", ""),
    };
    let changed = sqlx::query(&format!(
        "UPDATE {table} SET section_id = ?3 WHERE id = ?2 AND practice_id = ?1 {live}"
    ))
    .bind(practice_id)
    .bind(member_id)
    .bind(section_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(changed > 0)
}
