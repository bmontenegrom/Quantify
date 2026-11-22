use super::crud::{
    insert_aggregate, insert_curve, insert_intermediate, insert_point_result, insert_quantity,
    insert_result,
};
use super::{
    AggregateInput, CurveInput, IntermediateInput, PointResultInput, PracticeAggregate,
    PracticeCurve, PracticeDefinition, PracticeIntermediate, PracticePart, PracticePointResult,
    PracticeSection, QuantityInput, ResultInput,
};
use crate::db::{Practice, PracticeQuantity, PracticeResult};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashSet;
use uuid::Uuid;

pub const TEMPLATE_VERSION: u32 = 1;
pub const ANALYSIS_KINDS: [&str; 3] = ["estadistico", "regresion_lineal", "curva"];
pub const MAX_OPERATORS: i64 = 20;

#[derive(Debug, Serialize, Deserialize)]
pub struct PracticeTemplate {
    #[serde(default)]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub analysis_kind: Option<String>,
    #[serde(default)]
    pub x_formula: Option<String>,
    #[serde(default)]
    pub y_formula: Option<String>,
    #[serde(default)]
    pub operator_count: Option<i64>,
    #[serde(default)]
    pub parts: Vec<TemplatePart>,
    #[serde(default)]
    pub sections: Vec<TemplateSection>,
    #[serde(default)]
    pub quantities: Vec<QuantityInput>,
    #[serde(default)]
    pub results: Vec<ResultInput>,
    #[serde(default)]
    pub intermediates: Vec<IntermediateInput>,
    #[serde(default)]
    pub point_results: Vec<PointResultInput>,
    #[serde(default)]
    pub aggregates: Vec<AggregateInput>,
    #[serde(default)]
    pub curves: Vec<CurveInput>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplatePart {
    pub title: String,
    pub analysis_kind: Option<String>,
    pub help_text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateSection {
    pub title: String,
    pub part: Option<usize>,
    pub help_text: String,
    pub quantities: Vec<String>,
    pub results: Vec<String>,
}

pub async fn export_template(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<PracticeTemplate>> {
    let conn = &mut *pool.acquire().await?;
    let Some(def) = super::definition_in(conn, practice_id).await? else {
        return Ok(None);
    };
    let name: String = sqlx::query_scalar("SELECT name FROM practices WHERE id = ?1")
        .bind(practice_id)
        .fetch_one(&mut *conn)
        .await?;
    let display = |symbol: &str| def.display_symbols.get(symbol).cloned();
    Ok(Some(PracticeTemplate {
        schema_version: TEMPLATE_VERSION,
        name,
        description: def.description.clone(),
        analysis_kind: def.analysis_kind.clone(),
        x_formula: def.x_formula.clone(),
        y_formula: def.y_formula.clone(),
        operator_count: def.operator_count,
        parts: def
            .parts
            .iter()
            .map(|p| TemplatePart {
                title: p.title.clone(),
                analysis_kind: p.analysis_kind.clone(),
                help_text: p.help_text.clone(),
            })
            .collect(),
        sections: def
            .sections
            .iter()
            .map(|s| TemplateSection {
                title: s.title.clone(),
                part: def
                    .parts
                    .iter()
                    .position(|p| Some(&p.id) == s.part_id.as_ref()),
                help_text: s.help_text.clone(),
                quantities: s.quantities.clone(),
                results: s.results.clone(),
            })
            .collect(),
        quantities: def
            .quantities
            .iter()
            .map(|q| QuantityInput {
                display_symbol: display(&q.symbol),
                symbol: q.symbol.clone(),
                name: q.name.clone(),
                unit: q.unit.clone(),
                repeated: q.repeated,
                quantity: q.quantity.clone(),
                is_given: q.is_given,
                replicas_per_point: q.replicas_per_point,
                per_point: q.per_point,
                has_uncertainty: q.has_uncertainty,
                optional: q.optional,
                default_value: Some(q.default_value),
                slot_count: q.slot_count,
                label_mode: Some(q.label_mode.clone()),
                input_widget: Some(q.input_widget.clone()),
            })
            .collect(),
        results: def
            .results
            .iter()
            .map(|r| ResultInput {
                display_symbol: display(&r.symbol),
                symbol: r.symbol.clone(),
                name: r.name.clone(),
                unit: r.unit.clone(),
                formula: r.formula.clone(),
                tolerance: Some(r.tolerance),
                is_final: r.is_final,
                has_uncertainty: r.has_uncertainty,
                compare_with: (!r.compare_with.is_empty()).then(|| r.compare_with.clone()),
            })
            .collect(),
        intermediates: def
            .intermediates
            .iter()
            .map(|i| IntermediateInput {
                display_symbol: display(&i.symbol),
                symbol: i.symbol.clone(),
                name: i.name.clone(),
                unit: i.unit.clone(),
                formula: i.formula.clone(),
                show_live: i.show_live,
                on_means: i.on_means,
            })
            .collect(),
        point_results: def
            .point_results
            .iter()
            .map(|p| PointResultInput {
                display_symbol: display(&p.symbol),
                symbol: p.symbol.clone(),
                name: p.name.clone(),
                unit: p.unit.clone(),
                formula: p.formula.clone(),
            })
            .collect(),
        aggregates: def
            .aggregates
            .iter()
            .map(|a| AggregateInput {
                display_symbol: display(&a.symbol),
                symbol: a.symbol.clone(),
                name: a.name.clone(),
                unit: a.unit.clone(),
                formula: a.formula.clone(),
                is_final: a.is_final,
            })
            .collect(),
        curves: def
            .curves
            .iter()
            .map(|c| CurveInput {
                x_formula: c.x_formula.clone(),
                y_formula: c.y_formula.clone(),
                x_log: c.x_log,
            })
            .collect(),
    }))
}

/// Lo que impide escribir una plantilla; el resto lo muestra el panel de problemas del borrador.
///
/// ```
/// use quantify::practices::{check_template, PracticeTemplate};
/// let t: PracticeTemplate = serde_json::from_value(serde_json::json!({
///     "schema_version": 1, "name": "P", "analysis_kind": "estadistico", "quantities": [
///         {"symbol": "x", "name": "x", "unit": "", "repeated": false},
///         {"symbol": "x", "name": "otra", "unit": "", "repeated": false}
///     ]
/// })).unwrap();
/// assert!(check_template(&t).unwrap_err().contains("\"x\""));
/// ```
pub fn check_template(t: &PracticeTemplate) -> Result<(), String> {
    if t.schema_version != TEMPLATE_VERSION {
        return Err(format!(
            "El archivo usa la versión {} del formato y esta versión de Quantify lee la {}.",
            t.schema_version, TEMPLATE_VERSION
        ));
    }
    if t.name.trim().is_empty() {
        return Err("La práctica necesita un nombre.".into());
    }
    if t.analysis_kind.is_none() {
        return Err("La práctica necesita un tipo de análisis.".into());
    }
    let kinds = std::iter::once(&t.analysis_kind).chain(t.parts.iter().map(|p| &p.analysis_kind));
    if let Some(kind) = kinds
        .flatten()
        .find(|k| !ANALYSIS_KINDS.contains(&k.trim()))
    {
        return Err(format!(
            "El tipo de análisis \"{kind}\" no existe: tiene que ser estadistico, \
             regresion_lineal o curva."
        ));
    }
    if t.operator_count.is_some_and(|n| n > MAX_OPERATORS) {
        return Err(format!(
            "La cantidad de operadores no puede superar {MAX_OPERATORS}."
        ));
    }
    let titles = t
        .parts
        .iter()
        .map(|p| &p.title)
        .chain(t.sections.iter().map(|s| &s.title));
    if titles.into_iter().any(|title| title.trim().is_empty()) {
        return Err("Hay una parte o una sección sin título.".into());
    }
    if let Some(r) = t
        .results
        .iter()
        .find(|r| r.tolerance.flatten().is_some_and(|v| v < 0.0))
    {
        return Err(format!(
            "\"{}\" tiene una tolerancia negativa.",
            r.symbol.trim()
        ));
    }
    let formulas = t
        .results
        .iter()
        .map(|r| (&r.symbol, &r.formula))
        .chain(t.intermediates.iter().map(|i| (&i.symbol, &i.formula)))
        .chain(t.point_results.iter().map(|p| (&p.symbol, &p.formula)))
        .chain(t.aggregates.iter().map(|a| (&a.symbol, &a.formula)));
    let symbols = t
        .quantities
        .iter()
        .map(|q| &q.symbol)
        .chain(formulas.clone().map(|(s, _)| s));
    let mut seen = HashSet::new();
    for symbol in symbols {
        let symbol = symbol.trim();
        if symbol.is_empty() {
            return Err("Hay una magnitud o un mensurando sin símbolo.".into());
        }
        if !seen.insert(symbol) {
            return Err(format!("El símbolo \"{symbol}\" aparece más de una vez."));
        }
    }
    if let Some((symbol, _)) = formulas.clone().find(|(_, f)| f.trim().is_empty()) {
        return Err(format!("\"{}\" no tiene fórmula.", symbol.trim()));
    }
    if t.curves
        .iter()
        .any(|c| c.x_formula.trim().is_empty() || c.y_formula.trim().is_empty())
    {
        return Err("Hay una curva sin las fórmulas de ambos ejes.".into());
    }
    let quantities: HashSet<&str> = t.quantities.iter().map(|q| q.symbol.trim()).collect();
    let results: HashSet<&str> = t.results.iter().map(|r| r.symbol.trim()).collect();
    let mut placed = HashSet::new();
    for section in &t.sections {
        if section.part.is_some_and(|p| p >= t.parts.len()) {
            return Err(format!(
                "La sección \"{}\" apunta a una parte que no existe.",
                section.title
            ));
        }
        let members = section
            .quantities
            .iter()
            .map(|s| (s, quantities.contains(s.trim())))
            .chain(
                section
                    .results
                    .iter()
                    .map(|s| (s, results.contains(s.trim()))),
            );
        for (symbol, exists) in members {
            if !exists {
                return Err(format!(
                    "La sección \"{}\" incluye \"{symbol}\", que no está en la práctica.",
                    section.title
                ));
            }
            if !placed.insert(symbol.trim()) {
                return Err(format!("\"{symbol}\" está en más de una sección."));
            }
        }
    }
    Ok(())
}

pub fn template_definition(t: &PracticeTemplate) -> PracticeDefinition {
    let kind = |k: &Option<String>| k.as_deref().map(|k| k.trim().to_string());
    let display_symbols = t
        .quantities
        .iter()
        .map(|q| (&q.symbol, &q.display_symbol))
        .chain(t.results.iter().map(|r| (&r.symbol, &r.display_symbol)))
        .chain(
            t.intermediates
                .iter()
                .map(|i| (&i.symbol, &i.display_symbol)),
        )
        .chain(
            t.point_results
                .iter()
                .map(|p| (&p.symbol, &p.display_symbol)),
        )
        .chain(t.aggregates.iter().map(|a| (&a.symbol, &a.display_symbol)))
        .filter_map(|(symbol, display)| {
            let display = display.as_deref()?.trim();
            (!display.is_empty()).then(|| (symbol.trim().to_string(), display.to_string()))
        })
        .collect();
    PracticeDefinition {
        practice_id: String::new(),
        description: t.description.trim().to_string(),
        status: "borrador".into(),
        analysis_kind: kind(&t.analysis_kind),
        x_formula: t.x_formula.clone(),
        y_formula: t.y_formula.clone(),
        operator_count: t.operator_count.filter(|n| *n > 1),
        parts: t
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| PracticePart {
                id: format!("parte-{i}"),
                title: p.title.trim().to_string(),
                position: i as i64,
                analysis_kind: kind(&p.analysis_kind),
                help_text: p.help_text.clone(),
                ..Default::default()
            })
            .collect(),
        sections: t
            .sections
            .iter()
            .enumerate()
            .map(|(i, s)| PracticeSection {
                id: format!("seccion-{i}"),
                part_id: s.part.map(|p| format!("parte-{p}")),
                title: s.title.trim().to_string(),
                position: i as i64,
                help_text: s.help_text.clone(),
                quantities: s.quantities.iter().map(|q| q.trim().to_string()).collect(),
                results: s.results.iter().map(|r| r.trim().to_string()).collect(),
                ..Default::default()
            })
            .collect(),
        quantities: t
            .quantities
            .iter()
            .enumerate()
            .map(|(i, q)| PracticeQuantity {
                id: format!("magnitud-{i}"),
                symbol: q.symbol.trim().to_string(),
                name: q.name.trim().to_string(),
                unit: q.unit.trim().to_string(),
                repeated: q.repeated,
                quantity: q.quantity.clone(),
                position: i as i64,
                is_given: q.is_given,
                replicas_per_point: q.replicas_per_point,
                per_point: q.per_point,
                has_uncertainty: q.has_uncertainty,
                optional: q.optional,
                default_value: q.default_value.flatten(),
                slot_count: q.slot_count,
                label_mode: q
                    .label_mode
                    .clone()
                    .unwrap_or_else(|| super::DEFAULT_LABEL_MODE.into()),
                input_widget: q
                    .input_widget
                    .clone()
                    .unwrap_or_else(|| super::DEFAULT_INPUT_WIDGET.into()),
                ..Default::default()
            })
            .collect(),
        results: t
            .results
            .iter()
            .enumerate()
            .map(|(i, r)| PracticeResult {
                id: format!("mensurando-{i}"),
                symbol: r.symbol.trim().to_string(),
                name: r.name.trim().to_string(),
                unit: r.unit.trim().to_string(),
                formula: r.formula.trim().to_string(),
                position: i as i64,
                tolerance: r.tolerance.flatten(),
                is_final: r.is_final,
                has_uncertainty: r.has_uncertainty,
                compare_with: r.compare_with().unwrap_or_default().to_string(),
                ..Default::default()
            })
            .collect(),
        intermediates: t
            .intermediates
            .iter()
            .enumerate()
            .map(|(i, it)| PracticeIntermediate {
                id: format!("intermedia-{i}"),
                position: i as i64,
                symbol: it.symbol.trim().to_string(),
                name: it.name.trim().to_string(),
                unit: it.unit.trim().to_string(),
                formula: it.formula.trim().to_string(),
                show_live: it.show_live,
                on_means: it.on_means,
                ..Default::default()
            })
            .collect(),
        point_results: t
            .point_results
            .iter()
            .enumerate()
            .map(|(i, p)| PracticePointResult {
                id: format!("derivada-{i}"),
                position: i as i64,
                symbol: p.symbol.trim().to_string(),
                name: p.name.trim().to_string(),
                unit: p.unit.trim().to_string(),
                formula: p.formula.trim().to_string(),
                ..Default::default()
            })
            .collect(),
        aggregates: t
            .aggregates
            .iter()
            .enumerate()
            .map(|(i, a)| PracticeAggregate {
                id: format!("agregado-{i}"),
                position: i as i64,
                symbol: a.symbol.trim().to_string(),
                name: a.name.trim().to_string(),
                unit: a.unit.trim().to_string(),
                formula: a.formula.trim().to_string(),
                is_final: a.is_final,
                ..Default::default()
            })
            .collect(),
        curves: t
            .curves
            .iter()
            .enumerate()
            .map(|(i, c)| PracticeCurve {
                id: format!("curva-{i}"),
                position: i as i64,
                x_formula: c.x_formula.trim().to_string(),
                y_formula: c.y_formula.trim().to_string(),
                x_log: c.x_log,
                ..Default::default()
            })
            .collect(),
        display_symbols,
        current_version: None,
    }
    .with_effective_kinds()
}

pub async fn import_template(
    pool: &SqlitePool,
    t: &PracticeTemplate,
    created_by: &str,
) -> anyhow::Result<Practice> {
    check_template(t).map_err(anyhow::Error::msg)?;
    let mut tx = crate::db::begin_write(pool).await?;
    let id = super::lifecycle::free_practice_id(&mut tx, &super::slugify(&t.name)).await?;
    let formula = |f: &Option<String>| {
        f.as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_string)
    };
    sqlx::query(
        "INSERT INTO practices (id, name, description, analysis_kind, x_formula, y_formula, \
         operator_count, status, created_by, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'borrador', ?8, ?9)",
    )
    .bind(&id)
    .bind(t.name.trim())
    .bind(t.description.trim())
    .bind(t.analysis_kind.as_deref().map(str::trim))
    .bind(formula(&t.x_formula))
    .bind(formula(&t.y_formula))
    .bind(t.operator_count.filter(|n| *n > 1))
    .bind(created_by)
    .bind(chrono::Utc::now())
    .execute(&mut *tx)
    .await?;
    let mut parts = Vec::new();
    for (position, part) in t.parts.iter().enumerate() {
        let part_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO practice_parts (id, practice_id, title, position, analysis_kind, help_text) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&part_id)
        .bind(&id)
        .bind(part.title.trim())
        .bind(position as i64)
        .bind(part.analysis_kind.as_deref().map(str::trim))
        .bind(part.help_text.trim())
        .execute(&mut *tx)
        .await?;
        parts.push(part_id);
    }
    for (position, q) in t.quantities.iter().enumerate() {
        insert_quantity(&mut tx, &id, position as i64, q).await?;
    }
    for (position, r) in t.results.iter().enumerate() {
        insert_result(&mut tx, &id, position as i64, r).await?;
    }
    for (position, i) in t.intermediates.iter().enumerate() {
        insert_intermediate(&mut tx, &id, position as i64, i).await?;
    }
    for (position, p) in t.point_results.iter().enumerate() {
        insert_point_result(&mut tx, &id, position as i64, p).await?;
    }
    for (position, a) in t.aggregates.iter().enumerate() {
        insert_aggregate(&mut tx, &id, position as i64, a).await?;
    }
    for (position, c) in t.curves.iter().enumerate() {
        insert_curve(&mut tx, &id, position as i64, c).await?;
    }
    for (position, section) in t.sections.iter().enumerate() {
        let section_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO practice_sections (id, practice_id, part_id, title, position, help_text) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&section_id)
        .bind(&id)
        .bind(section.part.map(|p| &parts[p]))
        .bind(section.title.trim())
        .bind(position as i64)
        .bind(section.help_text.trim())
        .execute(&mut *tx)
        .await?;
        for (table, symbols) in [
            ("practice_quantities", &section.quantities),
            ("practice_results", &section.results),
        ] {
            for symbol in symbols {
                sqlx::query(&format!(
                    "UPDATE {table} SET section_id = ?1 WHERE practice_id = ?2 AND symbol = ?3"
                ))
                .bind(&section_id)
                .bind(&id)
                .bind(symbol.trim())
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    super::practice(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la práctica recién importada no aparece"))
}
