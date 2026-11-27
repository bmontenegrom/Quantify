use super::formula::{compile_formula, eval_compiled};
use super::*;
use crate::db::{self, InstrumentScale, PracticeQuantity, PracticeResult};
use crate::practices::{PracticeAggregate, PracticeIntermediate, PracticePointResult};
use crate::uncertainty::BModel;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::str::FromStr;
use tempfile::TempDir;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() < tol
}

pub(crate) async fn drop_parts(pool: &SqlitePool, practice_id: &str) {
    sqlx::query("DELETE FROM practice_parts WHERE practice_id = ?1")
        .bind(practice_id)
        .execute(pool)
        .await
        .unwrap();
}

pub(crate) fn flattened(mut analysis: FormAnalysis) -> FormAnalysis {
    for part in std::mem::take(&mut analysis.parts) {
        let a = part.analysis;
        analysis.quantities.extend(a.quantities);
        analysis.derived.extend(a.derived);
        analysis.scatters.extend(a.scatters);
        analysis.point_results.extend(a.point_results);
        analysis.aggregates.extend(a.aggregates);
        analysis.warnings.extend(a.warnings);
        analysis.regression = analysis.regression.or(a.regression);
    }
    analysis
}

fn by_determination(analysis: &FormAnalysis) -> HashMap<String, f64> {
    let mut found = HashMap::new();
    let views = std::iter::once(analysis).chain(analysis.parts.iter().map(|p| &p.analysis));
    for view in views {
        for d in &view.derived {
            found.insert(d.symbol.clone(), d.value);
        }
        for (k, det) in view.determinations.iter().enumerate() {
            for d in &det.derived {
                found.insert(format!("{}{}", d.symbol, k + 1), d.value);
            }
        }
    }
    found
}

fn hidrostatica_input(
    def: &crate::practices::PracticeDefinition,
    value: impl Fn(&str, usize) -> f64,
) -> Vec<MeasurementInput> {
    def.quantities
        .iter()
        .map(|q| MeasurementInput {
            quantity_id: q.id.clone(),
            instrument_id: None,
            scale_id: None,
            values: if q.per_determination {
                vec![]
            } else {
                vec![value(&q.symbol, 0)]
            },
            given_u: None,
            point_replicas: None,
            operator_replicas: q
                .per_determination
                .then(|| (1..=3).map(|k| vec![value(&q.symbol, k)]).collect()),
            operator_instruments: None,
        })
        .collect()
}

pub(crate) async fn p1_partes(pool: &SqlitePool) {
    crate::practices::seed_fixture(
        pool,
        include_str!("../../tests/fixtures/p1-partes.json"),
        "p1-partes",
    )
    .await;
}

pub(crate) async fn setup() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("t.db").to_string_lossy());
    let opts = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    db::migrate(&pool).await.unwrap();
    crate::practices::seed_library(&pool).await.unwrap();
    db::seed_users(&pool).await.unwrap();
    (pool, dir)
}

#[tokio::test]
async fn validate_student_results_accepts_final_aggregates() {
    let (pool, _dir) = setup().await;
    validate_student_results(
        &crate::practices::definition(&pool, "fluidos-2")
            .await
            .unwrap()
            .unwrap(),
        &[
            db::StudentResultInput {
                symbol: "Re_max".into(),
                value: 55000.0,
                u_expanded: None,
            },
            db::StudentResultInput {
                symbol: "M_teorico".into(),
                value: 0.86,
                u_expanded: None,
            },
        ],
    )
    .unwrap();

    let err = validate_student_results(
        &crate::practices::definition(&pool, "fluidos-2")
            .await
            .unwrap()
            .unwrap(),
        &[db::StudentResultInput {
            symbol: "no_existe".into(),
            value: 1.0,
            u_expanded: None,
        }],
    )
    .unwrap_err();
    assert!(err.to_string().contains("no_existe"));
}

#[tokio::test]
async fn validate_student_results_accepts_point_results_per_run() {
    let (pool, _dir) = setup().await;
    validate_student_results(
        &crate::practices::definition(&pool, "viscosidad")
            .await
            .unwrap()
            .unwrap(),
        &[
            db::StudentResultInput {
                symbol: "Re#0".into(),
                value: 0.4,
                u_expanded: None,
            },
            db::StudentResultInput {
                symbol: "Re#3".into(),
                value: 0.9,
                u_expanded: None,
            },
        ],
    )
    .unwrap();

    for bad in ["Re#x", "Re#-0", "Re#01", "Nope#0"] {
        let err = validate_student_results(
            &crate::practices::definition(&pool, "viscosidad")
                .await
                .unwrap()
                .unwrap(),
            &[db::StudentResultInput {
                symbol: bad.into(),
                value: 1.0,
                u_expanded: None,
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains(bad));
    }
}

#[test]
fn check_student_result_symbols_accepts_finals_and_per_run() {
    use crate::practices::{PracticeAggregate, PracticeDefinition, PracticePointResult};
    let aggregate = |symbol: &str, is_final: bool| PracticeAggregate {
        id: symbol.into(),
        practice_id: "viscosidad".into(),
        position: 0,
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "".into(),
        formula: "slope".into(),
        is_final,
        has_uncertainty: true,
    };
    let definition = PracticeDefinition {
        description: String::new(),
        practice_id: "viscosidad".into(),
        status: "borrador".into(),
        analysis_kind: Some("regresion_lineal".into()),
        x_formula: None,
        y_formula: None,
        quantities: vec![],
        results: vec![db::PracticeResult {
            id: "mu".into(),
            practice_id: "viscosidad".into(),
            symbol: "mu".into(),
            name: "Viscosidad".into(),
            unit: "Pa.s".into(),
            formula: "slope".into(),
            position: 0,
            tolerance: None,
            is_final: true,
            has_uncertainty: true,
            ..Default::default()
        }],
        curves: vec![],
        intermediates: vec![],
        point_results: vec![PracticePointResult {
            id: "Re".into(),
            practice_id: "viscosidad".into(),
            position: 0,
            symbol: "Re".into(),
            name: "Reynolds".into(),
            unit: "".into(),
            formula: "rho_f*(dx/t)*2*R/mu".into(),
        }],
        aggregates: vec![aggregate("Re_medio", true), aggregate("Re_min", false)],
        parts: vec![],
        sections: vec![],
        display_symbols: Default::default(),
        current_version: None,
        determinations: None,
    };
    let sr = |symbol: &str| db::StudentResultInput {
        symbol: symbol.into(),
        value: 1.0,
        u_expanded: None,
    };

    assert!(check_student_result_symbols(
        &definition,
        &[sr("mu"), sr("Re_medio"), sr("Re#0"), sr("Re#12")],
    )
    .is_ok());
    assert!(check_student_result_symbols(&definition, &[]).is_ok());
    for bad in ["Re_min", "nope", "Q#0", "Re#x"] {
        let err = check_student_result_symbols(&definition, &[sr(bad)]).unwrap_err();
        assert!(err.contains(bad), "{bad} debe rechazarse");
    }
}

fn quantity(symbol: &str) -> PracticeQuantity {
    PracticeQuantity {
        id: format!("q-{symbol}"),
        practice_id: "p1-estadistica".into(),
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "mm".into(),
        repeated: true,
        quantity: None,
        position: 0,
        is_given: false,
        replicas_per_point: None,
        per_point: true,
        has_uncertainty: true,
        optional: false,
        default_value: None,
        slot_count: None,
        ..Default::default()
    }
}

fn measurement(symbol: &str, values: &[f64]) -> MeasurementInput {
    MeasurementInput {
        quantity_id: format!("q-{symbol}"),
        instrument_id: None,
        scale_id: None,
        values: values.to_vec(),
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    }
}

fn reg(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    results: &[PracticeResult],
    x: &str,
    y: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    compute_regresion(
        quantities,
        intermediates,
        results,
        &[],
        &[],
        &HashMap::new(),
        x,
        y,
        measurements,
    )
}

fn curve<'a>(x_formula: &'a str, y_formula: &'a str, x_log: bool) -> CurveSpec<'a> {
    CurveSpec {
        x_formula,
        y_formula,
        x_log,
    }
}

fn fab_scale() -> InstrumentScale {
    InstrumentScale {
        id: "s1".into(),
        instrument_id: "i1".into(),
        label: "x".into(),
        full_scale: None,
        step: 1.0,
        appreciation: None,
        internal_res: None,
        internal_res_u: None,
        b_model: "fabricante".into(),
        spec_pct_reading: Some(1.0),
        spec_step_coeff: Some(5.0),
        spec_fixed: Some(0.0),
        u_cal_pct: 0.0,
        u_cal_fixed: 0.0,
        unit: "A".into(),
        position: 1,
    }
}

fn type_b_in(scale: &InstrumentScale, unit: &str, value: f64) -> (f64, Vec<String>) {
    let q = PracticeQuantity {
        unit: unit.into(),
        ..quantity("x")
    };
    let m = MeasurementInput {
        instrument_id: Some("i1".into()),
        scale_id: Some("s1".into()),
        ..measurement("x", &[value])
    };
    let scales = HashMap::from([("s1".to_string(), scale.clone())]);
    let analysis = compute(&[q], &[], &scales, &[m], None).unwrap();
    (analysis.quantities[0].result.u_b, analysis.warnings)
}

#[test]
fn the_type_b_is_converted_to_the_unit_of_the_quantity() {
    let rule = InstrumentScale {
        b_model: "resolucion".into(),
        step: 1.0,
        spec_pct_reading: None,
        spec_step_coeff: None,
        unit: "mm".into(),
        label: "regla".into(),
        ..fab_scale()
    };
    let rect = 2.0 * 3f64.sqrt();
    for (unit, step) in [("mm", 1.0), ("cm", 0.1), ("m", 0.001)] {
        let (u_b, _) = type_b_in(&rule, unit, 1.0);
        assert!(close(u_b, step / rect, 1e-15), "{unit}: {u_b}");
    }

    let densimeter = InstrumentScale {
        b_model: "apreciacion".into(),
        step: 0.001,
        appreciation: Some(0.001),
        u_cal_fixed: 0.001,
        spec_pct_reading: None,
        spec_step_coeff: None,
        unit: "g/cm3".into(),
        ..fab_scale()
    };
    let (in_g_cm3, _) = type_b_in(&densimeter, "g/cm3", 1.0);
    let (in_kg_m3, _) = type_b_in(&densimeter, "kg/m3", 1000.0);
    let expected = ((0.001f64 / 6f64.sqrt()).powi(2) + 0.001f64.powi(2)).sqrt();
    assert!(close(in_g_cm3, expected, 1e-15));
    assert!(close(in_kg_m3, 1000.0 * expected, 1e-12));

    let clock = InstrumentScale {
        unit: "s".into(),
        label: "milesimas".into(),
        ..rule.clone()
    };
    let (u_b, warnings) = type_b_in(&clock, "g", 1.0);
    assert!(close(u_b, 1.0 / rect, 1e-15), "sin convertir, como antes");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("mide en s y la magnitud está en g")
                && w.contains("\"milesimas\" no se convirtió")),
        "{warnings:?}"
    );

    let thermometer = InstrumentScale {
        b_model: "fabricante".into(),
        step: 0.1,
        spec_pct_reading: Some(1.0),
        spec_step_coeff: Some(0.0),
        spec_fixed: Some(0.0),
        unit: "°C".into(),
        ..fab_scale()
    };
    let at = |unit: &str, value: f64| type_b_in(&thermometer, unit, value).0;
    assert!(close(at("K", 273.15), at("°C", 0.0), 1e-12));
    assert!(close(at("K", 373.15), at("°C", 100.0), 1e-12));
    let kelvin = InstrumentScale {
        unit: "K".into(),
        ..thermometer.clone()
    };
    let (in_celsius, _) = type_b_in(&kelvin, "°C", 0.0);
    let (in_kelvin, _) = type_b_in(&kelvin, "K", 273.15);
    assert!(close(in_celsius, in_kelvin, 1e-12));

    let divisions = InstrumentScale {
        unit: "div".into(),
        ..rule
    };
    let (u_b, warnings) = type_b_in(&divisions, "mm", 1.0);
    assert!(close(u_b, 1.0 / rect, 1e-15));
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no se reconoce la unidad div") && w.contains("no se convirtió")),
        "{warnings:?}"
    );
    assert!(type_b_in(&divisions, "div", 1.0).1.is_empty());
    let empty = compute(
        &[PracticeQuantity {
            unit: "mm".into(),
            ..quantity("x")
        }],
        &[],
        &HashMap::from([("s1".to_string(), divisions)]),
        &[MeasurementInput {
            instrument_id: Some("i1".into()),
            scale_id: Some("s1".into()),
            ..measurement("x", &[])
        }],
        None,
    )
    .unwrap();
    assert!(
        !empty.warnings.iter().any(|w| w.contains("no se convirtió")),
        "sin lecturas no hay incertidumbre que convertir: {:?}",
        empty.warnings
    );
}

#[test]
fn scale_spec_maps_models_and_options() {
    let spec = scale_spec(&fab_scale()).unwrap();
    assert_eq!(spec.b_model, BModel::Fabricante);
    assert_eq!(spec.spec_pct_reading, 1.0);
    assert_eq!(spec.spec_step_coeff, 5.0);
    assert_eq!(spec.spec_fixed, 0.0);

    let mut bad = fab_scale();
    bad.b_model = "otro".into();
    assert!(scale_spec(&bad).is_err());
}

#[test]
fn compile_formula_rejects_unknown_symbol() {
    let symbols = vec!["l".to_string(), "a".to_string()];
    assert!(compile_formula("l*a", &symbols).is_ok());
    assert!(compile_formula("l*z", &symbols).is_err());
    assert!(compile_formula("(l*a", &symbols).is_err());
}

#[test]
fn an_integer_result_counts_as_a_number() {
    let symbols = vec!["L".to_string()];
    let tree = compile_formula("max(L, 0)", &symbols).unwrap();
    assert_eq!(eval_compiled(&tree, &HashMap::from([("L", -2.0)])), 0.0);
    assert_eq!(eval_compiled(&tree, &HashMap::from([("L", 3.5)])), 3.5);
    let tree = compile_formula("min(L, 5)", &symbols).unwrap();
    assert_eq!(eval_compiled(&tree, &HashMap::from([("L", 9.0)])), 5.0);
}

#[test]
fn integer_literals_divide_as_decimals() {
    let symbols = vec!["L".to_string(), "T1".to_string()];
    let eval = |formula: &str| {
        let tree = compile_formula(formula, &symbols).unwrap();
        eval_compiled(&tree, &HashMap::from([("L", 4.0), ("T1", 3.0)]))
    };
    assert_eq!(eval("1/2"), 0.5);
    assert_eq!(eval("math::pow(L, 1/2)"), 2.0);
    assert_eq!(eval("T1/2"), 1.5);
    assert_eq!(eval("2.5*2"), 5.0);
    assert!((eval("1e-3*2") - 0.002).abs() < 1e-15);
    assert!((eval("2.5E-3*L") - 0.01).abs() < 1e-15);
    assert_eq!(eval("6e+2/L"), 150.0);
    assert!((eval("2*e-1") - (2.0 * std::f64::consts::E - 1.0)).abs() < 1e-12);
    assert!((eval("math::log10(100)") - 2.0).abs() < 1e-12);
}

