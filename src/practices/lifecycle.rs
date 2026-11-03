use super::{NewPracticeInput, PracticeInfoInput};
use crate::db::{Practice, PRACTICE_COLS};
use sqlx::{SqliteConnection, SqlitePool};

const DEFINITION_TABLES: [&str; 6] = [
    "practice_quantities",
    "practice_results",
    "practice_curves",
    "practice_intermediates",
    "practice_point_results",
    "practice_aggregates",
];

/// Id legible de una práctica a partir de su nombre: minúsculas, sin tildes, solo `[a-z0-9-]`.
///
/// ```
/// use quantify::practices::slugify;
/// assert_eq!(slugify("Péndulo de prueba (2026)"), "pendulo-de-prueba-2026");
/// assert_eq!(slugify("¿?"), "practica");
/// ```
pub fn slugify(name: &str) -> String {
    let folded: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    let slug = folded
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "practica".into()
    } else {
        slug
    }
}

async fn free_practice_id(conn: &mut SqliteConnection, base: &str) -> anyhow::Result<String> {
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        let taken: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM practices WHERE id = ?1")
            .bind(&candidate)
            .fetch_optional(&mut *conn)
            .await?;
        if taken.is_none() {
            return Ok(candidate);
        }
        n += 1;
    }
}

pub async fn practice(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Option<Practice>> {
    Ok(sqlx::query_as::<_, Practice>(&format!(
        "SELECT {PRACTICE_COLS} FROM practices WHERE id = ?1"
    ))
    .bind(practice_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_practice(
    pool: &SqlitePool,
    input: &NewPracticeInput,
    created_by: &str,
) -> anyhow::Result<Practice> {
    let mut tx = pool.begin().await?;
    let id = free_practice_id(&mut tx, &slugify(&input.name)).await?;
    sqlx::query(
        "INSERT INTO practices (id, name, description, analysis_kind, status, created_by, created_at) \
         VALUES (?1, ?2, ?3, ?4, 'borrador', ?5, ?6)",
    )
    .bind(&id)
    .bind(input.name.trim())
    .bind(input.description.trim())
    .bind(input.analysis_kind.trim())
    .bind(created_by)
    .bind(chrono::Utc::now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    practice(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la practica recien creada no aparece"))
}

pub async fn update_practice(
    pool: &SqlitePool,
    practice_id: &str,
    input: &PracticeInfoInput,
) -> anyhow::Result<Option<Practice>> {
    sqlx::query("UPDATE practices SET name = ?2, description = ?3 WHERE id = ?1")
        .bind(practice_id)
        .bind(input.name.trim())
        .bind(input.description.trim())
        .execute(pool)
        .await?;
    practice(pool, practice_id).await
}

pub async fn duplicate_practice(
    pool: &SqlitePool,
    source_id: &str,
    name: &str,
    created_by: &str,
) -> anyhow::Result<Option<Practice>> {
    let mut tx = pool.begin().await?;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM practices WHERE id = ?1")
        .bind(source_id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Ok(None);
    }
    let id = free_practice_id(&mut tx, &slugify(name)).await?;
    let copied: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('practices') \
         WHERE name NOT IN ('id', 'name', 'status', 'created_by', 'created_at')",
    )
    .fetch_all(&mut *tx)
    .await?;
    let copied = copied.join(", ");
    sqlx::query(&format!(
        "INSERT INTO practices (id, name, status, created_by, created_at, {copied}) \
         SELECT ?2, ?3, 'borrador', ?4, ?5, {copied} FROM practices WHERE id = ?1"
    ))
    .bind(source_id)
    .bind(&id)
    .bind(name.trim())
    .bind(created_by)
    .bind(chrono::Utc::now())
    .execute(&mut *tx)
    .await?;
    for table in DEFINITION_TABLES {
        let columns: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT name FROM pragma_table_info('{table}') WHERE name NOT IN ('id', 'practice_id')"
        ))
        .fetch_all(&mut *tx)
        .await?;
        let list = columns.join(", ");
        sqlx::query(&format!(
            "INSERT INTO {table} (id, practice_id, {list}) \
             SELECT lower(hex(randomblob(16))), ?2, {list} FROM {table} WHERE practice_id = ?1"
        ))
        .bind(source_id)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    }
    let parts: Vec<String> =
        sqlx::query_scalar("SELECT id FROM practice_parts WHERE practice_id = ?1")
            .bind(source_id)
            .fetch_all(&mut *tx)
            .await?;
    let mut part_map = std::collections::HashMap::new();
    for old in parts {
        let new = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO practice_parts (id, practice_id, title, position, analysis_kind, \
             x_formula, y_formula, operator_count) \
             SELECT ?1, ?2, title, position, analysis_kind, x_formula, y_formula, operator_count \
             FROM practice_parts WHERE id = ?3",
        )
        .bind(&new)
        .bind(&id)
        .bind(&old)
        .execute(&mut *tx)
        .await?;
        part_map.insert(old, new);
    }
    let sections: Vec<(String, Option<String>, String, i64)> = sqlx::query_as(
        "SELECT id, part_id, title, position FROM practice_sections WHERE practice_id = ?1",
    )
    .bind(source_id)
    .fetch_all(&mut *tx)
    .await?;
    for (old, part_id, title, position) in sections {
        let new = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO practice_sections (id, practice_id, part_id, title, position) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&new)
        .bind(&id)
        .bind(part_id.and_then(|p| part_map.get(&p).cloned()))
        .bind(title)
        .bind(position)
        .execute(&mut *tx)
        .await?;
        for table in ["practice_quantities", "practice_results"] {
            sqlx::query(&format!(
                "UPDATE {table} SET section_id = ?1 WHERE practice_id = ?2 AND section_id = ?3"
            ))
            .bind(&new)
            .bind(&id)
            .bind(&old)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    practice(pool, &id).await
}

#[derive(Debug, PartialEq, Eq)]
pub enum StatusChange {
    Done,
    NotFound,
    NotAllowed(String),
}

pub async fn set_practice_status(
    pool: &SqlitePool,
    practice_id: &str,
    status: &str,
) -> anyhow::Result<StatusChange> {
    let Some(current) = practice(pool, practice_id).await? else {
        return Ok(StatusChange::NotFound);
    };
    let allowed = matches!(
        (current.status.as_str(), status),
        ("borrador", "publicada") | ("publicada", "archivada") | ("archivada", "publicada")
    );
    if !allowed {
        return Ok(StatusChange::NotAllowed(current.status));
    }
    let changed = sqlx::query("UPDATE practices SET status = ?2 WHERE id = ?1 AND status = ?3")
        .bind(practice_id)
        .bind(status)
        .bind(&current.status)
        .execute(pool)
        .await?
        .rows_affected();
    if changed == 0 {
        return Ok(StatusChange::NotAllowed(current.status));
    }
    Ok(StatusChange::Done)
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeletePractice {
    Deleted,
    NotFound,
    NotDraft,
    HasSubmissions,
}

pub async fn delete_practice(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<DeletePractice> {
    let Some(current) = practice(pool, practice_id).await? else {
        return Ok(DeletePractice::NotFound);
    };
    if current.status != "borrador" {
        return Ok(DeletePractice::NotDraft);
    }
    let (submissions,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE practice_id = ?1")
            .bind(practice_id)
            .fetch_one(pool)
            .await?;
    if submissions > 0 {
        return Ok(DeletePractice::HasSubmissions);
    }
    let deleted = sqlx::query(
        "DELETE FROM practices WHERE id = ?1 AND status = 'borrador' \
         AND NOT EXISTS (SELECT 1 FROM submissions WHERE practice_id = ?1)",
    )
    .bind(practice_id)
    .execute(pool)
    .await?
    .rows_affected();
    if deleted == 0 {
        return Ok(DeletePractice::NotDraft);
    }
    Ok(DeletePractice::Deleted)
}
