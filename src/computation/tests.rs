use super::*;
use crate::db;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use tempfile::TempDir;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() < tol
}

async fn setup() -> (SqlitePool, TempDir) {
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

/// `validate_student_results` acepta agregados (Motor F) marcados `is_final` además de
/// resultados comunes: Re_max/M_teorico en Fluidos II son agregados, no `practice_results`.
#[tokio::test]
async fn validate_student_results_accepts_final_aggregates() {
    let (pool, _dir) = setup().await;
    validate_student_results(
        &pool,
        "fluidos-2",
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
    .await
    .unwrap();

    let err = validate_student_results(
        &pool,
        "fluidos-2",
        &[db::StudentResultInput {
            symbol: "no_existe".into(),
            value: 1.0,
            u_expanded: None,
        }],
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("no_existe"));
}

/// `validate_student_results` acepta resultados por corrida (Motor E) con símbolo compuesto
/// `Re#k`, y rechaza índices no numéricos o bases que no son point_result.
#[tokio::test]
async fn validate_student_results_accepts_point_results_per_run() {
    let (pool, _dir) = setup().await;
    validate_student_results(
        &pool,
        "viscosidad",
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
    .await
    .unwrap();

    for bad in ["Re#x", "Nope#0"] {
        let err = validate_student_results(
            &pool,
            "viscosidad",
            &[db::StudentResultInput {
                symbol: bad.into(),
                value: 1.0,
                u_expanded: None,
            }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains(bad));
    }
}

/// `check_student_result_symbols` (pura): acepta resultados finales, agregados `is_final` y
/// resultados por corrida `Re#k`; rechaza agregados no finales, símbolos desconocidos, bases que
/// no son point_result e índices de corrida no numéricos. Lista vacía siempre válida.
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

/// Atajo de test: `compute_regresion` sin derivadas por punto, agregados ni escalas (firma previa).
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
    // 'z' no es una magnitud declarada.
    assert!(compile_formula("l*z", &symbols).is_err());
    // paréntesis sin cerrar -> sintaxis inválida.
    assert!(compile_formula("(l*a", &symbols).is_err());
}

#[test]
fn compute_propagates_q_l_a_b() {
    // Q = l*a + l*b con medias 2,3,4 e incertidumbres dadas -> valor 14, u_Q 0.9
    // (mismo caso que el test analítico de uncertainty::propagate).
    // Para forzar u_c = 0.1/0.2/0.2 sin tipo B usamos lecturas con esa s/sqrt(n).
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
    // Una sola lectura -> u_A = 0, sin escala -> u_B = 0 -> u_c = 0.
    assert!(close(q_l.result.u_c, 0.0, 1e-12));
    assert_eq!(analysis.derived.len(), 1);
    assert!(close(analysis.derived[0].value, 14.0, 1e-9));
    // u_c todas cero -> u_Q = 0.
    assert!(close(analysis.derived[0].u, 0.0, 1e-9));
}

#[test]
fn compute_propagates_uncertainty_to_measurand() {
    // l con réplicas [9, 11] -> media 10, s = √2, u_A = s/√2 = 1.0; a=2, b=3 (sin u).
    // Q = l*a + l*b = 50; ∂Q/∂l = a+b = 5 -> u_Q = 5 * 1.0 = 5.0.
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
    // Motor D: T (repetida) se carga por operador; L (medida única) es compartida.
    // g = T + L. op1: T=10 → g=15 ; op2: T=20 → g=25. Sin promedio entre operadores.
    let t = quantity("T"); // repeated = true → por operador
    let mut l = quantity("L");
    l.repeated = false; // medida única → compartida
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

    // Compartida L una sola vez en `quantities`; nada en el `derived` de nivel superior.
    assert_eq!(a.quantities.len(), 1);
    assert_eq!(a.quantities[0].symbol, "L");
    assert!(a.derived.is_empty());

    // Un bloque por operador: su T y su g, sin promediar entre operadores.
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
    // P1 sembrada: T (periodo, repetido) + L (dado). Cargo réplicas de T con dispersión conocida.
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
    // Usuario docente (puede entregar sin estar en grupo); curso/grupo de prueba.
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
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    assert_eq!(detail.entry_mode, "form");
    // El analysis es el FormAnalysis serializado (tiene "quantities").
    assert!(detail.analysis.get("quantities").is_some());
    // La meta de depuración se persiste y se lee de vuelta intacta.
    let meta = detail.measurement_meta.expect("meta persistida");
    assert_eq!(meta["q1"]["bins"], 8);
    assert_eq!(meta["q1"]["discarded"][0], 9.9);
}

/// Las observaciones/comentarios del alumno son opcionales, se persisten recortando
/// espacios, y un texto en blanco (o ausente) queda como `None` en vez de una cadena vacía.
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
    };

    // Con texto (con espacios de sobra): se persiste recortado.
    let detail = create_form_submission(&pool, &user, mk(Some("  anduvo mal el generador  ")))
        .await
        .unwrap();
    assert_eq!(
        detail.student_comment.as_deref(),
        Some("anduvo mal el generador")
    );

    // En blanco: se persiste como None, no como cadena vacía.
    let blank = create_form_submission(&pool, &user, mk(Some("   ")))
        .await
        .unwrap();
    assert_eq!(blank.student_comment, None);

    // Ausente (None): también None.
    let absent = create_form_submission(&pool, &user, mk(None))
        .await
        .unwrap();
    assert_eq!(absent.student_comment, None);
}

/// El alumno puede entregar opcionalmente su resultado final (p. ej. `g`) junto con la
/// medición; queda persistido igual que si lo hubiese cargado luego por separado.
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
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    assert_eq!(detail.student_results.len(), 1);
    assert_eq!(detail.student_results[0].symbol, "g1");
    assert_eq!(detail.student_results[0].value, 9.8);
}

