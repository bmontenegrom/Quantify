use super::PracticeDefinition;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

const COSMETIC_KEYS: [&str; 3] = ["name", "title", "tolerance"];

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
    pub submissions: i64,
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
    if changes.is_empty() {
        changes.push("Cambia el orden o la organización en secciones".to_string());
    }
    Ok(changes)
}

/// Compara una versión publicada con el borrador. Los nombres, títulos y tolerancias son cosméticos.
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
    let volatile = ["status", "current_version", "position"];
    let comparable = [volatile.as_slice(), COSMETIC_KEYS.as_slice()].concat();
    let unchanged = |keys: &[&str]| -> anyhow::Result<bool> {
        Ok(shape(published, keys)? == shape(draft, keys)?)
    };
    if !unchanged(&comparable)? {
        return Ok(DefinitionComparison {
            kind: DraftChangeKind::Structural,
            changes: list_changes(published, draft)?,
        });
    }
    let kind = if unchanged(&volatile)? {
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
    Ok(Some(DraftChanges {
        version: current.number,
        kind: comparison.kind,
        changes: comparison.changes,
        submissions,
    }))
}
