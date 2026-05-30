//! Definición de prácticas: magnitudes de entrada y mensurandos derivados.
//!
//! Las definiciones son **globales por práctica** (no por curso). Una vez definida P1
//! con sus magnitudes y fórmulas, cualquier curso que habilite P1 usa la misma definición.
//! El cálculo de incertidumbres (Fase 4) lee esta definición para saber qué medir y qué derivar.

use crate::db::{PracticeQuantity, PracticeResult};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Datos para crear o actualizar una magnitud de entrada de una práctica.
#[derive(Debug, Deserialize)]
pub struct QuantityInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// `true` si el estudiante toma varias réplicas (tipo A); `false` para medida única.
    pub repeated: bool,
    /// Magnitud física para sugerir instrumentos compatibles (opcional).
    pub quantity: Option<String>,
}

/// Datos para crear o actualizar un mensurando derivado de una práctica.
#[derive(Debug, Deserialize)]
pub struct ResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// Expresión matemática usando los símbolos de las magnitudes de la práctica.
    pub formula: String,
}

/// Definición completa de una práctica: tipo de análisis, magnitudes y mensurandos.
#[derive(Debug, Serialize)]
pub struct PracticeDefinition {
    pub practice_id: String,
    pub analysis_kind: Option<String>,
    pub quantities: Vec<PracticeQuantity>,
    pub results: Vec<PracticeResult>,
}

/// Devuelve la definición completa de una práctica (quantities + results).
pub async fn definition(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT analysis_kind FROM practices WHERE id = ?1")
            .bind(practice_id)
            .fetch_optional(pool)
            .await?;
    let Some((analysis_kind,)) = row else {
        return Ok(None);
    };
    let quantities = quantities_for(pool, practice_id).await?;
    let results = results_for(pool, practice_id).await?;
    Ok(Some(PracticeDefinition {
        practice_id: practice_id.to_string(),
        analysis_kind,
        quantities,
        results,
    }))
}

/// Crea una magnitud en la práctica; asigna la siguiente posición disponible.
pub async fn create_quantity(
    pool: &SqlitePool,
    practice_id: &str,
    input: QuantityInput,
) -> anyhow::Result<PracticeQuantity> {
    let position: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM practice_quantities WHERE practice_id = ?1",
    )
    .bind(practice_id)
    .fetch_one(pool)
    .await?;
    let id = {
        let mut conn = pool.acquire().await?;
        insert_quantity(&mut conn, practice_id, position.0, &input).await?
    };
    fetch_quantity(pool, &id).await
}

/// Actualiza los datos de una magnitud. Devuelve `None` si no existe.
pub async fn update_quantity(
    pool: &SqlitePool,
    quantity_id: &str,
    input: QuantityInput,
) -> anyhow::Result<Option<PracticeQuantity>> {
    let result = sqlx::query(
        "UPDATE practice_quantities \
         SET symbol = ?2, name = ?3, unit = ?4, repeated = ?5, quantity = ?6 \
         WHERE id = ?1",
    )
    .bind(quantity_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.repeated)
    .bind(input.quantity.as_deref())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_quantity(pool, quantity_id).await?))
}

/// Elimina una magnitud por id. Devuelve `true` si existía.
pub async fn delete_quantity(pool: &SqlitePool, quantity_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM practice_quantities WHERE id = ?1")
        .bind(quantity_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Crea un mensurando derivado en la práctica; asigna la siguiente posición disponible.
pub async fn create_result(
    pool: &SqlitePool,
    practice_id: &str,
    input: ResultInput,
) -> anyhow::Result<PracticeResult> {
    let position: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM practice_results WHERE practice_id = ?1",
    )
    .bind(practice_id)
    .fetch_one(pool)
    .await?;
    let id = {
        let mut conn = pool.acquire().await?;
        insert_result(&mut conn, practice_id, position.0, &input).await?
    };
    fetch_result(pool, &id).await
}

/// Actualiza un mensurando derivado. Devuelve `None` si no existe.
pub async fn update_result(
    pool: &SqlitePool,
    result_id: &str,
    input: ResultInput,
) -> anyhow::Result<Option<PracticeResult>> {
    let res = sqlx::query(
        "UPDATE practice_results \
         SET symbol = ?2, name = ?3, unit = ?4, formula = ?5 \
         WHERE id = ?1",
    )
    .bind(result_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.formula.trim())
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_result(pool, result_id).await?))
}

