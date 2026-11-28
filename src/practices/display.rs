use super::PracticeDefinition;
use crate::db::DISPLAY_SYMBOL_TABLES;
use std::collections::BTreeMap;

pub(super) async fn display_symbols_for(
    conn: &mut sqlx::SqliteConnection,
    practice_id: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let selects: Vec<String> = DISPLAY_SYMBOL_TABLES
        .iter()
        .map(|table| {
            let retired = if *table == "practice_quantities" {
                " AND removed = 0"
            } else {
                ""
            };
            format!(
                "SELECT symbol, display_symbol FROM {table} \
                 WHERE practice_id = ?1 AND display_symbol <> ''{retired}"
            )
        })
        .collect();
    let rows: Vec<(String, String)> =
        sqlx::query_as(sqlx::AssertSqlSafe(selects.join(" UNION ALL ")))
            .bind(practice_id)
            .fetch_all(conn)
            .await?;
    Ok(rows.into_iter().collect())
}

pub fn version_symbols(definition: &PracticeDefinition) -> impl Iterator<Item = &str> {
    let quantities = definition.quantities.iter().map(|q| q.symbol.as_str());
    let results = definition.results.iter().map(|r| r.symbol.as_str());
    let intermediates = definition.intermediates.iter().map(|i| i.symbol.as_str());
    let point_results = definition.point_results.iter().map(|p| p.symbol.as_str());
    let aggregates = definition.aggregates.iter().map(|a| a.symbol.as_str());
    quantities
        .chain(results)
        .chain(intermediates)
        .chain(point_results)
        .chain(aggregates)
}
