use super::crud::{
    insert_aggregate, insert_curve, insert_intermediate, insert_point_result, insert_quantity,
    insert_result,
};
use super::{
    AggregateInput, CurveInput, Determinations, IntermediateInput, PointResultInput,
    PracticeDefinition, QuantityInput, ResultInput,
};
use crate::db::Practice;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

pub const TEMPLATE_VERSION: u32 = 1;
pub const ANALYSIS_KINDS: [&str; 3] = ["estadistico", "regresion_lineal", "curva"];
pub const MAX_DETERMINATIONS: i64 = 20;
const MAX_PART_KEY: usize = 40;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinations: Option<Determinations>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinations: Option<Determinations>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
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

pub struct SymbolRow<'a> {
    pub symbol: &'a str,
    pub display: &'a Option<String>,
    pub formula: Option<&'a str>,
}

impl PracticeTemplate {
    pub fn symbol_rows(&self) -> impl Iterator<Item = SymbolRow<'_>> {
        fn row<'a>(
            symbol: &'a str,
            display: &'a Option<String>,
            formula: Option<&'a str>,
        ) -> SymbolRow<'a> {
            SymbolRow {
                symbol,
                display,
                formula,
            }
        }
        self.quantities
            .iter()
            .map(|q| row(&q.symbol, &q.display_symbol, None))
            .chain(
                self.results
                    .iter()
                    .map(|r| row(&r.symbol, &r.display_symbol, Some(&r.formula))),
            )
            .chain(
                self.intermediates
                    .iter()
                    .map(|i| row(&i.symbol, &i.display_symbol, Some(&i.formula))),
            )
            .chain(
                self.point_results
                    .iter()
                    .map(|p| row(&p.symbol, &p.display_symbol, Some(&p.formula))),
            )
            .chain(
                self.aggregates
                    .iter()
                    .map(|a| row(&a.symbol, &a.display_symbol, Some(&a.formula))),
            )
    }
}

fn is_part_key(key: &str) -> bool {
    let edge = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    key.len() <= MAX_PART_KEY
        && key.starts_with(edge)
        && key.ends_with(edge)
        && key.chars().all(|c| edge(c) || c == '-')
}

fn part_key(part: &TemplatePart) -> Option<&str> {
    part.key.as_deref().map(str::trim).filter(|k| !k.is_empty())
}

pub fn axis(formula: Option<&str>) -> Option<String> {
    formula
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_string)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportSource {
    #[default]
    Borrador,
    Publicada,
}

pub async fn export_template(
    pool: &SqlitePool,
    practice_id: &str,
    source: ExportSource,
) -> anyhow::Result<Option<PracticeTemplate>> {
    let mut tx = crate::db::begin_read(pool).await?;
    let Some(practice) = super::practice_in(&mut tx, practice_id).await? else {
        return Ok(None);
    };
    let def = match source {
        ExportSource::Borrador => super::definition_in(&mut tx, practice_id).await?,
        ExportSource::Publicada => super::versions::current_version_in(&mut tx, practice_id)
            .await?
            .map(|v| v.definition),
    };
    Ok(def.map(|def| template_from(&def, practice.name)))
}

