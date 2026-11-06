use super::formula::compile_formula;
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
        analysis.operators.extend(a.operators);
        analysis.point_results.extend(a.point_results);
        analysis.aggregates.extend(a.aggregates);
        analysis.warnings.extend(a.warnings);
        analysis.regression = analysis.regression.or(a.regression);
    }
    analysis
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
    db::seed_practices(&pool).await.unwrap();
    db::seed_users(&pool).await.unwrap();
    crate::practices::seed_definitions(&pool).await.unwrap();
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

    for bad in ["Re#x", "Nope#0"] {
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
    };
    let definition = PracticeDefinition {
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
        }],
        curves: vec![],
        operator_count: None,
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

#[test]
fn compute_with_operators_derives_per_operator() {
    let t = quantity("T");
    let mut l = quantity("L");
    l.repeated = false;
    let quantities = vec![t, l];
    let results = vec![PracticeResult {
        id: "r1".into(),
        practice_id: "p1-estadistica".into(),
        symbol: "g".into(),
        name: "g".into(),
        unit: "u".into(),
        formula: "T + L".into(),
        position: 0,
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
    }];
    let measurements = vec![
        MeasurementInput {
            quantity_id: "q-T".into(),
            instrument_id: None,
            scale_id: None,
            values: vec![],
            given_u: None,
            point_replicas: None,
            operator_replicas: Some(vec![vec![10.0, 10.0], vec![20.0, 20.0]]),
        },
        measurement("L", &[5.0]),
    ];
    let a = compute(
        &quantities,
        &results,
        &HashMap::new(),
        &measurements,
        Some(2),
    )
    .unwrap();

    assert_eq!(a.quantities.len(), 1);
    assert_eq!(a.quantities[0].symbol, "L");
    assert!(a.derived.is_empty());

    assert_eq!(a.operators.len(), 2);
    assert_eq!(a.operators[0].label, "Operador 1");
    assert_eq!(a.operators[0].quantities.len(), 1);
    assert_eq!(a.operators[0].quantities[0].symbol, "T");
    assert!(close(a.operators[0].quantities[0].result.mean, 10.0, 1e-12));
    assert!(close(a.operators[0].derived[0].value, 15.0, 1e-9));
    assert!(close(a.operators[1].quantities[0].result.mean, 20.0, 1e-12));
    assert!(close(a.operators[1].derived[0].value, 25.0, 1e-9));
}

#[tokio::test]
async fn analyze_uses_type_a_with_replicas() {
    let (pool, _dir) = setup().await;
    drop_parts(&pool, "p1-estadistica").await;
    let def = crate::practices::definition(&pool, "p1-estadistica")
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
    }];
    let analysis = analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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
async fn operator_submission_stores_operator_index_per_operator() {
    let (pool, _dir) = setup().await;
    drop_parts(&pool, "p1-estadistica").await;
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
    crate::practices::set_operator_count(&pool, "p1-estadistica", 2)
        .await
        .unwrap();
    let def = crate::practices::definition(&pool, "p1-estadistica")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(def.operator_count, Some(2));
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![],
            given_u: None,
            point_replicas: None,
            operator_replicas: Some(vec![vec![1.0, 1.1], vec![2.0, 2.1, 2.2]]),
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-estadistica", None)
        .await
        .unwrap();
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

    let operators = detail.analysis["operators"].as_array().unwrap();
    assert_eq!(operators.len(), 2);
}

#[tokio::test]
async fn point_based_submission_stores_point_index_per_point() {
    let (pool, _dir) = setup().await;
    drop_parts(&pool, "p1-estadistica").await;
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
    crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-estadistica",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-estadistica")
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![
            MeasurementInput {
                quantity_id: qid("T1"),
                instrument_id: None,
                scale_id: None,
                values: vec![1.0, 2.0, 3.0],
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
            },
            MeasurementInput {
                quantity_id: qid("T2"),
                instrument_id: None,
                scale_id: None,
                values: vec![],
                given_u: None,
                point_replicas: Some(vec![vec![4.0, 4.2], vec![5.0, 5.1], vec![6.0, 5.9]]),
                operator_replicas: None,
            },
        ],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-estadistica", None)
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
}