#[test]
fn compute_propagates_q_l_a_b() {
    let quantities = vec![quantity("l"), quantity("a"), quantity("b")];
    let results = vec![PracticeResult {
        id: "r1".into(),
        practice_id: "p1-estadistica".into(),
        symbol: "Q".into(),
        name: "Area".into(),
        unit: "mm2".into(),
        formula: "l*a + l*b".into(),
        position: 0,
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
        ..Default::default()
    }];
    let measurements = vec![
        measurement("l", &[2.0]),
        measurement("a", &[3.0]),
        measurement("b", &[4.0]),
    ];
    let analysis = compute(&quantities, &results, &HashMap::new(), &measurements, None).unwrap();
    assert_eq!(analysis.quantities.len(), 3);
    let q_l = &analysis.quantities[0];
    assert_eq!(q_l.symbol, "l");
    assert!(close(q_l.result.mean, 2.0, 1e-12));
    assert!(close(q_l.result.u_c, 0.0, 1e-12));
    assert_eq!(analysis.derived.len(), 1);
    assert!(close(analysis.derived[0].value, 14.0, 1e-9));
    assert!(close(analysis.derived[0].u, 0.0, 1e-9));
}

#[test]
fn compute_propagates_uncertainty_to_measurand() {
    let quantities = vec![quantity("l"), quantity("a"), quantity("b")];
    let results = vec![PracticeResult {
        id: "r1".into(),
        practice_id: "p1-estadistica".into(),
        symbol: "Q".into(),
        name: "Area".into(),
        unit: "mm2".into(),
        formula: "l*a + l*b".into(),
        position: 0,
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
        ..Default::default()
    }];
    let measurements = vec![
        measurement("l", &[9.0, 11.0]),
        measurement("a", &[2.0]),
        measurement("b", &[3.0]),
    ];
    let analysis = compute(&quantities, &results, &HashMap::new(), &measurements, None).unwrap();
    let q_l = &analysis.quantities[0];
    assert!(close(q_l.result.u_a, 1.0, 1e-12));
    let q = &analysis.derived[0];
    assert!(close(q.value, 50.0, 1e-9));
    assert!(close(q.u, 5.0, 1e-6));
    assert!(close(q.u_expanded, 10.0, 1e-6));
}

#[test]
fn compute_warns_on_missing_readings() {
    let quantities = vec![quantity("l")];
    let analysis = compute(&quantities, &[], &HashMap::new(), &[], None).unwrap();
    assert_eq!(analysis.warnings.len(), 1);
    assert!(analysis.warnings[0].contains("no tiene lecturas"));
}

#[tokio::test]
async fn analyze_uses_type_a_with_replicas() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    drop_parts(&pool, "p1-partes").await;
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let measurements = vec![MeasurementInput {
        quantity_id: t_id,
        instrument_id: None,
        scale_id: None,
        values: vec![10.0, 12.0, 11.0],
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    }];
    let analysis = analyze(&pool, "p1-partes", &measurements).await.unwrap();
    let q_t = analysis
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap();
    assert_eq!(q_t.result.n, 3);
    assert!(close(q_t.result.mean, 11.0, 1e-12));
    assert!(q_t.result.u_a > 0.0);
}

#[tokio::test]
async fn create_form_submission_persists_and_reads_back() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: Some(serde_json::json!({ "q1": { "bins": 8, "discarded": [9.9] } })),
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    assert_eq!(detail.entry_mode, "form");
    assert!(detail.analysis.get("quantities").is_some());
    let meta = detail.measurement_meta.expect("meta persistida");
    assert_eq!(meta["q1"]["bins"], 8);
    assert_eq!(meta["q1"]["discarded"][0], 9.9);
}

#[tokio::test]
async fn create_form_submission_trims_and_persists_student_comment() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let mk = |comment: Option<&str>| FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: comment.map(String::from),
        practice_version: None,
    };

    let detail = create_form_submission(&pool, &user, mk(Some("  anduvo mal el generador  ")))
        .await
        .unwrap();
    assert_eq!(
        detail.student_comment.as_deref(),
        Some("anduvo mal el generador")
    );

    let blank = create_form_submission(&pool, &user, mk(Some("   ")))
        .await
        .unwrap();
    assert_eq!(blank.student_comment, None);

    let absent = create_form_submission(&pool, &user, mk(None))
        .await
        .unwrap();
    assert_eq!(absent.student_comment, None);
}

#[tokio::test]
async fn create_form_submission_persists_optional_student_result() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: None,
        table_number: None,
        student_results: vec![db::StudentResultInput {
            symbol: "g1".into(),
            value: 9.8,
            u_expanded: Some(0.1),
        }],
        student_comment: None,
        practice_version: None,
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    assert_eq!(detail.student_results.len(), 1);
    assert_eq!(detail.student_results[0].symbol, "g1");
    assert_eq!(detail.student_results[0].value, 9.8);
}

#[tokio::test]
async fn create_form_submission_rejects_unknown_student_result_symbol() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: None,
        table_number: None,
        student_results: vec![db::StudentResultInput {
            symbol: "no-existe".into(),
            value: 1.0,
            u_expanded: None,
        }],
        student_comment: None,
        practice_version: None,
    };
    let err = create_form_submission(&pool, &user, input)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no-existe"));
}

#[tokio::test]
async fn determination_submission_stores_each_determination_with_its_instrument() {
    let (pool, _dir) = setup().await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-estadistica")
        .await
        .unwrap()
        .unwrap();
    db::seed_academic(&pool).await.unwrap();
    crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
        .await
        .unwrap();
    let (instrument_id, scale_id): (String, String) =
        sqlx::query_as("SELECT instrument_id, id FROM instrument_scales WHERE unit = 's' LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T")
        .unwrap()
        .id
        .clone();
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![],
            given_u: None,
            point_replicas: None,
            operator_replicas: Some(vec![vec![1.0, 1.1], vec![2.0, 2.1, 2.2]]),
            operator_instruments: Some(vec![
                Default::default(),
                crate::computation::InstrumentChoice {
                    instrument_id: Some(instrument_id),
                    scale_id: Some(scale_id.clone()),
                },
            ]),
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let rows = db::measurements_for(&pool, &detail.id).await.unwrap();
    let t_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == t_id).collect();

    assert_eq!(t_rows.len(), 5);
    for (op, expected_n) in [(0i64, 2usize), (1, 3)] {
        let mut reps: Vec<i64> = t_rows
            .iter()
            .filter(|m| m.operator_index == op)
            .map(|m| m.replicate_index)
            .collect();
        reps.sort_unstable();
        assert_eq!(reps, (0..expected_n as i64).collect::<Vec<_>>());
    }
    for m in &t_rows {
        let expected = (m.operator_index == 1).then_some(scale_id.as_str());
        assert_eq!(
            m.scale_id.as_deref(),
            expected,
            "operador {}",
            m.operator_index
        );
    }

    let determinations = detail.analysis["determinations"].as_array().unwrap();
    assert_eq!(determinations.len(), 3);
    let t_of = |k: usize| {
        determinations[k]["quantities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["symbol"] == "T")
            .unwrap()
            .clone()
    };
    assert_eq!(t_of(0)["scale_id"], serde_json::Value::Null);
    assert_eq!(t_of(0)["result"]["u_b"], 0.0);
    assert_eq!(t_of(1)["scale_id"], scale_id.as_str());
    assert!(t_of(1)["result"]["u_b"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn point_based_submission_stores_point_index_per_point() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    drop_parts(&pool, "p1-partes").await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    crate::practices::set_analysis_kind(&pool, "p1-partes", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-partes",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE practice_quantities SET per_point = 0 \
         WHERE practice_id = 'p1-partes' AND symbol = 'T3'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let qid = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![
            MeasurementInput {
                quantity_id: qid("T1"),
                instrument_id: None,
                scale_id: None,
                values: vec![1.0, 2.0, 3.0],
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
                operator_instruments: None,
            },
            MeasurementInput {
                quantity_id: qid("T3"),
                instrument_id: None,
                scale_id: None,
                values: vec![7.0, 7.1, 7.2, 7.3],
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
                operator_instruments: None,
            },
            MeasurementInput {
                quantity_id: qid("T2"),
                instrument_id: None,
                scale_id: None,
                values: vec![],
                given_u: None,
                point_replicas: Some(vec![vec![4.0, 4.2], vec![5.0, 5.1], vec![6.0, 5.9]]),
                operator_replicas: None,
                operator_instruments: None,
            },
        ],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-partes", None)
        .await
        .unwrap();
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let rows = db::measurements_for(&pool, &detail.id).await.unwrap();

    let mut t_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T1")).collect();
    t_rows.sort_by_key(|m| m.point_index);
    assert_eq!(t_rows.len(), 3);
    for (i, m) in t_rows.iter().enumerate() {
        assert_eq!(m.point_index, i as i64);
        assert_eq!(m.replicate_index, 0);
        assert!(close(m.value, (i + 1) as f64, 1e-9));
    }

    let tmed_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T2")).collect();
    assert_eq!(tmed_rows.len(), 6);
    assert!(tmed_rows
        .iter()
        .any(|m| m.point_index == 0 && m.replicate_index == 1 && close(m.value, 4.2, 1e-9)));
    assert!(tmed_rows
        .iter()
        .any(|m| m.point_index == 2 && m.replicate_index == 0 && close(m.value, 6.0, 1e-9)));

    let shared: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T3")).collect();
    assert_eq!(shared.len(), 4);
    assert!(shared.iter().all(|m| m.point_index == 0));
    let mut reps: Vec<_> = shared.iter().map(|m| m.replicate_index).collect();
    reps.sort();
    assert_eq!(
        reps,
        vec![0, 1, 2, 3],
        "las réplicas de una compartida no son puntos"
    );
}

#[tokio::test]
async fn point_based_submission_stores_variable_replicas_per_point() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    drop_parts(&pool, "p1-partes").await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    crate::practices::set_analysis_kind(&pool, "p1-partes", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-partes",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let qid = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![
            MeasurementInput {
                quantity_id: qid("T1"),
                instrument_id: None,
                scale_id: None,
                values: vec![1.0, 2.0, 3.0],
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
                operator_instruments: None,
            },
            MeasurementInput {
                quantity_id: qid("T2"),
                instrument_id: None,
                scale_id: None,
                values: vec![],
                given_u: None,
                point_replicas: Some(vec![vec![4.0], vec![5.0, 5.1, 5.2], vec![6.0, 6.1]]),
                operator_replicas: None,
                operator_instruments: None,
            },
        ],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-partes", None)
        .await
        .unwrap();
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let rows = db::measurements_for(&pool, &detail.id).await.unwrap();

    let tmed_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T2")).collect();
    assert_eq!(tmed_rows.len(), 6);
    for (point, expected_n) in [(0i64, 1usize), (1, 3), (2, 2)] {
        let mut reps: Vec<i64> = tmed_rows
            .iter()
            .filter(|m| m.point_index == point)
            .map(|m| m.replicate_index)
            .collect();
        reps.sort_unstable();
        assert_eq!(reps.len(), expected_n, "réplicas del punto {point}");
        assert_eq!(reps, (0..expected_n as i64).collect::<Vec<_>>());
    }

    let points = detail.analysis["scatters"][0]["points"].as_array().unwrap();
    assert_eq!(points.len(), 3);
    let y = |i: usize| points[i][1].as_f64().unwrap();
    assert!(close(y(0), 4.0, 1e-9));
    assert!(close(y(1), 5.1, 1e-9));
    assert!(close(y(2), 6.05, 1e-9));
}

#[tokio::test]
async fn update_form_submission_replaces_measurements_and_is_editable() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    drop_parts(&pool, "p1-partes").await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let mk = |vals: Vec<f64>| FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-partes", None)
        .await
        .unwrap();
    let created = create_form_submission(&pool, &user, mk(vec![5.0, 5.2, 4.9]))
        .await
        .unwrap();
    assert!(created.can_edit);
    assert!(created.editable_until.is_some());

    let edited = update_form_submission(
        &pool,
        &created.id,
        &mk(vec![10.0, 12.0, 11.0]).measurements,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let vals: Vec<f64> = edited.measurements.iter().map(|m| m.value).collect();
    assert_eq!(vals, vec![10.0, 12.0, 11.0]);
    let q_t = edited.analysis["quantities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["symbol"] == "T1")
        .unwrap();
    assert!((q_t["result"]["mean"].as_f64().unwrap() - 11.0).abs() < 1e-9);
}

#[tokio::test]
async fn cancel_submission_deletes_and_frees_table() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let mk = || FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: None,
        table_number: Some(1),
        student_results: vec![db::StudentResultInput {
            symbol: "g1".into(),
            value: 9.8,
            u_expanded: Some(0.1),
        }],
        student_comment: Some("un comentario".into()),
        practice_version: None,
    };
    let created = create_form_submission(&pool, &user, mk()).await.unwrap();

    assert_eq!(created.student_results.len(), 1);
    assert_eq!(created.members.len(), 1);

    assert!(db::find_existing_report(&pool, "p1-partes", &group.id, 1)
        .await
        .unwrap()
        .is_some());

    let existed = db::delete_submission(&pool, &created.id).await.unwrap();
    assert!(existed);

    assert!(db::submission_detail(&pool, &created.id)
        .await
        .unwrap()
        .is_none());

    assert!(db::find_existing_report(&pool, "p1-partes", &group.id, 1)
        .await
        .unwrap()
        .is_none());

    assert!(!db::delete_submission(&pool, &created.id).await.unwrap());

    let recreated = create_form_submission(&pool, &user, mk()).await.unwrap();
    assert_ne!(recreated.id, created.id);
    assert_eq!(recreated.table_number, Some(1));
}

#[tokio::test]
async fn update_form_submission_replaces_student_comment() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let measurements = vec![MeasurementInput {
        quantity_id: t_id,
        instrument_id: None,
        scale_id: None,
        values: vec![5.0, 5.2, 4.9],
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    }];
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: measurements.clone(),
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    let created = create_form_submission(&pool, &user, input).await.unwrap();
    assert_eq!(created.student_comment, None);

    let edited = update_form_submission(
        &pool,
        &created.id,
        &measurements,
        None,
        None,
        Some("faltó una réplica por corte de luz"),
    )
    .await
    .unwrap();
    assert_eq!(
        edited.student_comment.as_deref(),
        Some("faltó una réplica por corte de luz")
    );

    let cleared = update_form_submission(&pool, &created.id, &measurements, None, None, None)
        .await
        .unwrap();
    assert_eq!(cleared.student_comment, None);
}

#[tokio::test]
async fn analyze_rejects_foreign_quantity_id() {
    let (pool, _dir) = setup().await;
    let measurements = vec![MeasurementInput {
        quantity_id: "no-pertenece".into(),
        instrument_id: None,
        scale_id: None,
        values: vec![1.0],
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    }];
    assert!(analyze(&pool, "p1-estadistica", &measurements)
        .await
        .is_err());
}

