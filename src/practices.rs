//! Definición de prácticas: magnitudes de entrada y mensurandos derivados.
//!
//! Las definiciones son **globales por práctica** (no por curso). Una vez definida P1
//! con sus magnitudes y fórmulas, cualquier curso que habilite P1 usa la misma definición.
//! El cálculo de incertidumbres (Fase 4) lee esta definición para saber qué medir y qué derivar.

use crate::db::{next_position, PracticeQuantity, PracticeResult};
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
    /// En regresión/curva: `true` = se mide por punto (tabla de la serie); `false` = escalar
    /// compartido (Motor E). Default `true` (comportamiento previo).
    #[serde(default = "default_true")]
    pub per_point: bool,
    /// `false` solo tiene efecto combinado con `is_given`: pide únicamente "Valor" (sin
    /// instrumento ni campo U), computado con U = 0. Default `true` (comportamiento previo).
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
    /// `true` si puede quedar sin lecturas sin bloquear el envío del formulario.
    #[serde(default)]
    pub optional: bool,
}

/// Default `true` para campos booleanos opcionales (p. ej. `per_point`).
fn default_true() -> bool {
    true
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
    /// `true` si es el resultado central que el alumno debe entregar para esta práctica.
    #[serde(default)]
    pub is_final: bool,
    /// `false` oculta la ±U de este mensurando en toda la UI. Default `true` (comportamiento
    /// previo). Reemplaza el Set hardcodeado `RESULTS_WITHOUT_U` del frontend.
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
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
    /// Solo `regresion_lineal` (Motor E): magnitudes derivadas por punto, post-ajuste (tabla por
    /// corrida, p. ej. Reynolds).
    pub point_results: Vec<PracticePointResult>,
    /// Solo `regresion_lineal` (Motor F): mensurandos agregados escalares, post-ajuste (un valor,
    /// con acceso a los extremos de cada magnitud por punto: `X_first`/`X_first2`/`X_last`/`X_last2`).
    pub aggregates: Vec<PracticeAggregate>,
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

/// Magnitud derivada **por punto, post-ajuste** (Motor E) de una práctica `regresion_lineal`: su
/// `formula` se evalúa en cada punto con las magnitudes/intermedias del punto + `slope`/`intercept`
/// + los mensurandos derivados, produciendo una columna por corrida (p. ej. Reynolds).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticePointResult {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Datos para crear o actualizar una magnitud derivada por punto.
#[derive(Debug, Deserialize)]
pub struct PointResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Mensurando **agregado** escalar (Motor F) de una práctica `regresion_lineal`: su `formula` se
/// evalúa una vez tras el ajuste y puede usar escalares compartidos, `slope`/`intercept`, los
/// mensurandos, los agregados anteriores, y los extremos de cada magnitud por punto (`X_first`,
/// `X_first2`, `X_last`, `X_last2`). Un valor, sin incertidumbre.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticeAggregate {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Datos para crear o actualizar un mensurando agregado.
#[derive(Debug, Deserialize)]
pub struct AggregateInput {
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
    let point_results = point_results_for(pool, practice_id).await?;
    let aggregates = aggregates_for(pool, practice_id).await?;
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
        point_results,
        aggregates,
    }))
}

/// `point_results` (Motor E), `aggregates` (Motor F) e `intermediates` (Motor C) son, en la base,
/// la misma forma: `(id, practice_id, position, symbol, name, unit, formula)`, con el mismo CRUD
/// (fetch-all/create/update/delete/fetch-by-id). Solo cambian de tabla y de tipo Rust — `curves`
/// no entra acá porque tiene otras columnas (`x_formula`/`y_formula`/`x_log`, sin `name`/`unit`) y
/// una operación extra (`move_curve`) que las otras tres no tienen.
trait SymbolFormulaRow: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin {
    const TABLE: &'static str;
}

impl SymbolFormulaRow for PracticePointResult {
    const TABLE: &'static str = "practice_point_results";
}

impl SymbolFormulaRow for PracticeAggregate {
    const TABLE: &'static str = "practice_aggregates";
}

impl SymbolFormulaRow for PracticeIntermediate {
    const TABLE: &'static str = "practice_intermediates";
}

async fn symbol_formula_rows_for<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<T>> {
    let query = format!(
        "SELECT id, practice_id, position, symbol, name, unit, formula \
         FROM {} WHERE practice_id = ?1 ORDER BY position, id",
        T::TABLE
    );
    Ok(sqlx::query_as::<_, T>(&query)
        .bind(practice_id)
        .fetch_all(pool)
        .await?)
}

async fn fetch_symbol_formula_row<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    id: &str,
) -> anyhow::Result<T> {
    let query = format!(
        "SELECT id, practice_id, position, symbol, name, unit, formula FROM {} WHERE id = ?1",
        T::TABLE
    );
    Ok(sqlx::query_as::<_, T>(&query)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

/// Inserta una fila symbol/name/unit/formula nueva, asignándole la siguiente posición, y la
/// devuelve. El llamador ya validó y recortó `symbol`/`formula` (el mensaje de error es distinto
/// por tipo, así que queda del lado de cada wrapper público).
async fn create_symbol_formula_row<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    name: &str,
    unit: &str,
    formula: &str,
) -> anyhow::Result<T> {
    let position = next_position(pool, T::TABLE, "practice_id", practice_id).await?;
    let id = Uuid::new_v4().to_string();
    let query = format!(
        "INSERT INTO {} (id, practice_id, position, symbol, name, unit, formula) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        T::TABLE
    );
    sqlx::query(&query)
        .bind(&id)
        .bind(practice_id)
        .bind(position)
        .bind(symbol)
        .bind(name)
        .bind(unit)
        .bind(formula)
        .execute(pool)
        .await?;
    fetch_symbol_formula_row::<T>(pool, &id).await
}

/// Actualiza una fila symbol/name/unit/formula **de esa práctica**. Devuelve `None` si no existe.
async fn update_symbol_formula_row<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    practice_id: &str,
    row_id: &str,
    symbol: &str,
    name: &str,
    unit: &str,
    formula: &str,
) -> anyhow::Result<Option<T>> {
    let query = format!(
        "UPDATE {} SET symbol = ?3, name = ?4, unit = ?5, formula = ?6 \
         WHERE id = ?1 AND practice_id = ?2",
        T::TABLE
    );
    let result = sqlx::query(&query)
        .bind(row_id)
        .bind(practice_id)
        .bind(symbol)
        .bind(name)
        .bind(unit)
        .bind(formula)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_symbol_formula_row::<T>(pool, row_id).await?))
}

/// Elimina una fila symbol/name/unit/formula de esa práctica por id. Devuelve `true` si existía.
async fn delete_symbol_formula_row<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    practice_id: &str,
    row_id: &str,
) -> anyhow::Result<bool> {
    let query = format!(
        "DELETE FROM {} WHERE id = ?1 AND practice_id = ?2",
        T::TABLE
    );
    let result = sqlx::query(&query)
        .bind(row_id)
        .bind(practice_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Lee las magnitudes derivadas por punto de una práctica (Motor E), ordenadas por posición.
pub async fn point_results_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticePointResult>> {
    symbol_formula_rows_for(pool, practice_id).await
}

/// Crea una magnitud derivada por punto; asigna la siguiente posición. Símbolo y fórmula obligatorios.
pub async fn create_point_result(
    pool: &SqlitePool,
    practice_id: &str,
    input: PointResultInput,
) -> anyhow::Result<PracticePointResult> {
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        anyhow::bail!("la magnitud derivada por punto necesita símbolo y fórmula");
    }
    create_symbol_formula_row(
        pool,
        practice_id,
        symbol,
        input.name.trim(),
        input.unit.trim(),
        formula,
    )
    .await
}

/// Actualiza una magnitud derivada por punto de esa práctica. Devuelve `None` si no existe.
pub async fn update_point_result(
    pool: &SqlitePool,
    practice_id: &str,
    point_result_id: &str,
    input: PointResultInput,
) -> anyhow::Result<Option<PracticePointResult>> {
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        anyhow::bail!("la magnitud derivada por punto necesita símbolo y fórmula");
    }
    update_symbol_formula_row(
        pool,
        practice_id,
        point_result_id,
        symbol,
        input.name.trim(),
        input.unit.trim(),
        formula,
    )
    .await
}

/// Elimina una magnitud derivada por punto de esa práctica por id. Devuelve `true` si existía.
pub async fn delete_point_result(
    pool: &SqlitePool,
    practice_id: &str,
    point_result_id: &str,
) -> anyhow::Result<bool> {
    delete_symbol_formula_row::<PracticePointResult>(pool, practice_id, point_result_id).await
}

/// Lee los mensurandos agregados de una práctica (Motor F), ordenados por posición.
pub async fn aggregates_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeAggregate>> {
    symbol_formula_rows_for(pool, practice_id).await
}

/// Crea un mensurando agregado; asigna la siguiente posición. Símbolo y fórmula obligatorios.
pub async fn create_aggregate(
    pool: &SqlitePool,
    practice_id: &str,
    input: AggregateInput,
) -> anyhow::Result<PracticeAggregate> {
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        anyhow::bail!("el mensurando agregado necesita símbolo y fórmula");
    }
    create_symbol_formula_row(
        pool,
        practice_id,
        symbol,
        input.name.trim(),
        input.unit.trim(),
        formula,
    )
    .await
}

/// Actualiza un mensurando agregado de esa práctica. Devuelve `None` si no existe.
pub async fn update_aggregate(
    pool: &SqlitePool,
    practice_id: &str,
    aggregate_id: &str,
    input: AggregateInput,
) -> anyhow::Result<Option<PracticeAggregate>> {
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        anyhow::bail!("el mensurando agregado necesita símbolo y fórmula");
    }
    update_symbol_formula_row(
        pool,
        practice_id,
        aggregate_id,
        symbol,
        input.name.trim(),
        input.unit.trim(),
        formula,
    )
    .await
}

