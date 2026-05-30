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
    /// Solo para `regresion_lineal`: expresiones por punto de los ejes `x` e `y` del ajuste.
    pub x_formula: Option<String>,
    pub y_formula: Option<String>,
    pub quantities: Vec<PracticeQuantity>,
    pub results: Vec<PracticeResult>,
}

/// Devuelve la definición completa de una práctica (quantities + results).
pub async fn definition(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT analysis_kind, x_formula, y_formula FROM practices WHERE id = ?1")
            .bind(practice_id)
            .fetch_optional(pool)
            .await?;
    let Some((analysis_kind, x_formula, y_formula)) = row else {
        return Ok(None);
    };
    let quantities = quantities_for(pool, practice_id).await?;
    let results = results_for(pool, practice_id).await?;
    Ok(Some(PracticeDefinition {
        practice_id: practice_id.to_string(),
        analysis_kind,
        x_formula,
        y_formula,
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

/// `true` si ya existe otra magnitud con ese `symbol` en la práctica. `exclude_id` permite
/// ignorar la fila que se está editando (para que renombrar a su propio símbolo no falle).
pub async fn quantity_symbol_taken(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    exclude_id: Option<&str>,
) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities \
         WHERE practice_id = ?1 AND symbol = ?2 AND id <> ?3",
    )
    .bind(practice_id)
    .bind(symbol.trim())
    .bind(exclude_id.unwrap_or(""))
    .fetch_one(pool)
    .await?;
    Ok(count.0 > 0)
}

/// `true` si ya existe otro mensurando con ese `symbol` en la práctica. `exclude_id` permite
/// ignorar la fila que se está editando.
pub async fn result_symbol_taken(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    exclude_id: Option<&str>,
) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_results \
         WHERE practice_id = ?1 AND symbol = ?2 AND id <> ?3",
    )
    .bind(practice_id)
    .bind(symbol.trim())
    .bind(exclude_id.unwrap_or(""))
    .fetch_one(pool)
    .await?;
    Ok(count.0 > 0)
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

/// Actualiza las fórmulas de eje (`x`, `y`) usadas en el ajuste de una práctica de regresión.
/// Una cadena vacía guarda `NULL`. Devuelve `true` si la práctica existía.
pub async fn set_regression_formulas(
    pool: &SqlitePool,
    practice_id: &str,
    x_formula: &str,
    y_formula: &str,
) -> anyhow::Result<bool> {
    let norm = |s: &str| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };
    let result = sqlx::query("UPDATE practices SET x_formula = ?2, y_formula = ?3 WHERE id = ?1")
        .bind(practice_id)
        .bind(norm(x_formula))
        .bind(norm(y_formula))
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Helpers cortos para construir las definiciones del seed.
fn qty(symbol: &str, name: &str, unit: &str, repeated: bool, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated,
        quantity: Some(quantity.into()),
    }
}

fn res(symbol: &str, name: &str, unit: &str, formula: &str) -> ResultInput {
    ResultInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        formula: formula.into(),
    }
}

/// Siembra la definición de una práctica (magnitudes + mensurandos). Idempotente:
/// no hace nada si la práctica ya tiene magnitudes cargadas.
async fn seed_practice(
    pool: &SqlitePool,
    practice_id: &str,
    quantities: &[QuantityInput],
    results: &[ResultInput],
) -> anyhow::Result<()> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1")
            .bind(practice_id)
            .fetch_one(pool)
            .await?;
    if count.0 > 0 {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    for (pos, q) in quantities.iter().enumerate() {
        insert_quantity(&mut conn, practice_id, pos as i64 + 1, q).await?;
    }
    for (pos, r) in results.iter().enumerate() {
        insert_result(&mut conn, practice_id, pos as i64 + 1, r).await?;
    }
    Ok(())
}