#[tokio::test]
async fn create_form_submission_rejects_unknown_course_and_rolls_back() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let t_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap()
        .id
        .clone();
    let input = FormSubmissionInput {
        course_id: "curso-fantasma".into(),
        group_id: "grupo-fantasma".into(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![1.0],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    assert!(create_form_submission(&pool, &user, input).await.is_err());
    let subs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM submissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(subs.0, 0);
}

fn result(symbol: &str, formula: &str) -> PracticeResult {
    PracticeResult {
        id: format!("r-{symbol}"),
        practice_id: "p".into(),
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "u".into(),
        formula: formula.into(),
        position: 0,
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
        ..Default::default()
    }
}

const CAL_X: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
const CAL_Y: [f64; 5] = [0.21, 0.39, 0.62, 0.79, 1.02];

fn check_inverse_interpolation(xs: [f64; 5], ys: [f64; 5], sample_readings: &[f64], u_sample: f64) {
    let mut sample = quantity("A_m");
    sample.per_point = false;
    let quantities = vec![quantity("c"), quantity("A"), sample];
    let results = vec![
        result("c_m", "(A_m - intercept)/slope"),
        result("y0", "A_m"),
    ];
    let measurements = vec![
        measurement("c", &xs),
        measurement("A", &ys),
        measurement("A_m", sample_readings),
    ];
    let a = reg(&quantities, &[], &results, "c", "A", &measurements).unwrap();
    let fit = a.regression.unwrap();
    let y0_u = a.derived.iter().find(|d| d.symbol == "y0").unwrap().u;
    assert!(close(y0_u, u_sample, 1e-9));
    let c_m = a.derived.iter().find(|d| d.symbol == "c_m").unwrap();

    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let ss_xx = xs.iter().map(|x| (x - mean_x).powi(2)).sum::<f64>();
    let m = fit.slope;
    let s2 = xs
        .iter()
        .zip(ys)
        .map(|(x, y)| (y - mean_y - m * (x - mean_x)).powi(2))
        .sum::<f64>()
        / (n - 2.0);
    let y0 = sample_readings.iter().sum::<f64>() / sample_readings.len() as f64;
    let textbook = ((u_sample / m).powi(2)
        + s2 / m.powi(2) * (1.0 / n + (y0 - mean_y).powi(2) / (m * m * ss_xx)))
        .sqrt();

    assert!(close(
        c_m.value,
        mean_x + (y0 - mean_y) / m,
        1e-9 * mean_x.max(1.0)
    ));
    assert!(
        close(c_m.u, textbook, 1e-6 * textbook),
        "{} vs {textbook}",
        c_m.u
    );
    if mean_x.abs() > 1.0 {
        let independent = ((u_sample / m).powi(2)
            + (fit.u_intercept / m).powi(2)
            + ((y0 - fit.intercept) / (m * m) * fit.u_slope).powi(2))
        .sqrt();
        assert!((independent - textbook).abs() > 0.1 * textbook);
    }
}

#[test]
fn a_finite_value_with_no_uncertainty_gets_its_own_warning() {
    let mut zero = quantity("z");
    zero.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), zero];
    let results = vec![result("r", "math::sqrt(z)")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        measurement("z", &[-0.1, 0.1]),
    ];
    let a = reg(&quantities, &[], &results, "px", "py", &measurements).unwrap();
    let r = a.derived.iter().find(|d| d.symbol == "r").unwrap();
    assert_eq!(r.value, 0.0);
    assert!(!r.u.is_finite());
    let warning = a.warnings.iter().find(|w| w.contains("\"r\"")).unwrap();
    assert!(warning.contains("no se puede calcular"), "{warning}");
    assert!(!warning.contains("division por cero"), "{warning}");
}

#[test]
fn a_tied_maximum_takes_the_uncertainty_of_one_branch_and_warns() {
    let mut d1 = quantity("d1");
    d1.per_point = false;
    let mut d2 = quantity("d2");
    d2.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), d1, d2];
    let results = vec![
        result("r", "max(d1, d2)"),
        result("r2", "max(d2, d1)"),
        result("u2", "d2"),
    ];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        measurement("d1", &[12.4, 12.6]),
        measurement("d2", &[12.0, 13.0]),
    ];
    let a = reg(&quantities, &[], &results, "px", "py", &measurements).unwrap();
    let u = |s: &str| a.derived.iter().find(|d| d.symbol == s).unwrap().u;
    assert!(close(u("r"), u("u2"), 1e-12));
    assert!(close(u("r2"), u("u2"), 1e-12));
    let warning = a.warnings.iter().find(|w| w.contains("\"r\"")).unwrap();
    assert!(warning.contains("quiebre"), "{warning}");
}

#[test]
fn a_result_without_uncertainty_does_not_warn_about_it() {
    let mut zero = quantity("z");
    zero.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), zero];
    let mut r = result("r", "math::sqrt(z)");
    r.has_uncertainty = false;
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        measurement("z", &[-0.1, 0.1]),
    ];
    let a = reg(&quantities, &[], &[r], "px", "py", &measurements).unwrap();
    assert!(
        a.warnings.iter().all(|w| !w.contains("\"r\"")),
        "{:?}",
        a.warnings
    );
}

#[test]
fn inverse_interpolation_matches_textbook_uncertainty() {
    check_inverse_interpolation(CAL_X, CAL_Y, &[0.55, 0.57, 0.56], 0.01 / 3.0_f64.sqrt());
}

#[test]
fn inverse_interpolation_survives_a_large_x_offset() {
    let xs = CAL_X.map(|x| x + 1e8);
    check_inverse_interpolation(xs, CAL_Y, &[0.55, 0.57, 0.56], 0.01 / 3.0_f64.sqrt());
    check_inverse_interpolation(xs, CAL_Y, &[0.56], 0.0);
}

#[test]
fn inverse_interpolation_with_x_centered_near_zero() {
    let xs = [-2.0, -1.0, 0.0, 1.0, 2.0].map(|x| x + 1e-10);
    let ys = [-0.98, -0.01, 1.02, 1.98, 3.01];
    check_inverse_interpolation(xs, ys, &[2.0], 0.0);
}

#[test]
fn compute_regresion_fits_known_line() {
    let quantities = vec![quantity("px"), quantity("py")];
    let results = vec![result("m", "slope"), result("b0", "intercept")];
    let measurements = vec![
        measurement("px", &[0.0, 1.0, 2.0, 3.0]),
        measurement("py", &[1.0, 3.0, 5.0, 7.0]),
    ];
    let a = reg(&quantities, &[], &results, "px", "py", &measurements).unwrap();
    let reg = a.regression.unwrap();
    assert!(close(reg.slope, 2.0, 1e-9));
    assert!(close(reg.intercept, 1.0, 1e-9));
    assert!(close(reg.r_squared, 1.0, 1e-9));
    assert_eq!(reg.points.len(), 4);
    assert!(close(
        a.derived.iter().find(|d| d.symbol == "m").unwrap().value,
        2.0,
        1e-9
    ));
    assert!(close(
        a.derived.iter().find(|d| d.symbol == "b0").unwrap().value,
        1.0,
        1e-9
    ));
}

#[test]
fn compute_regresion_uses_per_point_intermediate_averaged_over_replicas() {
    let quantities = vec![quantity("V"), quantity("t"), quantity("py")];
    let intermediates = vec![PracticeIntermediate {
        id: "i1".into(),
        practice_id: "p1-estadistica".into(),
        position: 0,
        symbol: "Q".into(),
        name: "Caudal".into(),
        unit: "u".into(),
        formula: "V/t".into(),
        show_live: false,
        on_means: false,
    }];
    let rep = |groups: Vec<Vec<f64>>, id: &str| MeasurementInput {
        quantity_id: id.into(),
        instrument_id: None,
        scale_id: None,
        values: vec![],
        given_u: None,
        point_replicas: Some(groups),
        operator_replicas: None,
        operator_instruments: None,
    };
    let measurements = vec![
        rep(vec![vec![10.0, 10.0], vec![20.0, 20.0]], "q-V"),
        rep(vec![vec![1.0, 2.0], vec![1.0, 2.0]], "q-t"),
        measurement("py", &[100.0, 200.0]),
    ];
    let a = reg(&quantities, &intermediates, &[], "Q", "py", &measurements).unwrap();
    let reg = a.regression.unwrap();
    assert_eq!(reg.points, vec![(7.5, 100.0), (15.0, 200.0)]);
}

fn point_rep(id: &str, groups: Vec<Vec<f64>>) -> MeasurementInput {
    MeasurementInput {
        quantity_id: id.into(),
        instrument_id: None,
        scale_id: None,
        values: vec![],
        given_u: None,
        point_replicas: Some(groups),
        operator_replicas: None,
        operator_instruments: None,
    }
}

#[test]
fn intermediate_broadcasts_single_value_magnitudes_over_replicas() {
    let quantities = vec![quantity("h"), quantity("V"), quantity("t"), quantity("py")];
    let intermediates = vec![PracticeIntermediate {
        id: "i1".into(),
        practice_id: "p1-estadistica".into(),
        position: 0,
        symbol: "D".into(),
        name: "D".into(),
        unit: "u".into(),
        formula: "h*V/t".into(),
        show_live: false,
        on_means: false,
    }];
    let measurements = vec![
        measurement("h", &[2.0, 3.0]),
        point_rep("q-V", vec![vec![10.0, 10.0], vec![20.0, 20.0]]),
        point_rep("q-t", vec![vec![1.0, 2.0], vec![1.0, 2.0]]),
        measurement("py", &[100.0, 200.0]),
    ];
    let a = reg(&quantities, &intermediates, &[], "D", "py", &measurements).unwrap();
    assert_eq!(
        a.regression.unwrap().points,
        vec![(15.0, 100.0), (45.0, 200.0)]
    );
}

#[test]
fn intermediate_formula_can_use_constants() {
    let quantities = vec![quantity("r"), quantity("py")];
    let intermediates = vec![PracticeIntermediate {
        id: "i1".into(),
        practice_id: "p1-estadistica".into(),
        position: 0,
        symbol: "A".into(),
        name: "Area".into(),
        unit: "u".into(),
        formula: "pi*r*r".into(),
        show_live: false,
        on_means: false,
    }];
    let measurements = vec![
        measurement("r", &[2.0, 3.0]),
        measurement("py", &[10.0, 20.0]),
    ];
    let a = reg(&quantities, &intermediates, &[], "A", "py", &measurements).unwrap();
    let points = a.regression.unwrap().points;
    assert!(close(points[0].0, std::f64::consts::PI * 4.0, 1e-9));
    assert!(close(points[1].0, std::f64::consts::PI * 9.0, 1e-9));
}

#[test]
fn intermediate_rejects_mismatched_replica_counts() {
    let quantities = vec![quantity("V"), quantity("t"), quantity("py")];
    let intermediates = vec![PracticeIntermediate {
        id: "i1".into(),
        practice_id: "p1-estadistica".into(),
        position: 0,
        symbol: "Q".into(),
        name: "Q".into(),
        unit: "u".into(),
        formula: "V/t".into(),
        show_live: false,
        on_means: false,
    }];
    let measurements = vec![
        point_rep("q-V", vec![vec![10.0, 20.0], vec![10.0, 20.0]]),
        point_rep("q-t", vec![vec![1.0], vec![1.0]]),
        measurement("py", &[100.0, 200.0]),
    ];
    assert!(reg(&quantities, &intermediates, &[], "Q", "py", &measurements).is_err());
}

#[test]
fn intermediate_can_reference_an_earlier_intermediate() {
    let quantities = vec![quantity("V"), quantity("t"), quantity("py")];
    let intermediates = vec![
        PracticeIntermediate {
            id: "i1".into(),
            practice_id: "p1-estadistica".into(),
            position: 0,
            symbol: "Q".into(),
            name: "Q".into(),
            unit: "u".into(),
            formula: "V/t".into(),
            show_live: false,
            on_means: false,
        },
        PracticeIntermediate {
            id: "i2".into(),
            practice_id: "p1-estadistica".into(),
            position: 1,
            symbol: "R".into(),
            name: "R".into(),
            unit: "u".into(),
            formula: "Q*2".into(),
            show_live: false,
            on_means: false,
        },
    ];
    let measurements = vec![
        point_rep("q-V", vec![vec![10.0, 10.0], vec![20.0, 20.0]]),
        point_rep("q-t", vec![vec![1.0, 2.0], vec![1.0, 2.0]]),
        measurement("py", &[100.0, 200.0]),
    ];
    let a = reg(&quantities, &intermediates, &[], "R", "py", &measurements).unwrap();
    assert_eq!(
        a.regression.unwrap().points,
        vec![(15.0, 100.0), (30.0, 200.0)]
    );
}

#[test]
fn compute_regresion_uses_pi_and_sqrt_in_axis_formulas() {
    let quantities = vec![quantity("f"), quantity("a")];
    let results = vec![result("tau", "slope")];
    let measurements = vec![
        measurement("f", &[1.0, 2.0, 3.0]),
        measurement("a", &[4.0, 9.0, 16.0]),
    ];
    let analysis = compute_regresion(
        &quantities,
        &[],
        &results,
        &[],
        &[],
        &HashMap::new(),
        "2*pi*f",
        "math::sqrt(a)",
        &measurements,
    )
    .unwrap();
    let reg = analysis.regression.unwrap();
    assert!(close(reg.slope, 1.0 / (2.0 * std::f64::consts::PI), 1e-9));
    assert!(close(reg.intercept, 1.0, 1e-9));
    assert_eq!(reg.x_label, "2*pi*f");
    assert_eq!(reg.y_label, "math::sqrt(a)");
    assert!(close(
        analysis
            .derived
            .iter()
            .find(|d| d.symbol == "tau")
            .unwrap()
            .value,
        1.0 / (2.0 * std::f64::consts::PI),
        1e-9
    ));
}

#[test]
fn compute_regresion_shared_scalar_measurand_and_point_result() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let results = vec![result("m", "slope * c")];
    let point_results = vec![PracticePointResult {
        id: "pr1".into(),
        practice_id: "p".into(),
        position: 0,
        symbol: "Re".into(),
        name: "Reynolds".into(),
        unit: "".into(),
        formula: "px * m".into(),
    }];
    let measurements = vec![
        measurement("px", &[1.0, 2.0]),
        measurement("py", &[2.0, 4.0]),
        measurement("c", &[10.0]),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &results,
        &point_results,
        &[],
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    assert!(close(
        a.derived.iter().find(|d| d.symbol == "m").unwrap().value,
        20.0,
        1e-9
    ));
    let re = a.point_results.iter().find(|p| p.symbol == "Re").unwrap();
    assert_eq!(re.values.len(), 2);
    assert!(close(re.values[0], 20.0, 1e-9));
    assert!(close(re.values[1], 40.0, 1e-9));
}

