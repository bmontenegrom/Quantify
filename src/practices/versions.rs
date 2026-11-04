use super::PracticeDefinition;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct PracticeVersion {
    pub id: String,
    pub number: i64,
    pub definition: PracticeDefinition,
}

pub async fn current_version(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeVersion>> {
    let row: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT v.id, v.number, v.definition_json FROM practices p \
         JOIN practice_versions v ON v.id = p.current_version_id WHERE p.id = ?1",
    )
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;
    row.map(|(id, number, json)| {
        Ok(PracticeVersion {
            id,
            number,
            definition: serde_json::from_str(&json)?,
        })
    })
    .transpose()
}

pub async fn version_definition(
    pool: &SqlitePool,
    version_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT definition_json FROM practice_versions WHERE id = ?1")
            .bind(version_id)
            .fetch_optional(pool)
            .await?;
    row.map(|(json,)| Ok(serde_json::from_str(&json)?))
        .transpose()
}

pub async fn published_definition(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    if let Some(version) = current_version(pool, practice_id).await? {
        return Ok(Some(version.definition));
    }
    super::definition(pool, practice_id).await
}

pub async fn submission_definition(
    pool: &SqlitePool,
    submission_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT practice_id, practice_version_id FROM submissions WHERE id = ?1")
            .bind(submission_id)
            .fetch_optional(pool)
            .await?;
    let Some((practice_id, version_id)) = row else {
        return Ok(None);
    };
    if let Some(version_id) = version_id {
        if let Some(definition) = version_definition(pool, &version_id).await? {
            return Ok(Some(definition));
        }
    }
    super::definition(pool, &practice_id).await
}

pub async fn publish_version(
    pool: &SqlitePool,
    practice_id: &str,
    published_by: Option<&str>,
) -> anyhow::Result<Option<PracticeVersion>> {
    let Some(mut definition) = super::definition(pool, practice_id).await? else {
        return Ok(None);
    };
    let mut tx = pool.begin().await?;
    let id = uuid::Uuid::new_v4().to_string();
    let (number,): (i64,) = sqlx::query_as(
        "INSERT INTO practice_versions \
         (id, practice_id, number, definition_json, published_at, published_by) \
         SELECT ?1, ?2, n, json_set(?3, '$.current_version', n), ?4, ?5 FROM \
         (SELECT COALESCE(MAX(number), 0) + 1 AS n FROM practice_versions WHERE practice_id = ?2) \
         RETURNING number",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(serde_json::to_string(&definition)?)
    .bind(chrono::Utc::now())
    .bind(published_by)
    .fetch_one(&mut *tx)
    .await?;
    definition.current_version = Some(number);
    sqlx::query("UPDATE practices SET current_version_id = ?1 WHERE id = ?2")
        .bind(&id)
        .bind(practice_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(PracticeVersion {
        id,
        number,
        definition,
    }))
}

pub async fn ensure_versions(pool: &SqlitePool) -> anyhow::Result<()> {
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM practices \
         WHERE status IN ('publicada', 'archivada') AND current_version_id IS NULL",
    )
    .fetch_all(pool)
    .await?;
    for practice_id in missing {
        publish_version(pool, &practice_id, None).await?;
    }
    link_legacy_submissions(pool).await
}

const LEGACY_SUBMISSIONS: &str = "2026-11-entregas-a-su-version";

async fn link_legacy_submissions(pool: &SqlitePool) -> anyhow::Result<()> {
    let (done,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM applied_migrations WHERE name = ?1")
        .bind(LEGACY_SUBMISSIONS)
        .fetch_one(pool)
        .await?;
    if done > 0 {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE submissions SET practice_version_id = \
         (SELECT current_version_id FROM practices WHERE practices.id = submissions.practice_id) \
         WHERE practice_version_id IS NULL",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_SUBMISSIONS)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn has_published_version(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM practices WHERE id = ?1 AND current_version_id IS NOT NULL")
            .bind(practice_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

pub async fn quantity_in_a_version(
    pool: &SqlitePool,
    practice_id: &str,
    quantity_id: &str,
) -> anyhow::Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM practice_versions v, json_each(v.definition_json, '$.quantities') q \
         WHERE v.practice_id = ?1 AND json_extract(q.value, '$.id') = ?2 LIMIT 1",
    )
    .bind(practice_id)
    .bind(quantity_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}
