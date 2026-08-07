use super::*;
use crate::db;
use seed::{fix_ca_rlc_labels, seed_ca_rlc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tempfile::TempDir;

/// Pool temporal migrado con las tres prácticas sembradas.
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
    (pool, dir)
}

fn sample_quantity() -> QuantityInput {
    QuantityInput {
        symbol: "l".into(),
        name: "Longitud".into(),
        unit: "mm".into(),
        repeated: true,
        quantity: Some("longitud".into()),
        is_given: false,
        replicas_per_point: None,
        per_point: true,
        has_uncertainty: true,
        optional: false,
    }
}

fn sample_result() -> ResultInput {
    ResultInput {
        symbol: "Q".into(),
        name: "Area".into(),
        unit: "mm2".into(),
        formula: "l*a".into(),
        tolerance: None,
        is_final: false,
        has_uncertainty: true,
    }
}

#[tokio::test]
async fn definition_returns_none_for_unknown_practice() {
    let (pool, _dir) = setup().await;
    assert!(definition(&pool, "no-existe").await.unwrap().is_none());
}

#[tokio::test]
async fn create_and_list_quantities() {
    let (pool, _dir) = setup().await;
    let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
        .await
        .unwrap();
    assert_eq!(q.symbol, "l");
    assert_eq!(q.practice_id, "p1-estadistica");

    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.quantities.len(), 1);
    assert_eq!(def.quantities[0].id, q.id);
}