#[test]
fn compute_regresion_aggregates_use_endpoints_measurands_and_chain() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let results = vec![result("m", "slope * c")];
    let agg = |symbol: &str, formula: &str| PracticeAggregate {
        id: format!("a-{symbol}"),
        practice_id: "p".into(),
        position: 0,
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "".into(),
        formula: formula.into(),
        is_final: false,
        has_uncertainty: true,
    };
    let aggregates = vec![
        agg("ep", "px_first + px_last"),
        agg("mid", "px_first2 + px_last2"),
        agg("g", "m + slope"),
        agg("chained", "ep + g"),
    ];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.0, 6.0]),
        measurement("c", &[10.0]),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &results,
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    let val = |sym: &str| {
        a.aggregates
            .iter()
            .find(|x| x.symbol == sym)
            .unwrap_or_else(|| panic!("falta agregado {sym}"))
            .value
    };
    assert!(close(val("ep"), 4.0, 1e-9));
    assert!(close(val("mid"), 4.0, 1e-9));
    assert!(close(val("g"), 22.0, 1e-9));
    assert!(close(val("chained"), 26.0, 1e-9));
}

#[test]
fn compute_regresion_aggregate_non_finite_warns() {
    let quantities = vec![quantity("px"), quantity("py")];
    let aggregates = vec![PracticeAggregate {
        id: "a-bad".into(),
        practice_id: "p".into(),
        position: 0,
        symbol: "bad".into(),
        name: "Agregado roto".into(),
        unit: "".into(),
        formula: "1 / (px_first - px_first)".into(),
        is_final: false,
        has_uncertainty: true,
    }];
    let measurements = vec![
        measurement("px", &[1.0, 2.0]),
        measurement("py", &[2.0, 4.0]),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &[],
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    assert!(!a.aggregates[0].value.is_finite());
    assert!(
        a.warnings
            .iter()
            .any(|w| w.contains("bad") && w.contains("no dio un valor finito")),
        "debe avisar del agregado no finito: {:?}",
        a.warnings
    );
}

#[test]
fn compute_regresion_aggregate_endpoints_use_own_series_not_contexts() {
    let quantities = vec![quantity("px"), quantity("py"), quantity("z")];
    let agg = |symbol: &str, formula: &str| PracticeAggregate {
        id: format!("a-{symbol}"),
        practice_id: "p".into(),
        position: 0,
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "".into(),
        formula: formula.into(),
        is_final: false,
        has_uncertainty: true,
    };
    let aggregates = vec![
        agg("z_end", "z_last"),
        agg("z_end2", "z_last2"),
        agg("z_beg", "z_first"),
        agg("z_beg2", "z_first2"),
    ];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.0, 6.0]),
        measurement("z", &[10.0, 20.0, 30.0, 40.0]),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &[],
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    let val = |sym: &str| {
        a.aggregates
            .iter()
            .find(|x| x.symbol == sym)
            .unwrap_or_else(|| panic!("falta agregado {sym}"))
            .value
    };
    assert!(
        close(val("z_end"), 40.0, 1e-9),
        "z_last debe ser el último de la serie de z"
    );
    assert!(
        close(val("z_end2"), 30.0, 1e-9),
        "z_last2 debe ser el penúltimo de la serie de z"
    );
    assert!(close(val("z_beg"), 10.0, 1e-9));
    assert!(close(val("z_beg2"), 20.0, 1e-9));
    assert!(
        a.warnings
            .iter()
            .any(|w| w.contains("\"z\"") && w.contains("4 punto") && w.contains("3 del ajuste")),
        "debe avisar del desalineamiento de z: {:?}",
        a.warnings
    );
}

#[test]
fn compute_regresion_aggregate_endpoint_misalignment_warns_only_if_referenced() {
    let quantities = vec![quantity("px"), quantity("py"), quantity("z")];
    let aggregates = vec![PracticeAggregate {
        id: "a-s".into(),
        practice_id: "p".into(),
        position: 0,
        symbol: "s".into(),
        name: "s".into(),
        unit: "".into(),
        formula: "slope".into(),
        is_final: false,
        has_uncertainty: true,
    }];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.0, 6.0]),
        measurement("z", &[10.0, 20.0, 30.0, 40.0]),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &[],
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    assert!(
        !a.warnings.iter().any(|w| w.contains("\"z\"")),
        "no debe avisar de z si ningún extremo de z se usa: {:?}",
        a.warnings
    );
}

#[test]
fn intermediate_can_use_shared_scalar_across_points() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let intermediates = vec![PracticeIntermediate {
        id: "i1".into(),
        practice_id: "p".into(),
        position: 0,
        symbol: "D".into(),
        name: "D".into(),
        unit: "u".into(),
        formula: "px/c".into(),
        show_live: false,
        on_means: false,
    }];
    let measurements = vec![
        measurement("px", &[1.0, 2.0]),
        measurement("py", &[1.0, 2.0]),
        measurement("c", &[10.0]),
    ];
    let a = compute_regresion(
        &quantities,
        &intermediates,
        &[],
        &[],
        &[],
        &HashMap::new(),
        "D",
        "py",
        &measurements,
    )
    .unwrap();
    assert_eq!(a.regression.unwrap().points, vec![(0.1, 1.0), (0.2, 2.0)]);
}

#[test]
fn shared_scalar_with_multiple_readings_collapses_to_one_value() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let intermediates = vec![PracticeIntermediate {
        id: "i1".into(),
        practice_id: "p".into(),
        position: 0,
        symbol: "D".into(),
        name: "D".into(),
        unit: "u".into(),
        formula: "px/c".into(),
        show_live: false,
        on_means: false,
    }];
    let measurements = vec![
        measurement("px", &[1.0, 2.0]),
        measurement("py", &[1.0, 2.0]),
        measurement("c", &[10.0, 20.0]),
    ];
    let pts = compute_regresion(
        &quantities,
        &intermediates,
        &[],
        &[],
        &[],
        &HashMap::new(),
        "D",
        "py",
        &measurements,
    )
    .unwrap()
    .regression
    .unwrap()
    .points;
    assert!(close(pts[0].0, 1.0 / 15.0, 1e-9));
    assert!(close(pts[1].0, 2.0 / 15.0, 1e-9));
}

#[test]
fn compute_regresion_needs_at_least_two_points() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![measurement("px", &[1.0]), measurement("py", &[2.0])];
    assert!(reg(&quantities, &[], &[], "px", "py", &measurements).is_err());
}

#[test]
fn compute_curva_builds_scatter_without_fit() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[4.0, 9.0, 16.0]),
    ];
    let (a, contexts) =
        compute_curva(&quantities, &[], &[curve("px", "py", false)], &measurements).unwrap();
    assert_eq!(contexts.len(), 3);
    assert!(a.regression.is_none());
    assert!(a.derived.is_empty());
    assert_eq!(a.scatters.len(), 1);
    let scatter = &a.scatters[0];
    assert_eq!(scatter.points, vec![(1.0, 4.0), (2.0, 9.0), (3.0, 16.0)]);
    assert_eq!(scatter.x_label, "px");
    assert_eq!(scatter.y_label, "py");
    assert!(!scatter.x_log);
}

#[test]
fn compute_curva_builds_one_scatter_per_curve() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[4.0, 9.0, 16.0]),
    ];
    let (a, _) = compute_curva(
        &quantities,
        &[],
        &[curve("px", "py", false), curve("py", "px", false)],
        &measurements,
    )
    .unwrap();
    assert_eq!(a.scatters.len(), 2);
    assert_eq!(
        a.scatters[0].points,
        vec![(1.0, 4.0), (2.0, 9.0), (3.0, 16.0)]
    );
    assert_eq!(a.scatters[0].x_label, "px");
    assert_eq!(
        a.scatters[1].points,
        vec![(4.0, 1.0), (9.0, 2.0), (16.0, 3.0)]
    );
    assert_eq!(a.scatters[1].x_label, "py");
}

#[test]
fn compute_curva_needs_at_least_two_points() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![measurement("px", &[1.0]), measurement("py", &[2.0])];
    assert!(compute_curva(&quantities, &[], &[curve("px", "py", false)], &measurements).is_err());
}

#[test]
fn compute_curva_rejects_non_positive_x_when_log() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        measurement("px", &[0.0, 10.0]),
        measurement("py", &[1.0, 2.0]),
    ];
    assert!(compute_curva(&quantities, &[], &[curve("px", "py", true)], &measurements).is_err());
}

#[test]
fn build_points_ignores_quantities_not_in_axes() {
    let quantities = vec![quantity("px"), quantity("py"), quantity("aux")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[4.0, 5.0, 6.0]),
    ];
    let (a, _) =
        compute_curva(&quantities, &[], &[curve("px", "py", false)], &measurements).unwrap();
    assert_eq!(
        a.scatters[0].points,
        vec![(1.0, 4.0), (2.0, 5.0), (3.0, 6.0)]
    );
}

#[test]
fn build_points_averages_per_point_replicas() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0]),
        MeasurementInput {
            quantity_id: "q-py".into(),
            instrument_id: None,
            scale_id: None,
            values: vec![],
            given_u: None,
            point_replicas: Some(vec![vec![3.0, 5.0], vec![8.0, 10.0, 12.0]]),
            operator_replicas: None,
            operator_instruments: None,
        },
    ];
    let a = reg(&quantities, &[], &[], "px", "py", &measurements).unwrap();
    let reg = a.regression.unwrap();
    assert_eq!(reg.points, vec![(1.0, 4.0), (2.0, 10.0)]);
    assert!(close(reg.slope, 6.0, 1e-9));
}

fn measurement_for(
    def: &crate::practices::PracticeDefinition,
    symbol: &str,
    values: &[f64],
) -> MeasurementInput {
    let id = def
        .quantities
        .iter()
        .find(|q| q.symbol == symbol)
        .unwrap()
        .id
        .clone();
    MeasurementInput {
        quantity_id: id,
        instrument_id: None,
        scale_id: None,
        values: values.to_vec(),
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    }
}

#[tokio::test]
async fn seeded_fluidos_1_computes_regression_mu_and_reynolds() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "fluidos-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
    assert_eq!(def.intermediates.len(), 1, "Q");
    assert_eq!(def.point_results.len(), 1, "Re");
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let mk = |sym: &str,
              values: Vec<f64>,
              given_u: Option<f64>,
              point_replicas: Option<Vec<Vec<f64>>>| {
        MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values,
            given_u,
            point_replicas,
            operator_replicas: None,
            operator_instruments: None,
        }
    };
    let measurements = vec![
        mk("h", vec![0.30, 0.10], None, None),
        mk(
            "V",
            vec![],
            None,
            Some(vec![vec![1e-4, 1e-4], vec![1e-4, 1e-4]]),
        ),
        mk(
            "t",
            vec![],
            None,
            Some(vec![vec![10.0, 10.0], vec![20.0, 20.0]]),
        ),
        mk("R", vec![5e-4], Some(1e-6), None),
        mk("L", vec![0.10], Some(1e-4), None),
        mk("g", vec![9.8], Some(0.01), None),
        mk("rho", vec![1000.0], None, None),
        mk("Temp", vec![20.0], None, None),
    ];
    let a = analyze(&pool, "fluidos-1", &measurements).await.unwrap();
    let reg = a.regression.expect("hay ajuste");
    assert_eq!(reg.points.len(), 2);
    let mu = a.derived.iter().find(|d| d.symbol == "mu").expect("mu");
    assert!(mu.value.is_finite() && mu.value > 0.0);
    let re = a
        .point_results
        .iter()
        .find(|p| p.symbol == "Re")
        .expect("Re");
    assert_eq!(re.values.len(), 2);
    assert!(re.values.iter().all(|v| v.is_finite()));
}

#[tokio::test]
async fn seeded_viscosidad_computes_regression_mu_and_reynolds() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "viscosidad")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
    let symbols: Vec<&str> = def
        .intermediates
        .iter()
        .map(|i| i.symbol.as_str())
        .collect();
    assert_eq!(symbols, ["v"], "solo la columna en vivo");
    assert!(
        def.intermediates[0].on_means,
        "v = dx/t sobre el tiempo medio, como el eje"
    );
    assert_eq!(def.y_formula.as_deref(), Some("dx/t"));
    assert_eq!(def.point_results.len(), 1, "Re");
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let mk = |sym: &str,
              values: Vec<f64>,
              given_u: Option<f64>,
              point_replicas: Option<Vec<Vec<f64>>>| {
        MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values,
            given_u,
            point_replicas,
            operator_replicas: None,
            operator_instruments: None,
        }
    };
    let measurements = vec![
        mk("R", vec![1e-3, 2e-3], None, None),
        mk(
            "t",
            vec![],
            None,
            Some(vec![vec![20.0, 20.0, 20.0], vec![5.0, 5.0, 5.0]]),
        ),
        mk("dx", vec![0.20], Some(1e-3), None),
        mk("rho_e", vec![7800.0], None, None),
        mk("rho_f", vec![1260.0], None, None),
        mk("g", vec![9.8], Some(0.01), None),
        mk("Temp", vec![20.0], None, None),
    ];
    let a = analyze(&pool, "viscosidad", &measurements).await.unwrap();
    let reg = a.regression.expect("hay ajuste");
    assert_eq!(reg.points.len(), 2);
    assert!(close(reg.points[0].0, 1e-6, 1e-12));
    assert!(close(reg.points[0].1, 0.01, 1e-9));
    let mu = a.derived.iter().find(|d| d.symbol == "mu").expect("mu");
    assert!(mu.value.is_finite());
    let re = a
        .point_results
        .iter()
        .find(|p| p.symbol == "Re")
        .expect("Re");
    assert_eq!(re.values.len(), 2);
    assert!(re.values.iter().all(|v| v.is_finite()));
}

#[tokio::test]
async fn analyze_routes_curva_to_scatter() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    drop_parts(&pool, "p1-partes").await;
    crate::practices::set_analysis_kind(&pool, "p1-partes", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-partes",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let measurements = vec![
        measurement_for(&def, "T1", &[1.0, 2.0, 3.0]),
        measurement_for(&def, "T2", &[4.0, 5.0, 6.0]),
    ];
    let analysis = analyze(&pool, "p1-partes", &measurements).await.unwrap();
    assert!(analysis.regression.is_none());
    assert_eq!(analysis.scatters.len(), 1);
    assert_eq!(
        analysis.scatters[0].points,
        vec![(1.0, 4.0), (2.0, 5.0), (3.0, 6.0)]
    );
}