/// Elimina un mensurando agregado de esa práctica por id. Devuelve `true` si existía.
pub async fn delete_aggregate(
    pool: &SqlitePool,
    practice_id: &str,
    aggregate_id: &str,
) -> anyhow::Result<bool> {
    delete_symbol_formula_row::<PracticeAggregate>(pool, practice_id, aggregate_id).await
}

/// Lee las magnitudes intermedias por punto de una práctica (Motor C), ordenadas por posición.
pub async fn intermediates_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeIntermediate>> {
    symbol_formula_rows_for(pool, practice_id).await
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
    create_symbol_formula_row(
        pool,
        practice_id,
        symbol,
        input.name.trim(),
        input.unit.trim(),
        formula,
    )
    .await
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
    update_symbol_formula_row(
        pool,
        practice_id,
        intermediate_id,
        symbol,
        input.name.trim(),
        input.unit.trim(),
        formula,
    )
    .await
}

/// Elimina una magnitud intermedia de esa práctica por id. Devuelve `true` si existía.
pub async fn delete_intermediate(
    pool: &SqlitePool,
    practice_id: &str,
    intermediate_id: &str,
) -> anyhow::Result<bool> {
    delete_symbol_formula_row::<PracticeIntermediate>(pool, practice_id, intermediate_id).await
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
    let position = next_position(pool, "practice_curves", "practice_id", practice_id).await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_curves (id, practice_id, position, x_formula, y_formula, x_log) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(position)
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
    let position = next_position(pool, "practice_quantities", "practice_id", practice_id).await?;
    let id = {
        let mut conn = pool.acquire().await?;
        insert_quantity(&mut conn, practice_id, position, &input).await?
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
             replicas_per_point = ?8, per_point = ?9, has_uncertainty = ?10, optional = ?11 \
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
    .bind(input.per_point)
    .bind(input.has_uncertainty)
    .bind(input.optional)
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
    let position = next_position(pool, "practice_results", "practice_id", practice_id).await?;
    let id = {
        let mut conn = pool.acquire().await?;
        insert_result(&mut conn, practice_id, position, &input).await?
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
                 SET symbol = ?2, name = ?3, unit = ?4, formula = ?5, is_final = ?6, \
                     has_uncertainty = ?7 \
                 WHERE id = ?1",
        )
        .bind(result_id)
        .bind(input.symbol.trim())
        .bind(input.name.trim())
        .bind(input.unit.trim())
        .bind(input.formula.trim())
        .bind(input.is_final)
        .bind(input.has_uncertainty)
        .execute(pool)
        .await?
        .rows_affected(),
        Some(tol) => sqlx::query(
            "UPDATE practice_results \
                 SET symbol = ?2, name = ?3, unit = ?4, formula = ?5, tolerance = ?6, is_final = ?7, \
                     has_uncertainty = ?8 \
                 WHERE id = ?1",
        )
        .bind(result_id)
        .bind(input.symbol.trim())
        .bind(input.name.trim())
        .bind(input.unit.trim())
        .bind(input.formula.trim())
        .bind(tol)
        .bind(input.is_final)
        .bind(input.has_uncertainty)
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

/// `true` si `symbol` ya está tomado por alguna magnitud, mensurando o magnitud intermedia de la
/// práctica (los tres comparten un mismo espacio de símbolos en las fórmulas).
///
/// `exclude_*_id` permiten ignorar la fila que se está editando (para que renombrar a su propio
/// símbolo no falle).
#[allow(clippy::too_many_arguments)]
pub async fn symbol_taken_in_practice(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    exclude_quantity_id: Option<&str>,
    exclude_result_id: Option<&str>,
    exclude_intermediate_id: Option<&str>,
    exclude_point_result_id: Option<&str>,
    exclude_aggregate_id: Option<&str>,
) -> anyhow::Result<bool> {
    let sym = symbol.trim();
    let count = |table: &str, exclude: Option<&str>| {
        let q = format!(
            "SELECT COUNT(*) FROM {table} WHERE practice_id = ?1 AND symbol = ?2 AND id <> ?3"
        );
        let exclude = exclude.unwrap_or("").to_string();
        let practice_id = practice_id.to_string();
        let sym = sym.to_string();
        async move {
            let row: (i64,) = sqlx::query_as(&q)
                .bind(practice_id)
                .bind(sym)
                .bind(exclude)
                .fetch_one(pool)
                .await?;
            anyhow::Ok(row.0 > 0)
        }
    };
    Ok(count("practice_quantities", exclude_quantity_id).await?
        || count("practice_results", exclude_result_id).await?
        || count("practice_intermediates", exclude_intermediate_id).await?
        || count("practice_point_results", exclude_point_result_id).await?
        || count("practice_aggregates", exclude_aggregate_id).await?)
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
        per_point: true,
        has_uncertainty: true,
        optional: false,
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
        per_point: false,
        has_uncertainty: true,
        optional: false,
    }
}

/// Igual que [`qty_given`], pero sin campo de incertidumbre: el formulario pide solo "Valor" (sin
/// instrumento ni U), y se computa con U = 0. Para datos de tabla que no tienen incertidumbre
/// propia (p. ej. un tiempo leído de una tabla de referencia).
fn no_u(mut q: QuantityInput) -> QuantityInput {
    q.has_uncertainty = false;
    q
}

/// Marca una magnitud como opcional: puede quedar sin lecturas sin bloquear el envío.
fn opt(mut q: QuantityInput) -> QuantityInput {
    q.optional = true;
    q
}

/// Magnitud medida **por punto con réplicas** (regresión/curva): grilla de `replicas` por punto.
fn qty_replicas(
    symbol: &str,
    name: &str,
    unit: &str,
    quantity: &str,
    replicas: i64,
) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated: true,
        quantity: Some(quantity.into()),
        is_given: false,
        replicas_per_point: Some(replicas),
        per_point: true,
        has_uncertainty: true,
        optional: false,
    }
}

/// Escalar **compartido** medido una sola vez (no por punto, no dato de cátedra): p. ej. la
/// densidad medida con densímetro al final de la práctica.
fn qty_shared(symbol: &str, name: &str, unit: &str, quantity: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: name.into(),
        unit: unit.into(),
        repeated: false,
        quantity: Some(quantity.into()),
        is_given: false,
        replicas_per_point: None,
        per_point: false,
        has_uncertainty: true,
        optional: false,
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
        is_final: false,
        has_uncertainty: true,
    }
}

/// Igual que [`res`], pero marcado como resultado central que el alumno debe entregar.
fn res_final(symbol: &str, name: &str, unit: &str, formula: &str) -> ResultInput {
    ResultInput {
        is_final: true,
        ..res(symbol, name, unit, formula)
    }
}

/// Igual que [`res`]/[`res_final`], pero oculta la ±U en toda la UI (reemplaza el hack
/// `RESULTS_WITHOUT_U` del frontend). El valor propagado de fondo no cambia, solo su display.
fn res_no_u(mut r: ResultInput) -> ResultInput {
    r.has_uncertainty = false;
    r
}

/// Siembra la definición de una práctica (magnitudes + mensurandos). Idempotente: no hace nada si
/// la práctica ya tiene magnitudes. Devuelve `true` si la sembró ahora (`false` si ya existía),
/// para que el llamador siembre los extras (intermedias/derivadas) solo en el alta fresca y no los
/// re-cree si el docente los borró luego.
async fn seed_practice(
    pool: &SqlitePool,
    practice_id: &str,
    quantities: &[QuantityInput],
    results: &[ResultInput],
) -> anyhow::Result<bool> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1")
            .bind(practice_id)
            .fetch_one(pool)
            .await?;
    if count.0 > 0 {
        return Ok(false);
    }
    let mut conn = pool.acquire().await?;
    for (pos, q) in quantities.iter().enumerate() {
        insert_quantity(&mut conn, practice_id, pos as i64 + 1, q).await?;
    }
    for (pos, r) in results.iter().enumerate() {
        insert_result(&mut conn, practice_id, pos as i64 + 1, r).await?;
    }
    Ok(true)
}

/// `true` si la práctica no tiene una magnitud con ese símbolo. Para migraciones puntuales que
/// agregan símbolos nuevos a una práctica ya sembrada (`seed_practice` es idempotente y no
/// re-siembra), evitando duplicar el `INSERT` en una base que ya los tiene.
async fn quantity_missing(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1 AND symbol = ?2",
    )
    .bind(practice_id)
    .bind(symbol)
    .fetch_one(pool)
    .await?;
    Ok(count.0 == 0)
}

/// Igual que [`quantity_missing`], para mensurandos derivados.
async fn result_missing(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
) -> anyhow::Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_results WHERE practice_id = ?1 AND symbol = ?2",
    )
    .bind(practice_id)
    .bind(symbol)
    .fetch_one(pool)
    .await?;
    Ok(count.0 == 0)
}

