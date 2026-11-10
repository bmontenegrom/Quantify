use super::PracticeDefinition;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

const COSMETIC_KEYS: [&str; 7] = [
    "name",
    "title",
    "tolerance",
    "display_symbols",
    "label_mode",
    "input_widget",
    "compare_with",
];
const VOLATILE_KEYS: [&str; 3] = ["status", "current_version", "position"];
const FREELY_ORDERED: [&str; 4] = ["quantities", "results", "parts", "sections"];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DraftChangeKind {
    None,
    Cosmetic,
    Structural,
}

#[derive(Debug, PartialEq)]
pub struct DefinitionComparison {
    pub kind: DraftChangeKind,
    pub changes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DraftChanges {
    pub version: i64,
    pub kind: DraftChangeKind,
    pub changes: Vec<String>,
    pub fingerprint: String,
    pub submissions: i64,
    pub version_submissions: i64,
    pub last_update: Option<VersionUpdate>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VersionUpdate {
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<String>,
}

struct Item {
    id: String,
    label: String,
    formula: Option<String>,
}

fn strip(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(object) => {
            for key in keys {
                object.remove(*key);
            }
            object
                .iter_mut()
                .filter(|(key, _)| key.as_str() != "display_symbols")
                .for_each(|(_, v)| strip(v, keys));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| strip(v, keys)),
        _ => {}
    }
}

fn shape<T: Serialize>(value: &T, keys: &[&str]) -> anyhow::Result<Value> {
    let mut value = serde_json::to_value(value)?;
    strip(&mut value, keys);
    Ok(value)
}

fn sort_strings(value: Option<&mut Value>) {
    if let Some(Value::Array(items)) = value {
        items.sort_by_key(|item| item.to_string());
    }
}

fn ignoring_order(mut definition: Value) -> Value {
    for key in FREELY_ORDERED {
        if let Some(Value::Array(items)) = definition.get_mut(key) {
            items.sort_by_key(|item| item["id"].to_string());
            if key == "sections" {
                for section in items.iter_mut() {
                    sort_strings(section.get_mut("quantities"));
                    sort_strings(section.get_mut("results"));
                }
            }
        }
    }
    definition
}

fn structure(definition: &PracticeDefinition) -> anyhow::Result<Value> {
    let keys = [VOLATILE_KEYS.as_slice(), COSMETIC_KEYS.as_slice()].concat();
    Ok(ignoring_order(shape(definition, &keys)?))
}

fn section_members(definition: &PracticeDefinition) -> BTreeMap<&str, (Option<&str>, Vec<&str>)> {
    definition
        .sections
        .iter()
        .map(|section| {
            let quantities = section.quantities.iter().filter_map(|symbol| {
                let found = definition.quantities.iter().find(|q| &q.symbol == symbol);
                found.map(|q| q.id.as_str())
            });
            let results = section.results.iter().filter_map(|symbol| {
                let found = definition.results.iter().find(|r| &r.symbol == symbol);
                found.map(|r| r.id.as_str())
            });
            let mut members: Vec<&str> = quantities.chain(results).collect();
            members.sort_unstable();
            (section.id.as_str(), (section.part_id.as_deref(), members))
        })
        .collect()
}

pub fn draft_fingerprint(draft: &PracticeDefinition) -> anyhow::Result<String> {
    let mut hasher = DefaultHasher::new();
    shape(draft, &["status", "current_version"])?
        .to_string()
        .hash(&mut hasher);
    Ok(format!("{:x}", hasher.finish()))
}

fn diff_items<T: Serialize>(
    what: &str,
    order: Option<&str>,
    before: &[T],
    after: &[T],
    key: impl Fn(&T) -> Item,
    changes: &mut Vec<String>,
) -> anyhow::Result<()> {
    let ignored = [COSMETIC_KEYS.as_slice(), &["position"]].concat();
    let old: Vec<(Item, &T)> = before.iter().map(|x| (key(x), x)).collect();
    let new: Vec<Item> = after.iter().map(&key).collect();
    for (item, value) in new.iter().zip(after) {
        match old.iter().find(|(o, _)| o.id == item.id) {
            None => changes.push(format!("Se agrega {what} {}", item.label)),
            Some((o, _)) if o.formula != item.formula => {
                changes.push(format!("Cambia la fórmula de {}", item.label));
            }
            Some((_, previous)) if shape(*previous, &ignored)? != shape(value, &ignored)? => {
                changes.push(format!("Cambia {what} {}", item.label));
            }
            Some(_) => {}
        }
    }
    for (item, _) in &old {
        if !new.iter().any(|n| n.id == item.id) {
            changes.push(format!("Se quita {what} {}", item.label));
        }
    }
    if let Some(list) = order {
        let kept_old = old.iter().map(|(o, _)| o.id.as_str());
        let kept_old: Vec<&str> = kept_old
            .filter(|id| new.iter().any(|n| n.id == *id))
            .collect();
        let kept_new = new.iter().map(|n| n.id.as_str());
        let kept_new: Vec<&str> = kept_new.filter(|id| kept_old.contains(id)).collect();
        if kept_old != kept_new {
            changes.push(format!("Cambia el orden de {list}"));
        }
    }
    Ok(())
}

fn with_formula(id: &str, symbol: &str, formula: &str) -> Item {
    Item {
        id: id.to_string(),
        label: symbol.to_string(),
        formula: Some(formula.to_string()),
    }
}

fn list_changes(
    published: &PracticeDefinition,
    draft: &PracticeDefinition,
) -> anyhow::Result<Vec<String>> {
    let mut changes = Vec::new();
    if published.analysis_kind != draft.analysis_kind {
        changes.push("Cambia el tipo de análisis".to_string());
    }
    if (&published.x_formula, &published.y_formula) != (&draft.x_formula, &draft.y_formula) {
        changes.push("Cambian los ejes de la regresión".to_string());
    }
    if published.operator_count != draft.operator_count {
        changes.push("Cambia la cantidad de operadores".to_string());
    }
    diff_items(
        "la parte",
        None,
        &published.parts,
        &draft.parts,
        |p| Item {
            id: p.id.clone(),
            label: p.title.clone(),
            formula: None,
        },
        &mut changes,
    )?;
    diff_items(
        "la magnitud",
        None,
        &published.quantities,
        &draft.quantities,
        |q| Item {
            id: q.id.clone(),
            label: q.symbol.clone(),
            formula: None,
        },
        &mut changes,
    )?;
    diff_items(
        "el mensurando",
        None,
        &published.results,
        &draft.results,
        |r| with_formula(&r.id, &r.symbol, &r.formula),
        &mut changes,
    )?;
    diff_items(
        "la intermedia",
        Some("las intermedias"),
        &published.intermediates,
        &draft.intermediates,
        |i| with_formula(&i.id, &i.symbol, &i.formula),
        &mut changes,
    )?;
    diff_items(
        "el resultado por punto",
        Some("los resultados por punto"),
        &published.point_results,
        &draft.point_results,
        |p| with_formula(&p.id, &p.symbol, &p.formula),
        &mut changes,
    )?;
    diff_items(
        "el agregado",
        Some("los agregados"),
        &published.aggregates,
        &draft.aggregates,
        |a| with_formula(&a.id, &a.symbol, &a.formula),
        &mut changes,
    )?;
    diff_items(
        "la curva",
        Some("las curvas"),
        &published.curves,
        &draft.curves,
        |c| Item {
            id: c.id.clone(),
            label: format!("{} vs {}", c.y_formula, c.x_formula),
            formula: None,
        },
        &mut changes,
    )?;
    if section_members(published) != section_members(draft) {
        changes.push("Cambia la organización en secciones".to_string());
    }
    Ok(changes)
}

fn tolerance_changes(published: &PracticeDefinition, draft: &PracticeDefinition) -> Vec<String> {
    draft
        .results
        .iter()
        .filter(|result| {
            let before = published.results.iter().find(|r| r.id == result.id);
            before.is_some_and(|r| r.tolerance != result.tolerance)
        })
        .map(|result| format!("Cambia la tolerancia de {}", result.symbol))
        .collect()
}

fn display_changes(published: &PracticeDefinition, draft: &PracticeDefinition) -> Vec<String> {
    let before: std::collections::BTreeSet<&str> =
        super::display::version_symbols(published).collect();
    let after: std::collections::BTreeSet<&str> = super::display::version_symbols(draft).collect();
    let symbols = before
        .intersection(&after)
        .filter(|symbol| {
            published.display_symbols.get(**symbol) != draft.display_symbols.get(**symbol)
        })
        .map(|symbol| format!("Cambia cómo se muestra {symbol}"));
    let quantities = draft.quantities.iter().flat_map(|new| {
        let old = published.quantities.iter().find(|q| q.id == new.id);
        let label = old
            .filter(|q| q.label_mode != new.label_mode)
            .map(|_| format!("Cambia el rótulo de {}", new.symbol));
        let widget = old
            .filter(|q| q.input_widget != new.input_widget)
            .map(|_| format!("Cambia la entrada de {}", new.symbol));
        label.into_iter().chain(widget)
    });
    let comparisons = draft.results.iter().filter_map(|new| {
        let old = published.results.iter().find(|r| r.id == new.id)?;
        (old.compare_with != new.compare_with)
            .then(|| format!("Cambia con qué se compara {}", new.symbol))
    });
    symbols.chain(quantities).chain(comparisons).collect()
}

pub fn change_kind(
    published: &PracticeDefinition,
    draft: &PracticeDefinition,
) -> anyhow::Result<DraftChangeKind> {
    if structure(published)? != structure(draft)? {
        return Ok(DraftChangeKind::Structural);
    }
    Ok(
        if shape(published, &VOLATILE_KEYS)? == shape(draft, &VOLATILE_KEYS)? {
            DraftChangeKind::None
        } else {
            DraftChangeKind::Cosmetic
        },
    )
}

/// Compara una versión publicada con el borrador; nombres, títulos, tolerancias y orden son cosméticos.
///
/// ```
/// use quantify::db::PracticeResult;
/// use quantify::practices::{compare_definitions, DraftChangeKind, PracticeDefinition};
///
/// let result = PracticeResult {
///     id: "r".into(), practice_id: "p".into(), symbol: "g".into(), name: "g".into(),
///     unit: "m/s2".into(), formula: "2*L".into(), position: 0, tolerance: None,
///     is_final: true, has_uncertainty: true,
///     ..Default::default()
/// };
/// let published = PracticeDefinition { results: vec![result.clone()], ..Default::default() };
/// let kind = |draft: &PracticeDefinition| compare_definitions(&published, draft).unwrap().kind;
/// assert_eq!(kind(&published), DraftChangeKind::None);
///
/// let renamed = PracticeResult {
///     name: "Gravedad".into(), tolerance: Some(1.0), compare_with: "L".into(), ..result.clone()
/// };
/// let draft = PracticeDefinition { results: vec![renamed], ..Default::default() };
/// assert_eq!(kind(&draft), DraftChangeKind::Cosmetic);
///
/// let refactored = PracticeResult { formula: "3*L".into(), ..result };
/// let draft = PracticeDefinition { results: vec![refactored], ..Default::default() };
/// let comparison = compare_definitions(&published, &draft).unwrap();
/// assert_eq!(comparison.kind, DraftChangeKind::Structural);
/// assert_eq!(comparison.changes, ["Cambia la fórmula de g"]);
/// ```
pub fn compare_definitions(
    published: &PracticeDefinition,
    draft: &PracticeDefinition,
) -> anyhow::Result<DefinitionComparison> {
    let kind = change_kind(published, draft)?;
    let changes = match kind {
        DraftChangeKind::None => Vec::new(),
        DraftChangeKind::Cosmetic => [
            tolerance_changes(published, draft),
            display_changes(published, draft),
        ]
        .concat(),
        DraftChangeKind::Structural => [
            list_changes(published, draft)?,
            display_changes(published, draft),
        ]
        .concat(),
    };
    Ok(DefinitionComparison { kind, changes })
}

pub async fn draft_changes(
    pool: &SqlitePool,
    practice_id: &str,
) -> anyhow::Result<Option<DraftChanges>> {
    let Some(current) = super::current_version(pool, practice_id).await? else {
        return Ok(None);
    };
    let Some(draft) = super::definition(pool, practice_id).await? else {
        return Ok(None);
    };
    let comparison = compare_definitions(&current.definition, &draft)?;
    let (submissions,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE practice_id = ?1")
            .bind(practice_id)
            .fetch_one(pool)
            .await?;
    let (version_submissions,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE practice_version_id = ?1")
            .bind(&current.id)
            .fetch_one(pool)
            .await?;
    let last_update: Option<VersionUpdate> = sqlx::query_as(
        "SELECT v.updated_at, u.display_name AS updated_by \
         FROM practice_version_updates v LEFT JOIN users u ON u.id = v.updated_by \
         WHERE v.version_id = ?1 ORDER BY v.updated_at DESC, v.rowid DESC LIMIT 1",
    )
    .bind(&current.id)
    .fetch_optional(pool)
    .await?;
    Ok(Some(DraftChanges {
        version: current.number,
        kind: comparison.kind,
        changes: comparison.changes,
        fingerprint: draft_fingerprint(&draft)?,
        submissions,
        version_submissions,
        last_update,
    }))
}
