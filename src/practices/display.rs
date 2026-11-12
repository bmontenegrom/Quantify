use super::{PracticeDefinition, PracticeIntermediate};
use crate::db::DISPLAY_SYMBOL_TABLES;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

const LEGACY_DISPLAY_SYMBOLS: &str = "2026-11-simbolos-a-mostrar";
const LEGACY_QUANTITY_PRESENTATION: &str = "2026-11-rotulo-y-entrada";
const LEGACY_COMPARE_WITH: &str = "2026-11-comparar-con";
const LEGACY_LIVE_COLUMNS: &str = "2026-11-columnas-en-vivo";
const LEGACY_LIVE: [(&str, &str, &str); 2] = [("p2-cc", "P", "I*I*R"), ("fluidos-1", "Q", "V/t")];
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

fn legacy_compare_with(definition: &PracticeDefinition, result: &str) -> Option<String> {
    let r = definition.results.iter().find(|r| r.symbol == result)?;
    let base = result.strip_suffix("_t")?;
    let q = definition.quantities.iter().find(|q| q.symbol == base)?;
    let in_scope = definition
        .comparable_quantities(result)
        .iter()
        .any(|s| s == base);
    (in_scope && q.unit.trim() == r.unit.trim()).then(|| base.to_string())
}

pub(super) async fn migrate_legacy_compare_with(pool: &SqlitePool) -> anyhow::Result<()> {
    if crate::db::migration_applied(pool, LEGACY_COMPARE_WITH).await? {
        return Ok(());
    }
    let practices: Vec<(String,)> = sqlx::query_as("SELECT id FROM practices")
        .fetch_all(pool)
        .await?;
    let mut drafts = Vec::new();
    for (practice_id,) in practices {
        if let Some(definition) = super::crud::definition(pool, &practice_id).await? {
            drafts.push(definition);
        }
    }
    let mut tx = pool.begin().await?;
    for definition in &drafts {
        for result in definition
            .results
            .iter()
            .filter(|r| r.compare_with.is_empty())
        {
            let Some(target) = legacy_compare_with(definition, &result.symbol) else {
                continue;
            };
            sqlx::query("UPDATE practice_results SET compare_with = ?1 WHERE id = ?2")
                .bind(target)
                .bind(&result.id)
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
                    "versión ilegible: queda sin migrar a comparar con y esta migración no se repite"
                );
                continue;
            }
        };
        for (i, result) in definition.results.iter().enumerate() {
            if !result.compare_with.is_empty() {
                continue;
            }
            let Some(target) = legacy_compare_with(&definition, &result.symbol) else {
                continue;
            };
            sqlx::query(
                "UPDATE practice_versions \
                 SET definition_json = json_set(definition_json, ?1, ?2) WHERE id = ?3",
            )
            .bind(format!("$.results[{i}].compare_with"))
            .bind(target)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_COMPARE_WITH)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn legacy_live(practice_id: &str, intermediate: &PracticeIntermediate) -> bool {
    let formula: String = intermediate.formula.split_whitespace().collect();
    let velocity = practice_id == "viscosidad" && formula == "dx/t";
    velocity
        || LEGACY_LIVE.iter().any(|(practice, symbol, seeded)| {
            *practice == practice_id && intermediate.symbol == *symbol && formula == *seeded
        })
}

fn legacy_velocity(definition: &PracticeDefinition, id: &str) -> Option<PracticeIntermediate> {
    let has = |symbol: &str| definition.quantities.iter().any(|q| q.symbol == symbol);
    let compatible = definition
        .intermediates
        .iter()
        .any(|i| legacy_live(&definition.practice_id, i));
    if definition.practice_id != "viscosidad" || !has("dx") || !has("t") || compatible {
        return None;
    }
    let taken: Vec<&str> = version_symbols(definition).collect();
    let symbol = ["v".to_string(), "v_lim".to_string()]
        .into_iter()
        .chain((2..).map(|n| format!("v_lim{n}")))
        .find(|s| !taken.contains(&s.as_str()))?;
    Some(PracticeIntermediate {
        id: id.into(),
        practice_id: definition.practice_id.clone(),
        position: definition
            .intermediates
            .iter()
            .map(|i| i.position + 1)
            .max()
            .unwrap_or(0),
        symbol,
        name: "Velocidad limite".into(),
        unit: "m/s".into(),
        formula: "dx/t".into(),
        show_live: true,
        on_means: true,
    })
}

