//! Definición de prácticas: magnitudes de entrada y mensurandos derivados.
//!
//! Las definiciones son **globales por práctica** (no por curso). Una vez definida P1
//! con sus magnitudes y fórmulas, cualquier curso que habilite P1 usa la misma definición.
//! El cálculo de incertidumbres (Fase 4) lee esta definición para saber qué medir y qué derivar.

use crate::db::{PracticeQuantity, PracticeResult};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Deserializador para `Option<Option<T>>` que distingue campo ausente de `null` explícito.
///
/// El derive estándar de serde mapea tanto "ausente" como `null` a `None`, por lo que
/// `Option<Option<T>>` no puede representar las tres variantes. Este helper envuelve
/// cualquier valor presente (incluso `null`) en `Some(...)`, preservando la semántica:
/// - campo ausente → `None`
/// - `null` explícito → `Some(None)`
/// - valor numérico → `Some(Some(v))`
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

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
    /// `true` si es un dato dado por la cátedra (valor ± U directo, sin instrumento ni réplicas).
    #[serde(default)]
    pub is_given: bool,
    /// Réplicas por punto (grilla) para magnitudes `repeated` en regresión/curva. `None` = sin grilla.
    #[serde(default)]
    pub replicas_per_point: Option<i64>,
}

/// Datos para crear o actualizar un mensurando derivado de una práctica.
#[derive(Debug, Deserialize)]
pub struct ResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// Expresión matemática usando los símbolos de las magnitudes de la práctica.
    pub formula: String,
    /// Tolerancia máxima aceptable como |Δ%|.
    ///
    /// `None` (campo ausente en el JSON) = no modificar la tolerancia existente.
    /// `Some(None)` (campo presente con valor `null`) = borrar la tolerancia.
    /// `Some(Some(v))` = fijar la tolerancia a `v`.
    #[serde(default, deserialize_with = "double_option")]
    pub tolerance: Option<Option<f64>>,
}

/// Definición completa de una práctica: tipo de análisis, magnitudes y mensurandos.
#[derive(Debug, Serialize)]
pub struct PracticeDefinition {
    pub practice_id: String,
    pub analysis_kind: Option<String>,
    /// Solo `regresion_lineal`: expresiones por punto de los ejes `x` e `y` del ajuste.
    pub x_formula: Option<String>,
    pub y_formula: Option<String>,
    pub quantities: Vec<PracticeQuantity>,
    pub results: Vec<PracticeResult>,
    /// Solo `curva`: curvas a graficar sobre el mismo barrido (una o varias, p. ej. en Filtros).
    pub curves: Vec<PracticeCurve>,
    /// Solo estadístico (Motor D): cantidad de operadores que cargan su propia serie. `None` o ≤1
    /// = sin operadores (comportamiento por defecto, una sola serie por magnitud).
    pub operator_count: Option<i64>,
    /// Solo regresión/curva (Motor C): magnitudes intermedias por punto (promedio del derivado por
    /// réplica), disponibles como símbolos en las fórmulas de eje.
    pub intermediates: Vec<PracticeIntermediate>,
}

/// Una curva de una práctica `curva`: un par de fórmulas de eje sobre el barrido común, con eje x
/// logarítmico opcional. `position` ordena las curvas en el gráfico.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticeCurve {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub x_formula: String,
    pub y_formula: String,
    pub x_log: bool,
}

/// Datos para crear o actualizar una curva de una práctica `curva`.
#[derive(Debug, Deserialize)]
pub struct CurveInput {
    pub x_formula: String,
    pub y_formula: String,
    #[serde(default)]
    pub x_log: bool,
}

/// Magnitud intermedia por punto (Motor C) de una práctica de regresión/curva: su `formula` se
/// evalúa por réplica de cada punto y se promedia, quedando disponible como símbolo en los ejes.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticeIntermediate {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Datos para crear o actualizar una magnitud intermedia por punto.
#[derive(Debug, Deserialize)]
pub struct IntermediateInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Fila cruda con la configuración de análisis de una práctica.
#[derive(sqlx::FromRow)]
struct PracticeConfigRow {
    analysis_kind: Option<String>,
    x_formula: Option<String>,
    y_formula: Option<String>,
    operator_count: Option<i64>,
}

