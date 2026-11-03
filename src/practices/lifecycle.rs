//! Ciclo de vida de una práctica (F1 del roadmap): alta en borrador, edición de nombre y
//! descripción, duplicado, cambio de estado y borrado de un borrador.

use super::{NewPracticeInput, PracticeInfoInput};
use crate::db::{Practice, PRACTICE_COLS};
use sqlx::{SqliteConnection, SqlitePool};

/// Tablas de la definición de una práctica, que se copian al duplicarla.
const DEFINITION_TABLES: [&str; 6] = [
    "practice_quantities",
    "practice_results",
    "practice_curves",
    "practice_intermediates",
    "practice_point_results",
    "practice_aggregates",
];

/// Id legible a partir del nombre: minúsculas, sin tildes, solo `[a-z0-9-]`.
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

/// Primer id libre a partir de `base`: `base`, `base-2`, `base-3`…
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

/// Una práctica por id (o `None`).
pub async fn practice(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Option<Practice>> {
    Ok(sqlx::query_as::<_, Practice>(&format!(
        "SELECT {PRACTICE_COLS} FROM practices WHERE id = ?1"
    ))
    .bind(practice_id)
    .fetch_optional(pool)
    .await?)
}

/// Crea una práctica vacía en borrador, con id derivado del nombre.
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

/// Cambia nombre y descripción. `None` si la práctica no existe.
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

/// Copia una práctica entera (configuración y definición, con ids nuevos) como borrador. No copia
/// sus habilitaciones en cursos, subgrupos ni mesas. `None` si el original no existe.
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
    sqlx::query(
        "INSERT INTO practices (id, name, description, analysis_kind, x_formula, y_formula, \
         operator_count, status, created_by, created_at) \
         SELECT ?2, ?3, description, analysis_kind, x_formula, y_formula, operator_count, \
         'borrador', ?4, ?5 FROM practices WHERE id = ?1",
    )
    .bind(source_id)
    .bind(&id)
    .bind(name.trim())
    .bind(created_by)
    .bind(chrono::Utc::now())
    .execute(&mut *tx)
    .await?;
    // Columnas leídas del esquema: una columna nueva de la definición se copia sin tocar esto.
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
    tx.commit().await?;
    practice(pool, &id).await
}

/// Resultado de pedir un cambio de estado.
#[derive(Debug, PartialEq, Eq)]
pub enum StatusChange {
    Done,
    NotFound,
    /// Transición no permitida desde el estado actual (que se informa).
    NotAllowed(String),
}

/// Cambia el estado: `borrador` → `publicada`, `publicada` → `archivada`, `archivada` →
/// `publicada`. No se vuelve a borrador. Lo que exige publicar lo valida la ruta antes.
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
    // Condicionado al estado leído: si otro pedido lo cambió en el medio, no se pisa.
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

/// Resultado de pedir el borrado de una práctica.
#[derive(Debug, PartialEq, Eq)]
pub enum DeletePractice {
    Deleted,
    NotFound,
    /// Solo se borra un borrador: una publicada o archivada se archiva.
    NotDraft,
    /// Tiene entregas (no se borran en cascada).
    HasSubmissions,
}

/// Borra un borrador sin entregas, con toda su definición (las tablas hijas van en cascada).
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
    sqlx::query("DELETE FROM practices WHERE id = ?1")
        .bind(practice_id)
        .execute(pool)
        .await?;
    Ok(DeletePractice::Deleted)
}