/// Siembra P1 (ver [`seed_definitions`]).
async fn seed_p1_estadistica(pool: &SqlitePool) -> anyhow::Result<()> {
    // P1 — Tratamiento estadístico de datos (péndulo simple), con 3 operadores independientes:
    // cada uno mide su propia serie de períodos (T1/T2/T3, cronómetro, sin cruzar datos entre
    // operadores) y tiene su propio g1/g2/g3 = 4*pi^2*L/T{n}^2. Operador 1 obligatorio; 2 y 3
    // opcionales (el alumno puede no cargarlos). L (cátedra), t_med y los mensurandos gamma/Q son
    // únicos para toda la práctica (no por operador). t_med ("t_1/2") es un dato de tabla sin
    // incertidumbre propia: solo pide el campo "Valor" (`no_u`, sin instrumento ni U). gamma y Q
    // se muestran sin ±U (`res_no_u`) aunque Q sí propaga, de fondo, la incertidumbre real del
    // período (usa el del operador 1 — ver el nombre del mensurando).
    seed_practice(
        pool,
        "p1-estadistica",
        &[
            qty_given("L", "Longitud del pendulo", "m", "longitud"),
            no_u(qty_given(
                "t_med",
                "Tiempo de semiamplitud (t1/2)",
                "s",
                "tiempo",
            )),
            qty("T1", "Periodo - Operador 1", "s", true, "tiempo"),
            opt(qty("T2", "Periodo - Operador 2", "s", true, "tiempo")),
            opt(qty("T3", "Periodo - Operador 3", "s", true, "tiempo")),
        ],
        &[
            res_no_u(res_final(
                "gamma",
                "Coeficiente de amortiguamiento",
                "1/s",
                "2*math::ln(2)/t_med",
            )),
            res_no_u(res_final(
                "Q",
                "Factor de calidad (usa el periodo del Operador 1)",
                "",
                "pi*t_med/(T1*math::ln(2))",
            )),
            res_final(
                "g1",
                "Aceleracion de gravedad - Operador 1",
                "m/s2",
                "4*pi^2*L/T1^2",
            ),
            res_final(
                "g2",
                "Aceleracion de gravedad - Operador 2",
                "m/s2",
                "4*pi^2*L/T2^2",
            ),
            res_final(
                "g3",
                "Aceleracion de gravedad - Operador 3",
                "m/s2",
                "4*pi^2*L/T3^2",
            ),
        ],
    )
    .await?;
    // Migración de forma: la definición original tenía un único T/g compartidos; ahora son T1/T2/T3
    // y g1/g2/g3 por operador, más el flag `has_uncertainty` en t_med/gamma/Q. `seed_practice` es
    // idempotente y no re-siembra sobre una base ya sembrada, así que las instalaciones existentes
    // necesitan este backfill puntual (no-op en instalaciones nuevas, que ya siembran la forma final
    // arriba). Cada bloque corre una única vez, guardado por el símbolo nuevo que da de alta: una
    // vez que T1/g1 existen, no se vuelve a tocar (así no se pisan ediciones del admin sobre
    // has_uncertainty/optional/formula hechas después de la migración).
    if quantity_missing(pool, "p1-estadistica", "T1").await? {
        // Una sola transacción: si el proceso muere a mitad de camino (p. ej. despues de borrar
        // `T` pero antes de insertar T2/T3), `quantity_missing(pool, "T1")` ya daria `false` en
        // el siguiente boot (T1 si se llego a insertar) y el bloque no se volveria a correr,
        // dejando la migracion incompleta para siempre.
        let mut tx = pool.begin().await?;
        // Borra primero las mediciones que referencian la magnitud vieja: `submission_measurements
        // .quantity_id` tiene FK a `practice_quantities(id)` sin `ON DELETE CASCADE`, así que
        // borrar la magnitud con mediciones reales cargadas violaría la constraint (con
        // `foreign_keys` activo) y tiraría el boot abajo. Si hubiera entregas reales sobre `T`,
        // se descartan sus mediciones en vez de eso.
        sqlx::query(
            "DELETE FROM submission_measurements WHERE quantity_id IN \
             (SELECT id FROM practice_quantities WHERE practice_id = 'p1-estadistica' AND symbol = 'T')",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM practice_quantities WHERE practice_id = 'p1-estadistica' AND symbol = 'T'",
        )
        .execute(&mut *tx)
        .await?;
        let base_pos: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(position), 0) FROM practice_quantities WHERE practice_id = ?1",
        )
        .bind("p1-estadistica")
        .fetch_one(&mut *tx)
        .await?;
        for (i, q) in [
            qty("T1", "Periodo - Operador 1", "s", true, "tiempo"),
            opt(qty("T2", "Periodo - Operador 2", "s", true, "tiempo")),
            opt(qty("T3", "Periodo - Operador 3", "s", true, "tiempo")),
        ]
        .iter()
        .enumerate()
        {
            insert_quantity(&mut tx, "p1-estadistica", base_pos.0 + i as i64 + 1, q).await?;
        }
        // t_med sin instrumento/U, T2/T3 opcionales: forma vieja de la base no tenía estos flags.
        sqlx::query(
            "UPDATE practice_quantities SET has_uncertainty = 0 \
             WHERE practice_id = 'p1-estadistica' AND symbol = 't_med'",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE practice_quantities SET optional = 1 \
             WHERE practice_id = 'p1-estadistica' AND symbol IN ('T2', 'T3')",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    if result_missing(pool, "p1-estadistica", "g1").await? {
        // Misma razón que la migración de arriba: todo o nada, para no quedar con g1 insertado
        // pero g2/g3 faltantes si el proceso muere a mitad de camino.
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM practice_results WHERE practice_id = 'p1-estadistica' \
             AND symbol IN ('g', 'Tmedio', 'delta')",
        )
        .execute(&mut *tx)
        .await?;
        let base_pos: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(position), 0) FROM practice_results WHERE practice_id = ?1",
        )
        .bind("p1-estadistica")
        .fetch_one(&mut *tx)
        .await?;
        for (i, r) in [
            res_final(
                "g1",
                "Aceleracion de gravedad - Operador 1",
                "m/s2",
                "4*pi^2*L/T1^2",
            ),
            res_final(
                "g2",
                "Aceleracion de gravedad - Operador 2",
                "m/s2",
                "4*pi^2*L/T2^2",
            ),
            res_final(
                "g3",
                "Aceleracion de gravedad - Operador 3",
                "m/s2",
                "4*pi^2*L/T3^2",
            ),
        ]
        .iter()
        .enumerate()
        {
            insert_result(&mut tx, "p1-estadistica", base_pos.0 + i as i64 + 1, r).await?;
        }
        // gamma/Q sin ±U, y Q pasa a referenciar T1 (antes T): forma vieja de la base no tenía
        // `has_uncertainty` ni el operador explícito en la fórmula/nombre.
        sqlx::query(
            "UPDATE practice_results SET has_uncertainty = 0 \
             WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q')",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE practice_results SET formula = 'pi*t_med/(T1*math::ln(2))', \
                 name = 'Factor de calidad (usa el periodo del Operador 1)' \
             WHERE practice_id = 'p1-estadistica' AND symbol = 'Q'",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    // gamma/Q pasaron a ser resultado final después del alta inicial de la práctica: invariante que
    // se re-aplica en cada boot (no solo en la migración de forma de arriba) para autocurar bases
    // donde haya quedado en `is_final = 0` por cualquier motivo.
    sqlx::query(
        "UPDATE practice_results SET is_final = 1 \
         WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    // `operator_count` (Motor D) es de una epoca anterior a T1/T2/T3: dividia UNA magnitud
    // repetida ("T") en N series, una por operador. Ahora cada operador ya es un simbolo propio
    // (T1/T2/T3, con su propio tab y su propio g_i), asi que Motor D queda incompatible: si
    // quedo seteado (p. ej. desde el admin, antes de este rediseño) vuelve a tratar T1/T2/T3 como
    // si fueran 3 series del mismo operador, mostrando 3 cronometros por tab en vez de 1 y
    // triplicando los mensurandos derivados. Se autocura en cada boot.
    sqlx::query(
        "UPDATE practices SET operator_count = NULL \
         WHERE id = 'p1-estadistica' AND operator_count IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Siembra P3 parte 1 (ver [`seed_definitions`]).
async fn seed_p3_relajacion(pool: &SqlitePool) -> anyhow::Result<()> {
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
            res_final(
                "tau_exp",
                "Tiempo de relajacion experimental",
                "s",
                "tmedio / math::ln(2)",
            ),
        ],
    )
    .await?;
    Ok(())
}

/// Siembra P2-CC (ver [`seed_definitions`]).
async fn seed_p2_cc(pool: &SqlitePool) -> anyhow::Result<()> {
    // P2 — Corriente continua unificada: una sola entrega con tres partes tematicas.
    // Escalares compartidos: R1, R2 y R3 se miden UNA vez (ohmetro) y valen para toda la
    // practica; Vg y RA pueden cambiar entre partes, asi que se miden por parte (sufijos
    // _s = serie, _p = paralelo, _c = curva de potencia). Los voltajes VRi_s / VRi_p se miden
    // con multimetro y se comparan con las teoricas VRi_s_t / VRi_p_t (resultados finales que
    // el alumno calcula a mano con propagacion). Por punto: R (carga variable) e I; intermedia
    // P = I^2*R y curva P vs R. Los finales experimentales de potencia (P_max_e / RP_max_e)
    // usan los alias de extremos del camino curva (`P_max`, `R_at_P_max`) con U = 0; por eso
    // sus formulas no son editables desde la UI admin (check_formula no conoce los alias).
    // Migración de forma: mientras se desarrollaba la unificación (#43) algunas bases quedaron
    // sembradas con una forma intermedia de p2-cc (símbolos con sufijo _serie/_paralelo/_potencia,
    // p. ej. `Vg_serie`/`RA_serie`, en vez de los `_s`/`_p`/`_c` actuales). `seed_practice` es
    // idempotente y no resiembra sobre una base que ya tiene filas, así que esas bases quedan
    // desincronizadas de PRACTICE_SECTIONS (constants.js) para siempre: el front no encuentra los
    // símbolos esperados, no arma `data-section` y las tabs Serie/Paralelo/Potencia dejan de
    // separar sus campos. No hay mediciones reales bajo esa forma intermedia (era de desarrollo),
    // así que en vez de renombrar símbolo a símbolo se limpia todo p2-cc y se deja que el seed de
    // abajo lo siembre de cero con la forma final.
    if !quantity_missing(pool, "p2-cc", "R").await?
        && quantity_missing(pool, "p2-cc", "Vg_s").await?
    {
        // Una sola transacción: si el proceso muere a mitad de camino, no queda un estado
        // parcial (p. ej. `practice_quantities` ya vacía pero `practice_results` todavía con
        // las filas viejas, que chocarían con `UNIQUE(practice_id, symbol)` al resembrar).
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM submission_measurements WHERE quantity_id IN \
             (SELECT id FROM practice_quantities WHERE practice_id = 'p2-cc')",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM practice_quantities WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM practice_results WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM practice_curves WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM practice_intermediates WHERE practice_id = 'p2-cc'")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    let fresh_p2cc = seed_practice(
        pool,
        "p2-cc",
        &[
            qty_shared("R1", "Resistencia R1 (compartida)", "ohm", "resistencia"),
            qty_shared("R2", "Resistencia R2 (compartida)", "ohm", "resistencia"),
            qty_shared("R3", "Resistencia R3 (compartida)", "ohm", "resistencia"),
            qty_shared("Vg_s", "Voltaje de la fuente", "V", "voltaje"),
            // RA es un dato de tabla segun la escala del amperimetro, no se mide: va como dato
            // dado por catedra (valor +/- U), igual en las tres partes.
            qty_given(
                "RA_s",
                "Resistencia interna del amperimetro",
                "ohm",
                "resistencia",
            ),
            qty_shared("VR1_s", "Voltaje medido en R1", "V", "voltaje"),
            qty_shared("VR2_s", "Voltaje medido en R2", "V", "voltaje"),
            qty_shared("VR3_s", "Voltaje medido en R3", "V", "voltaje"),
            qty_shared("Vg_p", "Voltaje de la fuente", "V", "voltaje"),
            qty_given(
                "RA_p",
                "Resistencia interna del amperimetro",
                "ohm",
                "resistencia",
            ),
            qty_shared("VR1_p", "Voltaje medido en R1", "V", "voltaje"),
            qty_shared("VR2_p", "Voltaje medido en R2", "V", "voltaje"),
            qty_shared("VR3_p", "Voltaje medido en R3", "V", "voltaje"),
            qty_shared("Vg_c", "Voltaje de la fuente", "V", "voltaje"),
            qty_given(
                "RA_c",
                "Resistencia interna del amperimetro",
                "ohm",
                "resistencia",
            ),
            qty(
                "R",
                "Resistencia externa (carga variable)",
                "ohm",
                false,
                "resistencia",
            ),
            qty("I", "Corriente de carga", "A", false, "corriente"),
        ],
        &[
            res_final(
                "I_s",
                "Corriente teorica",
                "A",
                "Vg_s / (R1 + R2 + R3 + RA_s)",
            ),
            res_final(
                "VR1_s_t",
                "Voltaje teorico en R1",
                "V",
                "Vg_s * R1 / (R1 + R2 + R3 + RA_s)",
            ),
            res_final(
                "VR2_s_t",
                "Voltaje teorico en R2",
                "V",
                "Vg_s * R2 / (R1 + R2 + R3 + RA_s)",
            ),
            res_final(
                "VR3_s_t",
                "Voltaje teorico en R3",
                "V",
                "Vg_s * R3 / (R1 + R2 + R3 + RA_s)",
            ),
            res(
                "Req",
                "Resistencia equivalente",
                "ohm",
                "R1 + RA_p + R2*R3/(R2+R3)",
            ),
            res_final(
                "I_p",
                "Corriente teorica",
                "A",
                "Vg_p / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "VR1_p_t",
                "Voltaje teorico en R1",
                "V",
                "Vg_p * R1 / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "VR2_p_t",
                "Voltaje teorico en R2",
                "V",
                "Vg_p * (R2*R3/(R2+R3)) / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "VR3_p_t",
                "Voltaje teorico en R3",
                "V",
                "Vg_p * (R2*R3/(R2+R3)) / (R1 + RA_p + R2*R3/(R2+R3))",
            ),
            res_final(
                "RP_max_t",
                "Resistencia de maxima transferencia teorica (Rth)",
                "ohm",
                "RA_c + R2*R3/(R2+R3)",
            ),
            res_final(
                "P_max_t",
                "Potencia maxima teorica",
                "W",
                "Vg_c*Vg_c/(4*(RA_c + R2*R3/(R2+R3)))",
            ),
            res_final(
                "P_max_e",
                "Potencia maxima experimental (de la tabla)",
                "W",
                "P_max",
            ),
            res_final(
                "RP_max_e",
                "Resistencia de maxima transferencia experimental",
                "ohm",
                "R_at_P_max",
            ),
        ],
    )
    .await?;
    if fresh_p2cc {
        create_intermediate(
            pool,
            "p2-cc",
            IntermediateInput {
                symbol: "P".into(),
                name: "Potencia disipada en R".into(),
                unit: "W".into(),
                formula: "I*I*R".into(),
            },
        )
        .await?;
        create_curve(
            pool,
            "p2-cc",
            CurveInput {
                x_formula: "R".into(),
                y_formula: "P".into(),
                x_log: false,
            },
        )
        .await?;
    }
    Ok(())
}

/// Siembra P3 parte 2 (ver [`seed_definitions`]).
async fn seed_p3_relajacion_desfasaje(pool: &SqlitePool) -> anyhow::Result<()> {
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
        &[res_final("tau", "Constante de tiempo RC", "s", "slope")],
    )
    .await?;
    Ok(())
}