#[tokio::test]
async fn analyze_curva_graphs_each_curve_in_the_list() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    drop_parts(&pool, "p1-partes").await;
    crate::practices::set_analysis_kind(&pool, "p1-partes", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-partes",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-partes",
        crate::practices::CurveInput {
            x_formula: "T2".into(),
            y_formula: "T1".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let measurements = vec![
        measurement_for(&def, "T1", &[1.0, 2.0, 3.0]),
        measurement_for(&def, "T2", &[4.0, 5.0, 6.0]),
    ];
    let analysis = analyze(&pool, "p1-partes", &measurements).await.unwrap();
    assert_eq!(analysis.scatters.len(), 2);
    assert_eq!(analysis.scatters[0].x_label, "T1");
    assert_eq!(analysis.scatters[0].y_label, "T2");
    assert_eq!(
        analysis.scatters[0].points,
        vec![(1.0, 4.0), (2.0, 5.0), (3.0, 6.0)]
    );
    assert_eq!(analysis.scatters[1].x_label, "T2");
    assert_eq!(
        analysis.scatters[1].points,
        vec![(4.0, 1.0), (5.0, 2.0), (6.0, 3.0)]
    );
}

#[tokio::test]
async fn analyze_curva_without_curves_errors() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    crate::practices::set_analysis_kind(&pool, "p1-partes", "curva")
        .await
        .unwrap();
    let by_part = analyze(&pool, "p1-partes", &[]).await.unwrap();
    assert!(by_part
        .warnings
        .iter()
        .any(|w| w.contains("No se pudo calcular la parte") && w.contains("curvas")));
    assert!(!by_part.parts.is_empty());

    drop_parts(&pool, "p1-partes").await;
    let result = analyze(&pool, "p1-partes", &[]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn analyze_ca_rlc_matches_series_rlc_theory() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "ca-rlc")
        .await
        .unwrap()
        .unwrap();
    let omega = 2.0 * std::f64::consts::PI * 1000.0;
    let xl = omega * 0.1;
    let xc = 1.0 / (omega * 1e-6);
    let z = ((100.0f64).powi(2) + (xl - xc).powi(2)).sqrt();
    let i_teo = 5.0 / z;
    let phi_teo = ((xl - xc) / 100.0).atan();
    let sin_r = -phi_teo.sin();
    let sin_c = -(phi_teo + std::f64::consts::FRAC_PI_2).sin();
    let sin_l = (std::f64::consts::FRAC_PI_2 - phi_teo).sin();

    let mut measurements = Vec::new();
    let mut push = |symbol: &str, value: f64| {
        let id = def
            .quantities
            .iter()
            .find(|q| q.symbol == symbol)
            .unwrap()
            .id
            .clone();
        measurements.push(MeasurementInput {
            quantity_id: id,
            instrument_id: None,
            scale_id: None,
            values: vec![value],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        });
    };
    push("R", 100.0);
    push("C", 1e-6);
    push("L", 0.1);
    push("Vg", 5.0);
    push("f_trabajo", 1000.0);
    push("VRpp", 2.0 * i_teo * 100.0);
    push("VCpp", 2.0 * i_teo * xc);
    push("VLpp", 2.0 * i_teo * xl);
    push("a_R", 1.0);
    push("b_R", sin_r);
    push("a_C", 1.0);
    push("b_C", sin_c);
    push("a_L", 1.0);
    push("b_L", sin_l);

    let analysis = analyze(&pool, "ca-rlc", &measurements).await.unwrap();
    let get = |symbol: &str| -> f64 {
        analysis
            .derived
            .iter()
            .find(|d| d.symbol == symbol)
            .unwrap()
            .value
    };

    assert!(close(
        get("f_res"),
        1.0 / (2.0 * std::f64::consts::PI * (0.1f64 * 1e-6).sqrt()),
        1e-6
    ));
    assert!(close(get("I_teo"), i_teo, 1e-9));
    assert!(close(get("VR_teo"), i_teo * 100.0, 1e-9));
    assert!(close(get("VR_exp"), i_teo * 100.0, 1e-9));
    assert!(close(get("VC_teo"), i_teo * xc, 1e-9));
    assert!(close(get("VC_exp"), i_teo * xc, 1e-9));
    assert!(close(get("VL_teo"), i_teo * xl, 1e-9));
    assert!(close(get("VL_exp"), i_teo * xl, 1e-9));
    let to_deg = 180.0 / std::f64::consts::PI;
    assert!(close(get("phiR_teo"), -phi_teo * to_deg, 1e-6));
    assert!(close(get("phiR_exp"), -phi_teo * to_deg, 1e-4));
    assert!(close(
        get("phiC_teo"),
        (-phi_teo - std::f64::consts::FRAC_PI_2) * to_deg,
        1e-6
    ));
    assert!(close(get("phiC_exp"), sin_c.asin() * to_deg, 1e-4));
    assert!(close(
        get("phiL_teo"),
        (-phi_teo + std::f64::consts::FRAC_PI_2) * to_deg,
        1e-6
    ));
    assert!(close(get("phiL_exp"), sin_l.asin() * to_deg, 1e-4));
}

#[tokio::test]
async fn analyze_hidrostatica_torque_cero_avisa_sin_romper_la_serializacion() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "hidrostatica")
        .await
        .unwrap()
        .unwrap();
    let measurements = hidrostatica_input(&def, |symbol, _| {
        def.quantities
            .iter()
            .find(|q| q.symbol == symbol)
            .and_then(|q| q.default_value)
            .unwrap_or(1.0)
    });

    let analysis = analyze(&pool, "hidrostatica", &measurements).await.unwrap();
    let warnings: Vec<&String> = analysis
        .parts
        .iter()
        .flat_map(|p| &p.analysis.warnings)
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("(rho =")),
        "falta la advertencia de valor no finito para rho: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("promedio de rho")),
        "falta la advertencia de valor no finito para rho_medio: {warnings:?}"
    );
    assert!(!warnings.iter().any(|w| w.contains("(E =")));
    let found = by_determination(&analysis);
    for symbol in ["rho1", "rho2", "rho3", "rho_medio"] {
        assert!(!found[symbol].is_finite(), "{symbol}");
    }
    let json = serde_json::to_string(&analysis).unwrap();
    assert!(json.contains("rho_medio"));
}

#[tokio::test]
async fn analyze_hidrostatica_matches_arquimedes_and_wilhelmy() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "hidrostatica")
        .await
        .unwrap()
        .unwrap();

    let g_ac = 9.80;
    let m_goma = 12.5;
    let rho_f = 0.998;
    let torque = 1.0 * 3.0 + 2.0 * 7.0;
    let empuje = (torque / 10.0) * g_ac / 1000.0;
    let rho_goma = rho_f * m_goma * 10.0 * 1000.0 / torque;

    let gamma = 0.5 * g_ac * 4.0 * 100.0 / (2.0 * 5.0 * 10.0 * 1000.0);

    let mut measurements = hidrostatica_input(&def, |symbol, _| match symbol {
        "m_goma" => m_goma,
        "rho_f" => rho_f,
        "g_ac" => g_ac,
        "b_E" => 10.0,
        "m_3" => 1.0,
        "m_7" => 2.0,
        "l" => 5.0,
        "d" => 10.0,
        "mw" => 0.5,
        "dw" => 4.0,
        b if b.starts_with('b') => b[1..].parse().unwrap(),
        _ => 0.0,
    });

    db::seed_academic(&pool).await.unwrap();
    crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
        .await
        .unwrap();
    let (instrument_id, scale_id): (String, String) = sqlx::query_as(
        "SELECT instrument_id, id FROM instrument_scales WHERE label = '0.1 g (calibracion 3 %)'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let goma_id = &def
        .quantities
        .iter()
        .find(|q| q.symbol == "m_goma")
        .unwrap()
        .id;
    let goma = measurements
        .iter_mut()
        .find(|m| &m.quantity_id == goma_id)
        .unwrap();
    goma.instrument_id = Some(instrument_id);
    goma.scale_id = Some(scale_id);

    let analysis = analyze(&pool, "hidrostatica", &measurements).await.unwrap();
    let found = by_determination(&analysis);
    let analysis = flattened(analysis);
    let u_b_goma = analysis
        .quantities
        .iter()
        .find(|q| q.symbol == "m_goma")
        .unwrap()
        .result
        .u_b;
    let esperado = ((0.1 / (2.0 * 3f64.sqrt())).powi(2) + (0.03 * m_goma).powi(2)).sqrt();
    assert!(
        close(u_b_goma, esperado, 1e-12),
        "u_B de la balanza sin la calibracion: {u_b_goma} != {esperado}"
    );
    assert!(
        analysis.warnings.is_empty(),
        "no deberia haber advertencias: {:?}",
        analysis.warnings
    );
    let get = |symbol: &str| -> f64 {
        *found
            .get(symbol)
            .unwrap_or_else(|| panic!("falta el mensurando {symbol}"))
    };

    for k in 1..=3 {
        assert!(close(get(&format!("E{k}")), empuje, 1e-12));
        assert!(close(get(&format!("rho{k}")), rho_goma, 1e-12));
        assert!(close(get(&format!("gamma{k}")), gamma, 1e-12));
    }
    assert!(close(get("E_medio"), empuje, 1e-12));
    assert!(close(get("rho_medio"), rho_goma, 1e-12));
    assert!(close(get("gamma_medio"), gamma, 1e-12));
    assert!(rho_goma > rho_f * 1000.0);
}

#[tokio::test]
async fn hidrostatica_un_slot_vacio_no_aporta_incertidumbre_a_su_medida() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "hidrostatica")
        .await
        .unwrap()
        .unwrap();
    db::seed_academic(&pool).await.unwrap();
    crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
        .await
        .unwrap();
    let (instrument_id, scale_id): (String, String) = sqlx::query_as(
        "SELECT instrument_id, id FROM instrument_scales WHERE label = '0.1 g (calibracion 3 %)'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let m8_id = &def
        .quantities
        .iter()
        .find(|q| q.symbol == "m_8")
        .unwrap()
        .id;
    let input = || {
        hidrostatica_input(&def, |symbol, k| match (symbol, k) {
            ("m_goma", _) => 12.5,
            ("rho_f", _) => 0.998,
            ("g_ac", _) => 9.8,
            ("b_E", _) => 10.0,
            ("m_3", _) => 1.0,
            ("m_7", 1 | 3) => 2.0,
            ("m_8", 2) => 2.0,
            ("l", _) => 5.0,
            ("d", _) => 10.0,
            ("mw", _) => 0.5,
            ("dw", _) => 4.0,
            (b, _) if b.starts_with('b') => b[1..].parse().unwrap(),
            _ => 0.0,
        })
    };
    let u_of = |analysis: &FormAnalysis, symbol: &str, k: usize| -> f64 {
        analysis
            .parts
            .iter()
            .flat_map(|p| p.analysis.determinations.get(k))
            .flat_map(|d| &d.derived)
            .find(|d| d.symbol == symbol)
            .unwrap()
            .u
    };

    let with_m8 = |choices: Vec<Option<(&str, &str)>>| {
        let mut measurements = input();
        let m8 = measurements
            .iter_mut()
            .find(|m| &m.quantity_id == m8_id)
            .unwrap();
        m8.operator_instruments = Some(
            choices
                .into_iter()
                .map(|c| crate::computation::InstrumentChoice {
                    instrument_id: c.map(|(i, _)| i.into()),
                    scale_id: c.map(|(_, s)| s.into()),
                })
                .collect(),
        );
        measurements
    };
    let balanza = Some((instrument_id.as_str(), scale_id.as_str()));
    let sin_balanza = analyze(&pool, "hidrostatica", &input()).await.unwrap();
    let con_balanza = analyze(&pool, "hidrostatica", &with_m8(vec![None, balanza, None]))
        .await
        .unwrap();
    let en_todas = analyze(&pool, "hidrostatica", &with_m8(vec![balanza; 3]))
        .await
        .unwrap();

    assert!(u_of(&con_balanza, "E", 1) > u_of(&sin_balanza, "E", 1));
    assert!(
        u_of(&en_todas, "E", 0) > u_of(&sin_balanza, "E", 0),
        "una lectura de 0 con instrumento conserva su incertidumbre"
    );
    for k in [0, 2] {
        assert_eq!(
            u_of(&con_balanza, "E", k),
            u_of(&sin_balanza, "E", k),
            "medida {}",
            k + 1
        );
        assert_eq!(
            u_of(&con_balanza, "rho", k),
            u_of(&sin_balanza, "rho", k),
            "medida {}",
            k + 1
        );
    }
}

#[tokio::test]
async fn analyze_rejects_a_slot_outside_the_balance() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "hidrostatica")
        .await
        .unwrap()
        .unwrap();
    let dw = def.quantities.iter().find(|q| q.symbol == "dw").unwrap();
    assert_eq!(dw.slot_count, Some(9));
    let with_value = |value: f64| {
        vec![MeasurementInput {
            quantity_id: dw.id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![],
            given_u: None,
            point_replicas: None,
            operator_replicas: Some(vec![vec![4.0], vec![value], vec![4.0]]),
            operator_instruments: None,
        }]
    };
    for bad in [0.0, 4.5, 10.0] {
        let err = analyze(&pool, "hidrostatica", &with_value(bad))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("entero de 1 a 9"), "{bad}: {err}");
    }
    assert!(analyze(&pool, "hidrostatica", &with_value(4.0))
        .await
        .is_ok());
}

#[tokio::test]
async fn analyze_validates_the_instruments_per_determination() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "hidrostatica")
        .await
        .unwrap()
        .unwrap();
    let id_of = |symbol: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == symbol)
            .unwrap()
            .id
            .clone()
    };
    db::seed_academic(&pool).await.unwrap();
    crate::instruments::seed_instruments(&pool, "fisica-experimental-i-2026")
        .await
        .unwrap();
    let (instrument_id, scale_id): (String, String) = sqlx::query_as(
        "SELECT instrument_id, id FROM instrument_scales WHERE label = '0.1 g (calibracion 3 %)'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let with = |shared: Option<&str>, groups: usize, instruments: usize| {
        vec![MeasurementInput {
            quantity_id: id_of("m_8"),
            instrument_id: shared.map(|_| instrument_id.clone()),
            scale_id: shared.map(Into::into),
            values: vec![],
            given_u: None,
            point_replicas: None,
            operator_replicas: Some(vec![vec![2.0]; groups]),
            operator_instruments: Some(vec![Default::default(); instruments]),
        }]
    };
    let m8 = |instruments: usize| with(None, 3, instruments);
    let err_of = |measurements: Vec<MeasurementInput>| {
        let pool = pool.clone();
        async move {
            analyze(&pool, "hidrostatica", &measurements)
                .await
                .unwrap_err()
                .to_string()
        }
    };

    assert!(analyze(&pool, "hidrostatica", &m8(3)).await.is_ok());
    let partial = analyze(&pool, "hidrostatica", &with(Some(&scale_id), 1, 1))
        .await
        .unwrap();
    assert!(
        !partial.warnings.iter().any(|w| w.contains("no existe")),
        "las medidas sin instrumento propio usan el compartido: {:?}",
        partial.warnings
    );
    let err = err_of(with(Some("escala-borrada"), 3, 3)).await;
    assert!(err.contains("la escala seleccionada no existe"), "{err}");
    let err = err_of(m8(4)).await;
    assert!(
        err.contains("instrumentos para más determinaciones"),
        "{err}"
    );

    let given = vec![MeasurementInput {
        quantity_id: id_of("b_E"),
        instrument_id: None,
        scale_id: None,
        values: vec![10.0],
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: Some(vec![]),
    }];
    let err = err_of(given).await;
    assert!(
        err.contains("no lleva un instrumento por determinación"),
        "{err}"
    );

    let grouped = vec![MeasurementInput {
        quantity_id: id_of("m_goma"),
        instrument_id: None,
        scale_id: None,
        values: vec![],
        given_u: None,
        point_replicas: None,
        operator_replicas: Some(vec![vec![12.5], vec![12.6]]),
        operator_instruments: None,
    }];
    let err = err_of(grouped).await;
    assert!(err.contains("no se carga por determinación"), "{err}");
}

