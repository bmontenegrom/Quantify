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
    let (last,): (Option<i64>,) =
        sqlx::query_as("SELECT MAX(number) FROM practice_versions WHERE practice_id = ?1")
            .bind(practice_id)
            .fetch_one(&mut *tx)
            .await?;
    let number = last.unwrap_or(0) + 1;
    definition.current_version = Some(number);
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_versions \
         (id, practice_id, number, definition_json, published_at, published_by) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(number)
    .bind(serde_json::to_string(&definition)?)
    .bind(chrono::Utc::now())
    .bind(published_by)
    .execute(&mut *tx)
    .await?;
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