/// Un símbolo que no es mensurando de la práctica se rechaza antes de crear la entrega.
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
    };
    let err = create_form_submission(&pool, &user, input)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no-existe"));
}

#[tokio::test]
async fn operator_submission_stores_operator_index_per_operator() {
    // Estadístico con operadores: la magnitud repetida T guarda cada operador con su
    // operator_index (replicate_index = réplica dentro del operador), para reconstruir al editar.
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
            // Operador 0: [1.0, 1.1] ; operador 1: [2.0, 2.1, 2.2].
            operator_replicas: Some(vec![vec![1.0, 1.1], vec![2.0, 2.1, 2.2]]),
        }],
        meta: None,
        table_number: None,
        student_results: vec![],
        student_comment: None,
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let rows = db::measurements_for(&pool, &detail.id).await.unwrap();
    let t_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == t_id).collect();

    // Cada operador guarda su cantidad propia de réplicas con replicate_index contiguo.
    assert_eq!(t_rows.len(), 5); // 2 + 3
    for (op, expected_n) in [(0i64, 2usize), (1, 3)] {
        let mut reps: Vec<i64> = t_rows
            .iter()
            .filter(|m| m.operator_index == op)
            .map(|m| m.replicate_index)
            .collect();
        reps.sort_unstable();
        assert_eq!(reps, (0..expected_n as i64).collect::<Vec<_>>());
    }

    // El análisis trae un bloque por operador (g por operador, sin agregado).
    let operators = detail.analysis["operators"].as_array().unwrap();
    assert_eq!(operators.len(), 2);
}

#[tokio::test]
async fn point_based_submission_stores_point_index_per_point() {
    // Una entrega por puntos (curva/regresión) guarda cada punto con su point_index (no en
    // replicate_index), para que la edición reconstruya la serie completa. Cubre el fix del
    // bug de prefill. Se usa `curva` (misma ruta de persistencia point_based, sin derivar
    // mensurandos que en p1 referencian T/L y no encajarían en el modo regresión).
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
    // P1 como curva (fixture generico, no la forma real de la practica): T1 (un valor por
    // punto) vs T2 (réplicas por punto).
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
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let rows = db::measurements_for(&pool, &detail.id).await.unwrap();

    // T1: un valor por punto → point_index 0,1,2 y replicate_index 0.
    let mut t_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T1")).collect();
    t_rows.sort_by_key(|m| m.point_index);
    assert_eq!(t_rows.len(), 3);
    for (i, m) in t_rows.iter().enumerate() {
        assert_eq!(m.point_index, i as i64);
        assert_eq!(m.replicate_index, 0);
        assert!(close(m.value, (i + 1) as f64, 1e-9));
    }

    // T2: réplicas por punto → point_index = punto, replicate_index = réplica.
    let tmed_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T2")).collect();
    assert_eq!(tmed_rows.len(), 6); // 3 puntos x 2 réplicas
    assert!(tmed_rows
        .iter()
        .any(|m| m.point_index == 0 && m.replicate_index == 1 && close(m.value, 4.2, 1e-9)));
    assert!(tmed_rows
        .iter()
        .any(|m| m.point_index == 2 && m.replicate_index == 0 && close(m.value, 6.0, 1e-9)));
}