/// Siembra Fluidos I (ver [`seed_definitions`]).
async fn seed_fluidos1(pool: &SqlitePool) -> anyhow::Result<()> {
    // Fluidos I — viscosidad por Hagen-Poiseuille. Por altura (punto) se miden V y t con 2
    // réplicas; Q = V/t (intermedia, promedio por punto). Ejes: 1/Q vs h/Q^2 (set en seed_practices).
    // Escalares compartidos: R, L, g (cátedra) y rho (medida única). `Temp` se registra solo como
    // referencia (para buscar la viscosidad de tablas a esa temperatura): no entra en ninguna
    // fórmula y va sin incertidumbre. Mensurando mu desde la pendiente; Reynolds por corrida.
    let fresh = seed_practice(
        pool,
        "fluidos-1",
        &[
            qty("h", "Altura del Mariotte", "m", false, "longitud"),
            qty_replicas("V", "Volumen recogido", "m3", "volumen", 2),
            qty_replicas("t", "Tiempo de descarga", "s", "tiempo", 2),
            qty_given("R", "Radio del capilar", "m", "longitud"),
            qty_given("L", "Longitud del capilar", "m", "longitud"),
            qty_given("g", "Aceleracion de la gravedad", "m/s2", "aceleracion"),
            qty_shared("rho", "Densidad del agua", "kg/m3", "densidad"),
            qty_shared(
                "Temp",
                "Temperatura del agua (referencia)",
                "C",
                "temperatura",
            ),
        ],
        &[res_final(
            "mu",
            "Viscosidad del agua",
            "Pa.s",
            "slope*(pi*rho*g*R^4)/(8*L)",
        )],
    )
    .await?;
    // Intermedia Q (Motor C) y derivada por corrida Reynolds (Motor E): solo en el alta fresca,
    // para no re-crearlas si el docente las edita/borra luego (`analysis_kind`/fórmulas se preservan
    // en `seed_practices`).
    if fresh {
        create_intermediate(
            pool,
            "fluidos-1",
            IntermediateInput {
                symbol: "Q".into(),
                name: "Caudal medio".into(),
                unit: "m3/s".into(),
                formula: "V/t".into(),
            },
        )
        .await?;
        create_point_result(
            pool,
            "fluidos-1",
            PointResultInput {
                symbol: "Re".into(),
                name: "Numero de Reynolds".into(),
                unit: "".into(),
                formula: "2*rho*Q/(pi*mu*R)".into(),
            },
        )
        .await?;
    }
    Ok(())
}

/// Siembra Viscosidad (ver [`seed_definitions`]).
async fn seed_viscosidad(pool: &SqlitePool) -> anyhow::Result<()> {
    // Viscosidad (Stokes) — ajuste v_lim vs R^2 (ejes en seed_practices: x=R^2, y=dx/t). Por esfera
    // (punto): R (un valor) y t (5 réplicas → Motor A promedia → t medio, así y = dx/t = v_lim).
    // Escalares compartidos: dx, rho_e, rho_f (medida única), g (cátedra); Temp de referencia.
    // Mensurando mu = (rho_e - rho_f)*2*g/(9*slope); Reynolds por corrida. Sin intermedia.
    let fresh_visc = seed_practice(
        pool,
        "viscosidad",
        &[
            qty("R", "Radio de la esfera", "m", false, "longitud"),
            qty_replicas("t", "Tiempo de caida", "s", "tiempo", 5),
            qty_shared("dx", "Distancia recorrida", "m", "longitud"),
            qty_shared("rho_e", "Densidad del acero", "kg/m3", "densidad"),
            qty_shared("rho_f", "Densidad de la glicerina", "kg/m3", "densidad"),
            qty_given("g", "Aceleracion de la gravedad", "m/s2", "aceleracion"),
            qty_shared(
                "Temp",
                "Temperatura de la glicerina (referencia)",
                "C",
                "temperatura",
            ),
        ],
        &[res_final(
            "mu",
            "Viscosidad de la glicerina",
            "Pa.s",
            "(rho_e - rho_f)*2*g/(9*slope)",
        )],
    )
    .await?;
    if fresh_visc {
        create_point_result(
            pool,
            "viscosidad",
            PointResultInput {
                symbol: "Re".into(),
                name: "Numero de Reynolds".into(),
                unit: "".into(),
                formula: "rho_f*(dx/t)*2*R/mu".into(),
            },
        )
        .await?;
    }
    Ok(())
}