fn velocity_display(v: &PracticeIntermediate) -> &str {
    if v.symbol == "v" {
        ""
    } else {
        "v"
    }
}

pub(super) async fn migrate_legacy_live_columns(pool: &SqlitePool) -> anyhow::Result<()> {
    if crate::db::migration_applied(pool, LEGACY_LIVE_COLUMNS).await? {
        return Ok(());
    }
    let velocity_id = uuid::Uuid::new_v4().to_string();
    let mut drafts = Vec::new();
    for practice in ["p2-cc", "fluidos-1", "viscosidad"] {
        if let Some(definition) = super::crud::definition(pool, practice).await? {
            drafts.push(definition);
        }
    }
    let mut tx = pool.begin().await?;
    for definition in &drafts {
        for intermediate in definition
            .intermediates
            .iter()
            .filter(|i| !i.show_live && legacy_live(&definition.practice_id, i))
        {
            sqlx::query("UPDATE practice_intermediates SET show_live = 1 WHERE id = ?1")
                .bind(&intermediate.id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(v) = legacy_velocity(definition, &velocity_id) {
            sqlx::query(
                "INSERT INTO practice_intermediates \
                 (id, practice_id, position, symbol, name, unit, formula, display_symbol, \
                  show_live, on_means) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 1)",
            )
            .bind(&v.id)
            .bind(&v.practice_id)
            .bind(v.position)
            .bind(&v.symbol)
            .bind(&v.name)
            .bind(&v.unit)
            .bind(&v.formula)
            .bind(velocity_display(&v))
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
                    "versión ilegible: queda sin columnas en vivo y esta migración no se repite"
                );
                continue;
            }
        };
        for (i, intermediate) in definition.intermediates.iter().enumerate() {
            if intermediate.show_live || !legacy_live(&definition.practice_id, intermediate) {
                continue;
            }
            sqlx::query(
                "UPDATE practice_versions \
                 SET definition_json = json_set(definition_json, ?1, json('true')) WHERE id = ?2",
            )
            .bind(format!("$.intermediates[{i}].show_live"))
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(v) = legacy_velocity(&definition, &velocity_id) {
            sqlx::query(
                "UPDATE practice_versions \
                 SET definition_json = json_insert(definition_json, '$.intermediates[#]', json(?1)) \
                 WHERE id = ?2",
            )
            .bind(serde_json::to_string(&v)?)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            if !velocity_display(&v).is_empty() {
                sqlx::query(
                    "UPDATE practice_versions \
                     SET definition_json = json_set(definition_json, ?1, ?2) WHERE id = ?3",
                )
                .bind(format!("$.display_symbols.{}", v.symbol))
                .bind(velocity_display(&v))
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    sqlx::query("INSERT INTO applied_migrations (name, applied_at) VALUES (?1, ?2)")
        .bind(LEGACY_LIVE_COLUMNS)
        .bind(chrono::Utc::now())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        legacy_compare_with, legacy_display_symbol, legacy_quantity_presentation,
        PracticeDefinition,
    };

    #[test]
    fn legacy_compare_with_pairs_x_t_with_quantity_x_of_its_scope_and_unit() {
        let quantity = |symbol: &str, unit: &str| crate::db::PracticeQuantity {
            symbol: symbol.into(),
            unit: unit.into(),
            ..Default::default()
        };
        let result = |symbol: &str, unit: &str| crate::db::PracticeResult {
            symbol: symbol.into(),
            unit: unit.into(),
            ..Default::default()
        };
        let definition = PracticeDefinition {
            quantities: vec![quantity("V", "V"), quantity("T", "ms")],
            results: vec![
                result("V_t", "V"),
                result("T_t", "s"),
                result("P_t", "W"),
                result("V", "V"),
            ],
            ..Default::default()
        };
        let pair = |r: &str| legacy_compare_with(&definition, r);
        assert_eq!(pair("V_t").as_deref(), Some("V"));
        assert_eq!(pair("T_t"), None, "otra unidad");
        assert_eq!(pair("P_t"), None, "sin magnitud P");
        assert_eq!(pair("V"), None, "sin sufijo _t");
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
