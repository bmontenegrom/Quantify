//! Tests "golden": congelan el análisis completo de cada práctica sembrada.
//!
//! Por cada `tests/golden/<practica>.input.json` (mediciones por símbolo, con el instrumento y la
//! escala por nombre) se corre `analyze` y se compara el JSON resultante con
//! `tests/golden/<practica>.expected.json`, con tolerancia numérica relativa. Cualquier cambio en
//! el cálculo de una práctica hace fallar este test: si el cambio es intencional, se regeneran los
//! esperados con `QUANTIFY_UPDATE_GOLDEN=1 cargo test golden` y el diff de los `.expected.json`
//! queda a la vista en la PR.

use super::tests::setup;
use super::*;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Una medición de entrada, identificada por símbolo (los ids de la base cambian en cada siembra).
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
    /// `"Instrumento / escala"`, tal como se llaman en el catálogo sembrado.
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

/// Saca los ids generados al sembrar (`quantity_id`, `instrument_id`, `scale_id`), que no son
/// parte del cálculo y cambian en cada base.
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

/// Compara dos JSON: números con tolerancia relativa 1e-9 (absoluta 1e-12 cerca de cero), el
/// resto exacto. Devuelve la primera diferencia con su ruta.
fn diff(path: &str, actual: &Value, expected: &Value) -> Option<String> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            let tol = 1e-12_f64.max(1e-9 * e.abs());
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

#[tokio::test]
async fn golden_practices_match() {
    let (pool, _dir) = setup().await;
    crate::db::seed_academic(&pool).await.unwrap();
    crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
        .await
        .unwrap();
    let scales: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT i.name || ' / ' || s.label, i.id, s.id \
         FROM instrument_scales s JOIN instruments i ON i.id = s.instrument_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let update = std::env::var("QUANTIFY_UPDATE_GOLDEN").is_ok_and(|v| v == "1");

    let mut inputs: Vec<PathBuf> = std::fs::read_dir(golden_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".input.json"))
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty(), "no hay entradas en tests/golden");

    let mut failures = Vec::new();
    for input_path in &inputs {
        let input: GoldenInput =
            serde_json::from_str(&std::fs::read_to_string(input_path).unwrap()).unwrap();
        let def = crate::practices::definition(&pool, &input.practice_id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("no existe la práctica {}", input.practice_id));
        let measurements: Vec<MeasurementInput> = input
            .measurements
            .into_iter()
            .map(|m| {
                let quantity_id = def
                    .quantities
                    .iter()
                    .find(|q| q.symbol == m.symbol)
                    .unwrap_or_else(|| panic!("{}: no existe {}", input.practice_id, m.symbol))
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
                    values: m.values,
                    given_u: m.given_u,
                    point_replicas: m.point_replicas,
                    operator_replicas: m.operator_replicas,
                }
            })
            .collect();

        let analysis = analyze(&pool, &input.practice_id, &measurements)
            .await
            .unwrap_or_else(|e| panic!("{}: analyze falló: {e:#}", input.practice_id));
        let mut actual = serde_json::to_value(&analysis).unwrap();
        strip_ids(&mut actual);

        let expected_path = PathBuf::from(
            input_path
                .to_string_lossy()
                .replace(".input.json", ".expected.json"),
        );
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
    }
    assert!(
        failures.is_empty(),
        "el análisis cambió respecto de tests/golden (si es intencional, regenerá con \
         QUANTIFY_UPDATE_GOLDEN=1 cargo test golden):\n  {}",
        failures.join("\n  ")
    );
}