fn template_from(def: &PracticeDefinition, name: String) -> PracticeTemplate {
    let display = |symbol: &str| def.display_symbols.get(symbol).cloned();
    PracticeTemplate {
        schema_version: TEMPLATE_VERSION,
        name,
        description: def.description.clone(),
        analysis_kind: def.analysis_kind.clone(),
        x_formula: def.x_formula.clone(),
        y_formula: def.y_formula.clone(),
        determinations: def.determinations.clone(),
        parts: def
            .parts
            .iter()
            .map(|p| TemplatePart {
                key: p
                    .id
                    .strip_prefix(&format!("{}-", def.practice_id))
                    .filter(|key| is_part_key(key))
                    .map(str::to_string),
                title: p.title.clone(),
                analysis_kind: p.analysis_kind.clone(),
                help_text: p.help_text.clone(),
                determinations: p.determinations.clone(),
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
                per_determination: q.per_determination,
                fixed: q.fixed,
                default_u: q.default_u.map(Some),
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
    }
}

pub fn version_error(found: u64) -> String {
    format!("El archivo usa la versión {found} del formato y esta versión de Quantify lee la {TEMPLATE_VERSION}.")
}

fn stored_kinds(t: &PracticeTemplate) -> (Option<String>, Vec<Option<String>>) {
    let trimmed = |k: &Option<String>| k.as_deref().map(|k| k.trim().to_string());
    let has_part_kinds = t.parts.iter().any(|p| p.analysis_kind.is_some());
    let part_kinds: Vec<Option<String>> = t
        .parts
        .iter()
        .map(|p| match trimmed(&p.analysis_kind) {
            None if has_part_kinds => Some("estadistico".into()),
            kind => kind,
        })
        .collect();
    let kind = if has_part_kinds {
        part_kinds
            .iter()
            .flatten()
            .find(|k| super::is_point_based(Some(k)))
            .cloned()
            .or_else(|| Some("estadistico".into()))
    } else {
        trimmed(&t.analysis_kind)
    };
    (kind, part_kinds)
}

fn effective_part_kinds(t: &PracticeTemplate) -> Vec<String> {
    let (analysis_kind, part_kinds) = stored_kinds(t);
    PracticeDefinition {
        analysis_kind,
        parts: part_kinds
            .into_iter()
            .enumerate()
            .map(|(i, analysis_kind)| super::PracticePart {
                id: i.to_string(),
                analysis_kind,
                ..Default::default()
            })
            .collect(),
        quantities: t
            .quantities
            .iter()
            .map(|q| crate::db::PracticeQuantity {
                symbol: q.symbol.clone(),
                per_point: q.per_point,
                is_given: q.is_given,
                ..Default::default()
            })
            .collect(),
        sections: t
            .sections
            .iter()
            .map(|s| super::PracticeSection {
                part_id: s.part.map(|i| i.to_string()),
                quantities: s.quantities.clone(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
    .part_kinds()
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
        return Err(version_error(t.schema_version.into()));
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
    let series_parts = t
        .parts
        .iter()
        .filter(|p| super::is_point_based(p.analysis_kind.as_deref().map(str::trim)))
        .count();
    if series_parts > 1 {
        return Err("Solo una parte puede tener una serie de puntos (regresión o curva).".into());
    }
    let mut keys = HashSet::new();
    for key in t.parts.iter().filter_map(part_key) {
        if !is_part_key(key) || !keys.insert(key) {
            return Err(format!(
                "La clave de parte \"{key}\" tiene que ser única, de hasta {MAX_PART_KEY} caracteres, y usar solo minúsculas, dígitos y guiones."
            ));
        }
    }
    for d in std::iter::once(&t.determinations)
        .chain(t.parts.iter().map(|p| &p.determinations))
        .flatten()
    {
        d.validate()?;
    }
    if t.determinations.is_some() && !t.parts.is_empty() {
        return Err("Una práctica con partes configura las determinaciones en cada parte.".into());
    }
    let point_based = |kind: &Option<String>| super::is_point_based(kind.as_deref().map(str::trim));
    let kinds = effective_part_kinds(t);
    if (t.determinations.is_some() && point_based(&t.analysis_kind))
        || t.parts
            .iter()
            .zip(&kinds)
            .any(|(p, kind)| p.determinations.is_some() && kind != "estadistico")
    {
        return Err("Las determinaciones son para una práctica o parte estadística.".into());
    }
    for q in t.quantities.iter().filter(|q| q.per_determination) {
        let has_determinations = if t.parts.is_empty() {
            t.determinations.is_some()
        } else {
            t.sections
                .iter()
                .find(|s| s.quantities.contains(&q.symbol))
                .and_then(|s| s.part)
                .and_then(|i| t.parts.get(i))
                .is_some_and(|p| p.determinations.is_some())
        };
        if !has_determinations {
            return Err(format!(
                "La magnitud \"{}\" se repite por determinación, pero no está en una práctica o parte con determinaciones.",
                q.symbol
            ));
        }
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
    let mut seen = HashSet::new();
    for row in t.symbol_rows() {
        let symbol = row.symbol.trim();
        if symbol.is_empty() {
            return Err("Hay una magnitud o un mensurando sin símbolo.".into());
        }
        if !seen.insert(symbol) {
            return Err(format!("El símbolo \"{symbol}\" aparece más de una vez."));
        }
        if row.formula.is_some_and(|f| f.trim().is_empty()) {
            return Err(format!("\"{symbol}\" no tiene fórmula."));
        }
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

pub async fn import_template(
    pool: &SqlitePool,
    t: &PracticeTemplate,
    created_by: &str,
) -> anyhow::Result<Practice> {
    let mut tx = crate::db::begin_write(pool).await?;
    let id = super::lifecycle::free_practice_id(&mut tx, &super::slugify(&t.name)).await?;
    write_template(&mut tx, t, &id, "borrador", Some(created_by)).await?;
    tx.commit().await?;
    super::practice(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la práctica recién importada no aparece"))
}

async fn has_practices(conn: &mut SqliteConnection) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM practices)")
            .fetch_one(conn)
            .await?,
    )
}

async fn part_exists(conn: &mut SqliteConnection, id: &str) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM practice_parts WHERE id = ?1)")
            .bind(id)
            .fetch_one(conn)
            .await?,
    )
}

pub async fn seed_library(pool: &SqlitePool) -> anyhow::Result<bool> {
    let mut imported = false;
    if !has_practices(&mut *pool.acquire().await?).await? {
        let mut tx = crate::db::begin_write(pool).await?;
        if !has_practices(&mut tx).await? {
            for (key, _) in super::LIBRARY {
                let template = super::library_template(key)
                    .ok_or_else(|| anyhow::anyhow!("falta la plantilla {key}"))??;
                write_template(&mut tx, &template, key, "publicada", None).await?;
            }
            imported = true;
        }
        tx.commit().await?;
    }
    super::ensure_versions(pool).await?;
    Ok(imported)
}

#[cfg(test)]
pub(crate) async fn seed_fixture(pool: &SqlitePool, json: &str, id: &str) {
    let template: PracticeTemplate = serde_json::from_str(json).unwrap();
    let mut tx = crate::db::begin_write(pool).await.unwrap();
    write_template(&mut tx, &template, id, "publicada", None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    super::ensure_versions(pool).await.unwrap();
}

async fn write_template(
    tx: &mut SqliteConnection,
    t: &PracticeTemplate,
    id: &str,
    status: &str,
    created_by: Option<&str>,
) -> anyhow::Result<()> {
    check_template(t).map_err(anyhow::Error::msg)?;
    let (kind, part_kinds) = stored_kinds(t);
    sqlx::query(
        "INSERT INTO practices (id, name, description, analysis_kind, x_formula, y_formula, \
         status, created_by, created_at, config_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(id)
    .bind(t.name.trim())
    .bind(t.description.trim())
    .bind(&kind)
    .bind(axis(t.x_formula.as_deref()))
    .bind(axis(t.y_formula.as_deref()))
    .bind(status)
    .bind(created_by)
    .bind(chrono::Utc::now())
    .bind(
        super::PracticeConfig {
            determinations: t.determinations.clone(),
        }
        .to_json(),
    )
    .execute(&mut *tx)
    .await?;
    let mut parts = Vec::new();
    for (position, (part, part_kind)) in t.parts.iter().zip(&part_kinds).enumerate() {
        let keyed = part_key(part).map(|key| format!("{id}-{key}"));
        let part_id = match keyed {
            Some(candidate) if !part_exists(tx, &candidate).await? => candidate,
            _ => Uuid::new_v4().to_string(),
        };
        sqlx::query(
            "INSERT INTO practice_parts \
             (id, practice_id, title, position, analysis_kind, help_text, config_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&part_id)
        .bind(id)
        .bind(part.title.trim())
        .bind(position as i64)
        .bind(part_kind)
        .bind(part.help_text.trim())
        .bind(
            super::PracticeConfig {
                determinations: part.determinations.clone(),
            }
            .to_json(),
        )
        .execute(&mut *tx)
        .await?;
        parts.push(part_id);
    }
    for (position, q) in t.quantities.iter().enumerate() {
        insert_quantity(tx, id, position as i64, q).await?;
    }
    for (position, r) in t.results.iter().enumerate() {
        insert_result(tx, id, position as i64, r).await?;
    }
    for (position, i) in t.intermediates.iter().enumerate() {
        insert_intermediate(tx, id, position as i64, i).await?;
    }
    for (position, p) in t.point_results.iter().enumerate() {
        insert_point_result(tx, id, position as i64, p).await?;
    }
    for (position, a) in t.aggregates.iter().enumerate() {
        insert_aggregate(tx, id, position as i64, a).await?;
    }
    for (position, c) in t.curves.iter().enumerate() {
        insert_curve(tx, id, position as i64, c).await?;
    }
    for (position, section) in t.sections.iter().enumerate() {
        let section_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO practice_sections (id, practice_id, part_id, title, position, help_text) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&section_id)
        .bind(id)
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
                .bind(id)
                .bind(symbol.trim())
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    Ok(())
}
