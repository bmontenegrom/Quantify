use super::PracticeDefinition;
use crate::db::DISPLAY_SYMBOL_TABLES;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

const LEGACY_DISPLAY_SYMBOLS: &str = "2026-11-simbolos-a-mostrar";

pub(super) async fn display_symbols_for(
    pool: &SqlitePool,
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
    let rows: Vec<(String, String)> = sqlx::query_as(&selects.join(" UNION ALL "))
        .bind(practice_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}

pub async fn set_display_symbol(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    display_symbol: &str,
) -> anyhow::Result<bool> {
    let mut updated = 0;
    for table in DISPLAY_SYMBOL_TABLES {
        updated += sqlx::query(&format!(
            "UPDATE {table} SET display_symbol = ?1 WHERE practice_id = ?2 AND symbol = ?3"
        ))
        .bind(display_symbol)
        .bind(practice_id)
        .bind(symbol)
        .execute(pool)
        .await?
        .rows_affected();
    }
    Ok(updated > 0)
}

fn legacy_display_symbol(symbol: &str) -> Option<String> {
    let fixed = match symbol {
        "tmedio" => Some("t_1/2"),
        "Vg" => Some("V_G"),
        "RA" => Some("R_A"),
        _ => None,
    };
    if let Some(display) = fixed {
        return Some(display.to_string());
    }
    for element in ["R", "C", "L"] {
        if symbol == format!("V{element}pp") {
            return Some(format!("V_{element}_PP"));
        }
        for kind in ["teo", "exp"] {
            if symbol == format!("V{element}_{kind}") {
                return Some(format!("V_{element}_{kind}"));
            }
            if symbol == format!("phi{element}_{kind}") {
                return Some(format!("phi_{element}_{kind}"));
            }
        }
    }
    let base = ["_s_t", "_p_t", "_c_t", "_s", "_p", "_c"]
        .iter()
        .find_map(|suffix| symbol.strip_suffix(suffix))?;
    ["Vg", "RA", "VR1", "VR2", "VR3", "I"]
        .contains(&base)
        .then(|| legacy_display_symbol(base).unwrap_or_else(|| base.to_string()))
}

pub(super) fn version_symbols(definition: &PracticeDefinition) -> impl Iterator<Item = &str> {
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

pub(super) async fn migrate_legacy_display_symbols(pool: &SqlitePool) -> anyhow::Result<()> {
    let tables_done = crate::db::migration_applied(pool, LEGACY_DISPLAY_SYMBOLS).await?;
    let mut tx = pool.begin().await?;
    if !tables_done {
        for table in DISPLAY_SYMBOL_TABLES {
            let rows: Vec<(String, String)> = sqlx::query_as(&format!(
                "SELECT id, symbol FROM {table} WHERE display_symbol = ''"
            ))
            .fetch_all(&mut *tx)
            .await?;
            for (id, symbol) in rows {
                let Some(display) = legacy_display_symbol(&symbol) else {
                    continue;
                };
                sqlx::query(&format!(
                    "UPDATE {table} SET display_symbol = ?1 WHERE id = ?2"
                ))
                .bind(display)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            }
        }
        sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
            .bind(LEGACY_DISPLAY_SYMBOLS)
            .bind(chrono::Utc::now())
            .execute(&mut *tx)
            .await?;
    }
    let versions: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, definition_json FROM practice_versions \
         WHERE CASE WHEN json_valid(definition_json) \
               THEN json_type(definition_json, '$.display_symbols') IS NULL ELSE 1 END",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (id, json) in versions {
        let definition = match super::versions::parse(&json) {
            Ok(definition) => definition,
            Err(error) => {
                tracing::warn!(
                    version_id = %id,
                    %error,
                    "versión ilegible: se reintenta la migración de símbolos a mostrar en cada arranque"
                );
                continue;
            }
        };
        let mut symbols: BTreeMap<String, String> = version_symbols(&definition)
            .filter_map(|symbol| Some((symbol.to_string(), legacy_display_symbol(symbol)?)))
            .collect();
        symbols.extend(definition.display_symbols);
        sqlx::query(
            "UPDATE practice_versions \
             SET definition_json = json_set(definition_json, '$.display_symbols', json(?1)) \
             WHERE id = ?2",
        )
        .bind(serde_json::to_string(&symbols)?)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::legacy_display_symbol;

    #[test]
    fn legacy_display_symbol_matches_the_old_frontend_specials() {
        let cases = [
            ("tmedio", Some("t_1/2")),
            ("Vg", Some("V_G")),
            ("RA", Some("R_A")),
            ("VRpp", Some("V_R_PP")),
            ("VC_teo", Some("V_C_teo")),
            ("phiL_exp", Some("phi_L_exp")),
            ("Vg_s", Some("V_G")),
            ("RA_c", Some("R_A")),
            ("VR1_s_t", Some("VR1")),
            ("I_p", Some("I")),
            ("T_oc", None),
            ("h_max", None),
            ("R_s", None),
        ];
        for (symbol, expected) in cases {
            assert_eq!(
                legacy_display_symbol(symbol).as_deref(),
                expected,
                "{symbol}"
            );
        }
    }
}