#[tokio::test]
async fn update_and_delete_quantity() {
    let (pool, _dir) = setup().await;
    let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
        .await
        .unwrap();

    let updated = update_quantity(
        &pool,
        &q.id,
        QuantityInput {
            symbol: "a".into(),
            name: "Ancho".into(),
            unit: "cm".into(),
            repeated: false,
            quantity: None,
            is_given: false,
            replicas_per_point: None,
            per_point: true,
            has_uncertainty: true,
            optional: false,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.symbol, "a");
    assert!(!updated.repeated);

    assert!(delete_quantity(&pool, &q.id).await.unwrap());
    assert!(!delete_quantity(&pool, &q.id).await.unwrap());
}

#[tokio::test]
async fn create_and_delete_result() {
    let (pool, _dir) = setup().await;
    let r = create_result(&pool, "p1-estadistica", sample_result())
        .await
        .unwrap();
    assert_eq!(r.symbol, "Q");
    assert_eq!(r.formula, "l*a");

    assert!(delete_result(&pool, &r.id).await.unwrap());
    assert!(!delete_result(&pool, &r.id).await.unwrap());
}

/// `is_final` se persiste al crear y se puede togglear al actualizar (checkbox docente en UI).
#[tokio::test]
async fn create_and_update_result_toggles_is_final() {
    let (pool, _dir) = setup().await;
    let r = create_result(
        &pool,
        "p1-estadistica",
        ResultInput {
            is_final: true,
            ..sample_result()
        },
    )
    .await
    .unwrap();
    assert!(r.is_final);

    let updated = update_result(
        &pool,
        &r.id,
        ResultInput {
            is_final: false,
            ..sample_result()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!updated.is_final);
}

#[tokio::test]
async fn duplicate_symbol_is_rejected() {
    let (pool, _dir) = setup().await;
    create_quantity(&pool, "p1-estadistica", sample_quantity())
        .await
        .unwrap();
    // Mismo símbolo en la misma práctica debe fallar (UNIQUE constraint).
    let err = create_quantity(&pool, "p1-estadistica", sample_quantity()).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn symbol_taken_detects_cross_table_collision() {
    let (pool, _dir) = setup().await;
    // Crea una magnitud con símbolo "l".
    let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
        .await
        .unwrap();

    // symbol_taken_in_practice lo detecta buscando en quantities.
    assert!(
        symbol_taken_in_practice(&pool, "p1-estadistica", "l", None, None, None, None, None)
            .await
            .unwrap()
    );
    // Excluir la misma magnitud (al renombrar) no debe reportar colisión.
    assert!(!symbol_taken_in_practice(
        &pool,
        "p1-estadistica",
        "l",
        Some(&q.id),
        None,
        None,
        None,
        None
    )
    .await
    .unwrap());

    // Crea un mensurando con símbolo "Q".
    let r = create_result(&pool, "p1-estadistica", sample_result())
        .await
        .unwrap();

    // Un mensurando nuevo con símbolo "l" (ya en quantities) es colisión cruzada.
    assert!(symbol_taken_in_practice(
        &pool,
        "p1-estadistica",
        "l",
        None,
        Some(&r.id),
        None,
        None,
        None
    )
    .await
    .unwrap());
    // Una magnitud nueva con símbolo "Q" (ya en results) es colisión cruzada.
    assert!(symbol_taken_in_practice(
        &pool,
        "p1-estadistica",
        "Q",
        Some(&q.id),
        None,
        None,
        None,
        None
    )
    .await
    .unwrap());

    // Una magnitud intermedia con símbolo "Iv": magnitudes/mensurandos nuevos deben colisionar.
    create_intermediate(
        &pool,
        "p1-estadistica",
        IntermediateInput {
            symbol: "Iv".into(),
            name: "Iv".into(),
            unit: "u".into(),
            formula: "l".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        symbol_taken_in_practice(&pool, "p1-estadistica", "Iv", None, None, None, None, None)
            .await
            .unwrap()
    );

    // Una magnitud derivada por punto con símbolo "Re": el resto debe colisionar con ella.
    create_point_result(
        &pool,
        "p1-estadistica",
        PointResultInput {
            symbol: "Re".into(),
            name: "Re".into(),
            unit: "".into(),
            formula: "L".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        symbol_taken_in_practice(&pool, "p1-estadistica", "Re", None, None, None, None, None)
            .await
            .unwrap()
    );

    // Un mensurando agregado (Motor F) con símbolo "Ma": el resto debe colisionar con él.
    create_aggregate(
        &pool,
        "p1-estadistica",
        AggregateInput {
            symbol: "Ma".into(),
            name: "Ma".into(),
            unit: "".into(),
            formula: "slope".into(),
            is_final: false,
        },
    )
    .await
    .unwrap();
    assert!(
        symbol_taken_in_practice(&pool, "p1-estadistica", "Ma", None, None, None, None, None)
            .await
            .unwrap()
    );

    // Símbolo inexistente no colisiona.
    assert!(!symbol_taken_in_practice(
        &pool,
        "p1-estadistica",
        "nuevo",
        None,
        None,
        None,
        None,
        None
    )
    .await
    .unwrap());
}

/// CRUD de mensurandos agregados (Motor F): alta asigna posición, lectura ordena, edición cambia
/// campos, baja elimina y devuelve `true`/`false` según existiera.
#[tokio::test]
async fn aggregate_crud_roundtrip() {
    let (pool, _dir) = setup().await;
    let mk = |symbol: &str, formula: &str| AggregateInput {
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "".into(),
        formula: formula.into(),
        is_final: false,
    };
    let a = create_aggregate(&pool, "p1-estadistica", mk("Re_max", "slope"))
        .await
        .unwrap();
    let b = create_aggregate(&pool, "p1-estadistica", mk("Re_min", "intercept"))
        .await
        .unwrap();
    assert!(b.position > a.position, "la 2da toma la siguiente posición");

    let listed = aggregates_for(&pool, "p1-estadistica").await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].symbol, "Re_max", "ordenado por posición");

    let updated = update_aggregate(&pool, "p1-estadistica", &a.id, mk("Re_max", "slope * 2"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.formula, "slope * 2");

    assert!(delete_aggregate(&pool, "p1-estadistica", &a.id)
        .await
        .unwrap());
    assert!(
        !delete_aggregate(&pool, "p1-estadistica", &a.id)
            .await
            .unwrap(),
        "borrar de nuevo devuelve false"
    );
    assert_eq!(
        aggregates_for(&pool, "p1-estadistica").await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn set_analysis_kind_updates_practice() {
    let (pool, _dir) = setup().await;
    assert!(
        set_analysis_kind(&pool, "p1-estadistica", "regresion_lineal")
            .await
            .unwrap()
    );
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
}

#[tokio::test]
async fn set_regression_formulas_updates_and_normalizes_empty() {
    let (pool, _dir) = setup().await;
    assert!(set_regression_formulas(
        &pool,
        "p1-estadistica",
        "2*pi*f",
        "b / math::sqrt(a*a - b*b)",
    )
    .await
    .unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.x_formula.as_deref(), Some("2*pi*f"));
    assert_eq!(def.y_formula.as_deref(), Some("b / math::sqrt(a*a - b*b)"));

    // Una cadena vacía (o solo espacios) guarda NULL.
    assert!(set_regression_formulas(&pool, "p1-estadistica", "   ", "")
        .await
        .unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.x_formula, None);
    assert_eq!(def.y_formula, None);

    // Práctica inexistente devuelve false.
    assert!(!set_regression_formulas(&pool, "no-existe", "f", "f")
        .await
        .unwrap());
}

#[tokio::test]
async fn curve_crud_roundtrip_and_ordering() {
    let (pool, _dir) = setup().await;
    // Alta de dos curvas: quedan ordenadas por posición creciente, con x_log por curva.
    let c1 = create_curve(
        &pool,
        "p1-estadistica",
        CurveInput {
            x_formula: " logw ".into(), // se recorta
            y_formula: "VR / Vg".into(),
            x_log: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(c1.x_formula, "logw");
    assert!(c1.x_log);
    create_curve(
        &pool,
        "p1-estadistica",
        CurveInput {
            x_formula: "logw".into(),
            y_formula: "phi".into(),
            x_log: false,
        },
    )
    .await
    .unwrap();

    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.curves.len(), 2);
    assert_eq!(def.curves[0].position, 1);
    assert_eq!(def.curves[0].y_formula, "VR / Vg");
    assert_eq!(def.curves[1].position, 2);
    assert_eq!(def.curves[1].y_formula, "phi");

    // Edición de una curva (acotada por práctica).
    let updated = update_curve(
        &pool,
        "p1-estadistica",
        &c1.id,
        CurveInput {
            x_formula: "logw".into(),
            y_formula: "Vg / VR".into(),
            x_log: false,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.y_formula, "Vg / VR");
    assert!(!updated.x_log);

    // Editar/borrar con la práctica equivocada no afecta la curva (el id no pertenece a esa
    // práctica): update → None, delete → false.
    assert!(update_curve(
        &pool,
        "p2-cc",
        &c1.id,
        CurveInput {
            x_formula: "a".into(),
            y_formula: "b".into(),
            x_log: false,
        },
    )
    .await
    .unwrap()
    .is_none());
    assert!(!delete_curve(&pool, "p2-cc", &c1.id).await.unwrap());

    // Baja correcta: queda una sola curva.
    assert!(delete_curve(&pool, "p1-estadistica", &c1.id).await.unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.curves.len(), 1);
    assert_eq!(def.curves[0].y_formula, "phi");
}

#[tokio::test]
async fn move_curve_swaps_position_with_neighbor() {
    let (pool, _dir) = setup().await;
    let mk = |y: &str| CurveInput {
        x_formula: "logw".into(),
        y_formula: y.into(),
        x_log: false,
    };
    let a = create_curve(&pool, "p1-estadistica", mk("a"))
        .await
        .unwrap();
    create_curve(&pool, "p1-estadistica", mk("b"))
        .await
        .unwrap();
    let c = create_curve(&pool, "p1-estadistica", mk("c"))
        .await
        .unwrap();

    // 'a' no puede subir (ya es la primera); 'c' no puede bajar (ya es la última).
    assert!(!move_curve(&pool, "p1-estadistica", &a.id, true)
        .await
        .unwrap());
    assert!(!move_curve(&pool, "p1-estadistica", &c.id, false)
        .await
        .unwrap());

    // Bajar 'a' la intercambia con 'b' → orden b, a, c.
    assert!(move_curve(&pool, "p1-estadistica", &a.id, false)
        .await
        .unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(
        def.curves
            .iter()
            .map(|c| c.y_formula.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a", "c"]
    );

    // Curva inexistente devuelve false.
    assert!(!move_curve(&pool, "p1-estadistica", "no-existe", true)
        .await
        .unwrap());
}

#[tokio::test]
async fn deleting_practice_cascades_to_curves() {
    let (pool, _dir) = setup().await;
    create_curve(
        &pool,
        "p1-estadistica",
        CurveInput {
            x_formula: "logw".into(),
            y_formula: "VR / Vg".into(),
            x_log: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(curves_for(&pool, "p1-estadistica").await.unwrap().len(), 1);

    // Con foreign_keys activo, borrar la práctica arrastra sus curvas (ON DELETE CASCADE).
    sqlx::query("DELETE FROM practices WHERE id = ?1")
        .bind("p1-estadistica")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(curves_for(&pool, "p1-estadistica").await.unwrap().len(), 0);
}

#[tokio::test]
async fn create_curve_requires_both_formulas() {
    let (pool, _dir) = setup().await;
    assert!(create_curve(
        &pool,
        "p1-estadistica",
        CurveInput {
            x_formula: "logw".into(),
            y_formula: "  ".into(),
            x_log: false,
        },
    )
    .await
    .is_err());
}

#[tokio::test]
async fn intermediate_crud_roundtrip() {
    let (pool, _dir) = setup().await;
    let q = create_intermediate(
        &pool,
        "p1-estadistica",
        IntermediateInput {
            symbol: " Q ".into(), // se recorta
            name: "Caudal".into(),
            unit: "m3/s".into(),
            formula: "V/t".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(q.symbol, "Q");
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.intermediates.len(), 1);
    assert_eq!(def.intermediates[0].formula, "V/t");

    // Editar acotado por práctica; práctica equivocada → None.
    assert!(update_intermediate(
        &pool,
        "p2-cc",
        &q.id,
        IntermediateInput {
            symbol: "Q".into(),
            name: "x".into(),
            unit: "x".into(),
            formula: "V*t".into(),
        },
    )
    .await
    .unwrap()
    .is_none());
    let updated = update_intermediate(
        &pool,
        "p1-estadistica",
        &q.id,
        IntermediateInput {
            symbol: "Q".into(),
            name: "Caudal".into(),
            unit: "m3/s".into(),
            formula: "V*t".into(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.formula, "V*t");

    // Símbolo/fórmula vacíos → error.
    assert!(create_intermediate(
        &pool,
        "p1-estadistica",
        IntermediateInput {
            symbol: "Z".into(),
            name: "z".into(),
            unit: "".into(),
            formula: "   ".into(),
        },
    )
    .await
    .is_err());

    assert!(delete_intermediate(&pool, "p1-estadistica", &q.id)
        .await
        .unwrap());
    assert_eq!(
        intermediates_for(&pool, "p1-estadistica")
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn seed_definitions_populates_p1_and_is_idempotent() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    // P1 péndulo, 3 operadores independientes: L (is_given) + t_med (dato sin incertidumbre,
    // "t_1/2") + T1 (obligatorio) + T2/T3 (opcionales), todos repeated.
    assert_eq!(def.quantities.len(), 5);
    let l = def.quantities.iter().find(|q| q.symbol == "L").unwrap();
    assert!(l.is_given);
    let t_med = def.quantities.iter().find(|q| q.symbol == "t_med").unwrap();
    assert!(t_med.is_given);
    assert!(
        !t_med.has_uncertainty,
        "t_med no deberia pedir incertidumbre"
    );
    let t1 = def.quantities.iter().find(|q| q.symbol == "T1").unwrap();
    assert!(t1.repeated);
    assert!(!t1.optional, "el operador 1 es obligatorio");
    for symbol in ["T2", "T3"] {
        let t = def.quantities.iter().find(|q| q.symbol == symbol).unwrap();
        assert!(t.repeated);
        assert!(t.optional, "{symbol} deberia ser opcional");
    }

    assert_eq!(def.results.len(), 5);
    for symbol in ["gamma", "Q", "g1", "g2", "g3"] {
        assert!(
            def.results.iter().any(|r| r.symbol == symbol),
            "falta el resultado {symbol}"
        );
    }
    // gamma, Q, g1/g2/g3 son los resultados centrales que el alumno debe entregar.
    for symbol in ["gamma", "Q", "g1", "g2", "g3"] {
        let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(r.is_final, "{symbol} deberia ser final");
    }
    // gamma y Q van sin ±U (t_med tampoco, pero es magnitud, no resultado); g1/g2/g3 sí
    // muestran su incertidumbre (propagada de T1/T2/T3 y L).
    for symbol in ["gamma", "Q"] {
        let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(
            !r.has_uncertainty,
            "{symbol} no deberia mostrar incertidumbre"
        );
    }
    for symbol in ["g1", "g2", "g3"] {
        let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(r.has_uncertainty, "{symbol} deberia mostrar incertidumbre");
    }
    // Q usa el periodo del operador 1 (aclarado en el nombre para el docente).
    let q = def.results.iter().find(|r| r.symbol == "Q").unwrap();
    assert!(q.formula.contains("T1"));
    assert!(q.name.contains("Operador 1"));

    // Segunda pasada: no debe duplicar.
    seed_definitions(&pool).await.unwrap();
    let def2 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def2.quantities.len(), 5);
    assert_eq!(def2.results.len(), 5);
}

/// Extremo a extremo sobre la práctica real (no un fixture sintético): Operador 1 con datos,
/// Operador 2/3 sin cargar (opcionales, no deben bloquear ni romper el análisis). Verifica que
/// g1 se computa, g2/g3 quedan como advertencia (no pánico), t_med/gamma/Q dan U = 0 pese a
/// que T1 sí tiene incertidumbre real, y Q usa T1 (confirmado en el valor, no solo la fórmula).
#[tokio::test]
async fn analyze_p1_estadistica_con_operadores_opcionales() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let mk =
        |sym: &str, vals: Vec<f64>, given_u: Option<f64>| crate::computation::MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values: vals,
            given_u,
            point_replicas: None,
            operator_replicas: None,
        };

    let (l, t_med) = (1.0_f64, 2.006_f64);
    let t1_vals = vec![2.0_f64, 2.02, 1.98];
    let t1_mean = t1_vals.iter().sum::<f64>() / t1_vals.len() as f64;
    let measurements = vec![
        mk("L", vec![l], Some(0.002)),
        // t_med no tiene campo U en el form: aunque llegara un given_u (p. ej. de una entrega
        // vieja), has_uncertainty=false debe ignorarlo y dejar u=0.
        mk("t_med", vec![t_med], Some(99.0)),
        mk("T1", t1_vals, None),
        // T2/T3 sin cargar: opcionales, no deben bloquear el análisis.
    ];

    let analysis = crate::computation::analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();

    let t1_q = analysis
        .quantities
        .iter()
        .find(|q| q.symbol == "T1")
        .unwrap();
    assert!(t1_q.result.u_c > 0.0, "T1 sí debe tener incertidumbre real");

    let t_med_q = analysis
        .quantities
        .iter()
        .find(|q| q.symbol == "t_med")
        .unwrap();
    assert_eq!(
        t_med_q.result.u_c, 0.0,
        "t_med sin instrumento/U debe dar u=0 pese a un given_u cargado"
    );

    let derived = |sym: &str| {
        analysis
            .derived
            .iter()
            .find(|d| d.symbol == sym)
            .unwrap_or_else(|| panic!("{sym} debe estar en derived"))
    };

    let g1 = derived("g1");
    let expected_g1 = 4.0 * std::f64::consts::PI.powi(2) * l / (t1_mean * t1_mean);
    assert!(
        (g1.value - expected_g1).abs() < 1e-9,
        "g1 esperado {expected_g1}, obtenido {}",
        g1.value
    );

    // g2/g3 dependen de T2/T3, que no se cargaron: no deben tirar la práctica abajo, solo
    // avisar y quedar no-finitos.
    assert!(!derived("g2").value.is_finite());
    assert!(!derived("g3").value.is_finite());
    assert!(
        analysis
            .warnings
            .iter()
            .any(|w| w.contains("T2") || w.contains("g2")),
        "debe haber una advertencia por T2/g2 sin datos"
    );

    // gamma y Q se muestran sin ±U (has_uncertainty=false), aunque Q propague de fondo la
    // incertidumbre real de T1.
    let gamma = derived("gamma");
    assert!(!gamma.has_uncertainty);
    let expected_gamma = 2.0 * std::f64::consts::LN_2 / t_med;
    assert!((gamma.value - expected_gamma).abs() < 1e-9);

    let q = derived("Q");
    assert!(!q.has_uncertainty);
    let expected_q = std::f64::consts::PI * t_med / (t1_mean * std::f64::consts::LN_2);
    assert!(
        (q.value - expected_q).abs() < 1e-9,
        "Q debe usar T1: esperado {expected_q}, obtenido {}",
        q.value
    );
}

/// gamma/Q pasaron a ser resultado final despues del alta inicial; una base ya sembrada con
/// el esquema viejo (is_final=0) debe actualizarse via backfill, no requiere resembrar.
#[tokio::test]
async fn seed_definitions_backfills_gamma_and_q_as_final() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    sqlx::query(
        "UPDATE practice_results SET is_final = 0 \
             WHERE practice_id = 'p1-estadistica' AND symbol IN ('gamma', 'Q')",
    )
    .execute(&pool)
    .await
    .unwrap();

    seed_definitions(&pool).await.unwrap();

    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    for symbol in ["gamma", "Q"] {
        let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(r.is_final, "{symbol} deberia quedar final tras el backfill");
    }
}

#[tokio::test]
async fn deleting_practice_cascades_to_definition() {
    let (pool, _dir) = setup().await;
    create_quantity(&pool, "p1-estadistica", sample_quantity())
        .await
        .unwrap();
    create_result(&pool, "p1-estadistica", sample_result())
        .await
        .unwrap();
    // Con foreign_keys activo, borrar la práctica debe arrastrar magnitudes y mensurandos.
    sqlx::query("DELETE FROM practices WHERE id = 'p1-estadistica'")
        .execute(&pool)
        .await
        .unwrap();
    let quantities: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'p1-estadistica'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let results: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_results WHERE practice_id = 'p1-estadistica'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(quantities.0, 0);
    assert_eq!(results.0, 0);
}

#[tokio::test]
async fn seed_definitions_populates_p3_relajacion() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
    assert_eq!(def.quantities.len(), 5);
    for symbol in ["R", "Rint", "C", "T_oc", "tmedio"] {
        assert!(
            def.quantities.iter().any(|q| q.symbol == symbol),
            "falta la magnitud {symbol}"
        );
    }
    let tau_t = def
        .results
        .iter()
        .find(|r| r.symbol == "tau_teorico")
        .unwrap();
    assert_eq!(tau_t.formula, "(R + Rint) * C");
    assert!(def.results.iter().any(|r| r.symbol == "tau_exp"));
}

// Verifica que las fórmulas sembradas de P3 son evaluables por el motor (sin NaN/errores).
// Las de P2 (p2-cc) las cubre `analyze_p2_cc_derives_results_and_aliases`.
#[tokio::test]
async fn seeded_p3_formulas_compute() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();

    // P3: R=10000, Rint=100, C=1e-8, tmedio=7e-5 -> tau_teorico=(10100)*1e-8=1.01e-4
    let def3 = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
    let m3: Vec<crate::computation::MeasurementInput> = def3
        .quantities
        .iter()
        .map(|q| {
            let v = match q.symbol.as_str() {
                "R" => 10000.0,
                "Rint" => 100.0,
                "C" => 1e-8,
                _ => 7e-5,
            };
            crate::computation::MeasurementInput {
                quantity_id: q.id.clone(),
                instrument_id: None,
                scale_id: None,
                values: vec![v],
                given_u: if q.is_given { Some(0.0) } else { None },
                point_replicas: None,
                operator_replicas: None,
            }
        })
        .collect();
    let a3 = crate::computation::compute(
        &def3.quantities,
        &def3.results,
        &Default::default(),
        &m3,
        None,
    )
    .unwrap();
    let tau_t = a3
        .derived
        .iter()
        .find(|d| d.symbol == "tau_teorico")
        .unwrap();
    assert!((tau_t.value - 1.01e-4).abs() < 1e-12);
}

#[tokio::test]
async fn seed_definitions_populates_p3_desfasaje() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p3-relajacion-desfasaje")
        .await
        .unwrap()
        .unwrap();
    // Es una práctica de regresión con las fórmulas de eje ya sembradas.
    assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
    assert_eq!(def.x_formula.as_deref(), Some("2*pi*f"));
    assert_eq!(def.y_formula.as_deref(), Some("b / math::sqrt(a*a - b*b)"));
    assert_eq!(def.quantities.len(), 3);
    for symbol in ["f", "a", "b"] {
        assert!(
            def.quantities.iter().any(|q| q.symbol == symbol),
            "falta la magnitud {symbol}"
        );
    }
    assert_eq!(def.results.len(), 1);
    assert_eq!(def.results[0].symbol, "tau");
    assert_eq!(def.results[0].formula, "slope");
}

// Ajuste de extremo a extremo sobre la definición sembrada de P3-parte2, con un caso
// construido: si tg(phi) = tau*omega, con a=1 y b=sin(phi)=t/sqrt(1+t^2), entonces
// y = b/sqrt(a^2-b^2) = tg(phi) = tau*omega, así que el ajuste recupera slope = tau.
#[tokio::test]
async fn seeded_p3_desfasaje_fits_known_tau() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p3-relajacion-desfasaje")
        .await
        .unwrap()
        .unwrap();

    let tau = 1e-3_f64;
    let freqs = [10.0_f64, 20.0, 30.0, 40.0, 50.0];
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let b_vals: Vec<f64> = freqs
        .iter()
        .map(|f| {
            let t = tau * 2.0 * std::f64::consts::PI * f;
            t / (1.0 + t * t).sqrt()
        })
        .collect();
    let measurements = vec![
        crate::computation::MeasurementInput {
            quantity_id: id("f"),
            instrument_id: None,
            scale_id: None,
            values: freqs.to_vec(),
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        },
        crate::computation::MeasurementInput {
            quantity_id: id("a"),
            instrument_id: None,
            scale_id: None,
            values: freqs.iter().map(|_| 1.0).collect(),
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        },
        crate::computation::MeasurementInput {
            quantity_id: id("b"),
            instrument_id: None,
            scale_id: None,
            values: b_vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
        },
    ];
    let analysis = crate::computation::compute_regresion(
        &def.quantities,
        &def.intermediates,
        &def.results,
        &def.point_results,
        &def.aggregates,
        &Default::default(),
        def.x_formula.as_deref().unwrap(),
        def.y_formula.as_deref().unwrap(),
        &measurements,
    )
    .unwrap();
    let reg = analysis.regression.unwrap();
    assert!(
        (reg.slope - tau).abs() < 1e-9,
        "slope {} != tau {tau}",
        reg.slope
    );
    assert!(reg.intercept.abs() < 1e-9);
    let tau_d = analysis.derived.iter().find(|d| d.symbol == "tau").unwrap();
    assert!((tau_d.value - tau).abs() < 1e-9);
}

/// La definición sembrada de Fluidos II se puebla (magnitudes + M_medio + 4 agregados) y
/// computa de extremo a extremo. Caso construido: t = slope*(sqrt(h_max)-sqrt(h)) con slope=100
/// e intercepto 0, así el ajuste recupera la pendiente y M_medio / los agregados dan los valores
/// calculados a mano (Re_max=55000, Re_min=25000, Re_medio=40000, M_teorico=0.86).
#[tokio::test]
async fn seeded_fluidos2_populates_and_computes() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "fluidos-2").await.unwrap().unwrap();

    // Definición: 11 magnitudes, 1 mensurando (M_medio), 4 agregados en orden.
    assert_eq!(def.quantities.len(), 11);
    assert_eq!(def.results.len(), 1);
    assert_eq!(def.results[0].symbol, "M_medio");
    assert_eq!(
        def.aggregates
            .iter()
            .map(|a| a.symbol.as_str())
            .collect::<Vec<_>>(),
        ["Re_max", "Re_min", "Re_medio", "M_teorico"],
    );
    assert!(
        def.aggregates.iter().all(|a| a.is_final),
        "los 4 agregados deben ser comparables (is_final)"
    );
    for symbol in ["mu_agua", "kp"] {
        let q = def.quantities.iter().find(|q| q.symbol == symbol).unwrap();
        assert!(!q.has_uncertainty, "{symbol} no debe pedir incertidumbre");
        assert!(q.is_given, "{symbol} es dato de tabla, sin instrumento");
    }

    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap_or_else(|| panic!("falta la magnitud {sym}"))
            .id
            .clone()
    };
    // x = sqrt(0.36) - sqrt(h) = [0, .1, .2, .3, .4]; con slope=100 -> t = [0,10,20,30,40].
    let h_vals = vec![0.36_f64, 0.25, 0.16, 0.09, 0.04];
    let t_vals = vec![0.0_f64, 10.0, 20.0, 30.0, 40.0];
    let per_point = |sym: &str, values: Vec<f64>| crate::computation::MeasurementInput {
        quantity_id: id(sym),
        instrument_id: None,
        scale_id: None,
        values,
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
    };
    let scalar = |sym: &str, value: f64| per_point(sym, vec![value]);
    let measurements = vec![
        per_point("h", h_vals),
        per_point("t", t_vals),
        scalar("h_max", 0.36),
        scalar("R_cap", 0.001),
        scalar("L_cap", 0.1),
        scalar("R_recip", 0.05),
        scalar("g", 9.8),
        scalar("rho", 1000.0),
        scalar("mu_agua", 1e-3),
        scalar("kp", 0.78),
        scalar("Temp", 20.0),
    ];

    let analysis = crate::computation::compute_regresion(
        &def.quantities,
        &def.intermediates,
        &def.results,
        &def.point_results,
        &def.aggregates,
        &Default::default(),
        def.x_formula.as_deref().unwrap(),
        def.y_formula.as_deref().unwrap(),
        &measurements,
    )
    .unwrap();

    let reg = analysis.regression.unwrap();
    assert!((reg.slope - 100.0).abs() < 1e-6, "slope {}", reg.slope);
    assert!(reg.intercept.abs() < 1e-6, "intercept {}", reg.intercept);

    // M_medio = 2*g*(slope*R_cap^2/(2*R_recip^2))^2 - 2.
    let m = analysis
        .derived
        .iter()
        .find(|d| d.symbol == "M_medio")
        .unwrap();
    assert!((m.value - (-1.99216)).abs() < 1e-5, "M_medio {}", m.value);

    let agg = |sym: &str| {
        analysis
            .aggregates
            .iter()
            .find(|a| a.symbol == sym)
            .unwrap_or_else(|| panic!("falta agregado {sym}"))
            .value
    };
    assert!(
        (agg("Re_max") - 55000.0).abs() < 1e-3,
        "Re_max {}",
        agg("Re_max")
    );
    assert!(
        (agg("Re_min") - 25000.0).abs() < 1e-3,
        "Re_min {}",
        agg("Re_min")
    );
    assert!(
        (agg("Re_medio") - 40000.0).abs() < 1e-3,
        "Re_medio {}",
        agg("Re_medio")
    );
    assert!(
        (agg("M_teorico") - 0.86).abs() < 1e-9,
        "M_teorico {}",
        agg("M_teorico")
    );
    // No debe haber avisos de desalineamiento ni de valores no finitos.
    assert!(
        analysis.warnings.is_empty(),
        "warnings: {:?}",
        analysis.warnings
    );
}

/// La definición sembrada de Filtros tiene 11 magnitudes (R/C1/C2/fpasaje_exp/fbloqueo_exp
/// medidos, L dado), 2 mensurandos finales (fpasaje/fbloqueo teoricos), 3 intermedias
/// y 2 curvas con x_log.
#[tokio::test]
async fn seeded_filtros_populates_and_computes() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "filtros").await.unwrap().unwrap();

    assert_eq!(def.quantities.len(), 11);
    for symbol in ["R", "C1", "C2", "fpasaje_exp", "fbloqueo_exp"] {
        let q = def.quantities.iter().find(|q| q.symbol == symbol).unwrap();
        assert!(!q.is_given, "{symbol} debe ser medido, no dado");
    }
    let l = def.quantities.iter().find(|q| q.symbol == "L").unwrap();
    assert!(l.is_given, "L debe seguir siendo dado");
    assert_eq!(
        def.results
            .iter()
            .map(|r| r.symbol.as_str())
            .collect::<Vec<_>>(),
        ["fpasaje", "fbloqueo"],
    );
    for symbol in ["fpasaje", "fbloqueo"] {
        let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(r.is_final, "{symbol} debe ser resultado final");
    }
    assert_eq!(
        def.intermediates
            .iter()
            .map(|i| i.symbol.as_str())
            .collect::<Vec<_>>(),
        ["omega", "razon", "phi"],
    );
    assert_eq!(def.curves.len(), 2);
    assert!(
        def.curves[0].x_log,
        "curva 1 (amplitud) debe tener x_log=true"
    );
    assert_eq!(def.curves[0].x_formula, "omega");
    assert_eq!(def.curves[0].y_formula, "razon");
    assert!(
        def.curves[1].x_log,
        "curva 2 (desfasaje) debe tener x_log=true"
    );
    assert_eq!(def.curves[1].y_formula, "phi");

    // Computo end-to-end: 3 puntos, omega=2*pi*f, razon=VRpp/Vgpp, phi=asin(b/a).
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let pt = |sym: &str, vals: Vec<f64>| crate::computation::MeasurementInput {
        quantity_id: id(sym),
        instrument_id: None,
        scale_id: None,
        values: vals,
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
    };
    // b/a = 0.5 → phi = asin(0.5) ≈ 0.5236 rad; VRpp/Vgpp = 0.8.
    let measurements = vec![
        pt("f", vec![100.0, 1000.0, 10000.0]),
        pt("VRpp", vec![0.8, 0.8, 0.8]),
        pt("Vgpp", vec![1.0, 1.0, 1.0]),
        pt("a", vec![1.0, 1.0, 1.0]),
        pt("b", vec![0.5, 0.5, 0.5]),
        pt("R", vec![100.0]),
        pt("C1", vec![1e-6]),
        pt("C2", vec![1e-6]),
        pt("fpasaje_exp", vec![1000.0]),
        pt("fbloqueo_exp", vec![5000.0]),
        pt("L", vec![1e-3]),
    ];
    let curves: Vec<crate::computation::CurveSpec> = def
        .curves
        .iter()
        .map(|c| crate::computation::CurveSpec {
            x_formula: &c.x_formula,
            y_formula: &c.y_formula,
            x_log: c.x_log,
        })
        .collect();
    let (analysis, _) = crate::computation::compute_curva(
        &def.quantities,
        &def.intermediates,
        &curves,
        &measurements,
    )
    .unwrap();
    assert_eq!(analysis.scatters.len(), 2);
    // Curva 1 (amplitud): x = omega = 2*pi*f; y = razon = 0.8.
    let amp = &analysis.scatters[0];
    assert!((amp.points[0].0 - 2.0 * std::f64::consts::PI * 100.0).abs() < 1e-6);
    assert!((amp.points[0].1 - 0.8).abs() < 1e-9);
    // Curva 2 (desfasaje): y = phi = asin(0.5) ≈ 0.5236.
    let ph = &analysis.scatters[1];
    assert!((ph.points[0].1 - (0.5_f64).asin()).abs() < 1e-9);
    assert!(ph.x_log);
}

/// La definición sembrada de P2-cc (corriente continua unificada) tiene 17 magnitudes
/// (15 escalares + R e I por punto), 13 mensurandos (12 finales; Req no lo es),
/// la intermedia P = I^2*R y la curva P vs R.
#[tokio::test]
async fn seeded_p2_cc_populates_definition() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p2-cc").await.unwrap().unwrap();

    assert_eq!(def.quantities.len(), 17);
    for symbol in [
        "R1", "R2", "R3", "Vg_s", "RA_s", "VR1_s", "VR2_s", "VR3_s", "Vg_p", "RA_p", "VR1_p",
        "VR2_p", "VR3_p", "Vg_c", "RA_c", "R", "I",
    ] {
        assert!(
            def.quantities.iter().any(|q| q.symbol == symbol),
            "falta la magnitud {symbol}"
        );
    }
    // Solo R e I son por punto; el resto son escalares compartidos. RA (por parte) es dato
    // de catedra (tabla segun la escala del amperimetro); el resto se mide.
    let given = ["RA_s", "RA_p", "RA_c"];
    for q in &def.quantities {
        let per_point = q.symbol == "R" || q.symbol == "I";
        assert_eq!(q.per_point, per_point, "per_point de {}", q.symbol);
        assert_eq!(
            q.is_given,
            given.contains(&q.symbol.as_str()),
            "is_given de {}",
            q.symbol
        );
    }
    assert_eq!(def.results.len(), 13);
    assert_eq!(def.results.iter().filter(|r| r.is_final).count(), 12);
    let req = def.results.iter().find(|r| r.symbol == "Req").unwrap();
    assert!(!req.is_final);
    let i_s = def.results.iter().find(|r| r.symbol == "I_s").unwrap();
    assert_eq!(i_s.formula, "Vg_s / (R1 + R2 + R3 + RA_s)");
    assert_eq!(def.intermediates.len(), 1);
    assert_eq!(def.intermediates[0].symbol, "P");
    assert_eq!(def.curves.len(), 1);
    assert!(!def.curves[0].x_log);
    assert_eq!(def.curves[0].x_formula, "R");
    assert_eq!(def.curves[0].y_formula, "P");
}

/// Integración: `analyze()` para p2-cc deriva los mensurandos de las tres partes y los
/// alias de extremos de la tabla de potencia.
///
/// Serie: I_s = Vg_s/(R1+R2+R3+RA_s) y VRi_s_t. Paralelo: Req, I_p y VRi_p_t.
/// Potencia: RP_max_t = Rth, P_max_t = Vg_c²/(4·Rth), y los experimentales
/// P_max_e = max(P) e RP_max_e = R en ese punto, ambos con U = 0. Además el análisis
/// expone las magnitudes escalares medidas (`quantities`), que antes se descartaban.
#[tokio::test]
async fn analyze_p2_cc_derives_results_and_aliases() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p2-cc").await.unwrap().unwrap();

    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let pt = |sym: &str, vals: Vec<f64>| crate::computation::MeasurementInput {
        quantity_id: id(sym),
        instrument_id: None,
        scale_id: None,
        values: vals,
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
    };

    let (r1, r2, r3) = (100.0_f64, 200.0_f64, 200.0_f64);
    let (vg_s, ra_s) = (8.0_f64, 10.0_f64);
    let (vg_p, ra_p) = (8.0_f64, 10.0_f64);
    let (vg_c, ra_c) = (10.0_f64, 100.0_f64);
    let rpar = r2 * r3 / (r2 + r3); // = 100.0
    let rth = ra_c + rpar; // = 200.0
    let rs = vec![100.0_f64, 200.0, 400.0];
    let is: Vec<f64> = rs.iter().map(|r| vg_c / (rth + r)).collect();
    let measurements = vec![
        pt("R1", vec![r1]),
        pt("R2", vec![r2]),
        pt("R3", vec![r3]),
        pt("Vg_s", vec![vg_s]),
        pt("RA_s", vec![ra_s]),
        pt("VR1_s", vec![1.55]),
        pt("VR2_s", vec![3.15]),
        pt("VR3_s", vec![3.15]),
        pt("Vg_p", vec![vg_p]),
        pt("RA_p", vec![ra_p]),
        pt("VR1_p", vec![3.8]),
        pt("VR2_p", vec![3.8]),
        pt("VR3_p", vec![3.8]),
        pt("Vg_c", vec![vg_c]),
        pt("RA_c", vec![ra_c]),
        pt("R", rs.clone()),
        pt("I", is),
    ];

    let analysis = crate::computation::analyze(&pool, "p2-cc", &measurements)
        .await
        .unwrap();

    // Las magnitudes escalares medidas se exponen (antes el camino curva las descartaba).
    assert_eq!(analysis.quantities.len(), 15);
    assert!(analysis.quantities.iter().any(|q| q.symbol == "VR1_s"));

    let derived = |sym: &str| {
        analysis
            .derived
            .iter()
            .find(|d| d.symbol == sym)
            .unwrap_or_else(|| panic!("{sym} debe estar en derived"))
    };

    // Serie: Rtot = 510.
    let rtot = r1 + r2 + r3 + ra_s;
    assert!((derived("I_s").value - vg_s / rtot).abs() < 1e-9);
    assert!((derived("VR1_s_t").value - vg_s * r1 / rtot).abs() < 1e-9);
    assert!((derived("VR3_s_t").value - vg_s * r3 / rtot).abs() < 1e-9);

    // Paralelo: Req = 210.
    let req = r1 + ra_p + rpar;
    assert!((derived("Req").value - req).abs() < 1e-9);
    assert!((derived("I_p").value - vg_p / req).abs() < 1e-9);
    assert!((derived("VR2_p_t").value - vg_p * rpar / req).abs() < 1e-9);

    // Potencia teorica: Rth = 200, P_max = 0.125 W.
    assert!((derived("RP_max_t").value - rth).abs() < 1e-9);
    assert!((derived("P_max_t").value - vg_c * vg_c / (4.0 * rth)).abs() < 1e-9);

    // Experimentales por alias: el punto R = 200 = Rth maximiza P en la tabla.
    let p_max_table = (vg_c / (rth + rth)).powi(2) * rth;
    let pme = derived("P_max_e");
    assert!(
        (pme.value - p_max_table).abs() < 1e-9,
        "P_max_e esperado {p_max_table}, obtenido {}",
        pme.value
    );
    assert_eq!(pme.u_expanded, 0.0, "P_max_e va sin incertidumbre");
    let rpe = derived("RP_max_e");
    assert!((rpe.value - 200.0).abs() < 1e-9);
    assert_eq!(rpe.u_expanded, 0.0, "RP_max_e va sin incertidumbre");
}

/// Integración: `analyze()` para filtros deriva fpasaje y fbloqueo correctamente.
///
/// Topología: C2||L en serie con C1 y R. Fórmulas teóricas:
///   fpasaje  = 1/(2π√(L(C1+C2)))   (resonancia serie)
///   fbloqueo = 1/(2π√(LC2))         (resonancia paralelo del tanque)
#[tokio::test]
async fn analyze_filtros_derives_fpasaje_fbloqueo() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "filtros").await.unwrap().unwrap();

    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let pt = |sym: &str, vals: Vec<f64>| crate::computation::MeasurementInput {
        quantity_id: id(sym),
        instrument_id: None,
        scale_id: None,
        values: vals,
        given_u: None,
        point_replicas: None,
        operator_replicas: None,
    };

    // Valores de componentes: R=1kΩ, C1=C2=10nF, L=10mH.
    let r = 1000.0_f64;
    let c1 = 10e-9_f64;
    let c2 = 10e-9_f64;
    let l = 10e-3_f64;
    let fp_expected = 1.0 / (2.0 * std::f64::consts::PI * (l * (c1 + c2)).sqrt());
    let fb_expected = 1.0 / (2.0 * std::f64::consts::PI * (l * c2).sqrt());

    // 3 puntos de barrido (valores arbitrarios; los escalares no dependen de ellos).
    let measurements = vec![
        pt("f", vec![1000.0, 5000.0, 10000.0]),
        pt("VRpp", vec![0.5, 1.0, 0.5]),
        pt("Vgpp", vec![1.0, 1.0, 1.0]),
        pt("a", vec![1.0, 1.0, 1.0]),
        pt("b", vec![0.5, 0.5, 0.5]),
        pt("R", vec![r]),
        pt("C1", vec![c1]),
        pt("C2", vec![c2]),
        pt("fpasaje_exp", vec![5000.0]),
        pt("fbloqueo_exp", vec![10000.0]),
        pt("L", vec![l]),
    ];

    let analysis = crate::computation::analyze(&pool, "filtros", &measurements)
        .await
        .unwrap();

    assert!(
        !analysis.derived.is_empty(),
        "derived debe contener fpasaje y fbloqueo"
    );
    let fp = analysis
        .derived
        .iter()
        .find(|d| d.symbol == "fpasaje")
        .expect("fpasaje debe estar en derived");
    assert!(
        (fp.value - fp_expected).abs() < 1.0,
        "fpasaje esperado {fp_expected:.2} Hz, obtenido {:.2}",
        fp.value
    );
    let fb = analysis
        .derived
        .iter()
        .find(|d| d.symbol == "fbloqueo")
        .expect("fbloqueo debe estar en derived");
    assert!(
        (fb.value - fb_expected).abs() < 1.0,
        "fbloqueo esperado {fb_expected:.2} Hz, obtenido {:.2}",
        fb.value
    );
    // fbloqueo > fpasaje (C1+C2 > C2 ⟹ √(L(C1+C2)) > √(LC2)).
    assert!(
        fb.value > fp.value,
        "fbloqueo ({}) debe ser mayor que fpasaje ({})",
        fb.value,
        fp.value
    );
}

