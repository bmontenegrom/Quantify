use super::{
    AggregateInput, CurveInput, IntermediateInput, PointResultInput, PracticeAggregate,
    PracticeCurve, PracticeDefinition, PracticeIntermediate, PracticePointResult, QuantityInput,
    ResultInput, INPUT_WIDGETS, LABEL_MODES,
};
use crate::db::{next_position, PracticeQuantity, PracticeResult};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct PracticeConfigRow {
    status: String,
    analysis_kind: Option<String>,
    x_formula: Option<String>,
    y_formula: Option<String>,
    operator_count: Option<i64>,
    current_version: Option<i64>,
}

pub async fn definition(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeDefinition>> {
    let row: Option<PracticeConfigRow> = sqlx::query_as(
        "SELECT status, analysis_kind, x_formula, y_formula, operator_count, \
         (SELECT number FROM practice_versions WHERE id = practices.current_version_id) \
         AS current_version FROM practices WHERE id = ?1",
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
    let parts = super::parts_for(pool, practice_id).await?;
    let sections = super::sections_for(pool, practice_id).await?;
    let display_symbols = super::display::display_symbols_for(pool, practice_id).await?;
    Ok(Some(
        PracticeDefinition {
            practice_id: practice_id.to_string(),
            status: row.status,
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
            parts,
            sections,
            display_symbols,
            current_version: row.current_version,
        }
        .with_effective_kinds(),
    ))
}

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

fn display_symbol(input: &Option<String>) -> Option<&str> {
    input.as_deref().map(str::trim)
}

fn label_mode(input: &QuantityInput) -> &str {
    input.label_mode.as_deref().unwrap_or(LABEL_MODES[0])
}

fn input_widget(input: &QuantityInput) -> &str {
    input.input_widget.as_deref().unwrap_or(INPUT_WIDGETS[0])
}

async fn create_symbol_formula_row<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    name: &str,
    unit: &str,
    formula: &str,
    display: Option<&str>,
) -> anyhow::Result<T> {
    let position = next_position(pool, T::TABLE, "practice_id", practice_id).await?;
    let id = Uuid::new_v4().to_string();
    let query = format!(
        "INSERT INTO {} (id, practice_id, position, symbol, name, unit, formula, display_symbol) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
        .bind(display.unwrap_or(""))
        .execute(pool)
        .await?;
    fetch_symbol_formula_row::<T>(pool, &id).await
}

#[allow(clippy::too_many_arguments)]
async fn update_symbol_formula_row<T: SymbolFormulaRow>(
    pool: &SqlitePool,
    practice_id: &str,
    row_id: &str,
    symbol: &str,
    name: &str,
    unit: &str,
    formula: &str,
    display: Option<&str>,
) -> anyhow::Result<Option<T>> {
    let query = format!(
        "UPDATE {} SET symbol = ?3, name = ?4, unit = ?5, formula = ?6, \
         display_symbol = COALESCE(?7, display_symbol) \
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
        .bind(display)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_symbol_formula_row::<T>(pool, row_id).await?))
}

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

pub async fn point_results_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticePointResult>> {
    symbol_formula_rows_for(pool, practice_id).await
}

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
        display_symbol(&input.display_symbol),
    )
    .await
}

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
        display_symbol(&input.display_symbol),
    )
    .await
}

pub async fn delete_point_result(
    pool: &SqlitePool,
    practice_id: &str,
    point_result_id: &str,
) -> anyhow::Result<bool> {
    delete_symbol_formula_row::<PracticePointResult>(pool, practice_id, point_result_id).await
}

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
         (id, practice_id, position, symbol, name, unit, formula, is_final, display_symbol) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&id)
    .bind(practice_id)
    .bind(position)
    .bind(symbol)
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(formula)
    .bind(input.is_final)
    .bind(display_symbol(&input.display_symbol).unwrap_or(""))
    .execute(pool)
    .await?;
    fetch_aggregate(pool, &id).await
}

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
         SET symbol = ?3, name = ?4, unit = ?5, formula = ?6, is_final = ?7, \
             display_symbol = COALESCE(?8, display_symbol) \
         WHERE id = ?1 AND practice_id = ?2",
    )
    .bind(aggregate_id)
    .bind(practice_id)
    .bind(symbol)
    .bind(input.name.trim())
    .bind(input.unit.trim())
    .bind(formula)
    .bind(input.is_final)
    .bind(display_symbol(&input.display_symbol))
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_aggregate(pool, aggregate_id).await?))
}

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

