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

pub async fn version_for_form(
    pool: &SqlitePool,
    practice_id: &str,
    loaded: Option<i64>,
) -> anyhow::Result<PracticeVersion> {
    let current = current_version(pool, practice_id).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "La práctica todavía no tiene una versión publicada: publicala para recibir \
             entregas (para probar el cálculo usá la vista previa)."
        )
    })?;
    if let Some(loaded) = loaded.filter(|n| *n != current.number) {
        anyhow::bail!(
            "La práctica pasó de la versión {loaded} a la {} mientras completabas el \
             formulario: recargá la página (lo que cargaste queda guardado como borrador).",
            current.number
        );
    }
    Ok(current)
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

static PUBLISHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub enum Publish {
    Published { number: i64 },
    Unchanged(i64),
    Rejected(Vec<String>),
    NotFound,
}

pub async fn publish_version(
    pool: &SqlitePool,
    practice_id: &str,
    published_by: Option<&str>,
) -> anyhow::Result<Option<PracticeVersion>> {
    let _guard = PUBLISHING.lock().await;
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(None);
    };
    let mut tx = pool.begin().await?;
    let version = insert_version(&mut tx, &definition, published_by).await?;
    tx.commit().await?;
    Ok(Some(version))
}

pub async fn publish_checked(
    pool: &SqlitePool,
    practice_id: &str,
    published_by: Option<&str>,
    problems: impl Fn(&PracticeDefinition) -> Vec<String>,
) -> anyhow::Result<Publish> {
    let _guard = PUBLISHING.lock().await;
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(Publish::NotFound);
    };
    let found = problems(&definition);
    if !found.is_empty() {
        return Ok(Publish::Rejected(found));
    }
    if let Some(number) = unchanged(pool, &definition).await? {
        return Ok(Publish::Unchanged(number));
    }
    let mut tx = pool.begin().await?;
    let version = insert_version(&mut tx, &definition, published_by).await?;
    tx.commit().await?;
    Ok(Publish::Published {
        number: version.number,
    })
}

pub async fn publish_draft(
    pool: &SqlitePool,
    practice_id: &str,
    published_by: Option<&str>,
    problems: impl Fn(&PracticeDefinition) -> Vec<String>,
) -> anyhow::Result<Publish> {
    let _guard = PUBLISHING.lock().await;
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(Publish::NotFound);
    };
    let found = problems(&definition);
    if !found.is_empty() {
        return Ok(Publish::Rejected(found));
    }
    let mut tx = pool.begin().await?;
    let changed = sqlx::query(
        "UPDATE practices SET status = 'publicada' WHERE id = ?1 AND status = 'borrador'",
    )
    .bind(practice_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(Publish::NotFound);
    }
    let current: Option<String> =
        sqlx::query_scalar("SELECT current_version_id FROM practices WHERE id = ?1")
            .bind(practice_id)
            .fetch_one(&mut *tx)
            .await?;
    let number = match current {
        Some(_) => definition.current_version.unwrap_or(1),
        None => {
            insert_version(&mut tx, &definition, published_by)
                .await?
                .number
        }
    };
    tx.commit().await?;
    Ok(Publish::Published { number })
}

async fn unchanged(pool: &SqlitePool, draft: &PracticeDefinition) -> anyhow::Result<Option<i64>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT v.number, v.definition_json FROM practices p \
         JOIN practice_versions v ON v.id = p.current_version_id WHERE p.id = ?1",
    )
    .bind(&draft.practice_id)
    .fetch_optional(pool)
    .await?;
    let Some((number, json)) = row else {
        return Ok(None);
    };
    let mut current: serde_json::Value = serde_json::from_str(&json)?;
    current["current_version"] = serde_json::Value::Null;
    let draft: serde_json::Value = serde_json::from_str(&snapshot(draft)?)?;
    Ok((draft == current).then_some(number))
}

pub async fn unchanged_since_current(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<i64>> {
    let Some(draft) = super::definition(pool, practice_id).await? else {
        return Ok(None);
    };
    unchanged(pool, &draft).await
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