/// Verifica que `double_option` distingue las tres variantes de `tolerance` en JSON:
/// campo ausente (no modificar), `null` explícito (borrar) y valor numérico (fijar).
#[test]
fn result_input_tolerance_serde_variants() {
    // Sin campo -> None (no modificar).
    let a: ResultInput =
        serde_json::from_str(r#"{"symbol":"Q","name":"N","unit":"m","formula":"x"}"#).unwrap();
    assert!(a.tolerance.is_none(), "campo ausente debe ser None");

    // `null` explícito -> Some(None) (borrar).
    let b: ResultInput = serde_json::from_str(
        r#"{"symbol":"Q","name":"N","unit":"m","formula":"x","tolerance":null}"#,
    )
    .unwrap();
    assert_eq!(b.tolerance, Some(None), "null debe ser Some(None)");

    // Valor numérico -> Some(Some(v)) (fijar).
    let c: ResultInput = serde_json::from_str(
        r#"{"symbol":"Q","name":"N","unit":"m","formula":"x","tolerance":5.0}"#,
    )
    .unwrap();
    assert_eq!(
        c.tolerance,
        Some(Some(5.0)),
        "número debe ser Some(Some(v))"
    );
}

/// `fix_ca_rlc_labels` corrige filas dejadas por una siembra vieja (`quantity = 'tension'`,
/// `unit = 'rad'`) sin pisar filas que ya están en el estado nuevo o que el docente editó.
#[tokio::test]
async fn fix_ca_rlc_labels_migrates_stale_rows_without_clobbering_new_ones() {
    let (pool, _dir) = setup().await;
    seed_ca_rlc(&pool).await.unwrap();

    // Simula una base sembrada antes del cambio: Vg en "tension" (nombre viejo) y phiR_teo en rad.
    sqlx::query(
        "UPDATE practice_quantities SET name = 'Tension del generador', quantity = 'tension' \
         WHERE practice_id = 'ca-rlc' AND symbol = 'Vg'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE practice_results SET unit = 'rad', formula = '-(math::atan(0))' \
         WHERE practice_id = 'ca-rlc' AND symbol = 'phiR_teo'",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Una edición del docente sobre un campo ya migrado: no debe tocarse.
    sqlx::query(
        "UPDATE practice_quantities SET name = 'Voltaje de la fuente (editado)' \
         WHERE practice_id = 'ca-rlc' AND symbol = 'VRpp'",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Simula una siembra vieja: I_exp existia (se elimino, la corriente no se mide) y
    // f_res_exp no existia todavia (magnitud nueva, se mide con el osciloscopio).
    sqlx::query(
        "INSERT INTO practice_results (id, practice_id, symbol, name, unit, formula, position) \
         VALUES ('r-i-exp', 'ca-rlc', 'I_exp', 'Corriente experimental', 'A', 'VRpp/(2*R)', 99)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .execute(&pool)
    .await
    .unwrap();

    fix_ca_rlc_labels(&pool).await.unwrap();

    let i_exp_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_results WHERE practice_id = 'ca-rlc' AND symbol = 'I_exp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        i_exp_count.0, 0,
        "I_exp debe borrarse (la corriente no se mide)"
    );

    let f_res_exp_name: (String,) = sqlx::query_as(
        "SELECT name FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(f_res_exp_name.0, "Frecuencia de resonancia experimental");

    // Idempotente: correrla de nuevo no debe fallar ni duplicar f_res_exp.
    fix_ca_rlc_labels(&pool).await.unwrap();
    let f_res_exp_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(f_res_exp_count.0, 1);

    let vg_name: (String,) = sqlx::query_as(
        "SELECT name FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'Vg'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vg_name.0, "Voltaje en el generador");

    let (phi_unit, phi_formula): (String, String) = sqlx::query_as(
        "SELECT unit, formula FROM practice_results WHERE practice_id = 'ca-rlc' AND symbol = 'phiR_teo'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(phi_unit, "°");
    assert!(phi_formula.contains("180/pi"));

    let vrpp_name: (String,) = sqlx::query_as(
        "SELECT name FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'VRpp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vrpp_name.0, "Voltaje de la fuente (editado)");
}