#[tokio::test]
async fn point_based_submission_stores_variable_replicas_per_point() {
    // Cada punto puede traer una cantidad distinta de réplicas (p.ej. una esfera medida 1 vez,
    // otra 3): se persisten todas con replicate_index 0..n del punto y el motor promedia con el
    // n real de cada punto. Cubre el caso de grilla "irregular".
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
    // Punto 0: 1 réplica; punto 1: 3 réplicas; punto 2: 2 réplicas. Medias: 4.0, 5.1, 6.05.
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
    };
    let detail = create_form_submission(&pool, &user, input).await.unwrap();
    let rows = db::measurements_for(&pool, &detail.id).await.unwrap();

    // Cada punto guarda su cantidad propia de réplicas (1 + 3 + 2 = 6), con replicate_index
    // contiguo desde 0 dentro del punto.
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

    // El motor promedia con el n real de cada punto: la curva usa (T, media de réplicas).
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
    };
    let created = create_form_submission(&pool, &user, mk(vec![5.0, 5.2, 4.9]))
        .await
        .unwrap();
    // Recién creada: editable (ventana abierta, pendiente, no visible).
    assert!(created.can_edit);
    assert!(created.editable_until.is_some());

    let edited = update_form_submission(
        &pool,
        &created.id,
        "p1-estadistica",
        &mk(vec![10.0, 12.0, 11.0]).measurements,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    // Las lecturas crudas reflejan los nuevos valores (3 réplicas: 10, 12, 11).
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

/// Cancelar (`db::delete_submission`) borra la entrega por completo: deja de existir
/// (`submission_detail` devuelve `None`) y libera la mesa para una nueva entrega en la
/// misma (práctica, grupo, mesa) — el índice único ya no choca.
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
    };
    let created = create_form_submission(&pool, &user, mk()).await.unwrap();

    // Antes de cancelar: hay resultado del alumno y un integrante (owner).
    assert_eq!(created.student_results.len(), 1);
    assert_eq!(created.members.len(), 1);

    // Cancelar: la mesa 1 de esta práctica/grupo ya está ocupada por `created`.
    assert!(
        db::find_existing_report(&pool, "p1-estadistica", &group.id, 1)
            .await
            .unwrap()
            .is_some()
    );

    let existed = db::delete_submission(&pool, &created.id).await.unwrap();
    assert!(existed);

    // La entrega deja de existir.
    assert!(db::submission_detail(&pool, &created.id)
        .await
        .unwrap()
        .is_none());

    // La mesa quedó libre: ya no hay informe para (práctica, grupo, 1).
    assert!(
        db::find_existing_report(&pool, "p1-estadistica", &group.id, 1)
            .await
            .unwrap()
            .is_none()
    );

    // Cancelar de nuevo (id ya borrado) es un no-op, no un error.
    assert!(!db::delete_submission(&pool, &created.id).await.unwrap());

    // Se puede crear una entrega nueva para la misma mesa sin chocar con el índice único.
    let recreated = create_form_submission(&pool, &user, mk()).await.unwrap();
    assert_ne!(recreated.id, created.id);
    assert_eq!(recreated.table_number, Some(1));
}

/// `update_form_submission` reemplaza el comentario del alumno igual que las lecturas:
/// se puede agregar uno donde no había, cambiarlo, y en blanco vuelve a quedar en `None`.
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
    };
    let created = create_form_submission(&pool, &user, input).await.unwrap();
    assert_eq!(created.student_comment, None);

    // Agrega un comentario donde no había.
    let edited = update_form_submission(
        &pool,
        &created.id,
        "p1-estadistica",
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

    // Lo borra (en blanco vuelve a None, no queda pegado el anterior).
    let cleared = update_form_submission(
        &pool,
        &created.id,
        "p1-estadistica",
        &measurements,
        None,
        None,
        None,
    )
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
    };
    assert!(create_form_submission(&pool, &user, input).await.is_err());
    // Rollback: no debe quedar ninguna entrega ni medición.
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
    // y = 2x + 1 con ejes triviales (x = px, y = py).
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
    // Los mensurandos derivan de slope/intercept.
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
    // Motor C: la intermedia Q = V/t se evalúa por réplica y se promedia por punto, NO como
    // media(V)/media(t). Punto 0: V=[10,10], t=[1,2] → Q=(10+5)/2=7.5 (media(V)/media(t) daría
    // 10/1.5≈6.67). Punto 1: V=[20,20], t=[1,2] → Q=(20+10)/2=15.
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