#[tokio::test]
async fn point_based_submission_stores_variable_replicas_per_point() {
    let (pool, _dir) = setup().await;
    drop_parts(&pool, "p1-estadistica").await;
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
    crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-estadistica",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-estadistica")
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![
            MeasurementInput {
                quantity_id: qid("T1"),
                instrument_id: None,
                scale_id: None,
                values: vec![1.0, 2.0, 3.0],
                given_u: None,
                point_replicas: None,
                operator_replicas: None,
            },
            MeasurementInput {
                quantity_id: qid("T2"),
                instrument_id: None,
                scale_id: None,
                values: vec![],
                given_u: None,
                point_replicas: Some(vec![vec![4.0], vec![5.0, 5.1, 5.2], vec![6.0, 6.1]]),
                operator_replicas: None,
            },
        ],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-estadistica", None)
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
    drop_parts(&pool, "p1-estadistica").await;
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
        practice_version: None,
    };
    crate::practices::publish_version(&pool, "p1-estadistica", None)
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![5.0, 5.2, 4.9],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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

    assert!(
        db::find_existing_report(&pool, "p1-estadistica", &group.id, 1)
            .await
            .unwrap()
            .is_some()
    );

    let existed = db::delete_submission(&pool, &created.id).await.unwrap();
    assert!(existed);

    assert!(db::submission_detail(&pool, &created.id)
        .await
        .unwrap()
        .is_none());

    assert!(
        db::find_existing_report(&pool, "p1-estadistica", &group.id, 1)
            .await
            .unwrap()
            .is_none()
    );

    assert!(!db::delete_submission(&pool, &created.id).await.unwrap());

    let recreated = create_form_submission(&pool, &user, mk()).await.unwrap();
    assert_ne!(recreated.id, created.id);
    assert_eq!(recreated.table_number, Some(1));
}

#[tokio::test]
async fn update_form_submission_replaces_student_comment() {
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
    }];
    let input = FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-estadistica".into(),
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
    }];
    assert!(analyze(&pool, "p1-estadistica", &measurements)
        .await
        .is_err());
}

