use super::PracticeDefinition;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

const COSMETIC_KEYS: [&str; 3] = ["name", "title", "tolerance"];
const VOLATILE_KEYS: [&str; 3] = ["status", "current_version", "position"];
const FREELY_ORDERED: [&str; 4] = ["quantities", "results", "parts", "sections"];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
            object.values_mut().for_each(|v| strip(v, keys));
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

fn order_changed(before: &[&str], after: &[&str]) -> bool {
    let common = |list: &[&str], other: &[&str]| -> Vec<String> {
        list.iter()
            .filter(|id| other.contains(id))
            .map(|id| id.to_string())
            .collect()
    };
    common(before, after) != common(after, before)
}

fn diff_items<T: Serialize>(
    what: &str,
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
        &published.results,
        &draft.results,
        |r| with_formula(&r.id, &r.symbol, &r.formula),
        &mut changes,
    )?;
    diff_items(
        "la intermedia",
        &published.intermediates,
        &draft.intermediates,
        |i| with_formula(&i.id, &i.symbol, &i.formula),
        &mut changes,
    )?;
    diff_items(
        "el resultado por punto",
        &published.point_results,
        &draft.point_results,
        |p| with_formula(&p.id, &p.symbol, &p.formula),
        &mut changes,
    )?;
    diff_items(
        "el agregado",
        &published.aggregates,
        &draft.aggregates,
        |a| with_formula(&a.id, &a.symbol, &a.formula),
        &mut changes,
    )?;
    diff_items(
        "la curva",
        &published.curves,
        &draft.curves,
        |c| Item {
            id: c.id.clone(),
            label: format!("{} vs {}", c.y_formula, c.x_formula),
            formula: None,
        },
        &mut changes,
    )?;
    let orders: [(&str, Vec<&str>, Vec<&str>); 4] = [
        (
            "las curvas",
            published.curves.iter().map(|c| c.id.as_str()).collect(),
            draft.curves.iter().map(|c| c.id.as_str()).collect(),
        ),
        (
            "las intermedias",
            published
                .intermediates
                .iter()
                .map(|i| i.id.as_str())
                .collect(),
            draft.intermediates.iter().map(|i| i.id.as_str()).collect(),
        ),
        (
            "los resultados por punto",
            published
                .point_results
                .iter()
                .map(|p| p.id.as_str())
                .collect(),
            draft.point_results.iter().map(|p| p.id.as_str()).collect(),
        ),
        (
            "los agregados",
            published.aggregates.iter().map(|a| a.id.as_str()).collect(),
            draft.aggregates.iter().map(|a| a.id.as_str()).collect(),
        ),
    ];
    for (what, before, after) in orders {
        if order_changed(&before, &after) {
            changes.push(format!("Cambia el orden de {what}"));
        }
    }
    if structure(published)?["sections"] != structure(draft)?["sections"] {
        changes.push("Cambia la organización en secciones".to_string());
    }
    Ok(changes)
}

/// Compara una versión publicada con el borrador. Los nombres, títulos, tolerancias y el orden de
/// las listas que no afecta el cálculo son cosméticos.
///
/// ```
/// use quantify::db::PracticeResult;
/// use quantify::practices::{compare_definitions, DraftChangeKind, PracticeDefinition};
///
/// let result = PracticeResult {
///     id: "r".into(), practice_id: "p".into(), symbol: "g".into(), name: "g".into(),
///     unit: "m/s2".into(), formula: "2*L".into(), position: 0, tolerance: None,
///     is_final: true, has_uncertainty: true,
/// };
/// let published = PracticeDefinition { results: vec![result.clone()], ..Default::default() };
/// let kind = |draft: &PracticeDefinition| compare_definitions(&published, draft).unwrap().kind;
/// assert_eq!(kind(&published), DraftChangeKind::None);
///
/// let renamed = PracticeResult { name: "Gravedad".into(), tolerance: Some(1.0), ..result.clone() };
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
    if structure(published)? != structure(draft)? {
        return Ok(DefinitionComparison {
            kind: DraftChangeKind::Structural,
            changes: list_changes(published, draft)?,
        });
    }
    let kind = if shape(published, &VOLATILE_KEYS)? == shape(draft, &VOLATILE_KEYS)? {
        DraftChangeKind::None
    } else {
        DraftChangeKind::Cosmetic
    };
    Ok(DefinitionComparison {
        kind,
        changes: Vec::new(),
    })
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
        submissions,
        version_submissions,
        last_update,
    }))
}
