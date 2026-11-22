use super::tests::setup;
use super::*;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct GoldenMeasurement {
    symbol: String,
    #[serde(default)]
    values: Vec<f64>,
    #[serde(default)]
    given_u: Option<f64>,
    #[serde(default)]
    point_replicas: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    operator_replicas: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    scale: Option<String>,
}

#[derive(Deserialize)]
struct GoldenInput {
    practice_id: String,
    measurements: Vec<GoldenMeasurement>,
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn strip_ids(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for key in ["quantity_id", "instrument_id", "scale_id"] {
                map.remove(key);
            }
            map.values_mut().for_each(strip_ids);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_ids),
        _ => {}
    }
}

fn sorted_lists(value: &mut Value) {
    match value {
        Value::Object(map) => map.values_mut().for_each(sorted_lists),
        Value::Array(items) => {
            items.iter_mut().for_each(sorted_lists);
            if items
                .iter()
                .all(|item| item.is_object() || item.is_string())
            {
                items.sort_by_key(|item| item.to_string());
            }
        }
        _ => {}
    }
}

fn diff(path: &str, actual: &Value, expected: &Value) -> Option<String> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            let tol = if e == 0.0 { 1e-12 } else { 1e-9 * e.abs() };
            ((a - e).abs() > tol).then(|| format!("{path}: {a} != {e} (esperado)"))
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Some(format!(
                    "{path}: {} elementos != {} (esperado)",
                    a.len(),
                    e.len()
                ));
            }
            a.iter()
                .zip(e)
                .enumerate()
                .find_map(|(i, (a, e))| diff(&format!("{path}[{i}]"), a, e))
        }
        (Value::Object(a), Value::Object(e)) => {
            let mut keys: Vec<_> = a.keys().chain(e.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|k| match (a.get(k), e.get(k)) {
                (Some(a), Some(e)) => diff(&format!("{path}.{k}"), a, e),
                (Some(_), None) => Some(format!("{path}.{k}: sobra (no está en el esperado)")),
                _ => Some(format!("{path}.{k}: falta")),
            })
        }
        _ => (actual != expected).then(|| format!("{path}: {actual} != {expected} (esperado)")),
    }
}

type Scales = Vec<(String, String, String)>;

async fn golden_setup() -> (sqlx::SqlitePool, tempfile::TempDir, Scales) {
    let (pool, dir) = setup().await;
    crate::db::seed_academic(&pool).await.unwrap();
    crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
        .await
        .unwrap();
    let scales = sqlx::query_as(
        "SELECT i.name || ' / ' || s.label, i.id, s.id \
         FROM instrument_scales s JOIN instruments i ON i.id = s.instrument_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    (pool, dir, scales)
}

fn golden_inputs() -> Vec<PathBuf> {
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(golden_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".input.json"))
        .collect();
    inputs.sort();
    inputs
}

fn read_input(path: &Path) -> GoldenInput {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn expected_path(input_path: &Path) -> PathBuf {
    PathBuf::from(
        input_path
            .to_string_lossy()
            .replace(".input.json", ".expected.json"),
    )
}

async fn golden_measurements(
    pool: &sqlx::SqlitePool,
    input: &GoldenInput,
    practice_id: &str,
    scales: &Scales,
) -> Vec<MeasurementInput> {
    let def = crate::practices::definition(pool, practice_id)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("no existe la práctica {practice_id}"));
    input
        .measurements
        .iter()
        .map(|m| {
            let quantity_id = def
                .quantities
                .iter()
                .find(|q| q.symbol == m.symbol)
                .unwrap_or_else(|| panic!("{practice_id}: no existe {}", m.symbol))
                .id
                .clone();
            let (instrument_id, scale_id) = match &m.scale {
                Some(name) => {
                    let (_, inst, scale) = scales
                        .iter()
                        .find(|(n, _, _)| n == name)
                        .unwrap_or_else(|| panic!("no existe la escala {name}"));
                    (Some(inst.clone()), Some(scale.clone()))
                }
                None => (None, None),
            };
            MeasurementInput {
                quantity_id,
                instrument_id,
                scale_id,
                values: m.values.clone(),
                given_u: m.given_u,
                point_replicas: m.point_replicas.clone(),
                operator_replicas: m.operator_replicas.clone(),
            }
        })
        .collect()
}