/// Helper de test: una magnitud con réplicas por punto.
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
    // Difusión: D = h*V/t, con h de un solo valor por punto y V,t con 2 réplicas. h se difunde.
    // Punto 0: h=2, V=[10,10], t=[1,2] → D=(2*10/1 + 2*10/2)/2 = (20+10)/2 = 15.
    // Punto 1: h=3, V=[20,20], t=[1,2] → D=(60+30)/2 = 45.
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
    // Una intermedia con una constante (pi): A = pi*r*r, r de un solo valor por punto.
    // pi lo precarga el evaluador; no debe quedar bindeado a NaN. r=[2,3] → A=pi*4, pi*9.
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
    // Dos magnitudes replicadas con distinto conteo (V con 2, t con 1) en una intermedia son
    // dato incompleto: NO se difunde la réplica faltante → el punto da NaN → se rechaza.
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
        point_rep("q-t", vec![vec![1.0], vec![1.0]]), // replicada pero con 1 sola réplica
        measurement("py", &[100.0, 200.0]),
    ];
    assert!(reg(&quantities, &intermediates, &[], "Q", "py", &measurements).is_err());
}

#[test]
fn intermediate_can_reference_an_earlier_intermediate() {
    // Encadenado: Q = V/t (promedio por réplica), R = Q*2 (ve a Q como su valor por punto).
    // Punto 0: Q=(10/1+10/2)/2=7.5 → R=15 ; punto 1: Q=(20/1+20/2)/2=15 → R=30.
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
    // x = 2*pi*f ; y = math::sqrt(a). f=[1,2,3], a=[4,9,16] -> x=2pi*{1,2,3}, y={2,3,4}.
    // y crece 1 por unidad de f, x crece 2pi por unidad de f -> slope = 1/(2pi), intercept = 1.
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
    // Las etiquetas de eje conservan las fórmulas para rotular el gráfico.
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
    // Motor E: px,py por punto (slope=2, intercept=0); c escalar compartido=10. Mensurando
    // m = slope*c = 20 (usa un escalar, no solo slope). Derivada por punto Re = px*m por corrida.
    let mut c = quantity("c");
    c.per_point = false; // escalar compartido (se carga una vez)
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
    // El mensurando usa el escalar compartido + slope: m = 2 * 10 = 20.
    assert!(close(
        a.derived.iter().find(|d| d.symbol == "m").unwrap().value,
        20.0,
        1e-9
    ));
    // La derivada por punto da un valor por corrida: Re = px * m = {20, 40}.
    let re = a.point_results.iter().find(|p| p.symbol == "Re").unwrap();
    assert_eq!(re.values.len(), 2);
    assert!(close(re.values[0], 20.0, 1e-9));
    assert!(close(re.values[1], 40.0, 1e-9));
}

