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
    /// `true` si es un resultado central que el alumno debe entregar (para comparar contra el
    /// valor automático): habilita el campo en "Mis cálculos" igual que `PracticeResult::is_final`.
    #[serde(default)]
    pub is_final: bool,
}

/// Datos para crear o actualizar un mensurando agregado.
#[derive(Debug, Deserialize)]
pub struct AggregateInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    #[serde(default)]
    pub is_final: bool,
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
/// (No usa el `SymbolFormulaRow` genérico: a diferencia de intermedias/point-results, tiene la
/// columna extra `is_final`.)
pub async fn aggregates_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeAggregate>> {
    Ok(sqlx::query_as::<_, PracticeAggregate>(
        "SELECT id, practice_id, position, symbol, name, unit, formula, is_final \
         FROM practice_aggregates WHERE practice_id = ?1 ORDER BY position, id",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_aggregate(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeAggregate> {
    Ok(sqlx::query_as::<_, PracticeAggregate>(
        "SELECT id, practice_id, position, symbol, name, unit, formula, is_final \
         FROM practice_aggregates WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
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
    let position = next_position(pool, "practice_aggregates", "practice_id", practice_id).await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_aggregates \
         (id, practice_id, position, symbol, name, unit, formula, is_final) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(position)
    .bind(symbol)
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(formula)
    .bind(input.is_final)
    .execute(pool)
    .await?;
    fetch_aggregate(pool, &id).await
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
    let result = sqlx::query(
        "UPDATE practice_aggregates \
         SET symbol = ?3, name = ?4, unit = ?5, formula = ?6, is_final = ?7 \
         WHERE id = ?1 AND practice_id = ?2",
    )
    .bind(aggregate_id)
    .bind(practice_id)
    .bind(symbol)
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(formula)
    .bind(input.is_final)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_aggregate(pool, aggregate_id).await?))
}

/// Elimina un mensurando agregado de esa práctica por id. Devuelve `true` si existía.
pub async fn delete_aggregate(
    pool: &SqlitePool,
    practice_id: &str,
    aggregate_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM practice_aggregates WHERE id = ?1 AND practice_id = ?2")
        .bind(aggregate_id)
        .bind(practice_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
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
            qty_given("rho_e", "Densidad del acero", "kg/m3", "densidad"),
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
    // Auto-curación: rho_e (densidad del acero) es un dato dado con incertidumbre, no una medida
    // con instrumento. Re-aplica en cada boot para bases sembradas antes del cambio.
    sqlx::query(
        "UPDATE practice_quantities SET is_given = 1 \
         WHERE practice_id = 'viscosidad' AND symbol = 'rho_e' AND is_given = 0",
    )
    .execute(pool)
    .await?;
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
            no_u(qty_given(
                "mu_agua",
                "Viscosidad del agua (de tabla segun T)",
                "Pa.s",
                "viscosidad",
            )),
            no_u(qty_given(
                "kp",
                "Factor geometrico K (def. 0.78)",
                "",
                "adimensional",
            )),
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
                is_final: true,
            },
            AggregateInput {
                symbol: "Re_min".into(),
                name: "Numero de Reynolds minimo".into(),
                unit: "".into(),
                formula:
                    "2*rho*((h_last2 - h_last)/(t_last - t_last2))*(R_recip^2/(mu_agua*R_cap))"
                        .into(),
                is_final: true,
            },
            AggregateInput {
                symbol: "Re_medio".into(),
                name: "Numero de Reynolds medio".into(),
                unit: "".into(),
                formula: "(Re_max + Re_min)/2".into(),
                is_final: true,
            },
            AggregateInput {
                symbol: "M_teorico".into(),
                name: "Coeficiente de perdidas teorico".into(),
                unit: "".into(),
                formula: "kp + 4*(L_cap/(2*R_cap))*(16/Re_medio)".into(),
                is_final: true,
            },
        ] {
            create_aggregate(pool, "fluidos-2", input).await?;
        }
    }
    // Auto-curación: re-aplica en cada boot para bases que hayan quedado a medio migrar.
    sqlx::query(
        "UPDATE practice_quantities SET has_uncertainty = 0, is_given = 1 \
         WHERE practice_id = 'fluidos-2' AND symbol IN ('mu_agua', 'kp') \
         AND (has_uncertainty = 1 OR is_given = 0)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE practice_aggregates SET is_final = 1 \
         WHERE practice_id = 'fluidos-2' \
         AND symbol IN ('Re_max', 'Re_min', 'Re_medio', 'M_teorico') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Siembra Filtros (ver [`seed_definitions`]).
async fn seed_filtros(pool: &SqlitePool) -> anyhow::Result<()> {
    // Filtros — barrido en frecuencia de un circuito RLC. Por punto: f (frecuencia fijada por el
    // alumno), VRpp y Vgpp (tensiones pico a pico medidas), a y b (semiejes de la figura de
    // Lissajous). R, C1 y C2 se miden una vez con instrumento (no son dados); fpasaje_exp y
    // fbloqueo_exp tambien se miden una vez, con el osciloscopio (frecuencia de pasaje/bloqueo
    // observada). El unico dato dado por la catedra es L. Intermedias: omega=2*pi*f (rad/s),
    // razon=VRpp/Vgpp (adimensional), phi=asin(b/a) (rad). Dos curvas (Motor B): razon vs omega
    // (amplitud) y phi vs omega (desfasaje), ambas con eje x logaritmico. Mensurandos teoricos,
    // comparables contra la frecuencia experimental medida (is_final): fpasaje=1/(2*pi*sqrt(L*
    // (C1+C2))) y fbloqueo=1/(2*pi*sqrt(L*C2)). Topologia confirmada: C2||L en serie con C1 y R.
    //
    // Migración de forma: la práctica quedó sembrada originalmente con R/C1/C2 como `qty_given`
    // (valor ± U cargado a mano) cuando en realidad son medidos por el alumno con instrumento, y
    // sin fpasaje_exp/fbloqueo_exp. `seed_practice` no resiembra sobre una base ya presente, así
    // que se limpia y se resiembra con la forma final (no hay mediciones reales bajo la forma
    // vieja, la práctica es reciente).
    if !quantity_missing(pool, "filtros", "L").await? {
        let given_r: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM practice_quantities \
             WHERE practice_id = 'filtros' AND symbol = 'R' AND is_given = 1",
        )
        .fetch_one(pool)
        .await?;
        let missing_exp = quantity_missing(pool, "filtros", "fpasaje_exp").await?;
        if given_r.0 > 0 || missing_exp {
            let mut tx = pool.begin().await?;
            sqlx::query(
                "DELETE FROM submission_measurements WHERE quantity_id IN \
                 (SELECT id FROM practice_quantities WHERE practice_id = 'filtros')",
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM practice_quantities WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM practice_results WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM practice_curves WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM practice_intermediates WHERE practice_id = 'filtros'")
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
    }
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
            qty_shared("R", "Resistencia", "ohm", "resistencia"),
            qty_shared("C1", "Capacitor C_1", "F", "capacitancia"),
            qty_shared("C2", "Capacitor C_2", "F", "capacitancia"),
            qty_shared(
                "fpasaje_exp",
                "Frecuencia de pasaje experimental",
                "Hz",
                "frecuencia",
            ),
            qty_shared(
                "fbloqueo_exp",
                "Frecuencia de bloqueo experimental",
                "Hz",
                "frecuencia",
            ),
            qty_given("L", "Inductor", "H", "inductancia"),
        ],
        &[
            // Topología: C2||L en serie con C1 y R.
            // Resonancia serie (pasaje): f = 1/(2π√(L(C1+C2)))
            // Resonancia paralelo del tanque (bloqueo): f = 1/(2π√(LC2))
            res_final(
                "fpasaje",
                "Frecuencia de pasaje teorica",
                "Hz",
                "1/(2*pi*math::sqrt(L*(C1+C2)))",
            ),
            res_final(
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
    // Auto-curación: re-aplica el flag correcto en cada boot para bases que hayan quedado a
    // medio migrar (p. ej. la migración de forma de arriba ya corrió una vez, pero R/C1/C2 o
    // fpasaje/fbloqueo quedaron con el flag viejo por algún otro motivo).
    sqlx::query(
        "UPDATE practice_quantities SET is_given = 0 \
         WHERE practice_id = 'filtros' AND symbol IN ('R', 'C1', 'C2') AND is_given = 1",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE practice_results SET is_final = 1 \
         WHERE practice_id = 'filtros' AND symbol IN ('fpasaje', 'fbloqueo') AND is_final = 0",
    )
    .execute(pool)
    .await?;
    // Nombres corregidos después del alta (subíndice de los capacitores, sin sufijo de
    // instrumento en las frecuencias experimentales): se resincronizan en cada boot.
    for (sym, name) in [
        ("C1", "Capacitor C_1"),
        ("C2", "Capacitor C_2"),
        ("fpasaje_exp", "Frecuencia de pasaje experimental"),
        ("fbloqueo_exp", "Frecuencia de bloqueo experimental"),
    ] {
        sqlx::query(
            "UPDATE practice_quantities SET name = ?2 \
             WHERE practice_id = 'filtros' AND symbol = ?1 AND name != ?2",
        )
        .bind(sym)
        .bind(name)
        .execute(pool)
        .await?;
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
mod tests;