pub async fn intermediates_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeIntermediate>> {
    symbol_formula_rows_for(pool, practice_id).await
}

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
        display_symbol(&input.display_symbol),
    )
    .await
}

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
        display_symbol(&input.display_symbol),
    )
    .await
}

pub async fn delete_intermediate(
    pool: &SqlitePool,
    practice_id: &str,
    intermediate_id: &str,
) -> anyhow::Result<bool> {
    delete_symbol_formula_row::<PracticeIntermediate>(pool, practice_id, intermediate_id).await
}

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

#[derive(Debug, Clone, Copy)]
pub enum Orderable {
    Curves,
    Quantities,
    Results,
    Parts,
    Sections,
}

impl Orderable {
    fn table_filter_and_order(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Orderable::Curves => ("practice_curves", "", "position, id"),
            Orderable::Quantities => ("practice_quantities", "AND removed = 0", "position, symbol"),
            Orderable::Results => ("practice_results", "", "position, symbol"),
            Orderable::Parts => ("practice_parts", "", "position, id"),
            Orderable::Sections => ("practice_sections", "", "position, id"),
        }
    }
}

pub async fn move_item(
    pool: &SqlitePool,
    list: Orderable,
    practice_id: &str,
    item_id: &str,
    up: bool,
) -> anyhow::Result<Option<bool>> {
    let (table, filter, order) = list.table_filter_and_order();
    let mut tx = pool.begin().await?;
    let mut ids: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT id FROM {table} WHERE practice_id = ?1 {filter} ORDER BY {order}"
    ))
    .bind(practice_id)
    .fetch_all(&mut *tx)
    .await?;
    let Some(idx) = ids.iter().position(|id| id == item_id) else {
        return Ok(None);
    };
    let neighbor = if up {
        idx.checked_sub(1)
    } else {
        Some(idx + 1).filter(|&j| j < ids.len())
    };
    let Some(j) = neighbor else {
        return Ok(Some(false));
    };
    ids.swap(idx, j);
    for (position, id) in ids.iter().enumerate() {
        sqlx::query(&format!("UPDATE {table} SET position = ?2 WHERE id = ?1"))
            .bind(id)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Some(true))
}

