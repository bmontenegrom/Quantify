use super::PracticeDefinition;
use sqlx::{SqliteConnection, SqlitePool};

#[derive(Debug, Clone)]
pub struct PracticeVersion {
    pub id: String,
    pub number: i64,
    pub definition: PracticeDefinition,
}

fn parse(json: &str) -> anyhow::Result<PracticeDefinition> {
    serde_json::from_str(json).map_err(|e| anyhow::Error::new(sqlx::Error::Decode(Box::new(e))))
}

fn version_from_row((id, number, json): (String, i64, String)) -> anyhow::Result<PracticeVersion> {
    Ok(PracticeVersion {
        id,
        number,
        definition: parse(&json)?,
    })
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
    row.map(version_from_row).transpose()
}

pub async fn version_by_number(
    pool: &SqlitePool,
    practice_id: &str,
    number: i64,
) -> anyhow::Result<Option<PracticeVersion>> {
    let row: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT id, number, definition_json FROM practice_versions \
         WHERE practice_id = ?1 AND number = ?2",
    )
    .bind(practice_id)
    .bind(number)
    .fetch_optional(pool)
    .await?;
    row.map(version_from_row).transpose()
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
    row.map(|(json,)| parse(&json)).transpose()
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

fn snapshot(definition: &PracticeDefinition) -> anyhow::Result<String> {
    let mut definition = definition.clone();
    definition.status = "publicada".into();
    definition.current_version = None;
    Ok(serde_json::to_string(&definition)?)
}

async fn insert_version(
    conn: &mut SqliteConnection,
    definition: &PracticeDefinition,
    published_by: Option<&str>,
) -> anyhow::Result<PracticeVersion> {
    let practice_id = &definition.practice_id;
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
    .bind(snapshot(definition)?)
    .bind(chrono::Utc::now())
    .bind(published_by)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE practices SET current_version_id = ?1 WHERE id = ?2")
        .bind(&id)
        .bind(practice_id)
        .execute(&mut *conn)
        .await?;
    if number == 1 {
        sqlx::query(
            "UPDATE submissions SET practice_version_id = ?1 \
             WHERE practice_id = ?2 AND practice_version_id IS NULL",
        )
        .bind(&id)
        .bind(practice_id)
        .execute(&mut *conn)
        .await?;
    }
    let mut definition = definition.clone();
    definition.status = "publicada".into();
    definition.current_version = Some(number);
    Ok(PracticeVersion {
        id,
        number,
        definition,
    })
}

pub async fn publish_version(
    pool: &SqlitePool,
    practice_id: &str,
    published_by: Option<&str>,
) -> anyhow::Result<Option<PracticeVersion>> {
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(None);
    };
    let mut tx = pool.begin().await?;
    let version = insert_version(&mut tx, &definition, published_by).await?;
    tx.commit().await?;
    Ok(Some(version))
}

pub async fn publish_draft(
    pool: &SqlitePool,
    practice_id: &str,
    published_by: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(false);
    };
    let mut tx = pool.begin().await?;
    let changed = sqlx::query(
        "UPDATE practices SET status = 'publicada' WHERE id = ?1 AND status = 'borrador'",
    )
    .bind(practice_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(false);
    }
    let (has_version,): (bool,) =
        sqlx::query_as("SELECT current_version_id IS NOT NULL FROM practices WHERE id = ?1")
            .bind(practice_id)
            .fetch_one(&mut *tx)
            .await?;
    if !has_version {
        insert_version(&mut tx, &definition, published_by).await?;
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn unchanged_since_current(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<i64>> {
    let Some(current) = current_version(pool, practice_id).await? else {
        return Ok(None);
    };
    let Some(draft) = super::definition(pool, practice_id).await? else {
        return Ok(None);
    };
    Ok((snapshot(&draft)? == snapshot(&current.definition)?).then_some(current.number))
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