/// Siembra Fluidos II (ver [`seed_definitions`]).
async fn seed_fluidos2(pool: &SqlitePool) -> anyhow::Result<()> {
    // Fluidos II — descarga de un recipiente por un capilar. Por punto: h (altura) y t (tiempo, con
    // t=0 en la altura maxima). Ejes (en seed_practices): x = sqrt(h_max) - sqrt(h), y = t. La
    // pendiente da M_medio = 2*g*(slope*R_cap^2/(2*R_recip^2))^2 - 2 (coef. medio de perdidas).
    // Escalares compartidos: R_cap, L_cap, R_recip (medidos con regla, con incertidumbre), g
    // (catedra), rho (densimetro al final), mu_agua (viscosidad del agua de tabla segun T), kp
    // (factor geometrico K, def. 0.78, editable) y h_max (altura inicial). Temp es referencia.
    // Mensurandos agregados (Motor F): Reynolds max/min usan el primer/ultimo par de puntos,
    // Reynolds medio los promedia y M_teorico cierra con la formula de la cuaderneta.
    let fresh_f2 = seed_practice(
        pool,
        "fluidos-2",
        &[
            qty("h", "Altura de la columna", "m", false, "longitud"),
            qty("t", "Tiempo de escurrimiento", "s", false, "tiempo"),
            qty_shared("h_max", "Altura inicial (maxima)", "m", "longitud"),
            qty_shared("R_cap", "Radio del capilar", "m", "longitud"),
            qty_shared("L_cap", "Longitud del capilar", "m", "longitud"),
            qty_shared("R_recip", "Radio del recipiente", "m", "longitud"),
            qty_given("g", "Aceleracion de la gravedad", "m/s2", "aceleracion"),
            qty_shared("rho", "Densidad del agua", "kg/m3", "densidad"),
            qty_shared(
                "mu_agua",
                "Viscosidad del agua (de tabla segun T)",
                "Pa.s",
                "viscosidad",
            ),
            qty_shared("kp", "Factor geometrico K (def. 0.78)", "", "adimensional"),
            qty_shared(
                "Temp",
                "Temperatura del agua (referencia)",
                "C",
                "temperatura",
            ),
        ],
        &[res_final(
            "M_medio",
            "Coeficiente medio de perdidas",
            "",
            "2*g*(slope*R_cap^2/(2*R_recip^2))^2 - 2",
        )],
    )
    .await?;
    // Mensurandos agregados (Motor F): se crean en orden porque se encadenan (Re_medio usa
    // Re_max/Re_min; M_teorico usa Re_medio). Solo en el alta fresca, para no re-crearlos si el
    // docente los edita/borra luego. Reynolds max/min referencian el primer/ultimo par de puntos
    // (h_first/h_first2/t_first/t_first2 y h_last/h_last2/t_last/t_last2, alias del Motor F).
    if fresh_f2 {
        for input in [
            AggregateInput {
                symbol: "Re_max".into(),
                name: "Numero de Reynolds maximo".into(),
                unit: "".into(),
                formula:
                    "2*rho*((h_first - h_first2)/(t_first2 - t_first))*(R_recip^2/(mu_agua*R_cap))"
                        .into(),
            },
            AggregateInput {
                symbol: "Re_min".into(),
                name: "Numero de Reynolds minimo".into(),
                unit: "".into(),
                formula:
                    "2*rho*((h_last2 - h_last)/(t_last - t_last2))*(R_recip^2/(mu_agua*R_cap))"
                        .into(),
            },
            AggregateInput {
                symbol: "Re_medio".into(),
                name: "Numero de Reynolds medio".into(),
                unit: "".into(),
                formula: "(Re_max + Re_min)/2".into(),
            },
            AggregateInput {
                symbol: "M_teorico".into(),
                name: "Coeficiente de perdidas teorico".into(),
                unit: "".into(),
                formula: "kp + 4*(L_cap/(2*R_cap))*(16/Re_medio)".into(),
            },
        ] {
            create_aggregate(pool, "fluidos-2", input).await?;
        }
    }
    Ok(())
}

/// Siembra Filtros (ver [`seed_definitions`]).
async fn seed_filtros(pool: &SqlitePool) -> anyhow::Result<()> {
    // Filtros — barrido en frecuencia de un circuito RLC. Por punto: f (frecuencia fijada por el
    // alumno), VRpp y Vgpp (tensiones pico a pico medidas), a y b (semiejes de la figura de
    // Lissajous). Componentes dados por la catedra: R, C1, C2, L. Intermedias: omega=2*pi*f
    // (rad/s), razon=VRpp/Vgpp (adimensional), phi=asin(b/a) (rad). Dos curvas (Motor B):
    // razon vs omega (amplitud) y phi vs omega (desfasaje), ambas con eje x logaritmico.
    // Mensurandos teoricos: fpasaje=1/(2*pi*sqrt(L*(C1+C2))) y fbloqueo=1/(2*pi*sqrt(L*C2)).
    // Topologia confirmada: C2||L en serie con C1 y R.
    let fresh_filtros = seed_practice(
        pool,
        "filtros",
        &[
            qty("f", "Frecuencia", "Hz", false, "frecuencia"),
            qty("VRpp", "Tension pico a pico en R", "V", false, "tension"),
            qty(
                "Vgpp",
                "Tension pico a pico del generador",
                "V",
                false,
                "tension",
            ),
            qty("a", "Semieje mayor de Lissajous", "V", false, "tension"),
            qty("b", "Semieje menor de Lissajous", "V", false, "tension"),
            qty_given("R", "Resistencia", "ohm", "resistencia"),
            qty_given("C1", "Capacitor 1", "F", "capacitancia"),
            qty_given("C2", "Capacitor 2", "F", "capacitancia"),
            qty_given("L", "Inductor", "H", "inductancia"),
        ],
        &[
            // Topología: C2||L en serie con C1 y R.
            // Resonancia serie (pasaje): f = 1/(2π√(L(C1+C2)))
            // Resonancia paralelo del tanque (bloqueo): f = 1/(2π√(LC2))
            res(
                "fpasaje",
                "Frecuencia de pasaje teorica",
                "Hz",
                "1/(2*pi*math::sqrt(L*(C1+C2)))",
            ),
            res(
                "fbloqueo",
                "Frecuencia de bloqueo teorica",
                "Hz",
                "1/(2*pi*math::sqrt(L*C2))",
            ),
        ],
    )
    .await?;
    if fresh_filtros {
        for (sym, name, unit, formula) in [
            ("omega", "Frecuencia angular", "rad/s", "2*pi*f"),
            ("razon", "Razon de amplitud VR/Vg", "", "VRpp/Vgpp"),
            ("phi", "Desfasaje", "rad", "math::asin(b/a)"),
        ] {
            create_intermediate(
                pool,
                "filtros",
                IntermediateInput {
                    symbol: sym.into(),
                    name: name.into(),
                    unit: unit.into(),
                    formula: formula.into(),
                },
            )
            .await?;
        }
        for (x, y, x_log) in [("omega", "razon", true), ("omega", "phi", true)] {
            create_curve(
                pool,
                "filtros",
                CurveInput {
                    x_formula: x.into(),
                    y_formula: y.into(),
                    x_log,
                },
            )
            .await?;
        }
    }

    Ok(())
}