async fn fetch_curve(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeCurve> {
    Ok(sqlx::query_as::<_, PracticeCurve>(
        "SELECT id, practice_id, position, x_formula, y_formula, x_log \
         FROM practice_curves WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

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

pub async fn update_quantity(
    pool: &SqlitePool,
    quantity_id: &str,
    input: QuantityInput,
) -> anyhow::Result<Option<PracticeQuantity>> {
    let result = sqlx::query(
        "UPDATE practice_quantities \
         SET symbol = ?2, name = ?3, unit = ?4, repeated = ?5, quantity = ?6, is_given = ?7, \
             replicas_per_point = ?8, per_point = ?9, has_uncertainty = ?10, optional = ?11, \
             slot_count = ?12, default_value = CASE WHEN ?13 THEN ?14 ELSE default_value END, \
             display_symbol = COALESCE(?15, display_symbol), label_mode = ?16, input_widget = ?17 \
         WHERE id = ?1 AND removed = 0",
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
    .bind(input.slot_count)
    .bind(input.default_value.is_some())
    .bind(input.default_value.flatten())
    .bind(display_symbol(&input.display_symbol))
    .bind(label_mode(&input))
    .bind(input_widget(&input))
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_quantity(pool, quantity_id).await?))
}

pub async fn submissions_off_slots(
    pool: &SqlitePool,
    quantity_id: &str,
    n: i64,
) -> anyhow::Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT submission_id) FROM submission_measurements \
         WHERE quantity_id = ?1 AND (value <> CAST(value AS INTEGER) OR value < 1 OR value > ?2)",
    )
    .bind(quantity_id)
    .bind(n)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub async fn create_result(
    pool: &SqlitePool,
    practice_id: &str,
    input: ResultInput,
    section_id: Option<&str>,
) -> anyhow::Result<PracticeResult> {
    let position = next_position(pool, "practice_results", "practice_id", practice_id).await?;
    let mut tx = pool.begin().await?;
    let id = insert_result(&mut tx, practice_id, position, &input).await?;
    if let Some(section_id) = section_id {
        sqlx::query(
            "UPDATE practice_results SET section_id = ?1 WHERE id = ?2 \
             AND EXISTS (SELECT 1 FROM practice_sections WHERE id = ?1 AND practice_id = ?3)",
        )
        .bind(section_id)
        .bind(&id)
        .bind(practice_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    fetch_result(pool, &id).await
}

pub async fn update_result(
    pool: &SqlitePool,
    result_id: &str,
    input: ResultInput,
) -> anyhow::Result<Option<PracticeResult>> {
    let rows = match input.tolerance {
        None => sqlx::query(
            "UPDATE practice_results \
                 SET symbol = ?2, name = ?3, unit = ?4, formula = ?5, is_final = ?6, \
                     has_uncertainty = ?7, display_symbol = COALESCE(?8, display_symbol) \
                 WHERE id = ?1",
        )
        .bind(result_id)
        .bind(input.symbol.trim())
        .bind(input.name.trim())
        .bind(input.unit.trim())
        .bind(input.formula.trim())
        .bind(input.is_final)
        .bind(input.has_uncertainty)
        .bind(display_symbol(&input.display_symbol))
        .execute(pool)
        .await?
        .rows_affected(),
        Some(tol) => sqlx::query(
            "UPDATE practice_results \
                 SET symbol = ?2, name = ?3, unit = ?4, formula = ?5, tolerance = ?6, is_final = ?7, \
                     has_uncertainty = ?8, display_symbol = COALESCE(?9, display_symbol) \
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
        .bind(display_symbol(&input.display_symbol))
        .execute(pool)
        .await?
        .rows_affected(),
    };
    if rows == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_result(pool, result_id).await?))
}

pub async fn delete_result(pool: &SqlitePool, result_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM practice_results WHERE id = ?1")
        .bind(result_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

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

async fn quantities_for(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Vec<PracticeQuantity>> {
    Ok(sqlx::query_as::<_, PracticeQuantity>(
        "SELECT id, practice_id, symbol, name, unit, repeated, quantity, position, is_given, \
         replicas_per_point, per_point, has_uncertainty, optional, default_value, slot_count, \
         label_mode, input_widget \
         FROM practice_quantities WHERE practice_id = ?1 AND removed = 0 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

async fn results_for(pool: &SqlitePool, practice_id: &str) -> anyhow::Result<Vec<PracticeResult>> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position, tolerance, is_final, has_uncertainty \
         FROM practice_results WHERE practice_id = ?1 ORDER BY position, symbol",
    )
    .bind(practice_id)
    .fetch_all(pool)
    .await?)
}

async fn fetch_quantity(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeQuantity> {
    Ok(sqlx::query_as::<_, PracticeQuantity>(
        "SELECT id, practice_id, symbol, name, unit, repeated, quantity, position, is_given, \
         replicas_per_point, per_point, has_uncertainty, optional, default_value, slot_count, \
         label_mode, input_widget \
         FROM practice_quantities WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

async fn fetch_result(pool: &SqlitePool, id: &str) -> anyhow::Result<PracticeResult> {
    Ok(sqlx::query_as::<_, PracticeResult>(
        "SELECT id, practice_id, symbol, name, unit, formula, position, tolerance, is_final, has_uncertainty \
         FROM practice_results WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?)
}

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
          replicas_per_point, per_point, has_uncertainty, optional, default_value, slot_count, \
          display_symbol, label_mode, input_widget) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
    .bind(input.default_value.flatten())
    .bind(input.slot_count)
    .bind(display_symbol(&input.display_symbol).unwrap_or(""))
    .bind(label_mode(input))
    .bind(input_widget(input))
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

pub(super) async fn insert_result(
    conn: &mut SqliteConnection,
    practice_id: &str,
    position: i64,
    input: &ResultInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO practice_results \
         (id, practice_id, symbol, name, unit, formula, position, tolerance, is_final, has_uncertainty, display_symbol) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
    .bind(display_symbol(&input.display_symbol).unwrap_or(""))
    .execute(&mut *conn)
    .await?;
    Ok(id)
}