/// Devuelve la definición completa de una práctica (quantities + results).
pub async fn definition(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    let row: Option<PracticeConfigRow> = sqlx::query_as(
        "SELECT analysis_kind, x_formula, y_formula, operator_count FROM practices WHERE id = ?1",
    )
    .bind(practice_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let quantities = quantities_for(pool, practice_id).await?;
    let results = results_for(pool, practice_id).await?;
    let curves = curves_for(pool, practice_id).await?;
    let intermediates = intermediates_for(pool, practice_id).await?;
    Ok(Some(PracticeDefinition {
        practice_id: practice_id.to_string(),
        analysis_kind: row.analysis_kind,
        x_formula: row.x_formula,
        y_formula: row.y_formula,
        quantities,
        results,
        curves,
        operator_count: row.operator_count,
        intermediates,
    }))
}

/// Lee las magnitudes intermedias por punto de una práctica (Motor C), ordenadas por posición.
pub async fn intermediates_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeIntermediate>> {
    Ok(sqlx::query_as::<_, PracticeIntermediate>(
        "SELECT id, practice_id, position, symbol, name, unit, formula \
         FROM practice_intermediates WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

/// Crea una magnitud intermedia; asigna la siguiente posición. Símbolo y fórmula obligatorios.
pub async fn create_intermediate(
    pool: &SqlitePool,
    practice_id: &str,
    input: IntermediateInput,
) -> anyhow::Result<PracticeIntermediate> {
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        anyhow::bail!("la magnitud intermedia necesita símbolo y fórmula");
    }
    let position: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM practice_intermediates WHERE practice_id = ?1",
    )
    .bind(practice_id)
    .fetch_one(pool)
    .await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_intermediates (id, practice_id, position, symbol, name, unit, formula) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(position.0)
    .bind(symbol)
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(formula)
    .execute(pool)
    .await?;
    fetch_intermediate(pool, &id).await
}

/// Actualiza una magnitud intermedia de esa práctica. Devuelve `None` si no existe.
pub async fn update_intermediate(
    pool: &SqlitePool,
    practice_id: &str,
    intermediate_id: &str,
    input: IntermediateInput,
) -> anyhow::Result<Option<PracticeIntermediate>> {
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        anyhow::bail!("la magnitud intermedia necesita símbolo y fórmula");
    }
    let result = sqlx::query(
        "UPDATE practice_intermediates SET symbol = ?3, name = ?4, unit = ?5, formula = ?6 \
         WHERE id = ?1 AND practice_id = ?2",
    )
    .bind(intermediate_id)
    .bind(practice_id)
    .bind(symbol)
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(formula)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_intermediate(pool, intermediate_id).await?))
}