/// Elimina un mensurando derivado por id. Devuelve `true` si existía.
pub async fn delete_result(pool: &SqlitePool, result_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM practice_results WHERE id = ?1")
        .bind(result_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Actualiza el tipo de análisis de una práctica. Devuelve `true` si existía.
pub async fn set_analysis_kind(
    pool: &SqlitePool,
    practice_id: &str,
    kind: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query("UPDATE practices SET analysis_kind = ?2 WHERE id = ?1")
        .bind(practice_id)
        .bind(kind)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Siembra la definición de P1 (magnitudes `l`, `a`, `b` + mensurando `Q = l*a + l*b`).
/// Idempotente: no hace nada si P1 ya tiene magnitudes cargadas.
pub async fn seed_definitions(pool: &SqlitePool) -> anyhow::Result<()> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'p1-estadistica'",
    )
    .fetch_one(pool)
    .await?;
    if count.0 > 0 {
        return Ok(());
    }

    let quantities = [
        QuantityInput {
            symbol: "l".into(),
            name: "Longitud del cordon".into(),
            unit: "mm".into(),
            repeated: true,
            quantity: Some("longitud".into()),
        },
        QuantityInput {
            symbol: "a".into(),
            name: "Ancho del cordon".into(),
            unit: "mm".into(),
            repeated: true,
            quantity: Some("longitud".into()),
        },
        QuantityInput {
            symbol: "b".into(),
            name: "Espesor del cordon".into(),
            unit: "mm".into(),
            repeated: true,
            quantity: Some("longitud".into()),
        },
    ];
    let mut conn = pool.acquire().await?;
    for (pos, q) in quantities.iter().enumerate() {
        insert_quantity(&mut conn, "p1-estadistica", pos as i64 + 1, q).await?;
    }

    // Q = l*a + l*b (area transversal del cordon, ejemplo de la cuaderneta Fisica 103)
    let result = ResultInput {
        symbol: "Q".into(),
        name: "Area transversal del cordon".into(),
        unit: "mm2".into(),
        formula: "l*a + l*b".into(),
    };
    insert_result(&mut conn, "p1-estadistica", 1, &result).await?;
    Ok(())
}

// ── Helpers internos ─────────────────────────────────────────────────────────

async fn quantities_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeQuantity>> {
    Ok(sqlx::query_as::<_, PracticeQuantity>(
        "SELECT id, practice_id, symbol, name, unit, repeated, quantity, position \
         FROM practice_quantities WHERE practice_id = ?1 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

async fn results_for(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Vec<PracticeResult>> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position \
         FROM practice_results WHERE practice_id = ?1 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_quantity(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeQuantity> {
    Ok(sqlx::query_as::<_, PracticeQuantity>(
        "SELECT id, practice_id, symbol, name, unit, repeated, quantity, position \
         FROM practice_quantities WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

async fn fetch_result(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeResult> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position \
         FROM practice_results WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

async fn insert_quantity(
    conn: &mut SqliteConnection,
    practice_id: &str,
    position: i64,
    input: &QuantityInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_quantities \
         (id, practice_id, symbol, name, unit, repeated, quantity, position) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.repeated)
    .bind(input.quantity.as_deref())
    .bind(position)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

async fn insert_result(
    conn: &mut SqliteConnection,
    practice_id: &str,
    position: i64,
    input: &ResultInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_results \
         (id, practice_id, symbol, name, unit, formula, position) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.formula.trim())
    .bind(position)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Pool temporal migrado con las tres prácticas sembradas.
    async fn setup() -> (SqlitePool, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("t.db").to_string_lossy());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        db::migrate(&pool).await.unwrap();
        db::seed_practices(&pool).await.unwrap();
        (pool, dir)
    }

    fn sample_quantity() -> QuantityInput {
        QuantityInput {
            symbol: "l".into(),
            name: "Longitud".into(),
            unit: "mm".into(),
            repeated: true,
            quantity: Some("longitud".into()),
        }
    }

    fn sample_result() -> ResultInput {
        ResultInput {
            symbol: "Q".into(),
            name: "Area".into(),
            unit: "mm2".into(),
            formula: "l*a".into(),
        }
    }

    #[tokio::test]
    async fn definition_returns_none_for_unknown_practice() {
        let (pool, _dir) = setup().await;
        assert!(definition(&pool, "no-existe").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn create_and_list_quantities() {
        let (pool, _dir) = setup().await;
        let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
            .await
            .unwrap();
        assert_eq!(q.symbol, "l");
        assert_eq!(q.practice_id, "p1-estadistica");

        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.quantities.len(), 1);
        assert_eq!(def.quantities[0].id, q.id);
    }

    #[tokio::test]
    async fn update_and_delete_quantity() {
        let (pool, _dir) = setup().await;
        let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
            .await
            .unwrap();

        let updated = update_quantity(
            &pool,
            &q.id,
            QuantityInput {
                symbol: "a".into(),
                name: "Ancho".into(),
                unit: "cm".into(),
                repeated: false,
                quantity: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.symbol, "a");
        assert!(!updated.repeated);

        assert!(delete_quantity(&pool, &q.id).await.unwrap());
        assert!(!delete_quantity(&pool, &q.id).await.unwrap());
    }

    #[tokio::test]
    async fn create_and_delete_result() {
        let (pool, _dir) = setup().await;
        let r = create_result(&pool, "p1-estadistica", sample_result())
            .await
            .unwrap();
        assert_eq!(r.symbol, "Q");
        assert_eq!(r.formula, "l*a");

        assert!(delete_result(&pool, &r.id).await.unwrap());
        assert!(!delete_result(&pool, &r.id).await.unwrap());
    }

    #[tokio::test]
    async fn duplicate_symbol_is_rejected() {
        let (pool, _dir) = setup().await;
        create_quantity(&pool, "p1-estadistica", sample_quantity())
            .await
            .unwrap();
        // Mismo símbolo en la misma práctica debe fallar (UNIQUE constraint).
        let err = create_quantity(&pool, "p1-estadistica", sample_quantity()).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn set_analysis_kind_updates_practice() {
        let (pool, _dir) = setup().await;
        assert!(
            set_analysis_kind(&pool, "p1-estadistica", "regresion_lineal")
                .await
                .unwrap()
        );
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
    }

    #[tokio::test]
    async fn seed_definitions_populates_p1_and_is_idempotent() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.quantities.len(), 3);
        assert!(def.quantities.iter().any(|q| q.symbol == "l"));
        assert!(def.quantities.iter().any(|q| q.symbol == "a"));
        assert!(def.quantities.iter().any(|q| q.symbol == "b"));
        assert_eq!(def.results.len(), 1);
        assert_eq!(def.results[0].formula, "l*a + l*b");

        // Segunda pasada: no debe duplicar.
        seed_definitions(&pool).await.unwrap();
        let def2 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def2.quantities.len(), 3);
        assert_eq!(def2.results.len(), 1);
    }

    #[tokio::test]
    async fn p2_and_p3_start_empty() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        for id in ["p2-corriente-continua", "p3-relajacion"] {
            let def = definition(&pool, id).await.unwrap().unwrap();
            assert!(
                def.quantities.is_empty(),
                "{id} should start with no quantities"
            );
            assert!(def.results.is_empty(), "{id} should start with no results");
        }
    }
}