async fn analysis_value(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
    measurements: &[MeasurementInput],
) -> Value {
    let analysis = analyze(pool, practice_id, measurements)
        .await
        .unwrap_or_else(|e| panic!("{practice_id}: analyze falló: {e:#}"));
    let mut value = serde_json::to_value(&analysis).unwrap();
    strip_ids(&mut value);
    value
}

fn rename_parts(value: &mut Value, names: &std::collections::HashMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match v {
                    Value::String(id) if key == "part_id" => {
                        if let Some(name) = names.get(id.as_str()) {
                            *id = name.clone();
                        }
                    }
                    _ => rename_parts(v, names),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| rename_parts(v, names)),
        _ => {}
    }
}

#[tokio::test]
async fn golden_practices_match_after_export_import() {
    let (pool, _dir, scales) = golden_setup().await;
    let mut failures = Vec::new();
    for input_path in golden_inputs() {
        let input = read_input(&input_path);
        let source = &input.practice_id;
        let template = crate::practices::export_template(&pool, source)
            .await
            .unwrap()
            .unwrap();
        let copy = crate::practices::import_template(&pool, &template, "u1")
            .await
            .unwrap();
        let parts = |id: String| {
            let pool = pool.clone();
            async move {
                let def = crate::practices::definition(&pool, &id)
                    .await
                    .unwrap()
                    .unwrap();
                def.parts.into_iter().map(|p| p.id).collect::<Vec<_>>()
            }
        };
        let names = parts(copy.id.clone())
            .await
            .into_iter()
            .zip(parts(source.clone()).await)
            .collect();
        let measurements = golden_measurements(&pool, &input, &copy.id, &scales).await;
        let mut actual = analysis_value(&pool, &copy.id, &measurements).await;
        rename_parts(&mut actual, &names);
        let expected: Value =
            serde_json::from_str(&std::fs::read_to_string(expected_path(&input_path)).unwrap())
                .unwrap();
        if let Some(d) = diff("", &actual, &expected) {
            failures.push(format!("{source}: {d}"));
        }
    }
    assert!(
        failures.is_empty(),
        "exportar e importar cambió el análisis:\n  {}",
        failures.join("\n  ")
    );
}

#[tokio::test]
async fn golden_practices_match() {
    let (pool, _dir, scales) = golden_setup().await;
    let update = std::env::var("QUANTIFY_UPDATE_GOLDEN").is_ok_and(|v| v == "1");
    let inputs = golden_inputs();
    for practice in crate::db::practices(&pool).await.unwrap() {
        let input = golden_dir().join(format!("{}.input.json", practice.id));
        assert!(
            inputs.contains(&input),
            "falta tests/golden/{}.input.json (cada práctica sembrada necesita su golden)",
            practice.id
        );
    }

    let mut failures = Vec::new();
    for input_path in &inputs {
        let input = read_input(input_path);
        let measurements = golden_measurements(&pool, &input, &input.practice_id, &scales).await;
        let mut actual = analysis_value(&pool, &input.practice_id, &measurements).await;
        let expected_path = expected_path(input_path);
        if update {
            let pretty = serde_json::to_string_pretty(&actual).unwrap() + "\n";
            std::fs::write(&expected_path, pretty).unwrap();
            continue;
        }
        let expected: Value = match std::fs::read_to_string(&expected_path) {
            Ok(text) => serde_json::from_str(&text).unwrap(),
            Err(_) => {
                failures.push(format!(
                    "{}: falta {} (generalo con QUANTIFY_UPDATE_GOLDEN=1)",
                    input.practice_id,
                    expected_path.display()
                ));
                continue;
            }
        };
        if let Some(d) = diff("", &actual, &expected) {
            failures.push(format!("{}: {d}", input.practice_id));
        }

        for table in [
            "practice_quantities",
            "practice_results",
            "practice_parts",
            "practice_sections",
        ] {
            sqlx::query(&format!(
                "UPDATE {table} SET position = -position WHERE practice_id = ?1"
            ))
            .bind(&input.practice_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        let mut reordered = analysis_value(&pool, &input.practice_id, &measurements).await;
        sorted_lists(&mut reordered);
        sorted_lists(&mut actual);
        if let Some(d) = diff("", &reordered, &actual) {
            failures.push(format!("{}: al reordenar, {d}", input.practice_id));
        }
    }
    assert!(
        failures.is_empty(),
        "el análisis cambió respecto de tests/golden (si es intencional, regenerá con \
         QUANTIFY_UPDATE_GOLDEN=1 cargo test golden):\n  {}",
        failures.join("\n  ")
    );
}