/// Siembra las definiciones iniciales de las prácticas (idempotente por práctica, ver
/// [`seed_practice`]). Las magnitudes/fórmulas salen de las técnicas de trabajo de Física 103.
/// Cada práctica es independiente (no comparten estado entre sí); una función por práctica
/// mantiene cada una navegable por separado en vez de un único bloque de ~700 líneas.
pub async fn seed_definitions(pool: &SqlitePool) -> anyhow::Result<()> {
    seed_p1_estadistica(pool).await?;
    seed_p3_relajacion(pool).await?;
    seed_p2_cc(pool).await?;
    seed_p3_relajacion_desfasaje(pool).await?;
    seed_fluidos1(pool).await?;
    seed_viscosidad(pool).await?;
    seed_fluidos2(pool).await?;
    seed_filtros(pool).await?;
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
         replicas_per_point, per_point, has_uncertainty, optional \
         FROM practice_quantities WHERE practice_id = ?1 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

/// Lee los mensurandos derivados de una práctica, ordenados por posición y símbolo.
async fn results_for(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Vec<PracticeResult>> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position, tolerance, is_final, has_uncertainty \
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
         replicas_per_point, per_point, has_uncertainty, optional \
         FROM practice_quantities WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Lee un mensurando derivado por su id.
async fn fetch_result(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeResult> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position, tolerance, is_final, has_uncertainty \
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
          replicas_per_point, per_point, has_uncertainty, optional) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
    .bind(input.per_point)
    .bind(input.has_uncertainty)
    .bind(input.optional)
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
         (id, practice_id, symbol, name, unit, formula, position, tolerance, is_final, has_uncertainty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(input.formula.trim())
    .bind(position)
    .bind(input.tolerance.flatten())
    .bind(input.is_final)
    .bind(input.has_uncertainty)
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
            per_point: true,
            has_uncertainty: true,
            optional: false,
        }
    }

    fn sample_result() -> ResultInput {
        ResultInput {
            symbol: "Q".into(),
            name: "Area".into(),
            unit: "mm2".into(),
            formula: "l*a".into(),
            tolerance: None,
            is_final: false,
            has_uncertainty: true,
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
                per_point: true,
                has_uncertainty: true,
                optional: false,
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

    /// `is_final` se persiste al crear y se puede togglear al actualizar (checkbox docente en UI).
    #[tokio::test]
    async fn create_and_update_result_toggles_is_final() {
        let (pool, _dir) = setup().await;
        let r = create_result(
            &pool,
            "p1-estadistica",
            ResultInput {
                is_final: true,
                ..sample_result()
            },
        )
        .await
        .unwrap();
        assert!(r.is_final);

        let updated = update_result(
            &pool,
            &r.id,
            ResultInput {
                is_final: false,
                ..sample_result()
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!updated.is_final);
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
        assert!(symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "l",
            None,
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());
        // Excluir la misma magnitud (al renombrar) no debe reportar colisión.
        assert!(!symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "l",
            Some(&q.id),
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());

        // Crea un mensurando con símbolo "Q".
        let r = create_result(&pool, "p1-estadistica", sample_result())
            .await
            .unwrap();

        // Un mensurando nuevo con símbolo "l" (ya en quantities) es colisión cruzada.
        assert!(symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "l",
            None,
            Some(&r.id),
            None,
            None,
            None
        )
        .await
        .unwrap());
        // Una magnitud nueva con símbolo "Q" (ya en results) es colisión cruzada.
        assert!(symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "Q",
            Some(&q.id),
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());

        // Una magnitud intermedia con símbolo "Iv": magnitudes/mensurandos nuevos deben colisionar.
        create_intermediate(
            &pool,
            "p1-estadistica",
            IntermediateInput {
                symbol: "Iv".into(),
                name: "Iv".into(),
                unit: "u".into(),
                formula: "l".into(),
            },
        )
        .await
        .unwrap();
        assert!(symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "Iv",
            None,
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());

        // Una magnitud derivada por punto con símbolo "Re": el resto debe colisionar con ella.
        create_point_result(
            &pool,
            "p1-estadistica",
            PointResultInput {
                symbol: "Re".into(),
                name: "Re".into(),
                unit: "".into(),
                formula: "L".into(),
            },
        )
        .await
        .unwrap();
        assert!(symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "Re",
            None,
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());

        // Un mensurando agregado (Motor F) con símbolo "Ma": el resto debe colisionar con él.
        create_aggregate(
            &pool,
            "p1-estadistica",
            AggregateInput {
                symbol: "Ma".into(),
                name: "Ma".into(),
                unit: "".into(),
                formula: "slope".into(),
            },
        )
        .await
        .unwrap();
        assert!(symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "Ma",
            None,
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());

        // Símbolo inexistente no colisiona.
        assert!(!symbol_taken_in_practice(
            &pool,
            "p1-estadistica",
            "nuevo",
            None,
            None,
            None,
            None,
            None
        )
        .await
        .unwrap());
    }

    /// CRUD de mensurandos agregados (Motor F): alta asigna posición, lectura ordena, edición cambia
    /// campos, baja elimina y devuelve `true`/`false` según existiera.
    #[tokio::test]
    async fn aggregate_crud_roundtrip() {
        let (pool, _dir) = setup().await;
        let mk = |symbol: &str, formula: &str| AggregateInput {
            symbol: symbol.into(),
            name: symbol.into(),
            unit: "".into(),
            formula: formula.into(),
        };
        let a = create_aggregate(&pool, "p1-estadistica", mk("Re_max", "slope"))
            .await
            .unwrap();
        let b = create_aggregate(&pool, "p1-estadistica", mk("Re_min", "intercept"))
            .await
            .unwrap();
        assert!(b.position > a.position, "la 2da toma la siguiente posición");

        let listed = aggregates_for(&pool, "p1-estadistica").await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].symbol, "Re_max", "ordenado por posición");

        let updated = update_aggregate(&pool, "p1-estadistica", &a.id, mk("Re_max", "slope * 2"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.formula, "slope * 2");

        assert!(delete_aggregate(&pool, "p1-estadistica", &a.id)
            .await
            .unwrap());
        assert!(
            !delete_aggregate(&pool, "p1-estadistica", &a.id)
                .await
                .unwrap(),
            "borrar de nuevo devuelve false"
        );
        assert_eq!(
            aggregates_for(&pool, "p1-estadistica").await.unwrap().len(),
            1
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
            "p2-cc",
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
        assert!(!delete_curve(&pool, "p2-cc", &c1.id).await.unwrap());

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
            "p2-cc",
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
        // P1 péndulo, 3 operadores independientes: L (is_given) + t_med (dato sin incertidumbre,
        // "t_1/2") + T1 (obligatorio) + T2/T3 (opcionales), todos repeated.
        assert_eq!(def.quantities.len(), 5);
        let l = def.quantities.iter().find(|q| q.symbol == "L").unwrap();
        assert!(l.is_given);
        let t_med = def.quantities.iter().find(|q| q.symbol == "t_med").unwrap();
        assert!(t_med.is_given);
        assert!(
            !t_med.has_uncertainty,
            "t_med no deberia pedir incertidumbre"
        );
        let t1 = def.quantities.iter().find(|q| q.symbol == "T1").unwrap();
        assert!(t1.repeated);
        assert!(!t1.optional, "el operador 1 es obligatorio");
        for symbol in ["T2", "T3"] {
            let t = def.quantities.iter().find(|q| q.symbol == symbol).unwrap();
            assert!(t.repeated);
            assert!(t.optional, "{symbol} deberia ser opcional");
        }

        assert_eq!(def.results.len(), 5);
        for symbol in ["gamma", "Q", "g1", "g2", "g3"] {
            assert!(
                def.results.iter().any(|r| r.symbol == symbol),
                "falta el resultado {symbol}"
            );
        }
        // gamma, Q, g1/g2/g3 son los resultados centrales que el alumno debe entregar.
        for symbol in ["gamma", "Q", "g1", "g2", "g3"] {
            let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
            assert!(r.is_final, "{symbol} deberia ser final");
        }
        // gamma y Q van sin ±U (t_med tampoco, pero es magnitud, no resultado); g1/g2/g3 sí
        // muestran su incertidumbre (propagada de T1/T2/T3 y L).
        for symbol in ["gamma", "Q"] {
            let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
            assert!(
                !r.has_uncertainty,
                "{symbol} no deberia mostrar incertidumbre"
            );
        }
        for symbol in ["g1", "g2", "g3"] {
            let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
            assert!(r.has_uncertainty, "{symbol} deberia mostrar incertidumbre");
        }
        // Q usa el periodo del operador 1 (aclarado en el nombre para el docente).
        let q = def.results.iter().find(|r| r.symbol == "Q").unwrap();
        assert!(q.formula.contains("T1"));
        assert!(q.name.contains("Operador 1"));

        // Segunda pasada: no debe duplicar.
        seed_definitions(&pool).await.unwrap();
        let def2 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        assert_eq!(def2.quantities.len(), 5);
        assert_eq!(def2.results.len(), 5);
    }

    /// Extremo a extremo sobre la práctica real (no un fixture sintético): Operador 1 con datos,
    /// Operador 2/3 sin cargar (opcionales, no deben bloquear ni romper el análisis). Verifica que
    /// g1 se computa, g2/g3 quedan como advertencia (no pánico), t_med/gamma/Q dan U = 0 pese a
    /// que T1 sí tiene incertidumbre real, y Q usa T1 (confirmado en el valor, no solo la fórmula).
    #[tokio::test]
    async fn analyze_p1_estadistica_con_operadores_opcionales() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        let id = |sym: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == sym)
                .unwrap()
                .id
                .clone()
        };
        let mk = |sym: &str, vals: Vec<f64>, given_u: Option<f64>| {
            crate::computation::MeasurementInput {
                quantity_id: id(sym),
                instrument_id: None,
                scale_id: None,
                values: vals,
                given_u,
                point_replicas: None,
                operator_replicas: None,
            }
        };

        let (l, t_med) = (1.0_f64, 2.006_f64);
        let t1_vals = vec![2.0_f64, 2.02, 1.98];
        let t1_mean = t1_vals.iter().sum::<f64>() / t1_vals.len() as f64;
        let measurements = vec![
            mk("L", vec![l], Some(0.002)),
            // t_med no tiene campo U en el form: aunque llegara un given_u (p. ej. de una entrega
            // vieja), has_uncertainty=false debe ignorarlo y dejar u=0.
            mk("t_med", vec![t_med], Some(99.0)),
            mk("T1", t1_vals, None),
            // T2/T3 sin cargar: opcionales, no deben bloquear el análisis.
        ];

        let analysis = crate::computation::analyze(&pool, "p1-estadistica", &measurements)
            .await
            .unwrap();

        let t1_q = analysis
            .quantities
            .iter()
            .find(|q| q.symbol == "T1")
            .unwrap();
        assert!(t1_q.result.u_c > 0.0, "T1 sí debe tener incertidumbre real");

        let t_med_q = analysis
            .quantities
            .iter()
            .find(|q| q.symbol == "t_med")
            .unwrap();
        assert_eq!(
            t_med_q.result.u_c, 0.0,
            "t_med sin instrumento/U debe dar u=0 pese a un given_u cargado"
        );

        let derived = |sym: &str| {
            analysis
                .derived
                .iter()
                .find(|d| d.symbol == sym)
                .unwrap_or_else(|| panic!("{sym} debe estar en derived"))
        };

        let g1 = derived("g1");
        let expected_g1 = 4.0 * std::f64::consts::PI.powi(2) * l / (t1_mean * t1_mean);
        assert!(
            (g1.value - expected_g1).abs() < 1e-9,
            "g1 esperado {expected_g1}, obtenido {}",
            g1.value
        );

        // g2/g3 dependen de T2/T3, que no se cargaron: no deben tirar la práctica abajo, solo
        // avisar y quedar no-finitos.
        assert!(!derived("g2").value.is_finite());
        assert!(!derived("g3").value.is_finite());
        assert!(
            analysis
                .warnings
                .iter()
                .any(|w| w.contains("T2") || w.contains("g2")),
            "debe haber una advertencia por T2/g2 sin datos"
        );

        // gamma y Q se muestran sin ±U (has_uncertainty=false), aunque Q propague de fondo la
        // incertidumbre real de T1.
        let gamma = derived("gamma");
        assert!(!gamma.has_uncertainty);
        let expected_gamma = 2.0 * std::f64::consts::LN_2 / t_med;
        assert!((gamma.value - expected_gamma).abs() < 1e-9);

        let q = derived("Q");
        assert!(!q.has_uncertainty);
        let expected_q = std::f64::consts::PI * t_med / (t1_mean * std::f64::consts::LN_2);
        assert!(
            (q.value - expected_q).abs() < 1e-9,
            "Q debe usar T1: esperado {expected_q}, obtenido {}",
            q.value
        );
    }

    /// gamma/Q pasaron a ser resultado final despues del alta inicial; una base ya sembrada con
    /// el esquema viejo (is_final=0) debe actualizarse via backfill, no requiere resembrar.
    #[tokio::test]
    async fn seed_definitions_backfills_gamma_and_q_as_final() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        sqlx::query(
            "UPDATE practice_results SET is_final = 0 \
             WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q')",
        )
        .execute(&pool)
        .await
        .unwrap();

        seed_definitions(&pool).await.unwrap();

        let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
        for symbol in ["gamma", "Q"] {
            let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
            assert!(r.is_final, "{symbol} deberia quedar final tras el backfill");
        }
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

    // Verifica que las fórmulas sembradas de P3 son evaluables por el motor (sin NaN/errores).
    // Las de P2 (p2-cc) las cubre `analyze_p2_cc_derives_results_and_aliases`.
    #[tokio::test]
    async fn seeded_p3_formulas_compute() {
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
            &def.point_results,
            &def.aggregates,
            &Default::default(),
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

    /// La definición sembrada de Fluidos II se puebla (magnitudes + M_medio + 4 agregados) y
    /// computa de extremo a extremo. Caso construido: t = slope*(sqrt(h_max)-sqrt(h)) con slope=100
    /// e intercepto 0, así el ajuste recupera la pendiente y M_medio / los agregados dan los valores
    /// calculados a mano (Re_max=55000, Re_min=25000, Re_medio=40000, M_teorico=0.86).
    #[tokio::test]
    async fn seeded_fluidos2_populates_and_computes() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "fluidos-2").await.unwrap().unwrap();

        // Definición: 11 magnitudes, 1 mensurando (M_medio), 4 agregados en orden.
        assert_eq!(def.quantities.len(), 11);
        assert_eq!(def.results.len(), 1);
        assert_eq!(def.results[0].symbol, "M_medio");
        assert_eq!(
            def.aggregates
                .iter()
                .map(|a| a.symbol.as_str())
                .collect::<Vec<_>>(),
            ["Re_max", "Re_min", "Re_medio", "M_teorico"],
        );

        let id = |sym: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == sym)
                .unwrap_or_else(|| panic!("falta la magnitud {sym}"))
                .id
                .clone()
        };
        // x = sqrt(0.36) - sqrt(h) = [0, .1, .2, .3, .4]; con slope=100 -> t = [0,10,20,30,40].
        let h_vals = vec![0.36_f64, 0.25, 0.16, 0.09, 0.04];
        let t_vals = vec![0.0_f64, 10.0, 20.0, 30.0, 40.0];
        let per_point = |sym: &str, values: Vec<f64>| crate::computation::MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        };
        let scalar = |sym: &str, value: f64| per_point(sym, vec![value]);
        let measurements = vec![
            per_point("h", h_vals),
            per_point("t", t_vals),
            scalar("h_max", 0.36),
            scalar("R_cap", 0.001),
            scalar("L_cap", 0.1),
            scalar("R_recip", 0.05),
            scalar("g", 9.8),
            scalar("rho", 1000.0),
            scalar("mu_agua", 1e-3),
            scalar("kp", 0.78),
            scalar("Temp", 20.0),
        ];

        let analysis = crate::computation::compute_regresion(
            &def.quantities,
            &def.intermediates,
            &def.results,
            &def.point_results,
            &def.aggregates,
            &Default::default(),
            def.x_formula.as_deref().unwrap(),
            def.y_formula.as_deref().unwrap(),
            &measurements,
        )
        .unwrap();

        let reg = analysis.regression.unwrap();
        assert!((reg.slope - 100.0).abs() < 1e-6, "slope {}", reg.slope);
        assert!(reg.intercept.abs() < 1e-6, "intercept {}", reg.intercept);

        // M_medio = 2*g*(slope*R_cap^2/(2*R_recip^2))^2 - 2.
        let m = analysis
            .derived
            .iter()
            .find(|d| d.symbol == "M_medio")
            .unwrap();
        assert!((m.value - (-1.99216)).abs() < 1e-5, "M_medio {}", m.value);

        let agg = |sym: &str| {
            analysis
                .aggregates
                .iter()
                .find(|a| a.symbol == sym)
                .unwrap_or_else(|| panic!("falta agregado {sym}"))
                .value
        };
        assert!(
            (agg("Re_max") - 55000.0).abs() < 1e-3,
            "Re_max {}",
            agg("Re_max")
        );
        assert!(
            (agg("Re_min") - 25000.0).abs() < 1e-3,
            "Re_min {}",
            agg("Re_min")
        );
        assert!(
            (agg("Re_medio") - 40000.0).abs() < 1e-3,
            "Re_medio {}",
            agg("Re_medio")
        );
        assert!(
            (agg("M_teorico") - 0.86).abs() < 1e-9,
            "M_teorico {}",
            agg("M_teorico")
        );
        // No debe haber avisos de desalineamiento ni de valores no finitos.
        assert!(
            analysis.warnings.is_empty(),
            "warnings: {:?}",
            analysis.warnings
        );
    }

    /// La definición sembrada de Filtros tiene 9 magnitudes, 2 mensurandos escalares
    /// (fpasaje, fbloqueo), 3 intermedias y 2 curvas con x_log.
    #[tokio::test]
    async fn seeded_filtros_populates_and_computes() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "filtros").await.unwrap().unwrap();

        assert_eq!(def.quantities.len(), 9);
        assert_eq!(
            def.results
                .iter()
                .map(|r| r.symbol.as_str())
                .collect::<Vec<_>>(),
            ["fpasaje", "fbloqueo"],
        );
        assert_eq!(
            def.intermediates
                .iter()
                .map(|i| i.symbol.as_str())
                .collect::<Vec<_>>(),
            ["omega", "razon", "phi"],
        );
        assert_eq!(def.curves.len(), 2);
        assert!(
            def.curves[0].x_log,
            "curva 1 (amplitud) debe tener x_log=true"
        );
        assert_eq!(def.curves[0].x_formula, "omega");
        assert_eq!(def.curves[0].y_formula, "razon");
        assert!(
            def.curves[1].x_log,
            "curva 2 (desfasaje) debe tener x_log=true"
        );
        assert_eq!(def.curves[1].y_formula, "phi");

        // Computo end-to-end: 3 puntos, omega=2*pi*f, razon=VRpp/Vgpp, phi=asin(b/a).
        let id = |sym: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == sym)
                .unwrap()
                .id
                .clone()
        };
        let pt = |sym: &str, vals: Vec<f64>| crate::computation::MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values: vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        };
        // b/a = 0.5 → phi = asin(0.5) ≈ 0.5236 rad; VRpp/Vgpp = 0.8.
        let measurements = vec![
            pt("f", vec![100.0, 1000.0, 10000.0]),
            pt("VRpp", vec![0.8, 0.8, 0.8]),
            pt("Vgpp", vec![1.0, 1.0, 1.0]),
            pt("a", vec![1.0, 1.0, 1.0]),
            pt("b", vec![0.5, 0.5, 0.5]),
            pt("R", vec![100.0]),
            pt("C1", vec![1e-6]),
            pt("C2", vec![1e-6]),
            pt("L", vec![1e-3]),
        ];
        let curves: Vec<crate::computation::CurveSpec> = def
            .curves
            .iter()
            .map(|c| crate::computation::CurveSpec {
                x_formula: &c.x_formula,
                y_formula: &c.y_formula,
                x_log: c.x_log,
            })
            .collect();
        let (analysis, _) = crate::computation::compute_curva(
            &def.quantities,
            &def.intermediates,
            &curves,
            &measurements,
        )
        .unwrap();
        assert_eq!(analysis.scatters.len(), 2);
        // Curva 1 (amplitud): x = omega = 2*pi*f; y = razon = 0.8.
        let amp = &analysis.scatters[0];
        assert!((amp.points[0].0 - 2.0 * std::f64::consts::PI * 100.0).abs() < 1e-6);
        assert!((amp.points[0].1 - 0.8).abs() < 1e-9);
        // Curva 2 (desfasaje): y = phi = asin(0.5) ≈ 0.5236.
        let ph = &analysis.scatters[1];
        assert!((ph.points[0].1 - (0.5_f64).asin()).abs() < 1e-9);
        assert!(ph.x_log);
    }

    /// La definición sembrada de P2-cc (corriente continua unificada) tiene 17 magnitudes
    /// (15 escalares + R e I por punto), 13 mensurandos (12 finales; Req no lo es),
    /// la intermedia P = I^2*R y la curva P vs R.
    #[tokio::test]
    async fn seeded_p2_cc_populates_definition() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p2-cc").await.unwrap().unwrap();

        assert_eq!(def.quantities.len(), 17);
        for symbol in [
            "R1", "R2", "R3", "Vg_s", "RA_s", "VR1_s", "VR2_s", "VR3_s", "Vg_p", "RA_p", "VR1_p",
            "VR2_p", "VR3_p", "Vg_c", "RA_c", "R", "I",
        ] {
            assert!(
                def.quantities.iter().any(|q| q.symbol == symbol),
                "falta la magnitud {symbol}"
            );
        }
        // Solo R e I son por punto; el resto son escalares compartidos. RA (por parte) es dato
        // de catedra (tabla segun la escala del amperimetro); el resto se mide.
        let given = ["RA_s", "RA_p", "RA_c"];
        for q in &def.quantities {
            let per_point = q.symbol == "R" || q.symbol == "I";
            assert_eq!(q.per_point, per_point, "per_point de {}", q.symbol);
            assert_eq!(
                q.is_given,
                given.contains(&q.symbol.as_str()),
                "is_given de {}",
                q.symbol
            );
        }
        assert_eq!(def.results.len(), 13);
        assert_eq!(def.results.iter().filter(|r| r.is_final).count(), 12);
        let req = def.results.iter().find(|r| r.symbol == "Req").unwrap();
        assert!(!req.is_final);
        let i_s = def.results.iter().find(|r| r.symbol == "I_s").unwrap();
        assert_eq!(i_s.formula, "Vg_s / (R1 + R2 + R3 + RA_s)");
        assert_eq!(def.intermediates.len(), 1);
        assert_eq!(def.intermediates[0].symbol, "P");
        assert_eq!(def.curves.len(), 1);
        assert!(!def.curves[0].x_log);
        assert_eq!(def.curves[0].x_formula, "R");
        assert_eq!(def.curves[0].y_formula, "P");
    }

    /// Integración: `analyze()` para p2-cc deriva los mensurandos de las tres partes y los
    /// alias de extremos de la tabla de potencia.
    ///
    /// Serie: I_s = Vg_s/(R1+R2+R3+RA_s) y VRi_s_t. Paralelo: Req, I_p y VRi_p_t.
    /// Potencia: RP_max_t = Rth, P_max_t = Vg_c²/(4·Rth), y los experimentales
    /// P_max_e = max(P) e RP_max_e = R en ese punto, ambos con U = 0. Además el análisis
    /// expone las magnitudes escalares medidas (`quantities`), que antes se descartaban.
    #[tokio::test]
    async fn analyze_p2_cc_derives_results_and_aliases() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "p2-cc").await.unwrap().unwrap();

        let id = |sym: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == sym)
                .unwrap()
                .id
                .clone()
        };
        let pt = |sym: &str, vals: Vec<f64>| crate::computation::MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values: vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        };

        let (r1, r2, r3) = (100.0_f64, 200.0_f64, 200.0_f64);
        let (vg_s, ra_s) = (8.0_f64, 10.0_f64);
        let (vg_p, ra_p) = (8.0_f64, 10.0_f64);
        let (vg_c, ra_c) = (10.0_f64, 100.0_f64);
        let rpar = r2 * r3 / (r2 + r3); // = 100.0
        let rth = ra_c + rpar; // = 200.0
        let rs = vec![100.0_f64, 200.0, 400.0];
        let is: Vec<f64> = rs.iter().map(|r| vg_c / (rth + r)).collect();
        let measurements = vec![
            pt("R1", vec![r1]),
            pt("R2", vec![r2]),
            pt("R3", vec![r3]),
            pt("Vg_s", vec![vg_s]),
            pt("RA_s", vec![ra_s]),
            pt("VR1_s", vec![1.55]),
            pt("VR2_s", vec![3.15]),
            pt("VR3_s", vec![3.15]),
            pt("Vg_p", vec![vg_p]),
            pt("RA_p", vec![ra_p]),
            pt("VR1_p", vec![3.8]),
            pt("VR2_p", vec![3.8]),
            pt("VR3_p", vec![3.8]),
            pt("Vg_c", vec![vg_c]),
            pt("RA_c", vec![ra_c]),
            pt("R", rs.clone()),
            pt("I", is),
        ];

        let analysis = crate::computation::analyze(&pool, "p2-cc", &measurements)
            .await
            .unwrap();

        // Las magnitudes escalares medidas se exponen (antes el camino curva las descartaba).
        assert_eq!(analysis.quantities.len(), 15);
        assert!(analysis.quantities.iter().any(|q| q.symbol == "VR1_s"));

        let derived = |sym: &str| {
            analysis
                .derived
                .iter()
                .find(|d| d.symbol == sym)
                .unwrap_or_else(|| panic!("{sym} debe estar en derived"))
        };

        // Serie: Rtot = 510.
        let rtot = r1 + r2 + r3 + ra_s;
        assert!((derived("I_s").value - vg_s / rtot).abs() < 1e-9);
        assert!((derived("VR1_s_t").value - vg_s * r1 / rtot).abs() < 1e-9);
        assert!((derived("VR3_s_t").value - vg_s * r3 / rtot).abs() < 1e-9);

        // Paralelo: Req = 210.
        let req = r1 + ra_p + rpar;
        assert!((derived("Req").value - req).abs() < 1e-9);
        assert!((derived("I_p").value - vg_p / req).abs() < 1e-9);
        assert!((derived("VR2_p_t").value - vg_p * rpar / req).abs() < 1e-9);

        // Potencia teorica: Rth = 200, P_max = 0.125 W.
        assert!((derived("RP_max_t").value - rth).abs() < 1e-9);
        assert!((derived("P_max_t").value - vg_c * vg_c / (4.0 * rth)).abs() < 1e-9);

        // Experimentales por alias: el punto R = 200 = Rth maximiza P en la tabla.
        let p_max_table = (vg_c / (rth + rth)).powi(2) * rth;
        let pme = derived("P_max_e");
        assert!(
            (pme.value - p_max_table).abs() < 1e-9,
            "P_max_e esperado {p_max_table}, obtenido {}",
            pme.value
        );
        assert_eq!(pme.u_expanded, 0.0, "P_max_e va sin incertidumbre");
        let rpe = derived("RP_max_e");
        assert!((rpe.value - 200.0).abs() < 1e-9);
        assert_eq!(rpe.u_expanded, 0.0, "RP_max_e va sin incertidumbre");
    }

    /// Integración: `analyze()` para filtros deriva fpasaje y fbloqueo correctamente.
    ///
    /// Topología: C2||L en serie con C1 y R. Fórmulas teóricas:
    ///   fpasaje  = 1/(2π√(L(C1+C2)))   (resonancia serie)
    ///   fbloqueo = 1/(2π√(LC2))         (resonancia paralelo del tanque)
    #[tokio::test]
    async fn analyze_filtros_derives_fpasaje_fbloqueo() {
        let (pool, _dir) = setup().await;
        seed_definitions(&pool).await.unwrap();
        let def = definition(&pool, "filtros").await.unwrap().unwrap();

        let id = |sym: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == sym)
                .unwrap()
                .id
                .clone()
        };
        let pt = |sym: &str, vals: Vec<f64>| crate::computation::MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values: vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        };

        // Valores de componentes: R=1kΩ, C1=C2=10nF, L=10mH.
        let r = 1000.0_f64;
        let c1 = 10e-9_f64;
        let c2 = 10e-9_f64;
        let l = 10e-3_f64;
        let fp_expected = 1.0 / (2.0 * std::f64::consts::PI * (l * (c1 + c2)).sqrt());
        let fb_expected = 1.0 / (2.0 * std::f64::consts::PI * (l * c2).sqrt());

        // 3 puntos de barrido (valores arbitrarios; los escalares no dependen de ellos).
        let measurements = vec![
            pt("f", vec![1000.0, 5000.0, 10000.0]),
            pt("VRpp", vec![0.5, 1.0, 0.5]),
            pt("Vgpp", vec![1.0, 1.0, 1.0]),
            pt("a", vec![1.0, 1.0, 1.0]),
            pt("b", vec![0.5, 0.5, 0.5]),
            pt("R", vec![r]),
            pt("C1", vec![c1]),
            pt("C2", vec![c2]),
            pt("L", vec![l]),
        ];

        let analysis = crate::computation::analyze(&pool, "filtros", &measurements)
            .await
            .unwrap();

        assert!(
            !analysis.derived.is_empty(),
            "derived debe contener fpasaje y fbloqueo"
        );
        let fp = analysis
            .derived
            .iter()
            .find(|d| d.symbol == "fpasaje")
            .expect("fpasaje debe estar en derived");
        assert!(
            (fp.value - fp_expected).abs() < 1.0,
            "fpasaje esperado {fp_expected:.2} Hz, obtenido {:.2}",
            fp.value
        );
        let fb = analysis
            .derived
            .iter()
            .find(|d| d.symbol == "fbloqueo")
            .expect("fbloqueo debe estar en derived");
        assert!(
            (fb.value - fb_expected).abs() < 1.0,
            "fbloqueo esperado {fb_expected:.2} Hz, obtenido {:.2}",
            fb.value
        );
        // fbloqueo > fpasaje (C1+C2 > C2 ⟹ √(L(C1+C2)) > √(LC2)).
        assert!(
            fb.value > fp.value,
            "fbloqueo ({}) debe ser mayor que fpasaje ({})",
            fb.value,
            fp.value
        );
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