#[test]
fn compute_skips_a_result_it_cannot_evaluate_and_keeps_the_rest() {
    let quantities = vec![quantity("l"), quantity("a")];
    let results = vec![result("ok", "l*a"), result("roto", "l*zzz")];
    let measurements = vec![measurement("l", &[2.0]), measurement("a", &[3.0])];
    let analysis = compute(&quantities, &results, &HashMap::new(), &measurements, None).unwrap();
    let symbols: Vec<&str> = analysis.derived.iter().map(|d| d.symbol.as_str()).collect();
    assert_eq!(symbols, ["ok"]);
    assert!(analysis
        .warnings
        .iter()
        .any(|w| w.contains("roto") && w.contains("zzz")));
}

#[test]
fn a_point_result_that_uses_a_skipped_result_is_skipped_too() {
    let quantities = vec![quantity("f"), quantity("a")];
    let results = vec![result("tau", "slope * zzz")];
    let point_results = vec![PracticePointResult {
        id: "pr".into(),
        practice_id: "p1-estadistica".into(),
        position: 0,
        symbol: "w".into(),
        name: "w".into(),
        unit: "".into(),
        formula: "f * tau".into(),
    }];
    let measurements = vec![
        measurement("f", &[1.0, 2.0, 3.0]),
        measurement("a", &[4.0, 9.0, 16.0]),
    ];
    let analysis = compute_regresion(
        &quantities,
        &[],
        &results,
        &point_results,
        &[],
        &HashMap::new(),
        "f",
        "a",
        &measurements,
    )
    .unwrap();
    assert!(analysis.derived.is_empty());
    assert!(analysis.point_results.is_empty());
    assert!(analysis.warnings.iter().any(|w| w.contains("tau")));
    assert!(analysis
        .warnings
        .iter()
        .any(|w| w.contains("derivada por punto w")));
}

#[test]
fn chained_results_propagate_against_the_inputs() {
    let x = PracticeQuantity {
        per_point: false,
        ..quantity("x")
    };
    let results = vec![
        result("S", "A + B"),
        result("A", "2*x"),
        result("B", "3*x"),
        result("E", "5*x"),
    ];
    let a = compute(
        &[x],
        &results,
        &HashMap::new(),
        &[measurement("x", &[1.0, 2.0, 3.0])],
        None,
    )
    .unwrap();
    let by = |s: &str| a.derived.iter().find(|d| d.symbol == s).unwrap();
    assert!(close(by("S").value, by("E").value, 1e-9));
    assert!(close(by("S").u, by("E").u, 1e-12));
    assert!(by("S").u > (by("A").u.powi(2) + by("B").u.powi(2)).sqrt() + 1e-6);
    let order: Vec<&str> = a.derived.iter().map(|d| d.symbol.as_str()).collect();
    assert_eq!(order, ["S", "A", "B", "E"]);
}

#[test]
fn a_cycle_skips_its_results_with_a_warning() {
    let x = PracticeQuantity {
        per_point: false,
        ..quantity("x")
    };
    let results = vec![
        result("A", "B + x"),
        result("B", "A*2"),
        result("C", "B + 1"),
        result("D", "x*2"),
    ];
    let a = compute(
        &[x],
        &results,
        &HashMap::new(),
        &[measurement("x", &[1.0, 2.0])],
        None,
    )
    .unwrap();
    let symbols: Vec<&str> = a.derived.iter().map(|d| d.symbol.as_str()).collect();
    assert_eq!(symbols, ["D"]);
    assert_eq!(
        a.warnings
            .iter()
            .filter(|w| w.contains("depende de sí mismo"))
            .count(),
        3
    );
}

#[tokio::test]
async fn curve_results_chain_and_skip_what_depends_on_series_values() {
    let (pool, _dir) = setup().await;
    let input = |symbol: &str, formula: &str| crate::practices::ResultInput {
        display_symbol: None,
        compare_with: None,
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "u".into(),
        formula: formula.into(),
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
    };
    for (symbol, formula) in [
        ("A", "f*2"),
        ("B", "A*3"),
        ("D", "E + 1"),
        ("E", "fpasaje*2"),
    ] {
        crate::practices::create_result(&pool, "filtros", input(symbol, formula), None)
            .await
            .unwrap();
    }
    let def = crate::practices::definition(&pool, "filtros")
        .await
        .unwrap()
        .unwrap();
    let golden: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/golden/filtros.input.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let measurements: Vec<MeasurementInput> = golden["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| MeasurementInput {
            quantity_id: def
                .quantities
                .iter()
                .find(|q| q.symbol == m["symbol"].as_str().unwrap())
                .unwrap()
                .id
                .clone(),
            instrument_id: None,
            scale_id: None,
            values: serde_json::from_value(m["values"].clone()).unwrap_or_default(),
            given_u: m["given_u"].as_f64(),
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        })
        .collect();
    let a = analyze(&pool, "filtros", &measurements).await.unwrap();
    let derived = |s: &str| a.derived.iter().find(|d| d.symbol == s);
    assert!(derived("A").is_none() && derived("B").is_none());
    let fpasaje = derived("fpasaje").unwrap().value;
    assert!(close(derived("E").unwrap().value, 2.0 * fpasaje, 1e-9));
    assert!(close(
        derived("D").unwrap().value,
        2.0 * fpasaje + 1.0,
        1e-9
    ));
    let warned = |symbol: &str, text: &str| {
        a.warnings
            .iter()
            .any(|w| w.contains(&format!("mensurando {symbol}:")) && w.contains(text))
    };
    assert!(warned("A", "\"f\""), "{:?}", a.warnings);
    assert!(warned("B", "no se pudo calcular"), "{:?}", a.warnings);
}

