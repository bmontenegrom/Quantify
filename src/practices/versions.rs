use super::PracticeDefinition;
use sqlx::{SqliteConnection, SqlitePool};

#[derive(Debug, Clone)]
pub struct PracticeVersion {
    pub id: String,
    pub number: i64,
    pub definition: PracticeDefinition,
}

pub(super) fn parse(json: &str) -> anyhow::Result<PracticeDefinition> {
    let definition: PracticeDefinition = serde_json::from_str(json)
        .map_err(|e| anyhow::Error::new(sqlx::Error::Decode(Box::new(e))))?;
    if definition.practice_id.is_empty() {
        return Err(anyhow::Error::new(sqlx::Error::Decode(
            "una versión guardada no tiene practice_id".into(),
        )));
    }
    Ok(definition)
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
    let row: Option<(String, i64, String, String)> = sqlx::query_as(
        "SELECT v.id, v.number, v.definition_json, p.status FROM practices p \
         JOIN practice_versions v ON v.id = p.current_version_id WHERE p.id = ?1",
    )
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;
    row.map(|(id, number, json, status)| {
        let mut version = version_from_row((id, number, json))?;
        version.definition.status = status;
        Ok(version)
    })
    .transpose()
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
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT v.definition_json, p.status FROM practice_versions v \
         JOIN practices p ON p.id = v.practice_id WHERE v.id = ?1",
    )
    .bind(version_id)
    .fetch_optional(pool)
    .await?;
    row.map(|(json, status)| {
        let mut definition = parse(&json)?;
        definition.status = status;
        Ok(definition)
    })
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
    published_definition(pool, &practice_id).await
}

pub async fn submission_display_symbols(
    pool: &SqlitePool,
    submission_id: &str,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT CAST(json_extract(v.definition_json, '$.display_symbols') AS TEXT) \
         FROM submissions s JOIN practices p ON p.id = s.practice_id \
         JOIN practice_versions v ON v.id = COALESCE(s.practice_version_id, p.current_version_id) \
         WHERE s.id = ?1",
    )
    .bind(submission_id)
    .fetch_optional(pool)
    .await?;
    let Some((Some(json),)) = row else {
        return Ok(Default::default());
    };
    Ok(serde_json::from_str(&json).unwrap_or_else(|error| {
        tracing::warn!(%submission_id, %error, "la versión de la entrega tiene símbolos a mostrar ilegibles");
        Default::default()
    }))
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
    UpdatedCurrent { number: i64 },
    Unchanged(i64),
    DraftChanged,
    Rejected(Vec<String>),
    IsDraft,
    NotFound,
}

pub enum DraftPublish {
    Published,
    Rejected(Vec<String>),
    NotDraft,
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
    expected_fingerprint: Option<&str>,
    problems: impl Fn(&PracticeDefinition) -> Vec<String>,
) -> anyhow::Result<Publish> {
    let _guard = PUBLISHING.lock().await;
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(Publish::NotFound);
    };
    if definition.status == "borrador" {
        return Ok(Publish::IsDraft);
    }
    if let Some(current) = current_version(pool, practice_id).await? {
        let number = current.number;
        let kind = super::change_kind(&current.definition, &definition)?;
        if kind == super::DraftChangeKind::None {
            return Ok(Publish::Unchanged(number));
        }
        let fingerprint = super::draft_fingerprint(&definition)?;
        if expected_fingerprint.is_some_and(|expected| expected != fingerprint) {
            return Ok(Publish::DraftChanged);
        }
        if kind == super::DraftChangeKind::Cosmetic {
            let mut tx = pool.begin().await?;
            sqlx::query(
                "UPDATE practice_versions \
                     SET definition_json = json_set(?1, '$.current_version', number) \
                     WHERE id = ?2",
            )
            .bind(snapshot(&definition)?)
            .bind(&current.id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO practice_version_updates (id, version_id, updated_at, updated_by) \
                     VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&current.id)
            .bind(chrono::Utc::now())
            .bind(published_by)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(Publish::UpdatedCurrent { number });
        }
    }
    let found = problems(&definition);
    if !found.is_empty() {
        return Ok(Publish::Rejected(found));
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
) -> anyhow::Result<DraftPublish> {
    let _guard = PUBLISHING.lock().await;
    let Some(definition) = super::definition(pool, practice_id).await? else {
        return Ok(DraftPublish::NotFound);
    };
    if definition.status != "borrador" {
        return Ok(DraftPublish::NotDraft);
    }
    let found = problems(&definition);
    if !found.is_empty() {
        return Ok(DraftPublish::Rejected(found));
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
        return Ok(DraftPublish::NotDraft);
    }
    insert_version(&mut tx, &definition, published_by).await?;
    tx.commit().await?;
    Ok(DraftPublish::Published)
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

#[derive(Debug, PartialEq)]
pub enum QuantityRemoval {
    Deleted,
    Retired,
    HasMeasurements,
    NotFound,
}

pub async fn remove_quantity(
    pool: &SqlitePool,
    practice_id: &str,
    quantity_id: &str,
) -> anyhow::Result<QuantityRemoval> {
    let _guard = PUBLISHING.lock().await;
    let (in_version, measured): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM practice_versions v, \
         json_each(v.definition_json, '$.quantities') q \
         WHERE v.practice_id = ?1 AND json_extract(q.value, '$.id') = ?2), \
         EXISTS (SELECT 1 FROM submission_measurements WHERE quantity_id = ?2)",
    )
    .bind(practice_id)
    .bind(quantity_id)
    .fetch_one(pool)
    .await?;
    if measured && !in_version {
        return Ok(QuantityRemoval::HasMeasurements);
    }
    let sql = if in_version {
        "UPDATE practice_quantities SET removed = 1, section_id = NULL, \
         symbol = symbol || '~' || id \
         WHERE id = ?1 AND practice_id = ?2 AND removed = 0"
    } else {
        "DELETE FROM practice_quantities WHERE id = ?1 AND practice_id = ?2 AND removed = 0"
    };
    let changed = sqlx::query(sql)
        .bind(quantity_id)
        .bind(practice_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(match (changed, in_version) {
        (0, _) => QuantityRemoval::NotFound,
        (_, true) => QuantityRemoval::Retired,
        (_, false) => QuantityRemoval::Deleted,
    })
}