/// Elimina una magnitud intermedia de esa práctica por id. Devuelve `true` si existía.
pub async fn delete_intermediate(
    pool: &SqlitePool,
    practice_id: &str,
    intermediate_id: &str,
) -> anyhow::Result<bool> {
    let result =
        sqlx::query("DELETE FROM practice_intermediates WHERE id = ?1 AND practice_id = ?2")
            .bind(intermediate_id)
            .bind(practice_id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Lee una magnitud intermedia por su id.
async fn fetch_intermediate(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeIntermediate> {
    Ok(sqlx::query_as::<_, PracticeIntermediate>(
        "SELECT id, practice_id, position, symbol, name, unit, formula \
         FROM practice_intermediates WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Lee las curvas de una práctica (Motor B), ordenadas por posición.
pub async fn curves_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeCurve>> {
    Ok(sqlx::query_as::<_, PracticeCurve>(
        "SELECT id, practice_id, position, x_formula, y_formula, x_log \
         FROM practice_curves WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

/// Crea una curva en la práctica; asigna la siguiente posición disponible. Las fórmulas se
/// recortan; ambas son obligatorias (una curva sin ejes no se puede graficar).
pub async fn create_curve(
    pool: &SqlitePool,
    practice_id: &str,
    input: CurveInput,
) -> anyhow::Result<PracticeCurve> {
    let x = input.x_formula.trim();
    let y = input.y_formula.trim();
    if x.is_empty() || y.is_empty() {
        anyhow::bail!("la curva necesita las fórmulas de ambos ejes");
    }
    let position: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM practice_curves WHERE practice_id = ?1",
    )
    .bind(practice_id)
    .fetch_one(pool)
    .await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_curves (id, practice_id, position, x_formula, y_formula, x_log) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(position.0)
    .bind(x)
    .bind(y)
    .bind(input.x_log)
    .execute(pool)
    .await?;
    fetch_curve(pool, &id).await
}

/// Actualiza las fórmulas y el flag `x_log` de una curva **de esa práctica**. Devuelve `None` si
/// no existe una curva con ese id en la práctica indicada.
pub async fn update_curve(
    pool: &SqlitePool,
    practice_id: &str,
    curve_id: &str,
    input: CurveInput,
) -> anyhow::Result<Option<PracticeCurve>> {
    let x = input.x_formula.trim();
    let y = input.y_formula.trim();
    if x.is_empty() || y.is_empty() {
        anyhow::bail!("la curva necesita las fórmulas de ambos ejes");
    }
    let result = sqlx::query(
        "UPDATE practice_curves SET x_formula = ?3, y_formula = ?4, x_log = ?5 \
         WHERE id = ?1 AND practice_id = ?2",
    )
    .bind(curve_id)
    .bind(practice_id)
    .bind(x)
    .bind(y)
    .bind(input.x_log)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_curve(pool, curve_id).await?))
}

/// Elimina una curva de esa práctica por id. Devuelve `true` si existía en la práctica indicada.
pub async fn delete_curve(
    pool: &SqlitePool,
    practice_id: &str,
    curve_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM practice_curves WHERE id = ?1 AND practice_id = ?2")
        .bind(curve_id)
        .bind(practice_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Mueve una curva una posición hacia arriba (`up = true`) o hacia abajo dentro de su práctica,
/// intercambiando su `position` con la de la curva vecina. Devuelve `false` si la curva no existe
/// en esa práctica o ya está en el extremo correspondiente.
pub async fn move_curve(
    pool: &SqlitePool,
    practice_id: &str,
    curve_id: &str,
    up: bool,
) -> anyhow::Result<bool> {
    // Lee y reordena dentro de la misma transacción: así dos reordenamientos concurrentes no se
    // pisan (SQLite aborta el segundo con un error de snapshot en vez de corromper el orden).
    let mut tx = pool.begin().await?;
    let curves = sqlx::query_as::<_, PracticeCurve>(
        "SELECT id, practice_id, position, x_formula, y_formula, x_log \
         FROM practice_curves WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(&mut *tx)
    .await?;
    let Some(idx) = curves.iter().position(|c| c.id == curve_id) else {
        return Ok(false);
    };
    let neighbor = if up {
        idx.checked_sub(1)
    } else {
        Some(idx + 1).filter(|&j| j < curves.len())
    };
    let Some(j) = neighbor else {
        return Ok(false);
    };
    for (id, position) in [
        (&curves[idx].id, curves[j].position),
        (&curves[j].id, curves[idx].position),
    ] {
        sqlx::query("UPDATE practice_curves SET position = ?2 WHERE id = ?1")
            .bind(id)
            .bind(position)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// Lee una curva por su id.
async fn fetch_curve(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeCurve> {
    Ok(sqlx::query_as::<_, PracticeCurve>(
        "SELECT id, practice_id, position, x_formula, y_formula, x_log \
         FROM practice_curves WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
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
         SET symbol = ?2, name = ?3, unit = ?4, repeated = ?5, quantity = ?6, is_given = ?7, \
             replicas_per_point = ?8 \
         WHERE id = ?1",
    )
    .bind(quantity_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.repeated)
    .bind(input.quantity.as_deref())
    .bind(input.is_given)
    .bind(input.replicas_per_point)
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
/// Si `input.tolerance` es `None` (campo ausente), la columna `tolerance` no se modifica.
pub async fn update_result(
    pool: &SqlitePool,
    result_id: &str,
    input: ResultInput,
) -> anyhow::Result<Option<PracticeResult>> {
    let rows = match input.tolerance {
        None => sqlx::query(
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
        .await?
        .rows_affected(),
        Some(tol) => sqlx::query(
            "UPDATE practice_results \
                 SET symbol = ?2, name = ?3, unit = ?4, formula = ?5, tolerance = ?6 \
                 WHERE id = ?1",
        )
        .bind(result_id)
        .bind(input.symbol.trim())
        .bind(input.name.trim())
        .bind(input.unit.trim())
        .bind(input.formula.trim())
        .bind(tol)
        .execute(pool)
        .await?
        .rows_affected(),
    };
    if rows == 0 {
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

/// `true` si `symbol` ya está tomado por alguna magnitud o mensurando de la práctica.
///
/// `exclude_quantity_id` / `exclude_result_id` permiten ignorar la fila que se está editando
/// (para que renombrar a su propio símbolo no falle).
pub async fn symbol_taken_in_practice(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    exclude_quantity_id: Option<&str>,
    exclude_result_id: Option<&str>,
) -> anyhow::Result<bool> {
    let sym = symbol.trim();
    let in_quantities: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities \
         WHERE practice_id = ?1 AND symbol = ?2 AND id <> ?3",
    )
    .bind(practice_id)
    .bind(sym)
    .bind(exclude_quantity_id.unwrap_or(""))
    .fetch_one(pool)
    .await?;
    if in_quantities.0 > 0 {
        return Ok(true);
    }
    let in_results: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_results \
         WHERE practice_id = ?1 AND symbol = ?2 AND id <> ?3",
    )
    .bind(practice_id)
    .bind(sym)
    .bind(exclude_result_id.unwrap_or(""))
    .fetch_one(pool)
    .await?;
    Ok(in_results.0 > 0)
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

/// Fija la cantidad de operadores de una práctica estadística (Motor D). `count <= 1` guarda `NULL`
/// (sin operadores, comportamiento por defecto). Devuelve `true` si la práctica existía.
pub async fn set_operator_count(
    pool: &SqlitePool,
    practice_id: &str,
    count: i64,
) -> anyhow::Result<bool> {
    let stored: Option<i64> = if count <= 1 { None } else { Some(count) };
    let result = sqlx::query("UPDATE practices SET operator_count = ?2 WHERE id = ?1")
        .bind(practice_id)
        .bind(stored)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Actualiza las fórmulas de eje (`x`, `y`) del ajuste lineal de una práctica `regresion_lineal`.
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

/// Construye un `QuantityInput` (magnitud de entrada) para el seed de definiciones.
fn qty(symbol: &str, name: &str, unit: &str, repeated: bool, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated,
        quantity: Some(quantity.into()),
        is_given: false,
        replicas_per_point: None,
    }
}

/// Construye un `QuantityInput` para un dato dado por la cátedra (valor ± U, sin réplicas).
fn qty_given(symbol: &str, name: &str, unit: &str, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated: false,
        quantity: Some(quantity.into()),
        is_given: true,
        replicas_per_point: None,
    }
}

/// Construye un `ResultInput` (mensurando derivado) para el seed de definiciones.
fn res(symbol: &str, name: &str, unit: &str, formula: &str) -> ResultInput {
    ResultInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        formula: formula.into(),
        tolerance: None,
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
    // P1 — Péndulo simple: T medido con cronómetro (réplicas), L dado por cátedra.
    // g = 4*pi^2*L/T^2 ; T y L en SI (s y m) para que g salga en m/s^2.
    // Tres secciones que comparten datos: (1) Periodos -> T (cronometro, replicas);
    // (2) Amortiguamiento -> t_med (t1/2) da delta=ln2/t1/2, gamma=2*delta y Q=w0/gamma;
    // (3) Gravedad -> g = 4*pi^2*L/T^2 (usa T medio y L dado por catedra).
    seed_practice(
        pool,
        "p1-estadistica",
        &[
            qty("T", "Periodo", "s", true, "tiempo"),
            qty_given("L", "Longitud del pendulo", "m", "longitud"),
            qty(
                "t_med",
                "Tiempo de semiamplitud (t1/2)",
                "s",
                false,
                "tiempo",
            ),
        ],
        &[
            res("Tmedio", "Periodo medio", "s", "T"),
            res(
                "delta",
                "Constante de amortiguamiento",
                "1/s",
                "math::ln(2)/t_med",
            ),
            res(
                "gamma",
                "Coeficiente de amortiguamiento",
                "1/s",
                "2*math::ln(2)/t_med",
            ),
            res("Q", "Factor de calidad", "", "pi*t_med/(T*math::ln(2))"),
            res("g", "Aceleracion de gravedad", "m/s2", "4*pi^2*L/T^2"),
        ],
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
            // Rint es un dato entregado por la cátedra (valor ± U), no lo mide el alumno.
            qty_given(
                "Rint",
                "Resistencia interna de la fuente",
                "ohm",
                "resistencia",
            ),
            qty("C", "Capacitancia", "F", false, "capacitancia"),
            // Periodo de la onda cuadrada de trabajo (se registra; debe permitir ver ~5*tau
            // en el semiperiodo de descarga). No entra en las formulas, queda como dato medido.
            qty("T_oc", "Periodo de la onda cuadrada", "s", false, "tiempo"),
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
                "tmedio / math::ln(2)",
            ),
        ],
    )
    .await?;

    // P2-serie — Circuito en serie: R1, R2 y R3 en serie con RA (resistencia interna del
    // amperimetro). I = Vg/(R1+R2+R3+RA) y la caida de tension en cada resistencia es V=I*R.
    // Medida unica (tipo A despreciable); incertidumbre tipo B (fabricante del tester).
    seed_practice(
        pool,
        "p2-serie",
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
                "I",
                "Intensidad de corriente",
                "A",
                "Vg / (R1 + R2 + R3 + RA)",
            ),
            res("VR1", "Tension en R1", "V", "Vg * R1 / (R1 + R2 + R3 + RA)"),
            res("VR2", "Tension en R2", "V", "Vg * R2 / (R1 + R2 + R3 + RA)"),
            res("VR3", "Tension en R3", "V", "Vg * R3 / (R1 + R2 + R3 + RA)"),
        ],
    )
    .await?;

    // P2-paralelo — Circuito mixto: R2 y R3 en paralelo, en serie con R1 y RA.
    // Req = R1 + RA + R2*R3/(R2+R3); I = Vg/Req.
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
                "R1 + RA + R2*R3/(R2+R3)",
            ),
            res(
                "I",
                "Intensidad de corriente teorica",
                "A",
                "Vg / (R1 + RA + R2*R3/(R2+R3))",
            ),
        ],
    )
    .await?;

    // P3 — parte 2 (desfasaje por figura de Lissajous). El alumno carga una serie de puntos
    // con f, a y b; las fórmulas de eje (en `practices.x_formula`/`y_formula`) derivan
    // x = 2*pi*f (= omega) y y = b/sqrt(a^2 - b^2) (= tg phi). La pendiente del ajuste es
    // RC = tau, que se referencia con el símbolo especial `slope`.
    seed_practice(
        pool,
        "p3-relajacion-desfasaje",
        &[
            qty("f", "Frecuencia", "Hz", true, "frecuencia"),
            qty(
                "a",
                "Amplitud de la senal en el eje y de la elipse",
                "div",
                true,
                "longitud",
            ),
            qty(
                "b",
                "Interseccion de la elipse con el eje y",
                "div",
                true,
                "longitud",
            ),
        ],
        &[res("tau", "Constante de tiempo RC", "s", "slope")],
    )
    .await?;

    Ok(())
}

/// Fija (o borra) la tolerancia porcentual de un mensurando derivado.
/// `None` elimina el veredicto para ese mensurando. Devuelve `true` si el mensurando
/// existe y pertenece a `practice_id`.
pub async fn set_result_tolerance(
    pool: &SqlitePool,
    result_id: &str,
    practice_id: &str,
    tolerance: Option<f64>,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE practice_results SET tolerance = ?2 WHERE id = ?1 AND practice_id = ?3",
    )
    .bind(result_id)
    .bind(tolerance)
    .bind(practice_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

// ── Helpers internos ─────────────────────────────────────────────────────────

/// Lee las magnitudes de entrada de una práctica, ordenadas por posición y símbolo.
async fn quantities_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeQuantity>> {
    Ok(sqlx::query_as::<_, PracticeQuantity>(
        "SELECT id, practice_id, symbol, name, unit, repeated, quantity, position, is_given, \
         replicas_per_point \
         FROM practice_quantities WHERE practice_id = ?1 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

/// Lee los mensurandos derivados de una práctica, ordenados por posición y símbolo.
async fn results_for(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Vec<PracticeResult>> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position, tolerance \
         FROM practice_results WHERE practice_id = ?1 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

/// Lee una magnitud de entrada por su id.
async fn fetch_quantity(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeQuantity> {
    Ok(sqlx::query_as::<_, PracticeQuantity>(
        "SELECT id, practice_id, symbol, name, unit, repeated, quantity, position, is_given, \
         replicas_per_point \
         FROM practice_quantities WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Lee un mensurando derivado por su id.
async fn fetch_result(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeResult> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position, tolerance \
         FROM practice_results WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Inserta una magnitud de entrada en la práctica con la posición dada; devuelve su id generado.
async fn insert_quantity(
    conn: &mut SqliteConnection,
    practice_id: &str,
    position: i64,
    input: &QuantityInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_quantities \
         (id, practice_id, symbol, name, unit, repeated, quantity, position, is_given, \
          replicas_per_point) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.repeated)
    .bind(input.quantity.as_deref())
    .bind(position)
    .bind(input.is_given)
    .bind(input.replicas_per_point)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

/// Inserta un mensurando derivado en la práctica con la posición dada; devuelve su id generado.
async fn insert_result(
    conn: &mut SqliteConnection,
    practice_id: &str,
    position: i64,
    input: &ResultInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_results \
         (id, practice_id, symbol, name, unit, formula, position, tolerance) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.formula.trim())
    .bind(position)
    .bind(input.tolerance.flatten())
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
            is_given: false,
            replicas_per_point: None,
        }
    }

    fn sample_result() -> ResultInput {
        ResultInput {
            symbol: "Q".into(),
            name: "Area".into(),
            unit: "mm2".into(),
            formula: "l*a".into(),
            tolerance: None,
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
                is_given: false,
                replicas_per_point: None,
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
    async fn symbol_taken_detects_cross_table_collision() {
        let (pool, _dir) = setup().await;
        // Crea una magnitud con símbolo "l".
        let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
            .await
            .unwrap();

        // symbol_taken_in_practice lo detecta buscando en quantities.
        assert!(
            symbol_taken_in_practice(&pool, "p1-estadistica", "l", None, None)
                .await
                .unwrap()
        );
        // Excluir la misma magnitud (al renombrar) no debe reportar colisión.
        assert!(
            !symbol_taken_in_practice(&pool, "p1-estadistica", "l", Some(&q.id), None)
                .await
                .unwrap()
        );

        // Crea un mensurando con símbolo "Q".
        let r = create_result(&pool, "p1-estadistica", sample_result())
            .await
            .unwrap();

        // Un mensurando nuevo con símbolo "l" (ya en quantities) es colisión cruzada.
        assert!(
            symbol_taken_in_practice(&pool, "p1-estadistica", "l", None, Some(&r.id))
                .await
                .unwrap()
        );
        // Una magnitud nueva con símbolo "Q" (ya en results) es colisión cruzada.
        assert!(
            symbol_taken_in_practice(&pool, "p1-estadistica", "Q", Some(&q.id), None)
                .await
                .unwrap()
        );
        // Símbolo inexistente no colisiona.
        assert!(
            !symbol_taken_in_practice(&pool, "p1-estadistica", "nuevo", None, None)
                .await
                .unwrap()
        );
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
            "b / math::sqrt(a*a - b*b)",
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
    async fn curve_crud_roundtrip_and_ordering() {
        let (pool, _dir) = setup().await;
        // Alta de dos curvas: quedan ordenadas por posición creciente, con x_log por curva.
        let c1 = create_curve(
            &pool,
            "p1-estadistica",
            CurveInput {
                x_formula: " logw ".into(), // se recorta
                y_formula: "VR / Vg".into(),
                x_log: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(c1.x_formula, "logw");
        assert!(c1.x_log);
        create_curve(
            &pool,
            "p1-estadistica",
            CurveInput {
                x_formula: "logw".into(),
                y_formula: "phi".into(),
                x_log: false,
            },
        )
        .await
        .unwrap();

        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.curves.len(), 2);
        assert_eq!(def.curves[0].position, 1);
        assert_eq!(def.curves[0].y_formula, "VR / Vg");
        assert_eq!(def.curves[1].position, 2);
        assert_eq!(def.curves[1].y_formula, "phi");

        // Edición de una curva (acotada por práctica).
        let updated = update_curve(
            &pool,
            "p1-estadistica",
            &c1.id,
            CurveInput {
                x_formula: "logw".into(),
                y_formula: "Vg / VR".into(),
                x_log: false,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.y_formula, "Vg / VR");
        assert!(!updated.x_log);

        // Editar/borrar con la práctica equivocada no afecta la curva (el id no pertenece a esa
        // práctica): update → None, delete → false.
        assert!(update_curve(
            &pool,
            "p2-serie",
            &c1.id,
            CurveInput {
                x_formula: "a".into(),
                y_formula: "b".into(),
                x_log: false,
            },
        )
        .await
        .unwrap()
        .is_none());
        assert!(!delete_curve(&pool, "p2-serie", &c1.id).await.unwrap());

        // Baja correcta: queda una sola curva.
        assert!(delete_curve(&pool, "p1-estadistica", &c1.id).await.unwrap());
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.curves.len(), 1);
        assert_eq!(def.curves[0].y_formula, "phi");
    }

    #[tokio::test]
    async fn move_curve_swaps_position_with_neighbor() {
        let (pool, _dir) = setup().await;
        let mk = |y: &str| CurveInput {
            x_formula: "logw".into(),
            y_formula: y.into(),
            x_log: false,
        };
        let a = create_curve(&pool, "p1-estadistica", mk("a"))
            .await
            .unwrap();
        create_curve(&pool, "p1-estadistica", mk("b"))
            .await
            .unwrap();
        let c = create_curve(&pool, "p1-estadistica", mk("c"))
            .await
            .unwrap();

        // 'a' no puede subir (ya es la primera); 'c' no puede bajar (ya es la última).
        assert!(!move_curve(&pool, "p1-estadistica", &a.id, true)
            .await
            .unwrap());
        assert!(!move_curve(&pool, "p1-estadistica", &c.id, false)
            .await
            .unwrap());

        // Bajar 'a' la intercambia con 'b' → orden b, a, c.
        assert!(move_curve(&pool, "p1-estadistica", &a.id, false)
            .await
            .unwrap());
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(
            def.curves
                .iter()
                .map(|c| c.y_formula.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );

        // Curva inexistente devuelve false.
        assert!(!move_curve(&pool, "p1-estadistica", "no-existe", true)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn deleting_practice_cascades_to_curves() {
        let (pool, _dir) = setup().await;
        create_curve(
            &pool,
            "p1-estadistica",
            CurveInput {
                x_formula: "logw".into(),
                y_formula: "VR / Vg".into(),
                x_log: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(curves_for(&pool, "p1-estadistica").await.unwrap().len(), 1);

        // Con foreign_keys activo, borrar la práctica arrastra sus curvas (ON DELETE CASCADE).
        sqlx::query("DELETE FROM practices WHERE id = ?1")
            .bind("p1-estadistica")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(curves_for(&pool, "p1-estadistica").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn create_curve_requires_both_formulas() {
        let (pool, _dir) = setup().await;
        assert!(create_curve(
            &pool,
            "p1-estadistica",
            CurveInput {
                x_formula: "logw".into(),
                y_formula: "  ".into(),
                x_log: false,
            },
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn intermediate_crud_roundtrip() {
        let (pool, _dir) = setup().await;
        let q = create_intermediate(
            &pool,
            "p1-estadistica",
            IntermediateInput {
                symbol: " Q ".into(), // se recorta
                name: "Caudal".into(),
                unit: "m3/s".into(),
                formula: "V/t".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(q.symbol, "Q");
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def.intermediates.len(), 1);
        assert_eq!(def.intermediates[0].formula, "V/t");

        // Editar acotado por práctica; práctica equivocada → None.
        assert!(update_intermediate(
            &pool,
            "p2-serie",
            &q.id,
            IntermediateInput {
                symbol: "Q".into(),
                name: "x".into(),
                unit: "x".into(),
                formula: "V*t".into(),
            },
        )
        .await
        .unwrap()
        .is_none());
        let updated = update_intermediate(
            &pool,
            "p1-estadistica",
            &q.id,
            IntermediateInput {
                symbol: "Q".into(),
                name: "Caudal".into(),
                unit: "m3/s".into(),
                formula: "V*t".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.formula, "V*t");

        // Símbolo/fórmula vacíos → error.
        assert!(create_intermediate(
            &pool,
            "p1-estadistica",
            IntermediateInput {
                symbol: "Z".into(),
                name: "z".into(),
                unit: "".into(),
                formula: "   ".into(),
            },
        )
        .await
        .is_err());

        assert!(delete_intermediate(&pool, "p1-estadistica", &q.id)
            .await
            .unwrap());
        assert_eq!(
            intermediates_for(&pool, "p1-estadistica")
                .await
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn seed_definitions_populates_p1_and_is_idempotent() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        // P1 péndulo: T (repeated) + L (is_given) + t_med (t1/2).
        assert_eq!(def.quantities.len(), 3);
        let t = def.quantities.iter().find(|q| q.symbol == "T").unwrap();
        assert!(t.repeated);
        let l = def.quantities.iter().find(|q| q.symbol == "L").unwrap();
        assert!(l.is_given);
        assert_eq!(def.results.len(), 5);
        for symbol in ["Tmedio", "delta", "gamma", "Q", "g"] {
            assert!(
                def.results.iter().any(|r| r.symbol == symbol),
                "falta el resultado {symbol}"
            );
        }

        // Segunda pasada: no debe duplicar.
        seed_definitions(&pool).await.unwrap();
        let def2 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def2.quantities.len(), 3);
        assert_eq!(def2.results.len(), 5);
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
    async fn seed_definitions_populates_p2_serie() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p2-serie").await.unwrap().unwrap();
        assert_eq!(def.quantities.len(), 5);
        for symbol in ["Vg", "R1", "R2", "R3", "RA"] {
            assert!(
                def.quantities.iter().any(|q| q.symbol == symbol),
                "falta la magnitud {symbol}"
            );
        }
        let i = def.results.iter().find(|r| r.symbol == "I").unwrap();
        assert_eq!(i.formula, "Vg / (R1 + R2 + R3 + RA)");
        assert!(def.results.iter().any(|r| r.symbol == "VR1"));
    }

    #[tokio::test]
    async fn seed_definitions_populates_p3_relajacion() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
        assert_eq!(def.quantities.len(), 5);
        for symbol in ["R", "Rint", "C", "T_oc", "tmedio"] {
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
                    given_u: if q.is_given { Some(0.0) } else { None },
                    point_replicas: None,
                    operator_replicas: None,
                }
            })
            .collect();
        let a3 = crate::computation::compute(
            &def3.quantities,
            &def3.results,
            &Default::default(),
            &m3,
            None,
        )
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
                    given_u: None,
                    point_replicas: None,
                    operator_replicas: None,
                }
            })
            .collect();
        let a2 = crate::computation::compute(
            &def2.quantities,
            &def2.results,
            &Default::default(),
            &m2,
            None,
        )
        .unwrap();
        let req = a2.derived.iter().find(|d| d.symbol == "Req").unwrap();
        assert!((req.value - 210.0).abs() < 1e-9);
        let i = a2.derived.iter().find(|d| d.symbol == "I").unwrap();
        assert!((i.value - 8.0 / 210.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn seed_definitions_populates_p3_desfasaje() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p3-relajacion-desfasaje")
            .await
            .unwrap()
            .unwrap();
        // Es una práctica de regresión con las fórmulas de eje ya sembradas.
        assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
        assert_eq!(def.x_formula.as_deref(), Some("2*pi*f"));
        assert_eq!(def.y_formula.as_deref(), Some("b / math::sqrt(a*a - b*b)"));
        assert_eq!(def.quantities.len(), 3);
        for symbol in ["f", "a", "b"] {
            assert!(
                def.quantities.iter().any(|q| q.symbol == symbol),
                "falta la magnitud {symbol}"
            );
        }
        assert_eq!(def.results.len(), 1);
        assert_eq!(def.results[0].symbol, "tau");
        assert_eq!(def.results[0].formula, "slope");
    }

    // Ajuste de extremo a extremo sobre la definición sembrada de P3-parte2, con un caso
    // construido: si tg(phi) = tau*omega, con a=1 y b=sin(phi)=t/sqrt(1+t^2), entonces
    // y = b/sqrt(a^2-b^2) = tg(phi) = tau*omega, así que el ajuste recupera slope = tau.
    #[tokio::test]
    async fn seeded_p3_desfasaje_fits_known_tau() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p3-relajacion-desfasaje")
            .await
            .unwrap()
            .unwrap();

        let tau = 1e-3_f64;
        let freqs = [10.0_f64, 20.0, 30.0, 40.0, 50.0];
        let id = |sym: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == sym)
                .unwrap()
                .id
                .clone()
        };
        let b_vals: Vec<f64> = freqs
            .iter()
            .map(|f| {
                let t = tau * 2.0 * std::f64::consts::PI * f;
                t / (1.0 + t * t).sqrt()
            })
            .collect();
        let measurements = vec![
            crate::computation::MeasurementInput {
                quantity_id: id("f"),
                instrument_id: None,
                scale_id: None,
                values: freqs.to_vec(),
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
            },
            crate::computation::MeasurementInput {
                quantity_id: id("a"),
                instrument_id: None,
                scale_id: None,
                values: freqs.iter().map(|_| 1.0).collect(),
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
            },
            crate::computation::MeasurementInput {
                quantity_id: id("b"),
                instrument_id: None,
                scale_id: None,
                values: b_vals,
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
            },
        ];
        let analysis = crate::computation::compute_regresion(
            &def.quantities,
            &def.intermediates,
            &def.results,
            def.x_formula.as_deref().unwrap(),
            def.y_formula.as_deref().unwrap(),
            &measurements,
        )
        .unwrap();
        let reg = analysis.regression.unwrap();
        assert!(
            (reg.slope - tau).abs() < 1e-9,
            "slope {} != tau {tau}",
            reg.slope
        );
        assert!(reg.intercept.abs() < 1e-9);
        let tau_d = analysis.derived.iter().find(|d| d.symbol == "tau").unwrap();
        assert!((tau_d.value - tau).abs() < 1e-9);
    }

    /// Verifica que `double_option` distingue las tres variantes de `tolerance` en JSON:
    /// campo ausente (no modificar), `null` explícito (borrar) y valor numérico (fijar).
    #[test]
    fn result_input_tolerance_serde_variants() {
        // Sin campo -> None (no modificar).
        let a: ResultInput =
            serde_json::from_str(r#"{"symbol":"Q","name":"N","unit":"m","formula":"x"}"#).unwrap();
        assert!(a.tolerance.is_none(), "campo ausente debe ser None");

        // `null` explícito -> Some(None) (borrar).
        let b: ResultInput = serde_json::from_str(
            r#"{"symbol":"Q","name":"N","unit":"m","formula":"x","tolerance":null}"#,
        )
        .unwrap();
        assert_eq!(b.tolerance, Some(None), "null debe ser Some(None)");

        // Valor numérico -> Some(Some(v)) (fijar).
        let c: ResultInput = serde_json::from_str(
            r#"{"symbol":"Q","name":"N","unit":"m","formula":"x","tolerance":5.0}"#,
        )
        .unwrap();
        assert_eq!(
            c.tolerance,
            Some(Some(5.0)),
            "número debe ser Some(Some(v))"
        );
    }
}
