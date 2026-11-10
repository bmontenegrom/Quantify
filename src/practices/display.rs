use super::PracticeDefinition;
use crate::db::DISPLAY_SYMBOL_TABLES;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

const LEGACY_DISPLAY_SYMBOLS: &str = "2026-11-simbolos-a-mostrar";
const LEGACY_QUANTITY_PRESENTATION: &str = "2026-11-rotulo-y-entrada";
const LEGACY_COMPARE_WITH: &str = "2026-11-comparar-con";
const LEGACY_SYMBOL_FIRST: [&str; 22] = [
    "Vg_s", "RA_s", "VR1_s", "VR2_s", "VR3_s", "Vg_p", "RA_p", "VR1_p", "VR2_p", "VR3_p", "Vg_c",
    "RA_c", "Vg", "VRpp", "VCpp", "VLpp", "a_R", "b_R", "a_C", "b_C", "a_L", "b_L",
];

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
    if crate::db::migration_applied(pool, LEGACY_DISPLAY_SYMBOLS).await? {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
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
    let versions: Vec<(String, String)> =
        sqlx::query_as("SELECT id, definition_json FROM practice_versions")
            .fetch_all(&mut *tx)
            .await?;
    for (id, json) in versions {
        let definition = match super::versions::parse(&json) {
            Ok(definition) => definition,
            Err(error) => {
                tracing::warn!(
                    version_id = %id,
                    %error,
                    "versión ilegible: queda sin migrar a símbolos a mostrar y esta migración no se repite"
                );
                continue;
            }
        };
        let mut symbols: BTreeMap<String, String> = version_symbols(&definition)
            .filter_map(|symbol| Some((symbol.to_string(), legacy_display_symbol(symbol)?)))
            .collect();
        symbols.extend(definition.display_symbols);
        if symbols.is_empty() {
            continue;
        }
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
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_DISPLAY_SYMBOLS)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) fn legacy_quantity_presentation(
    symbol: &str,
    name: &str,
    quantity: Option<&str>,
    repeated: bool,
    is_given: bool,
    analysis_kind: Option<&str>,
) -> (&'static str, &'static str) {
    let label = if LEGACY_SYMBOL_FIRST.contains(&symbol) {
        "simbolo"
    } else if symbol == "T_oc" && !mentions_t_oc(name) {
        "nombre_simbolo"
    } else {
        "nombre"
    };
    let series = matches!(analysis_kind, Some("regresion_lineal" | "curva"));
    let widget = if quantity == Some("tiempo") && !is_given && (repeated || !series) {
        "cronometro"
    } else {
        "teclado"
    };
    (label, widget)
}

fn mentions_t_oc(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("toc") || name.contains("t_oc")
}