/// Siembra las definiciones iniciales de las prácticas (idempotente por práctica).
/// Las magnitudes/fórmulas salen de las técnicas de trabajo de Física 103.
pub async fn seed_definitions(pool: &SqlitePool) -> anyhow::Result<()> {
    // P1 — Tratamiento estadístico: área del cordón Q = l*a + l*b (ejemplo de la cuaderneta).
    seed_practice(
        pool,
        "p1-estadistica",
        &[
            qty("l", "Longitud del cordon", "mm", true, "longitud"),
            qty("a", "Ancho del cordon", "mm", true, "longitud"),
            qty("b", "Espesor del cordon", "mm", true, "longitud"),
        ],
        &[res("Q", "Area transversal del cordon", "mm2", "l*a + l*b")],
    )
    .await?;

    // P3 — Relajación exponencial (parte 1, determinación directa de tau en un RC serie).
    // tau_teorico = (R + Rint)*C ; tau_exp = t_medio/ln2 (porque t_1/2 = tau*ln2).
    // Unidades SI (ohm, F, s) para que tau salga en segundos. Tipo A despreciable -> medida unica.
    // (La parte 2 por desfasaje es regresion_lineal y se agregara cuando este implementada.)
    seed_practice(
        pool,
        "p3-relajacion",
        &[
            qty("R", "Resistencia", "ohm", false, "resistencia"),
            qty(
                "Rint",
                "Resistencia interna de la fuente",
                "ohm",
                false,
                "resistencia",
            ),
            qty("C", "Capacitancia", "F", false, "capacitancia"),
            qty(
                "tmedio",
                "Tiempo de semidescarga (t1/2)",
                "s",
                false,
                "tiempo",
            ),
        ],
        &[
            res(
                "tau_teorico",
                "Tiempo de relajacion teorico",
                "s",
                "(R + Rint) * C",
            ),
            res(
                "tau_exp",
                "Tiempo de relajacion experimental",
                "s",
                "tmedio / 0.6931471805599453",
            ),
        ],
    )
    .await?;

    // P2 — Corriente continua. Circuito: R1 y RA (resistencia interna del amperimetro) en serie
    // con el paralelo de R2 y R3. Req = R1 + RA + 1/(1/R2 + 1/R3); I = Vg/Req.
    // Tipo A despreciable -> medida unica; incertidumbre tipo B (fabricante) del tester/amperimetro.
    seed_practice(
        pool,
        "p2-corriente-continua",
        &[
            qty("Vg", "Voltaje de la fuente", "V", false, "voltaje"),
            qty("R1", "Resistencia R1", "ohm", false, "resistencia"),
            qty("R2", "Resistencia R2", "ohm", false, "resistencia"),
            qty("R3", "Resistencia R3", "ohm", false, "resistencia"),
            qty(
                "RA",
                "Resistencia interna del amperimetro",
                "ohm",
                false,
                "resistencia",
            ),
        ],
        &[
            res(
                "Req",
                "Resistencia equivalente",
                "ohm",
                "R1 + RA + 1/(1/R2 + 1/R3)",
            ),
            res(
                "I",
                "Intensidad de corriente teorica",
                "A",
                "Vg / (R1 + RA + 1/(1/R2 + 1/R3))",
            ),
        ],
    )
    .await?;

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
            .create_if_missing(true)
            .foreign_keys(true);
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
    async fn set_regression_formulas_updates_and_normalizes_empty() {
        let (pool, _dir) = setup().await;
        assert!(set_regression_formulas(
            &pool,
            "p1-estadistica",
            "2*pi*f",
            "b / math::sqrt(a*a - b*b)"
        )
        .await
        .unwrap());
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.x_formula.as_deref(), Some("2*pi*f"));
        assert_eq!(def.y_formula.as_deref(), Some("b / math::sqrt(a*a - b*b)"));

        // Una cadena vacía (o solo espacios) guarda NULL.
        assert!(set_regression_formulas(&pool, "p1-estadistica", "   ", "")
            .await
            .unwrap());
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.x_formula, None);
        assert_eq!(def.y_formula, None);

        // Práctica inexistente devuelve false.
        assert!(!set_regression_formulas(&pool, "no-existe", "f", "f")
            .await
            .unwrap());
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
    async fn deleting_practice_cascades_to_definition() {
        let (pool, _dir) = setup().await;
        create_quantity(&pool, "p1-estadistica", sample_quantity())
            .await
            .unwrap();
        create_result(&pool, "p1-estadistica", sample_result())
            .await
            .unwrap();
        // Con foreign_keys activo, borrar la práctica debe arrastrar magnitudes y mensurandos.
        sqlx::query("DELETE FROM practices WHERE id = 'p1-estadistica'")
            .execute(&pool)
            .await
            .unwrap();
        let quantities: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'p1-estadistica'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let results: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM practice_results WHERE practice_id = 'p1-estadistica'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(quantities.0, 0);
        assert_eq!(results.0, 0);
    }

    #[tokio::test]
    async fn seed_definitions_populates_p2_corriente_continua() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p2-corriente-continua")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(def.quantities.len(), 5);
        for symbol in ["Vg", "R1", "R2", "R3", "RA"] {
            assert!(
                def.quantities.iter().any(|q| q.symbol == symbol),
                "falta la magnitud {symbol}"
            );
        }
        assert!(def.results.iter().any(|r| r.symbol == "I"));
    }

    #[tokio::test]
    async fn seed_definitions_populates_p3_relajacion() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
        assert_eq!(def.quantities.len(), 4);
        for symbol in ["R", "Rint", "C", "tmedio"] {
            assert!(
                def.quantities.iter().any(|q| q.symbol == symbol),
                "falta la magnitud {symbol}"
            );
        }
        let tau_t = def
            .results
            .iter()
            .find(|r| r.symbol == "tau_teorico")
            .unwrap();
        assert_eq!(tau_t.formula, "(R + Rint) * C");
        assert!(def.results.iter().any(|r| r.symbol == "tau_exp"));
    }

    // Verifica que las fórmulas sembradas de P2/P3 son evaluables por el motor (sin NaN/errores).
    #[tokio::test]
    async fn seeded_p2_p3_formulas_compute() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();

        // P3: R=10000, Rint=100, C=1e-8, tmedio=7e-5 -> tau_teorico=(10100)*1e-8=1.01e-4
        let def3 = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
        let m3: Vec<crate::computation::MeasurementInput> = def3
            .quantities
            .iter()
            .map(|q| {
                let v = match q.symbol.as_str() {
                    "R" => 10000.0,
                    "Rint" => 100.0,
                    "C" => 1e-8,
                    _ => 7e-5,
                };
                crate::computation::MeasurementInput {
                    quantity_id: q.id.clone(),
                    instrument_id: None,
                    scale_id: None,
                    values: vec![v],
                }
            })
            .collect();
        let a3 =
            crate::computation::compute(&def3.quantities, &def3.results, &Default::default(), &m3)
                .unwrap();
        let tau_t = a3
            .derived
            .iter()
            .find(|d| d.symbol == "tau_teorico")
            .unwrap();
        assert!((tau_t.value - 1.01e-4).abs() < 1e-12);

        // P2: Vg=8, R1=100, R2=200, R3=200, RA=10 -> Req=100+10+100=210; I=8/210
        let def2 = definition(&pool, "p2-corriente-continua")
            .await
            .unwrap()
            .unwrap();
        let m2: Vec<crate::computation::MeasurementInput> = def2
            .quantities
            .iter()
            .map(|q| {
                let v = match q.symbol.as_str() {
                    "Vg" => 8.0,
                    "RA" => 10.0,
                    "R1" => 100.0,
                    _ => 200.0,
                };
                crate::computation::MeasurementInput {
                    quantity_id: q.id.clone(),
                    instrument_id: None,
                    scale_id: None,
                    values: vec![v],
                }
            })
            .collect();
        let a2 =
            crate::computation::compute(&def2.quantities, &def2.results, &Default::default(), &m2)
                .unwrap();
        let req = a2.derived.iter().find(|d| d.symbol == "Req").unwrap();
        assert!((req.value - 210.0).abs() < 1e-9);
        let i = a2.derived.iter().find(|d| d.symbol == "I").unwrap();
        assert!((i.value - 8.0 / 210.0).abs() < 1e-9);
    }
}