fn derived_value(analysis: &serde_json::Value, symbol: &str) -> Option<f64> {
    let own = analysis["derived"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["symbol"] == symbol)
        .and_then(|d| d["value"].as_f64());
    own.or_else(|| {
        analysis["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|p| derived_value(&p["analysis"], symbol))
    })
}

#[tokio::test]
async fn editing_a_published_practice_does_not_change_old_submissions() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let m = |sym: &str, values: Vec<f64>, given_u: Option<f64>| MeasurementInput {
        quantity_id: id(sym),
        instrument_id: None,
        scale_id: None,
        values,
        given_u,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    };
    let measurements = vec![
        m("L", vec![1.0], Some(0.002)),
        m("t_med", vec![2.0], Some(0.0)),
        m("T1", vec![2.0, 2.02, 1.98], None),
    ];
    let mk = || FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: measurements.clone(),
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    let old = create_form_submission(&pool, &user, mk()).await.unwrap();
    let old_g1 = derived_value(&old.analysis, "g1").unwrap();

    sqlx::query(
        "UPDATE practice_results SET formula = '2*(' || formula || ')', tolerance = 5 \
         WHERE practice_id = 'p1-partes' AND symbol = 'g1'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let edited = update_form_submission(&pool, &old.id, &measurements, None, None, None)
        .await
        .unwrap();
    assert!((derived_value(&edited.analysis, "g1").unwrap() - old_g1).abs() < 1e-12);
    assert!(!edited.result_tolerances.contains_key("g1"));
    let old_def = crate::practices::submission_definition(&pool, &old.id)
        .await
        .unwrap()
        .unwrap();
    let old_formula = &old_def
        .results
        .iter()
        .find(|r| r.symbol == "g1")
        .unwrap()
        .formula;
    assert!(!old_formula.starts_with("2*("), "{old_formula}");

    let v2 = crate::practices::publish_version(&pool, "p1-partes", None)
        .await
        .unwrap()
        .unwrap();
    let new = create_form_submission(&pool, &user, mk()).await.unwrap();
    assert!((derived_value(&new.analysis, "g1").unwrap() - 2.0 * old_g1).abs() < 1e-9);
    assert_eq!(new.result_tolerances.get("g1"), Some(&5.0));
    let versions: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, practice_version_id FROM submissions WHERE id IN (?1, ?2)")
            .bind(&old.id)
            .bind(&new.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    let version_of = |sid: &str| versions.iter().find(|(s, _)| s == sid).unwrap().1.clone();
    assert_eq!(version_of(&new.id).as_deref(), Some(v2.id.as_str()));
    assert_ne!(version_of(&old.id), version_of(&new.id));

    for stale in [1, 99] {
        let err = create_form_submission(
            &pool,
            &user,
            FormSubmissionInput {
                practice_version: Some(stale),
                ..mk()
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("recargá"), "{err}");
    }
    let current_form = create_form_submission(
        &pool,
        &user,
        FormSubmissionInput {
            practice_version: Some(v2.number),
            ..mk()
        },
    )
    .await
    .unwrap();
    assert!((derived_value(&current_form.analysis, "g1").unwrap() - 2.0 * old_g1).abs() < 1e-9);
}

#[tokio::test]
async fn version_one_links_the_submissions_without_a_version() {
    let (pool, _dir) = setup().await;
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_versions WHERE practice_id = 'p1-estadistica'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    db::seed_academic(&pool).await.unwrap();
    db::seed_submissions(&pool).await.unwrap();
    let (seeded,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM submissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(seeded > 0);
    for sql in [
        "UPDATE submissions SET practice_version_id = NULL WHERE practice_id = 'p1-estadistica'",
        "UPDATE practices SET current_version_id = NULL WHERE id = 'p1-estadistica'",
        "DELETE FROM practice_versions WHERE practice_id = 'p1-estadistica'",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    crate::practices::ensure_versions(&pool).await.unwrap();
    crate::practices::ensure_versions(&pool).await.unwrap();
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_versions WHERE practice_id = 'p1-estadistica'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let (unlinked,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE practice_version_id IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unlinked, 0);
    let published = crate::practices::published_definition(&pool, "p1-estadistica")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(published.current_version, Some(1));
}

#[tokio::test]
async fn a_practice_without_a_published_version_rejects_submissions() {
    let (pool, _dir) = setup().await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let teacher = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    sqlx::query("UPDATE practices SET current_version_id = NULL WHERE id = 'p1-estadistica'")
        .execute(&pool)
        .await
        .unwrap();
    let err = create_form_submission(
        &pool,
        &teacher,
        FormSubmissionInput {
            course_id: course.id.clone(),
            group_id: group.id.clone(),
            practice_id: "p1-estadistica".into(),
            measurements: vec![],
            meta: None,
            table_number: None,
            student_results: vec![],
            student_comment: None,
            practice_version: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("versión publicada"), "{err}");
}

#[tokio::test]
async fn a_retired_quantity_stays_in_the_submissions_that_used_it() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let practice = "p1-partes";
    let def = crate::practices::definition(&pool, practice)
        .await
        .unwrap()
        .unwrap();
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let m = |sym: &str, values: Vec<f64>, given_u: Option<f64>| MeasurementInput {
        quantity_id: id(sym),
        instrument_id: None,
        scale_id: None,
        values,
        given_u,
        point_replicas: None,
        operator_replicas: None,
        operator_instruments: None,
    };
    let measurements = vec![
        m("L", vec![1.0], Some(0.002)),
        m("t_med", vec![2.0], Some(0.0)),
        m("T1", vec![2.0, 2.02, 1.98], None),
        m("T2", vec![2.1, 2.12, 2.08], None),
    ];
    let mk = |measurements: Vec<MeasurementInput>| FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: practice.into(),
        measurements,
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    let old = create_form_submission(&pool, &user, mk(measurements.clone()))
        .await
        .unwrap();
    let old_g2 = derived_value(&old.analysis, "g2").unwrap();

    sqlx::query("DELETE FROM practice_results WHERE practice_id = ?1 AND formula LIKE '%T2%'")
        .bind(practice)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        crate::practices::remove_quantity(&pool, practice, &id("T2"))
            .await
            .unwrap(),
        crate::practices::QuantityRemoval::Retired
    );
    let on_current_version = create_form_submission(&pool, &user, mk(measurements.clone()))
        .await
        .unwrap();
    assert!((derived_value(&on_current_version.analysis, "g2").unwrap() - old_g2).abs() < 1e-12);

    crate::practices::publish_version(&pool, practice, None)
        .await
        .unwrap();
    let edited = update_form_submission(&pool, &old.id, &measurements, None, None, None)
        .await
        .unwrap();
    assert!((derived_value(&edited.analysis, "g2").unwrap() - old_g2).abs() < 1e-12);
    assert!(edited
        .measurements
        .iter()
        .any(|x| x.quantity_id == id("T2")));
    let old_def = crate::practices::submission_definition(&pool, &old.id)
        .await
        .unwrap()
        .unwrap();
    assert!(old_def
        .quantities
        .iter()
        .any(|q| q.id == id("T2") && q.symbol == "T2"));

    let err = create_form_submission(&pool, &user, mk(measurements.clone()))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no corresponde"), "{err}");
    let without_t2: Vec<MeasurementInput> = measurements
        .iter()
        .filter(|x| x.quantity_id != id("T2"))
        .cloned()
        .collect();
    create_form_submission(&pool, &user, mk(without_t2))
        .await
        .unwrap();
}

#[test]
fn live_values_match_what_the_fit_uses_from_a_single_point() {
    let quantities = vec![quantity("h"), quantity("V"), quantity("t")];
    let intermediate = |symbol: &str, formula: &str, show_live| PracticeIntermediate {
        symbol: symbol.into(),
        formula: formula.into(),
        show_live,
        ..Default::default()
    };
    let intermediates = vec![
        intermediate("Q", "V/t", true),
        intermediate("Q2", "Q*h", false),
        intermediate("Qh", "Q2/h", true),
        intermediate("Qsq", "Q*Q", true),
    ];
    let measurements = vec![
        measurement("h", &[2.0]),
        point_rep("q-V", vec![vec![10.0, 10.0]]),
        point_rep("q-t", vec![vec![1.0, 0.0]]),
    ];
    let rows = live_values(&quantities, &intermediates, &measurements).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0],
        vec![
            LiveValue {
                symbol: "Q".into(),
                replicas: vec![Some(10.0), None],
                mean: None,
            },
            LiveValue {
                symbol: "Qh".into(),
                replicas: vec![None],
                mean: None,
            },
            LiveValue {
                symbol: "Qsq".into(),
                replicas: vec![None],
                mean: None,
            },
        ]
    );

    let measurements = vec![
        measurement("h", &[2.0]),
        point_rep("q-V", vec![vec![10.0, 10.0]]),
        point_rep("q-t", vec![vec![1.0, 2.0]]),
    ];
    let rows = live_values(&quantities, &intermediates, &measurements).unwrap();
    assert_eq!(rows[0][0].replicas, vec![Some(10.0), Some(5.0)]);
    assert_eq!(rows[0][0].mean, Some(7.5));
    assert_eq!(rows[0][1].replicas, vec![Some(7.5)]);
    assert_eq!(rows[0][2].replicas, vec![Some(56.25)]);
    assert_eq!(rows[0][2].mean, Some(56.25));
}

#[test]
fn an_intermediate_on_means_applies_its_formula_to_the_point_means() {
    let mut dx = quantity("dx");
    dx.per_point = false;
    let quantities = vec![quantity("R"), quantity("t"), dx];
    let v = PracticeIntermediate {
        symbol: "v".into(),
        formula: "dx/t".into(),
        show_live: true,
        on_means: true,
        ..Default::default()
    };
    let measurements = vec![
        measurement("R", &[1.0, 2.0]),
        point_rep(
            "q-t",
            vec![vec![1.0, 2.0, 3.0, 4.0, 5.0], vec![2.0, 2.0, 2.0, 2.0, 2.0]],
        ),
        measurement("dx", &[0.2]),
    ];
    let rows = live_values(&quantities, std::slice::from_ref(&v), &measurements).unwrap();
    assert_eq!(rows[0][0].replicas, vec![Some(0.2 / 3.0)]);
    assert_eq!(rows[1][0].mean, Some(0.1));

    let with_v = reg(
        &quantities,
        std::slice::from_ref(&v),
        &[],
        "R",
        "v",
        &measurements,
    )
    .unwrap();
    let direct = reg(&quantities, &[], &[], "R", "dx/t", &measurements).unwrap();
    let points = with_v.regression.unwrap().points;
    assert_eq!(points, direct.regression.unwrap().points);
    assert_eq!(points[0].1, rows[0][0].mean.unwrap());
}

#[test]
fn chained_intermediates_use_the_mean_of_the_earlier_one_in_the_fit() {
    let quantities = vec![quantity("V"), quantity("t"), quantity("py")];
    let intermediate = |symbol: &str, formula: &str| PracticeIntermediate {
        symbol: symbol.into(),
        formula: formula.into(),
        ..Default::default()
    };
    let intermediates = vec![intermediate("Q", "V/t"), intermediate("Qsq", "Q*Q")];
    let measurements = vec![
        point_rep("q-V", vec![vec![10.0, 10.0], vec![20.0, 20.0]]),
        point_rep("q-t", vec![vec![1.0, 2.0], vec![1.0, 2.0]]),
        measurement("py", &[100.0, 200.0]),
    ];
    let a = reg(&quantities, &intermediates, &[], "Qsq", "py", &measurements).unwrap();
    assert_eq!(
        a.regression.unwrap().points,
        vec![(56.25, 100.0), (225.0, 200.0)]
    );
}

fn det_result(symbol: &str, formula: &str) -> PracticeResult {
    PracticeResult {
        id: format!("r-{symbol}"),
        practice_id: "p".into(),
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "u".into(),
        formula: formula.into(),
        has_uncertainty: true,
        ..Default::default()
    }
}

fn determinations(average: bool) -> crate::practices::Determinations {
    crate::practices::Determinations {
        count: 3,
        label: "Medida".into(),
        average,
        display: Default::default(),
    }
}

#[test]
fn determinations_match_separate_symbols_and_a_hand_written_average() {
    let mut l = quantity("L");
    l.repeated = false;
    let mut t = quantity("T");
    t.per_determination = true;
    let replicas = vec![
        vec![2.0, 2.02, 1.98],
        vec![2.01, 1.99],
        vec![2.05, 1.95, 2.0],
    ];
    let measurements = vec![
        MeasurementInput {
            operator_replicas: Some(replicas.clone()),
            operator_instruments: None,
            ..measurement("T", &[])
        },
        measurement("L", &[1.0, 1.02, 0.98]),
    ];
    let results = vec![det_result("g", "4*pi^2*L/T^2"), det_result("doble", "2*L")];
    let d = determinations(true);
    let a = compute(
        &[l.clone(), t],
        &results,
        &HashMap::new(),
        &measurements,
        Some(&d),
    )
    .unwrap();

    let separate_quantities = vec![l, quantity("T1"), quantity("T2"), quantity("T3")];
    let mut separate_measurements = vec![measurement("L", &[1.0, 1.02, 0.98])];
    for (k, values) in replicas.iter().enumerate() {
        separate_measurements.push(measurement(&format!("T{}", k + 1), values));
    }
    let separate_results = vec![
        det_result("g1", "4*pi^2*L/T1^2"),
        det_result("g2", "4*pi^2*L/T2^2"),
        det_result("g3", "4*pi^2*L/T3^2"),
        det_result("g_medio", "(g1 + g2 + g3)/3"),
    ];
    let s = compute(
        &separate_quantities,
        &separate_results,
        &HashMap::new(),
        &separate_measurements,
        None,
    )
    .unwrap();
    let by_symbol = |list: &[DerivedComputation], symbol: &str| {
        let d = list.iter().find(|d| d.symbol == symbol).unwrap();
        (d.value, d.u)
    };

    assert_eq!(a.determinations.len(), 3);
    assert_eq!(a.determinations[1].label, "Medida 2");
    for (k, det) in a.determinations.iter().enumerate() {
        assert_eq!(det.quantities.len(), 1);
        assert_eq!(det.derived.len(), 1);
        let (value, u) = by_symbol(&det.derived, "g");
        let (expected, expected_u) = by_symbol(&s.derived, &format!("g{}", k + 1));
        assert!(
            close(value, expected, 1e-9 * expected),
            "g de la medida {}",
            k + 1
        );
        assert!(
            close(u, expected_u, 1e-9 * expected_u),
            "u(g) de la medida {}",
            k + 1
        );
        assert_eq!(det.derived[0].formula, "4*pi^2*L/T^2");
    }
    let (value, u) = by_symbol(&a.derived, "g_medio");
    let (expected, expected_u) = by_symbol(&s.derived, "g_medio");
    assert!(close(value, expected, 1e-9 * expected));
    assert!(close(u, expected_u, 1e-9 * expected_u));
    assert_eq!(
        a.derived.iter().filter(|d| d.symbol == "doble").count(),
        1,
        "un resultado común sale una sola vez"
    );
    assert_eq!(a.quantities.len(), 1);

    let without_average = compute(
        &[quantity("L"), {
            let mut t = quantity("T");
            t.per_determination = true;
            t
        }],
        &results,
        &HashMap::new(),
        &measurements,
        Some(&determinations(false)),
    )
    .unwrap();
    assert!(!without_average
        .derived
        .iter()
        .any(|d| d.symbol == "g_medio"));
}

#[test]
fn an_empty_determination_warns_with_its_label() {
    let mut t = quantity("T");
    t.per_determination = true;
    let measurements = vec![MeasurementInput {
        operator_replicas: Some(
            (1..=12)
                .map(|k| if k == 12 { vec![] } else { vec![k as f64] })
                .collect(),
        ),
        ..measurement("T", &[])
    }];
    let a = compute(
        &[t],
        &[det_result("f", "1/T")],
        &HashMap::new(),
        &measurements,
        Some(&crate::practices::Determinations {
            count: 12,
            ..determinations(false)
        }),
    )
    .unwrap();
    assert!(
        a.warnings
            .iter()
            .any(|w| w.starts_with("Medida 12:") && w.contains("no tiene lecturas")),
        "{:?}",
        a.warnings
    );
    assert!(
        a.warnings.iter().all(|w| w.starts_with("Medida 12:")),
        "la determinación 12 no se confunde con la 1: {:?}",
        a.warnings
    );
    assert!(
        a.warnings.iter().all(|w| !w.contains("__")),
        "{:?}",
        a.warnings
    );
}

#[test]
fn determination_warnings_are_grouped_and_the_average_is_not_blamed_on_one() {
    let mut t = quantity("T");
    t.per_determination = true;
    let measurements = vec![MeasurementInput {
        operator_replicas: Some(vec![vec![2.0], vec![0.0], vec![4.0]]),
        operator_instruments: None,
        ..measurement("T", &[])
    }];
    let a = compute(
        &[t],
        &[det_result("f", "1/T"), det_result("h", "T*zzz")],
        &HashMap::new(),
        &measurements,
        Some(&determinations(true)),
    )
    .unwrap();
    let about = |needle: &str| -> Vec<&String> {
        a.warnings.iter().filter(|w| w.contains(needle)).collect()
    };
    assert_eq!(about("mensurando h:").len(), 1, "{:?}", a.warnings);
    assert!(
        !about("mensurando h:")[0].starts_with("Medida"),
        "{:?}",
        a.warnings
    );
    assert!(
        about("\"f\" (f =")
            .iter()
            .any(|w| w.starts_with("Medida 2:")),
        "{:?}",
        a.warnings
    );
    assert!(
        a.warnings
            .iter()
            .filter(|w| w.contains("f_medio"))
            .all(|w| !w.starts_with("Medida") && w.contains("promedio de f en las 3")),
        "{:?}",
        a.warnings
    );
}

#[test]
fn a_quantity_missing_in_every_determination_warns_once() {
    let mut t = quantity("T");
    t.per_determination = true;
    let a = compute(
        &[t],
        &[det_result("f", "1/T")],
        &HashMap::new(),
        &[MeasurementInput {
            operator_replicas: Some(vec![vec![], vec![], vec![]]),
            operator_instruments: None,
            ..measurement("T", &[])
        }],
        Some(&determinations(false)),
    )
    .unwrap();
    let missing: Vec<_> = a
        .warnings
        .iter()
        .filter(|w| w.contains("no tiene lecturas"))
        .collect();
    assert_eq!(missing.len(), 1, "{:?}", a.warnings);
    assert!(!missing[0].starts_with("Medida"), "{:?}", a.warnings);
}

#[test]
fn a_result_named_like_a_generated_symbol_does_not_crash() {
    let mut t = quantity("T");
    t.per_determination = true;
    let analysis = compute(
        &[t],
        &[det_result("f", "1/T"), det_result("T__1", "2")],
        &HashMap::new(),
        &[MeasurementInput {
            operator_replicas: Some(vec![vec![2.0], vec![4.0], vec![5.0]]),
            operator_instruments: None,
            ..measurement("T", &[])
        }],
        Some(&determinations(false)),
    );
    assert!(analysis.is_ok());
}

fn per_determination_definition(average: bool) -> crate::practices::PracticeDefinition {
    let mut t = quantity("T");
    t.per_determination = true;
    t.per_point = false;
    crate::practices::PracticeDefinition {
        quantities: vec![t],
        results: vec![
            PracticeResult {
                is_final: true,
                ..det_result("g", "1/T")
            },
            det_result("doble", "2"),
            det_result("rel", "g_medio/2"),
        ],
        determinations: Some(determinations(average)),
        ..Default::default()
    }
}

#[test]
fn a_common_result_can_use_an_average() {
    let def = per_determination_definition(true);
    let measurements = vec![MeasurementInput {
        operator_replicas: Some(vec![vec![2.0, 2.1], vec![4.0, 4.2], vec![5.0, 5.1]]),
        operator_instruments: None,
        ..measurement("T", &[])
    }];
    let a = compute(
        &def.quantities,
        &def.results,
        &HashMap::new(),
        &measurements,
        def.determinations.as_ref(),
    )
    .unwrap();
    let find = |s: &str| a.derived.iter().find(|d| d.symbol == s).unwrap();
    let (medio, rel) = (find("g_medio"), find("rel"));
    assert!(close(rel.value, medio.value / 2.0, 1e-12));
    assert!(close(rel.u, medio.u / 2.0, 1e-12 + 1e-9 * medio.u));
    assert!(a.warnings.is_empty(), "{:?}", a.warnings);
}

#[test]
fn a_submission_needs_every_determination_of_a_required_quantity() {
    let def = per_determination_definition(true);
    let groups = |g: Vec<Vec<f64>>| {
        vec![MeasurementInput {
            operator_replicas: Some(g),
            operator_instruments: None,
            ..measurement("T", &[])
        }]
    };
    assert!(super::submission::check_determination_groups(
        &def,
        &groups(vec![vec![2.0], vec![3.0], vec![4.0]])
    )
    .is_ok());
    let short =
        super::submission::check_determination_groups(&def, &groups(vec![vec![2.0], vec![3.0]]))
            .unwrap_err();
    assert_eq!(short.to_string(), "\"T\": falta cargar Medida 3.");
    assert!(super::submission::check_determination_groups(
        &def,
        &groups(vec![vec![2.0], vec![], vec![4.0]])
    )
    .is_err());
    assert!(super::submission::check_determination_groups(&def, &[]).is_err());
    let mut optional = def.clone();
    optional.quantities[0].optional = true;
    assert!(super::submission::check_determination_groups(&optional, &[]).is_ok());
}

#[test]
fn students_report_each_determination_and_the_average() {
    let result = |symbol: &str| db::StudentResultInput {
        symbol: symbol.into(),
        value: 1.0,
        u_expanded: None,
    };
    let ok = |def: &crate::practices::PracticeDefinition, s: &str| {
        check_student_result_symbols(def, &[result(s)]).is_ok()
    };
    let with_average = per_determination_definition(true);
    for s in ["g#1", "g#3", "g_medio"] {
        assert!(ok(&with_average, s), "{s}");
    }
    for s in [
        "doble",
        "rel",
        "g",
        "g#0",
        "g#4",
        "g#x",
        "g#01",
        "g#+1",
        "doble#1",
        "doble_medio",
        "rel#1",
    ] {
        assert!(!ok(&with_average, s), "{s}");
    }
    let mut final_common = per_determination_definition(true);
    for r in final_common
        .results
        .iter_mut()
        .filter(|r| r.symbol == "doble")
    {
        r.is_final = true;
    }
    assert!(ok(&final_common, "doble"));
    let without_average = per_determination_definition(false);
    assert!(ok(&without_average, "g#2"));
    assert!(!ok(&without_average, "g_medio"));
    let mut not_final = per_determination_definition(true);
    for r in not_final.results.iter_mut().filter(|r| r.symbol == "g") {
        r.is_final = false;
    }
    assert!(!ok(&not_final, "g#1") && !ok(&not_final, "g_medio") && !ok(&not_final, "g"));
}

#[tokio::test]
async fn per_determination_measurements_are_checked_against_the_determinations() {
    let (pool, _dir) = setup().await;
    let def = per_determination_definition(true);
    let analyze = |m: MeasurementInput| {
        let (pool, def) = (pool.clone(), def.clone());
        async move { analyze_definition(&pool, &def, &[m]).await }
    };
    let groups = |n: usize| MeasurementInput {
        operator_replicas: Some(vec![vec![2.0]; n]),
        operator_instruments: None,
        ..measurement("T", &[])
    };
    assert!(analyze(groups(3)).await.is_ok());
    assert!(analyze(groups(2)).await.is_ok());
    let too_many = analyze(groups(4)).await.unwrap_err().to_string();
    assert!(too_many.contains("4 determinaciones"), "{too_many}");
    let flat = analyze(measurement("T", &[2.0]))
        .await
        .unwrap_err()
        .to_string();
    assert!(flat.contains("Medida 1"), "{flat}");
}

#[tokio::test]
async fn a_submission_stores_and_computes_the_fixed_value_not_the_sent_one() {
    let (pool, _dir) = setup().await;
    p1_partes(&pool).await;
    sqlx::query(
        "UPDATE practice_quantities SET fixed = 1, default_value = 1.0, default_u = 0.002          WHERE practice_id = 'p1-partes' AND symbol = 'L'",
    )
    .execute(&pool)
    .await
    .unwrap();
    crate::practices::publish_version(&pool, "p1-partes", None)
        .await
        .unwrap()
        .unwrap();
    let course = db::create_course(
        &pool,
        db::CreateCourse {
            name: "Curso".into(),
            term: "2026".into(),
        },
    )
    .await
    .unwrap();
    let group = db::create_group(
        &pool,
        &course.id,
        db::CreateGroup {
            name: "Grupo 1".into(),
            table_count: Some(4),
            group_type: None,
        },
    )
    .await
    .unwrap();
    let user = db::users(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|u| u.email == "docente@quantify.local")
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-partes")
        .await
        .unwrap()
        .unwrap();
    let l_id = def
        .quantities
        .iter()
        .find(|q| q.symbol == "L")
        .unwrap()
        .id
        .clone();
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-partes".into(),
        measurements: vec![MeasurementInput {
            quantity_id: l_id.clone(),
            values: vec![5.0],
            given_u: Some(99.0),
            ..Default::default()
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let l = detail.analysis["quantities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["symbol"] == "L")
        .unwrap();
    assert_eq!(l["result"]["mean"], 1.0);
    assert_eq!(l["result"]["u_c"], 0.002 / crate::uncertainty::EXPANSION_K);
    let stored: (f64, Option<f64>) = sqlx::query_as(
        "SELECT value, value_u FROM submission_measurements WHERE submission_id = ?1 AND quantity_id = ?2",
    )
    .bind(&detail.id)
    .bind(&l_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, (1.0, Some(0.002)));
}

fn point_result(symbol: &str, formula: &str) -> PracticePointResult {
    PracticePointResult {
        id: format!("pr-{symbol}"),
        practice_id: "p".into(),
        position: 0,
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "".into(),
        formula: formula.into(),
    }
}

fn regression_with(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    results: &[PracticeResult],
    point_results: &[PracticePointResult],
    scales: &HashMap<String, InstrumentScale>,
    measurements: &[MeasurementInput],
) -> FormAnalysis {
    compute_regresion(
        quantities,
        intermediates,
        results,
        point_results,
        &[],
        scales,
        "px",
        "py",
        measurements,
    )
    .unwrap()
}

#[test]
fn point_results_carry_the_uncertainty_of_their_point() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let replicas = MeasurementInput {
        point_replicas: Some(vec![vec![1.9, 2.1], vec![3.9, 4.1], vec![5.8, 6.2]]),
        ..measurement("py", &[])
    };
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        replicas,
        measurement("c", &[9.9, 10.1]),
    ];
    let results = vec![result("m", "slope * c")];
    let point_results = vec![
        point_result("twice", "2*py"),
        point_result("along", "slope*px"),
        point_result("zero", "m - slope*c"),
    ];
    let a = regression_with(
        &quantities,
        &[],
        &results,
        &point_results,
        &HashMap::new(),
        &measurements,
    );
    let u_of = |s: &str| &a.point_results.iter().find(|p| p.symbol == s).unwrap().u;
    let u_a = [0.1, 0.1, 0.2];
    for (i, u) in u_of("twice").iter().enumerate() {
        assert!(close(*u, 2.0 * u_a[i], 1e-12), "{i}: {u}");
    }
    let u_slope = a.regression.as_ref().unwrap().u_slope;
    for (i, u) in u_of("along").iter().enumerate() {
        assert!(close(*u, (i as f64 + 1.0) * u_slope, 1e-12), "{i}: {u}");
    }
    assert!(u_of("zero").iter().all(|u| *u == 0.0), "{:?}", u_of("zero"));
    let twice = a
        .point_results
        .iter()
        .find(|p| p.symbol == "twice")
        .unwrap();
    assert!(close(twice.u_expanded[2], 2.0 * twice.u[2], 1e-12));
}

#[test]
fn point_results_see_through_intermediates_and_instrument_scales() {
    let quantities = vec![
        PracticeQuantity {
            unit: "A".into(),
            ..quantity("px")
        },
        quantity("py"),
    ];
    let intermediates = vec![PracticeIntermediate {
        symbol: "q".into(),
        formula: "3*px".into(),
        ..Default::default()
    }];
    let measurements = vec![
        MeasurementInput {
            instrument_id: Some("i1".into()),
            scale_id: Some("s1".into()),
            ..measurement("px", &[1.0, 2.0, 3.0])
        },
        measurement("py", &[2.0, 4.1, 5.9]),
    ];
    let scales: HashMap<String, InstrumentScale> = [("s1".to_string(), fab_scale())].into();
    let a = regression_with(
        &quantities,
        &intermediates,
        &[],
        &[point_result("triple", "q")],
        &scales,
        &measurements,
    );
    let triple = &a.point_results[0];
    for (i, x) in [1.0, 2.0, 3.0].iter().enumerate() {
        let (u_b, _) = type_b_in(&fab_scale(), "A", *x);
        assert!(u_b > 0.0);
        assert!(close(triple.u[i], 3.0 * u_b, 1e-12), "{i}: {}", triple.u[i]);
    }
}

#[test]
fn point_uncertainties_reuse_the_last_reading_and_survive_a_bad_scale() {
    let quantities = vec![quantity("px"), quantity("py"), quantity("T"), quantity("w")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        MeasurementInput {
            point_replicas: Some(vec![vec![19.0, 21.0]]),
            ..measurement("T", &[])
        },
        MeasurementInput {
            instrument_id: Some("i1".into()),
            scale_id: Some("s1".into()),
            point_replicas: Some(vec![vec![0.9, 1.1], vec![1.9, 2.1], vec![2.9, 3.1]]),
            ..measurement("w", &[])
        },
    ];
    let bad = InstrumentScale {
        b_model: "raro".into(),
        ..fab_scale()
    };
    let scales: HashMap<String, InstrumentScale> = [("s1".to_string(), bad)].into();
    let a = regression_with(
        &quantities,
        &[],
        &[],
        &[point_result("t2", "2*T"), point_result("rw", "w")],
        &scales,
        &measurements,
    );
    assert!(a.regression.is_some());
    let t2 = a.point_results.iter().find(|p| p.symbol == "t2").unwrap();
    assert!(t2.u.iter().all(|u| close(*u, 2.0, 1e-12)), "{:?}", t2.u);
    let rw = a.point_results.iter().find(|p| p.symbol == "rw").unwrap();
    assert!(rw.u.iter().all(|u| u.is_nan()));
    assert!(
        a.warnings
            .iter()
            .any(|w| w.contains("quedan sin incertidumbre")),
        "{:?}",
        a.warnings
    );
    assert!(
        a.warnings.iter().any(|w| w.contains("\"rw\"")),
        "{:?}",
        a.warnings
    );
}

#[test]
fn a_point_result_that_is_not_derivable_in_a_run_warns() {
    let quantities = vec![quantity("px"), quantity("py"), quantity("w")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        MeasurementInput {
            point_replicas: Some(vec![vec![0.9, 1.1], vec![1.9, 2.1], vec![2.9, 3.1]]),
            ..measurement("w", &[])
        },
    ];
    let a = regression_with(
        &quantities,
        &[],
        &[],
        &[point_result("r", "math::sqrt(w - 1)")],
        &HashMap::new(),
        &measurements,
    );
    assert!(a.point_results[0].values[0] == 0.0);
    assert!(
        a.warnings
            .iter()
            .any(|w| w.contains("derivada por punto \"r\"") && w.contains("no se puede calcular")),
        "{:?}",
        a.warnings
    );
}

#[test]
fn point_uncertainties_only_look_at_what_the_point_results_use() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c, quantity("w")];
    let measurements = vec![
        MeasurementInput {
            instrument_id: Some("i1".into()),
            scale_id: Some("s1".into()),
            ..measurement("px", &[1.0, 2.0, 3.0])
        },
        MeasurementInput {
            point_replicas: Some(vec![vec![1.9, 2.1], vec![3.9, 4.1], vec![5.8, 6.2]]),
            ..measurement("py", &[])
        },
        measurement("c", &[9.9, 10.1]),
    ];
    let bad = InstrumentScale {
        b_model: "raro".into(),
        ..fab_scale()
    };
    let scales: HashMap<String, InstrumentScale> = [("s1".to_string(), bad)].into();
    let exact = PracticeResult {
        has_uncertainty: false,
        ..result("m", "slope * c")
    };
    let a = regression_with(
        &quantities,
        &[],
        &[exact],
        &[point_result("twice", "2*py"), point_result("scaled", "m*1")],
        &scales,
        &measurements,
    );
    assert!(
        a.warnings
            .iter()
            .all(|w| !w.contains("quedan sin incertidumbre")),
        "{:?}",
        a.warnings
    );
    let pr = |s: &str| a.point_results.iter().find(|p| p.symbol == s).unwrap();
    assert!(pr("twice").u.iter().all(|u| u.is_finite() && *u > 0.0));
    assert!(
        pr("scaled").u.iter().all(|u| *u == 0.0),
        "{:?}",
        pr("scaled").u
    );
}

#[test]
fn a_result_without_uncertainty_is_exact_for_everything_that_uses_it() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        measurement("c", &[9.9, 10.1]),
    ];
    let results = vec![
        PracticeResult {
            has_uncertainty: false,
            ..result("m1", "slope * c")
        },
        result("m2", "2*m1"),
        result("m3", "m1 + c"),
    ];
    let a = regression_with(
        &quantities,
        &[],
        &results,
        &[point_result("p", "m2")],
        &HashMap::new(),
        &measurements,
    );
    let derived = |s: &str| a.derived.iter().find(|d| d.symbol == s).unwrap();
    assert_eq!(derived("m2").u, 0.0);
    let u_c = 0.1;
    assert!(close(derived("m3").u, u_c, 1e-12), "{}", derived("m3").u);
    assert!(a.point_results[0].u.iter().all(|u| *u == 0.0));
}