#[test]
fn compute_regresion_aggregates_use_endpoints_measurands_and_chain() {
    // Motor F: px por punto = [1,2,3], py = [2,4,6] → slope=2. c escalar compartido = 10.
    // Mensurando m = slope*c = 20. Agregados escalares (un valor) que usan:
    //  - extremos por punto: ep = px_first + px_last = 1 + 3 = 4; mid = px_first2 + px_last2 = 2+2 = 4
    //  - un mensurando + slope: g = m + slope = 22
    //  - un agregado anterior (encadenable): chained = ep + g = 26
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
    // Motor F: un agregado con división por cero (px_first - px_first = 0) da no finito y debe
    // avisar (sin abortar el resto del análisis), igual que los mensurandos derivados.
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
    // z es magnitud por punto que NO aparece en los ejes (x=px, y=py): build_points la omite
    // del conditioning set y n_points = 3 (de px/py). Con la lógica anterior z_last leía
    // contexts[2]["z"] = z[2] = 30 aunque z tiene 4 filas (z[3]=40 es el extremo correcto).
    // Con la corrección se usa la serie propia de z, no contexts.
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
    // El extremo de z (4 puntos) sale de su serie propia, pero el ajuste tiene 3: debe avisar.
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
    // z (por punto) tiene 4 filas vs 3 del ajuste, pero NINGÚN agregado usa un extremo de z:
    // no debe avisar (sin ruido por magnitudes no referenciadas en extremos).
    let quantities = vec![quantity("px"), quantity("py"), quantity("z")];
    let aggregates = vec![PracticeAggregate {
        id: "a-s".into(),
        practice_id: "p".into(),
        position: 0,
        symbol: "s".into(),
        name: "s".into(),
        unit: "".into(),
        formula: "slope".into(), // no toca z_first/z_last/...
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
    // Motor E: una intermedia usa un escalar compartido (c, per_point=false), que se difunde a
    // todos los puntos. D = px/c con px=[1,2], c=10 → D=[0.1,0.2]. (Antes daba NaN en el 2º punto.)
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
    // Un escalar compartido con varias lecturas (c=[10,20]) se colapsa a su media (15) y se usa
    // igual en todos los puntos; no varía por punto. D = px/c → {1/15, 2/15} (no {1/10, 2/20}).
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
        measurement("c", &[10.0, 20.0]), // dos lecturas de un escalar compartido
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
    // Curva sin ajuste: evalúa los ejes y produce los puntos, sin slope/intercept ni derivados.
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
    // Motor B: varias curvas sobre el mismo barrido producen una entrada en `scatters` cada una.
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
    // Con eje x logarítmico, un x <= 0 es inválido.
    let quantities = vec![quantity("px"), quantity("py")];
    let measurements = vec![
        measurement("px", &[0.0, 10.0]),
        measurement("py", &[1.0, 2.0]),
    ];
    assert!(compute_curva(&quantities, &[], &[curve("px", "py", true)], &measurements).is_err());
}

#[test]
fn build_points_ignores_quantities_not_in_axes() {
    // 'aux' no aparece en las fórmulas de eje y no tiene mediciones: no debe bloquear ni
    // arrastrar el conteo de puntos (regresión: antes el mínimo común la incluía y daba 0).
    let quantities = vec![quantity("px"), quantity("py"), quantity("aux")];
    let measurements = vec![
        measurement("px", &[1.0, 2.0, 3.0]),
        measurement("py", &[4.0, 5.0, 6.0]),
        // 'aux' sin mediciones a propósito.
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
    // 'py' mide réplicas por punto: el motor usa la media de cada punto en el eje.
    // Punto 1: media(3,5)=4 ; punto 2: media(8,10,12)=10.
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
    // Pendiente de (1,4)-(2,10) = 6.
    assert!(close(reg.slope, 6.0, 1e-9));
}

/// Mediciones reales para una magnitud sembrada, buscando su id por símbolo en la definición.
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
    // La práctica Fluidos I sembrada: regresión h/Q² vs 1/Q con Q = V/t (intermedia), escalares
    // compartidos (R, L, g de cátedra; rho medida única) → μ de la pendiente, Reynolds por corrida.
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
        mk("h", vec![0.30, 0.10], None, None), // un valor por punto (2 alturas)
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
    // μ derivado de la pendiente + escalares compartidos: finito.
    let mu = a.derived.iter().find(|d| d.symbol == "mu").expect("mu");
    assert!(mu.value.is_finite() && mu.value > 0.0);
    // Reynolds: una columna por corrida, valores finitos.
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
    // Viscosidad (Stokes): ajuste v_lim (= dx/t medio) vs R^2; μ de la pendiente; Re por esfera.
    // Sin intermedia: y = dx/t usa la media de las réplicas de t (Motor A).
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
    // 2 esferas (puntos): radios distintos, 3 tiempos c/u; dx, densidades, g compartidos.
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
    // Punto: x = R^2, y = dx/t (t medio). Esfera 1: (1e-6, 0.20/20=0.01).
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
    // Configuramos P1 como curva con ejes sobre sus propias magnitudes (fixture generico:
    // T1 vs T2, no la forma real "estadistico" de la practica).
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
    // L y t_med son magnitudes auxiliares (no estan en los ejes T1 vs T2): se omiten a
    // proposito y build_points debe ignorarlas sin exigirles mediciones.
    let measurements = vec![
        measurement_for(&def, "T1", &[1.0, 2.0, 3.0]),
        measurement_for(&def, "T2", &[4.0, 5.0, 6.0]),
    ];
    let analysis = analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();
    assert!(analysis.regression.is_none());
    // Una única curva en la lista → un scatter sin ajuste ni mensurandos.
    assert_eq!(analysis.scatters.len(), 1);
    assert_eq!(
        analysis.scatters[0].points,
        vec![(1.0, 4.0), (2.0, 5.0), (3.0, 6.0)]
    );
}

#[tokio::test]
async fn analyze_curva_graphs_each_curve_in_the_list() {
    // Una práctica `curva` grafica una entrada en `scatters` por cada curva de la lista.
    let (pool, _dir) = setup().await;
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
    // Sin curvas definidas, el dispatcher debe fallar con un error claro (no entrar al cálculo).
    let result = analyze(&pool, "p1-estadistica", &[]).await;
    assert!(result.is_err());
}
