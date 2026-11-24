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
                operator_instruments: None,
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
        let template = crate::practices::export_template(
            &pool,
            source,
            crate::practices::ExportSource::Borrador,
        )
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

fn collect_derived(
    value: &Value,
    determination: Option<usize>,
    out: &mut std::collections::HashMap<String, (f64, f64)>,
) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                match (key.as_str(), v) {
                    ("determinations", Value::Array(items)) => {
                        for (i, item) in items.iter().enumerate() {
                            collect_derived(item, Some(i + 1), out);
                        }
                    }
                    ("derived", Value::Array(items)) => {
                        for item in items {
                            let symbol = item["symbol"].as_str().unwrap();
                            let name = format!(
                                "{symbol}{}",
                                determination.map(|k| k.to_string()).unwrap_or_default()
                            );
                            let number = |key: &str| item[key].as_f64().unwrap_or(f64::NAN);
                            let previous =
                                out.insert(name.clone(), (number("value"), number("u_expanded")));
                            assert!(
                                previous.is_none(),
                                "{name} aparece dos veces en el análisis"
                            );
                        }
                    }
                    _ => collect_derived(v, determination, out),
                }
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|v| collect_derived(v, determination, out)),
        _ => {}
    }
}

#[tokio::test]
async fn determinations_keep_the_numbers_of_the_hand_written_practices() {
    let (pool, _dir, scales) = golden_setup().await;
    let mut found = std::collections::HashMap::new();
    for id in ["p1-estadistica", "hidrostatica"] {
        let input = read_input(&golden_dir().join(format!("{id}.input.json")));
        let measurements = golden_measurements(&pool, &input, id, &scales).await;
        let mut practice = std::collections::HashMap::new();
        collect_derived(
            &analysis_value(&pool, id, &measurements).await,
            None,
            &mut practice,
        );
        for (symbol, v) in practice {
            assert!(
                found.insert(symbol.clone(), v).is_none(),
                "{symbol} aparece en las dos prácticas"
            );
        }
    }
    let today = [
        ("gamma", 0.014546635478697698, 0.0),
        ("Q1", 215.22443645230533, 0.18698322161452516),
        ("g1", 9.80185493545634, 0.025968686128033363),
        ("g2", 9.80478603911126, 0.04259724965624894),
        ("E1", 0.01666, 0.0002786257944588778),
        ("E2", 0.0165816, 0.00027748016575417225),
        ("E3", 0.0166992, 0.0002792079909599565),
        ("E_medio", 0.016646933333333332, 0.0002762012855269959),
        ("rho1", 7338.235294117647, 458.6085225124983),
        ("rho2", 7372.931442080379, 460.7965887076958),
        ("rho3", 7321.0093896713615, 457.52336722666536),
        ("rho_medio", 7344.058708623129, 458.71304845150377),
        ("gamma1", 0.019600000000000003, 0.00022632130553941764),
        ("gamma2", 0.020384000000000003, 0.00022632130550397843),
        ("gamma3", 0.019208000000000003, 0.00022632130553778247),
        ("gamma_medio", 0.01973066666666667, 0.00013066666667650308),
    ];
    for (symbol, value, u) in today {
        let (got_value, got_u) = found
            .get(symbol)
            .unwrap_or_else(|| panic!("falta {symbol}: {:?}", found.keys()));
        assert!(
            close_enough(*got_value, value) && close_enough(*got_u, u),
            "{symbol}: hoy {value} ± {u}, ahora {got_value} ± {got_u}"
        );
    }
}

fn close_enough(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1e-12)
}