pub(super) async fn migrate_legacy_quantity_presentation(pool: &SqlitePool) -> anyhow::Result<()> {
    if crate::db::migration_applied(pool, LEGACY_QUANTITY_PRESENTATION).await? {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    type Row = (
        String,
        String,
        String,
        Option<String>,
        bool,
        bool,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT q.id, q.symbol, q.name, q.quantity, q.repeated, q.is_given, p.analysis_kind \
         FROM practice_quantities q JOIN practices p ON p.id = q.practice_id",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (id, symbol, name, quantity, repeated, is_given, kind) in rows {
        let (label, widget) = legacy_quantity_presentation(
            &symbol,
            &name,
            quantity.as_deref(),
            repeated,
            is_given,
            kind.as_deref(),
        );
        sqlx::query(
            "UPDATE practice_quantities \
             SET label_mode = CASE label_mode WHEN 'nombre' THEN ?1 ELSE label_mode END, \
                 input_widget = CASE input_widget WHEN 'teclado' THEN ?2 ELSE input_widget END \
             WHERE id = ?3",
        )
        .bind(label)
        .bind(widget)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    let versions: Vec<(String, String)> =
        sqlx::query_as("SELECT id, definition_json FROM practice_versions")
            .fetch_all(&mut *tx)
            .await?;
    for (id, json) in versions {
        let definition = match super::versions::parse(&json) {
            Ok(definition) => definition,
            Err(error) => {
                tracing::warn!(
                    version_id = %id,
                    %error,
                    "versión ilegible: queda sin migrar a rótulo y entrada y esta migración no se repite"
                );
                continue;
            }
        };
        for (index, q) in definition.quantities.iter().enumerate() {
            let (label, widget) = legacy_quantity_presentation(
                &q.symbol,
                &q.name,
                q.quantity.as_deref(),
                q.repeated,
                q.is_given,
                definition.analysis_kind.as_deref(),
            );
            let label = if q.label_mode.is_empty() {
                label
            } else {
                &q.label_mode
            };
            let widget = if q.input_widget.is_empty() {
                widget
            } else {
                &q.input_widget
            };
            sqlx::query(&format!(
                "UPDATE practice_versions \
                 SET definition_json = json_set(definition_json, \
                     '$.quantities[{index}].label_mode', ?1, \
                     '$.quantities[{index}].input_widget', ?2) \
                 WHERE id = ?3"
            ))
            .bind(label)
            .bind(widget)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_QUANTITY_PRESENTATION)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn legacy_compare_with<'a>(result: &str, quantities: &[&'a str]) -> Option<&'a str> {
    let base = result.strip_suffix("_t")?;
    quantities.iter().copied().find(|q| *q == base)
}

pub(super) async fn migrate_legacy_compare_with(pool: &SqlitePool) -> anyhow::Result<()> {
    if crate::db::migration_applied(pool, LEGACY_COMPARE_WITH).await? {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let quantities: Vec<(String, String)> =
        sqlx::query_as("SELECT practice_id, symbol FROM practice_quantities WHERE removed = 0")
            .fetch_all(&mut *tx)
            .await?;
    let mut by_practice: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (practice_id, symbol) in &quantities {
        by_practice.entry(practice_id).or_default().push(symbol);
    }
    let results: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, practice_id, symbol FROM practice_results WHERE compare_with = ''",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (id, practice_id, symbol) in results {
        let symbols = by_practice
            .get(practice_id.as_str())
            .map_or(&[][..], Vec::as_slice);
        let Some(target) = legacy_compare_with(&symbol, symbols) else {
            continue;
        };
        sqlx::query("UPDATE practice_results SET compare_with = ?1 WHERE id = ?2")
            .bind(target)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let versions: Vec<(String, String)> =
        sqlx::query_as("SELECT id, definition_json FROM practice_versions")
            .fetch_all(&mut *tx)
            .await?;
    for (id, json) in versions {
        let mut definition = match super::versions::parse(&json) {
            Ok(definition) => definition,
            Err(error) => {
                tracing::warn!(
                    version_id = %id,
                    %error,
                    "versión ilegible: queda sin migrar a comparar con y esta migración no se repite"
                );
                continue;
            }
        };
        let symbols: Vec<&str> = definition
            .quantities
            .iter()
            .map(|q| q.symbol.as_str())
            .collect();
        let targets: Vec<Option<String>> = definition
            .results
            .iter()
            .map(|r| {
                r.compare_with
                    .is_empty()
                    .then(|| legacy_compare_with(&r.symbol, &symbols).map(str::to_string))
                    .flatten()
            })
            .collect();
        if targets.iter().all(Option::is_none) {
            continue;
        }
        for (result, target) in definition.results.iter_mut().zip(targets) {
            if let Some(target) = target {
                result.compare_with = target;
            }
        }
        sqlx::query(
            "UPDATE practice_versions \
             SET definition_json = json_set(definition_json, '$.results', json(?1)) \
             WHERE id = ?2",
        )
        .bind(serde_json::to_string(&definition.results)?)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_COMPARE_WITH)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{legacy_compare_with, legacy_display_symbol, legacy_quantity_presentation};

    #[test]
    fn legacy_compare_with_pairs_x_t_with_quantity_x() {
        let quantities = ["VR1_s", "VR2_s", "R1"];
        assert_eq!(legacy_compare_with("VR1_s_t", &quantities), Some("VR1_s"));
        assert_eq!(legacy_compare_with("P_max_t", &quantities), None);
        assert_eq!(legacy_compare_with("VR1_s", &quantities), None);
        assert_eq!(legacy_compare_with("R1_teo", &quantities), None);
    }

    #[test]
    fn legacy_quantity_presentation_matches_the_old_frontend_rules() {
        let series = Some("regresion_lineal");
        let single = Some("estadistico");
        let cases = [
            (
                "T1",
                Some("tiempo"),
                true,
                false,
                single,
                ("nombre", "cronometro"),
            ),
            (
                "t_med",
                Some("tiempo"),
                false,
                true,
                single,
                ("nombre", "teclado"),
            ),
            (
                "t_x",
                Some("tiempo"),
                false,
                false,
                single,
                ("nombre", "cronometro"),
            ),
            (
                "t_y",
                Some("tiempo"),
                false,
                false,
                None,
                ("nombre", "cronometro"),
            ),
            (
                "t",
                Some("tiempo"),
                true,
                false,
                series,
                ("nombre", "cronometro"),
            ),
            (
                "t",
                Some("tiempo"),
                false,
                false,
                series,
                ("nombre", "teclado"),
            ),
            (
                "t",
                Some("tiempo"),
                false,
                false,
                Some("curva"),
                ("nombre", "teclado"),
            ),
            (
                "T_oc",
                Some("tiempo"),
                false,
                false,
                series,
                ("nombre_simbolo", "teclado"),
            ),
            (
                "tmedio",
                Some("tiempo"),
                false,
                false,
                series,
                ("nombre", "teclado"),
            ),
            (
                "VRpp",
                Some("voltaje"),
                true,
                false,
                Some("curva"),
                ("simbolo", "teclado"),
            ),
            (
                "Vg_s",
                Some("voltaje"),
                true,
                false,
                single,
                ("simbolo", "teclado"),
            ),
            (
                "R_s",
                Some("resistencia"),
                true,
                false,
                single,
                ("nombre", "teclado"),
            ),
            ("x", None, true, false, single, ("nombre", "teclado")),
        ];
        for (symbol, quantity, repeated, is_given, kind, expected) in cases {
            assert_eq!(
                legacy_quantity_presentation(symbol, "Tiempo", quantity, repeated, is_given, kind),
                expected,
                "{symbol}"
            );
        }
        for name in ["T_oc medido", "Período Toc"] {
            assert_eq!(
                legacy_quantity_presentation("T_oc", name, Some("tiempo"), false, false, series),
                ("nombre", "teclado"),
                "{name}"
            );
        }
    }

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
