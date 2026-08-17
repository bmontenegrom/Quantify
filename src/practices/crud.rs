use super::{
    AggregateInput, CurveInput, IntermediateInput, PointResultInput, PracticeAggregate,
    PracticeCurve, PracticeDefinition, PracticeIntermediate, PracticePointResult, QuantityInput,
    ResultInput,
};
use crate::db::{next_position, PracticeQuantity, PracticeResult};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

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
///
// ponytail: `default_value` queda fuera del UPDATE a propósito. Lo pone el seed y el editor del
// admin no lo manda, así que incluirlo aquí lo borraría al editar cualquier otro campo. Se agrega
// al editor (y al UPDATE) si el docente necesita cambiarlo.
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
         replicas_per_point, per_point, has_uncertainty, optional, default_value \
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
         replicas_per_point, per_point, has_uncertainty, optional, default_value \
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
pub(super) async fn insert_quantity(
    conn: &mut SqliteConnection,
    practice_id: &str,
    position: i64,
    input: &QuantityInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_quantities \
         (id, practice_id, symbol, name, unit, repeated, quantity, position, is_given, \
          replicas_per_point, per_point, has_uncertainty, optional, default_value) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
    .bind(input.default_value)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

/// Inserta un mensurando derivado en la práctica con la posición dada; devuelve su id generado.
pub(super) async fn insert_result(
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