#[tokio::test]
async fn create_form_submission_rejects_unknown_course_and_rolls_back() {
    let (pool, _dir) = setup().await;
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
        practice_id: "p1-estadistica".into(),
        measurements: vec![MeasurementInput {
            quantity_id: t_id,
            instrument_id: None,
            scale_id: None,
            values: vec![1.0],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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
    }
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
    }];
    let rep = |groups: Vec<Vec<f64>>, id: &str| MeasurementInput {
        quantity_id: id.into(),
        instrument_id: None,
        scale_id: None,
        values: vec![],
        given_u: None,
        point_replicas: Some(groups),
        operator_replicas: None,
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
        },
        PracticeIntermediate {
            id: "i2".into(),
            practice_id: "p1-estadistica".into(),
            position: 1,
            symbol: "R".into(),
            name: "R".into(),
            unit: "u".into(),
            formula: "Q*2".into(),
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
    assert!(
        def.intermediates.is_empty(),
        "viscosidad no usa intermedias"
    );
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
    drop_parts(&pool, "p1-estadistica").await;
    crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-estadistica",
        crate::practices::CurveInput {
            x_formula: "T1".into(),
            y_formula: "T2".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-estadistica")
        .await
        .unwrap()
        .unwrap();
    let measurements = vec![
        measurement_for(&def, "T1", &[1.0, 2.0, 3.0]),
        measurement_for(&def, "T2", &[4.0, 5.0, 6.0]),
    ];
    let analysis = analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();
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
    drop_parts(&pool, "p1-estadistica").await;
    crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
        .await
        .unwrap();
    crate::practices::create_curve(
        &pool,
        "p1-estadistica",
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
        "p1-estadistica",
        crate::practices::CurveInput {
            x_formula: "T2".into(),
            y_formula: "T1".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();
    let def = crate::practices::definition(&pool, "p1-estadistica")
        .await
        .unwrap()
        .unwrap();
    let measurements = vec![
        measurement_for(&def, "T1", &[1.0, 2.0, 3.0]),
        measurement_for(&def, "T2", &[4.0, 5.0, 6.0]),
    ];
    let analysis = analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();
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
    crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
        .await
        .unwrap();
    let by_part = analyze(&pool, "p1-estadistica", &[]).await.unwrap();
    assert!(by_part
        .warnings
        .iter()
        .any(|w| w.contains("No se pudo calcular la parte") && w.contains("curvas")));
    assert!(!by_part.parts.is_empty());

    drop_parts(&pool, "p1-estadistica").await;
    let result = analyze(&pool, "p1-estadistica", &[]).await;
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
    let measurements: Vec<MeasurementInput> = def
        .quantities
        .iter()
        .map(|q| MeasurementInput {
            quantity_id: q.id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![q.default_value.unwrap_or(1.0)],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        })
        .collect();

    let analysis = flattened(analyze(&pool, "hidrostatica", &measurements).await.unwrap());
    for symbol in ["rho1", "rho2", "rho3", "rho_medio"] {
        assert!(
            analysis
                .warnings
                .iter()
                .any(|w| w.contains(&format!("({symbol} ="))),
            "falta la advertencia de valor no finito para {symbol}: {:?}",
            analysis.warnings
        );
        let derived = analysis
            .derived
            .iter()
            .find(|d| d.symbol == symbol)
            .unwrap();
        assert!(!derived.value.is_finite());
    }
    assert!(!analysis.warnings.iter().any(|w| w.contains("(E1 =")));
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

    let mut measurements = Vec::new();
    let mut push = |symbol: &str, value: f64| {
        let id = def
            .quantities
            .iter()
            .find(|q| q.symbol == symbol)
            .unwrap_or_else(|| panic!("falta la magnitud {symbol}"))
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
        });
    };
    push("m_goma", m_goma);
    push("rho_f", rho_f);
    push("g_ac", g_ac);
    push("b_E", 10.0);
    for i in 1..=9 {
        push(&format!("b{i}"), i as f64);
    }
    for k in 1..=3 {
        for i in 1..=9 {
            let masa = match i {
                3 => 1.0,
                7 => 2.0,
                _ => 0.0,
            };
            push(&format!("m{k}_{i}"), masa);
        }
    }
    push("l", 5.0);
    push("d", 10.0);
    for k in 1..=3 {
        push(&format!("mw{k}"), 0.5);
        push(&format!("dw{k}"), 4.0);
    }

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

    let analysis = flattened(analyze(&pool, "hidrostatica", &measurements).await.unwrap());
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
        analysis
            .derived
            .iter()
            .find(|d| d.symbol == symbol)
            .unwrap_or_else(|| panic!("falta el mensurando {symbol}"))
            .value
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
async fn analyze_rejects_a_slot_outside_the_balance() {
    let (pool, _dir) = setup().await;
    let def = crate::practices::definition(&pool, "hidrostatica")
        .await
        .unwrap()
        .unwrap();
    let dw1 = def.quantities.iter().find(|q| q.symbol == "dw1").unwrap();
    assert_eq!(dw1.slot_count, Some(9));
    let with_value = |value: f64| {
        vec![MeasurementInput {
            quantity_id: dw1.id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![value],
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
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
    };
    let measurements = vec![
        m("L", vec![1.0], Some(0.002)),
        m("t_med", vec![2.0], Some(0.0)),
        m("T1", vec![2.0, 2.02, 1.98], None),
    ];
    let mk = || FormSubmissionInput {
        course_id: course.id.clone(),
        group_id: group.id.clone(),
        practice_id: "p1-estadistica".into(),
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
         WHERE practice_id = 'p1-estadistica' AND symbol = 'g1'",
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

    let v2 = crate::practices::publish_version(&pool, "p1-estadistica", None)
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
    let practice = "p1-estadistica";
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