fn aggregate(symbol: &str, formula: &str) -> PracticeAggregate {
    PracticeAggregate {
        id: format!("ag-{symbol}"),
        symbol: symbol.into(),
        name: symbol.into(),
        formula: formula.into(),
        ..Default::default()
    }
}

#[test]
fn aggregates_carry_their_uncertainty_against_the_inputs() {
    let mut c = quantity("c");
    c.per_point = false;
    let quantities = vec![quantity("px"), quantity("py"), c];
    let measurements = vec![
        MeasurementInput {
            point_replicas: Some(vec![vec![0.9, 1.1], vec![1.9, 2.1], vec![2.8, 3.2]]),
            ..measurement("px", &[])
        },
        measurement("py", &[2.0, 4.1, 5.9]),
        measurement("c", &[9.9, 10.1]),
    ];
    let intermediates = vec![PracticeIntermediate {
        symbol: "q".into(),
        formula: "2*px".into(),
        ..Default::default()
    }];
    let results = vec![
        result("m", "slope * c"),
        PracticeResult {
            has_uncertainty: false,
            ..result("e", "slope * c")
        },
    ];
    let aggregates = vec![
        aggregate("span", "px_last - px_first"),
        aggregate("same", "slope * c"),
        aggregate("zero", "m - slope * c"),
        aggregate("qf", "q_first"),
        aggregate("qzero", "q_first - 2*px_first"),
        aggregate("twice", "2 * span"),
        aggregate("exact", "e * 1"),
    ];
    let a = compute_regresion(
        &quantities,
        &intermediates,
        &results,
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    let u = |s: &str| a.aggregates.iter().find(|g| g.symbol == s).unwrap().u;
    let (u_first, u_last) = (0.1, 0.2);
    assert!(
        close(u("span"), f64::hypot(u_first, u_last), 1e-12),
        "{}",
        u("span")
    );
    let m = a.derived.iter().find(|d| d.symbol == "m").unwrap().u;
    assert!(close(u("same"), m, 1e-12), "{} vs {m}", u("same"));
    assert_eq!(u("zero"), 0.0);
    assert!(close(u("qf"), 2.0 * u_first, 1e-12), "{}", u("qf"));
    assert_eq!(u("qzero"), 0.0);
    assert!(close(u("twice"), 2.0 * u("span"), 1e-12));
    assert_eq!(u("exact"), 0.0);
    let span = a.aggregates.iter().find(|g| g.symbol == "span").unwrap();
    assert!(close(span.u_expanded, 2.0 * span.u, 1e-12));
}

#[test]
fn an_aggregate_without_uncertainty_is_exact_for_the_next_ones() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        MeasurementInput {
            point_replicas: Some(vec![vec![0.9, 1.1], vec![1.9, 2.1], vec![2.8, 3.2]]),
            ..measurement("px", &[])
        },
        measurement("py", &[2.0, 4.1, 5.9]),
    ];
    let aggregates = vec![
        PracticeAggregate {
            has_uncertainty: false,
            ..aggregate("span", "px_last - px_first")
        },
        aggregate("twice", "2 * span"),
        aggregate("mixed", "span + px_first"),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &[],
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    let by = |s: &str| a.aggregates.iter().find(|g| g.symbol == s).unwrap();
    assert!(!by("span").has_uncertainty);
    assert_eq!(by("span").u, 0.0);
    assert!(close(by("span").value, 2.0, 1e-12));
    assert_eq!(by("twice").u, 0.0);
    assert!(close(by("mixed").u, 0.1, 1e-12), "{}", by("mixed").u);
}

#[test]
fn endpoints_that_name_the_same_point_share_their_uncertainty() {
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        MeasurementInput {
            point_replicas: Some(vec![vec![0.9, 1.1], vec![1.8, 2.2]]),
            ..measurement("px", &[])
        },
        measurement("py", &[2.0, 4.0]),
    ];
    let aggregates = vec![
        aggregate("zero", "px_first - px_last2"),
        aggregate("zero2", "px_first2 - px_last"),
    ];
    let a = compute_regresion(
        &quantities,
        &[],
        &[],
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    for g in &a.aggregates {
        assert_eq!(g.value, 0.0, "{}", g.symbol);
        assert_eq!(g.u, 0.0, "{}", g.symbol);
    }
}

#[test]
fn intermediate_endpoints_propagate_from_the_point_of_their_run() {
    let quantities = vec![quantity("px"), quantity("py"), quantity("z")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[2.0, 4.1, 5.9]),
        MeasurementInput {
            point_replicas: Some(vec![
                vec![9.9, 10.1],
                vec![19.9, 20.1],
                vec![29.9, 30.1],
                vec![39.8, 40.2],
            ]),
            ..measurement("z", &[])
        },
    ];
    let intermediates = vec![PracticeIntermediate {
        symbol: "q".into(),
        formula: "z*z".into(),
        ..Default::default()
    }];
    let aggregates = vec![
        aggregate("qe", "q_last"),
        aggregate("zero", "q_last - z_last2*z_last2"),
    ];
    let a = compute_regresion(
        &quantities,
        &intermediates,
        &[],
        &[],
        &aggregates,
        &HashMap::new(),
        "px",
        "py",
        &measurements,
    )
    .unwrap();
    let by = |s: &str| a.aggregates.iter().find(|g| g.symbol == s).unwrap();
    assert!(close(by("qe").value, 900.01, 1e-9), "{}", by("qe").value);
    assert!(close(by("qe").u, 2.0 * 30.0 * 0.1, 1e-9), "{}", by("qe").u);
    assert!(close(by("zero").value, 0.01, 1e-9));
    assert!(close(by("zero").u, 0.0, 1e-9), "{}", by("zero").u);
}
