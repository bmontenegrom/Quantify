use super::*;
use crate::db;
use seed::{
    ca_rlc_expanded, chain_seeded_formulas, fix_ca_rlc_labels, hidro_expanded,
    reset_legacy_hidrostatica, seed_ca_rlc, seed_hidrostatica,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tempfile::TempDir;

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
        default_value: None,
        slot_count: None,
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
            default_value: None,
            slot_count: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.symbol, "a");
    assert!(!updated.repeated);

    let with_default = |default_value: Option<Option<f64>>| QuantityInput {
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
        default_value,
        slot_count: None,
    };
    for (input, expected) in [
        (Some(Some(12.0)), Some(12.0)),
        (None, Some(12.0)),
        (Some(None), None),
    ] {
        let q = update_quantity(&pool, &q.id, with_default(input))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(q.default_value, expected, "pedido {input:?}");
    }

    assert_eq!(
        remove_quantity(&pool, "p1-estadistica", &q.id)
            .await
            .unwrap(),
        QuantityRemoval::Deleted
    );
    assert_eq!(
        remove_quantity(&pool, "p1-estadistica", &q.id)
            .await
            .unwrap(),
        QuantityRemoval::NotFound
    );
}

#[tokio::test]
async fn create_and_delete_result() {
    let (pool, _dir) = setup().await;
    let r = create_result(&pool, "p1-estadistica", sample_result(), None)
        .await
        .unwrap();
    assert_eq!(r.symbol, "Q");
    assert_eq!(r.formula, "l*a");

    assert!(delete_result(&pool, &r.id).await.unwrap());
    assert!(!delete_result(&pool, &r.id).await.unwrap());
}

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
        None,
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
    let err = create_quantity(&pool, "p1-estadistica", sample_quantity()).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn symbol_taken_detects_cross_table_collision() {
    let (pool, _dir) = setup().await;
    let q = create_quantity(&pool, "p1-estadistica", sample_quantity())
        .await
        .unwrap();

    assert!(
        symbol_taken_in_practice(&pool, "p1-estadistica", "l", None, None, None, None, None)
            .await
            .unwrap()
    );
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

    let r = create_result(&pool, "p1-estadistica", sample_result(), None)
        .await
        .unwrap();

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

    assert!(set_regression_formulas(&pool, "p1-estadistica", "   ", "")
        .await
        .unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.x_formula, None);
    assert_eq!(def.y_formula, None);

    assert!(!set_regression_formulas(&pool, "no-existe", "f", "f")
        .await
        .unwrap());
}

#[tokio::test]
async fn curve_crud_roundtrip_and_ordering() {
    let (pool, _dir) = setup().await;
    let c1 = create_curve(
        &pool,
        "p1-estadistica",
        CurveInput {
            x_formula: " logw ".into(),
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

    assert!(delete_curve(&pool, "p1-estadistica", &c1.id).await.unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.curves.len(), 1);
    assert_eq!(def.curves[0].y_formula, "phi");
}

#[tokio::test]
async fn move_item_swaps_a_curve_with_its_neighbor() {
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

    let mv = |id: String, up: bool| {
        let pool = pool.clone();
        async move {
            move_item(&pool, Orderable::Curves, "p1-estadistica", &id, up)
                .await
                .unwrap()
        }
    };
    assert_eq!(mv(a.id.clone(), true).await, Some(false));
    assert_eq!(mv(c.id.clone(), false).await, Some(false));

    assert_eq!(mv(a.id.clone(), false).await, Some(true));
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(
        def.curves
            .iter()
            .map(|c| c.y_formula.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a", "c"]
    );

    assert_eq!(mv("no-existe".into(), true).await, None);
}

#[tokio::test]
async fn move_item_reorders_quantities_and_results_even_with_tied_positions() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let symbols = |def: &PracticeDefinition| -> Vec<String> {
        def.quantities.iter().map(|q| q.symbol.clone()).collect()
    };
    sqlx::query("UPDATE practice_quantities SET position = 0 WHERE practice_id = 'p1-estadistica'")
        .execute(&pool)
        .await
        .unwrap();
    let before = symbols(&definition(&pool, "p1-estadistica").await.unwrap().unwrap());
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    let second = def.quantities[1].id.clone();
    assert_eq!(
        move_item(
            &pool,
            Orderable::Quantities,
            "p1-estadistica",
            &second,
            true
        )
        .await
        .unwrap(),
        Some(true)
    );
    let after = symbols(&definition(&pool, "p1-estadistica").await.unwrap().unwrap());
    let mut expected = before.clone();
    expected.swap(0, 1);
    assert_eq!(after, expected);

    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    let (first, second) = (def.results[0].symbol.clone(), def.results[1].symbol.clone());
    assert_eq!(
        move_item(
            &pool,
            Orderable::Results,
            "p1-estadistica",
            &def.results[0].id,
            false
        )
        .await
        .unwrap(),
        Some(true)
    );
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(
        (def.results[0].symbol.clone(), def.results[1].symbol.clone()),
        (second, first)
    );
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
            symbol: " Q ".into(),
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
    for symbol in ["gamma", "Q", "g1", "g2", "g3"] {
        let r = def.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(r.is_final, "{symbol} deberia ser final");
    }
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
    let q = def.results.iter().find(|r| r.symbol == "Q").unwrap();
    assert!(q.formula.contains("T1"));
    assert!(q.name.contains("Operador 1"));

    seed_definitions(&pool).await.unwrap();
    let def2 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def2.quantities.len(), 5);
    assert_eq!(def2.results.len(), 5);
}

#[tokio::test]
async fn seed_definitions_populates_hidrostatica_and_is_idempotent() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("estadistico"));

    assert_eq!(def.quantities.len(), 48);

    let by_symbol = |s: &str| def.quantities.iter().find(|q| q.symbol == s).unwrap();
    assert_eq!(by_symbol("b_E").default_value, Some(10.0));
    for i in 1..=9 {
        let b = by_symbol(&format!("b{i}"));
        assert!(b.is_given, "b{i} deberia ser dato de catedra");
        assert_eq!(b.default_value, Some(i as f64));
    }
    for k in 1..=3 {
        for i in 1..=9 {
            let m = by_symbol(&format!("m{k}_{i}"));
            assert!(!m.is_given && !m.repeated);
            assert_eq!(m.default_value, Some(0.0));
        }
    }
    assert!(!by_symbol("g_ac").has_uncertainty);

    assert_eq!(def.results.len(), 12);
    let res_by = |s: &str| def.results.iter().find(|r| r.symbol == s).unwrap();
    for symbol in ["E_medio", "rho_medio", "gamma_medio"] {
        assert!(res_by(symbol).is_final, "{symbol} deberia ser final");
    }
    for symbol in ["gamma1", "gamma2", "gamma3", "gamma_medio"] {
        assert!(
            !res_by(symbol).has_uncertainty,
            "{symbol} no deberia mostrar incertidumbre"
        );
    }
    for symbol in ["E1", "rho1", "E_medio", "rho_medio"] {
        assert!(
            res_by(symbol).has_uncertainty,
            "{symbol} deberia mostrar incertidumbre"
        );
    }
    let e1 = &res_by("E1").formula;
    for i in 1..=9 {
        assert!(e1.contains(&format!("m1_{i}*b{i}")), "E1 sin la ranura {i}");
    }
    assert!(!e1.contains("m2_"), "E1 no deberia usar la medida 2");

    seed_definitions(&pool).await.unwrap();
    let def2 = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    assert_eq!(def2.quantities.len(), 48);
    assert_eq!(def2.results.len(), 12);
}

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
        mk("t_med", vec![t_med], Some(99.0)),
        mk("T1", t1_vals, None),
    ];

    let analysis = crate::computation::tests::flattened(
        crate::computation::analyze(&pool, "p1-estadistica", &measurements)
            .await
            .unwrap(),
    );

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

    assert!(!derived("g2").value.is_finite());
    assert!(!derived("g3").value.is_finite());
    assert!(
        analysis
            .warnings
            .iter()
            .any(|w| w.contains("T2") || w.contains("g2")),
        "debe haber una advertencia por T2/g2 sin datos"
    );

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

    seed::run_seeds(&pool).await.unwrap();

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
    create_result(&pool, "p1-estadistica", sample_result(), None)
        .await
        .unwrap();
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
    assert_eq!(def.quantities.len(), 8);
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

#[tokio::test]
async fn seeded_p3_formulas_compute() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();

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
async fn seed_definitions_populates_p3_as_two_parts() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
    assert_eq!(def.x_formula.as_deref(), Some("2*pi*f"));
    assert_eq!(def.y_formula.as_deref(), Some("b / math::sqrt(a*a - b*b)"));
    let parts: Vec<(&str, &str)> = def
        .parts
        .iter()
        .map(|p| (p.id.as_str(), def.part_kind(p)))
        .collect();
    assert_eq!(
        parts,
        [
            ("p3-relajacion-directa", "estadistico"),
            ("p3-relajacion-desfasaje", "regresion_lineal")
        ]
    );
    for symbol in ["R", "Rint", "C", "T_oc", "tmedio"] {
        assert_eq!(def.quantity_part(symbol), Some("p3-relajacion-directa"));
        let q = def.quantities.iter().find(|q| q.symbol == symbol).unwrap();
        assert!(!q.per_point || q.is_given, "{symbol} no es una serie");
    }
    for symbol in ["f", "a", "b"] {
        assert_eq!(def.quantity_part(symbol), Some("p3-relajacion-desfasaje"));
    }
    assert_eq!(def.result_part("tau"), Some("p3-relajacion-desfasaje"));
    assert_eq!(def.result_part("tau_exp"), Some("p3-relajacion-directa"));
}

#[tokio::test]
async fn seeded_p3_desfasaje_fits_known_tau() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "p3-relajacion").await.unwrap().unwrap();

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
    let full = crate::computation::analyze(&pool, "p3-relajacion", &measurements)
        .await
        .unwrap();
    let analysis = &full
        .parts
        .iter()
        .find(|p| p.part_id == "p3-relajacion-desfasaje")
        .unwrap()
        .analysis;
    let reg = analysis.regression.as_ref().unwrap();
    assert!(
        (reg.slope - tau).abs() < 1e-9,
        "slope {} != tau {tau}",
        reg.slope
    );
    assert!(reg.intercept.abs() < 1e-9);
    let tau_d = analysis.derived.iter().find(|d| d.symbol == "tau").unwrap();
    assert!((tau_d.value - tau).abs() < 1e-9);
}

#[tokio::test]
async fn seeded_fluidos2_populates_and_computes() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let def = definition(&pool, "fluidos-2").await.unwrap().unwrap();

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
    assert!(
        analysis.warnings.is_empty(),
        "warnings: {:?}",
        analysis.warnings
    );
}

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
    let amp = &analysis.scatters[0];
    assert!((amp.points[0].0 - 2.0 * std::f64::consts::PI * 100.0).abs() < 1e-6);
    assert!((amp.points[0].1 - 0.8).abs() < 1e-9);
    let ph = &analysis.scatters[1];
    assert!((ph.points[0].1 - (0.5_f64).asin()).abs() < 1e-9);
    assert!(ph.x_log);
}

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
    let rpar = r2 * r3 / (r2 + r3);
    let rth = ra_c + rpar;
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

    let by_part = crate::computation::analyze(&pool, "p2-cc", &measurements)
        .await
        .unwrap();
    let shared: Vec<&str> = by_part
        .quantities
        .iter()
        .map(|q| q.symbol.as_str())
        .collect();
    assert_eq!(shared, ["R1", "R2", "R3"]);
    assert!(by_part.derived.is_empty());
    let kinds: Vec<(&str, &str)> = by_part
        .parts
        .iter()
        .map(|p| (p.part_id.as_str(), p.analysis_kind.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("p2-cc-serie", "estadistico"),
            ("p2-cc-paralelo", "estadistico"),
            ("p2-cc-potencia", "curva")
        ]
    );
    let part = |id: &str| {
        &by_part
            .parts
            .iter()
            .find(|p| p.part_id == id)
            .unwrap()
            .analysis
    };
    assert!(part("p2-cc-paralelo")
        .derived
        .iter()
        .any(|d| d.symbol == "Req"));
    assert!(part("p2-cc-potencia")
        .derived
        .iter()
        .any(|d| d.symbol == "P_max_e"));
    assert_eq!(part("p2-cc-potencia").scatters.len(), 1);
    assert!(part("p2-cc-serie")
        .quantities
        .iter()
        .all(|q| !shared.contains(&q.symbol.as_str())));
    let analysis = crate::computation::tests::flattened(by_part);

    assert_eq!(analysis.quantities.len(), 15);
    assert!(analysis.quantities.iter().any(|q| q.symbol == "VR1_s"));

    let derived = |sym: &str| {
        analysis
            .derived
            .iter()
            .find(|d| d.symbol == sym)
            .unwrap_or_else(|| panic!("{sym} debe estar en derived"))
    };

    let rtot = r1 + r2 + r3 + ra_s;
    assert!((derived("I_s").value - vg_s / rtot).abs() < 1e-9);
    assert!((derived("VR1_s_t").value - vg_s * r1 / rtot).abs() < 1e-9);
    assert!((derived("VR3_s_t").value - vg_s * r3 / rtot).abs() < 1e-9);

    let req = r1 + ra_p + rpar;
    assert!((derived("Req").value - req).abs() < 1e-9);
    assert!((derived("I_p").value - vg_p / req).abs() < 1e-9);
    assert!((derived("VR2_p_t").value - vg_p * rpar / req).abs() < 1e-9);

    assert!((derived("RP_max_t").value - rth).abs() < 1e-9);
    assert!((derived("P_max_t").value - vg_c * vg_c / (4.0 * rth)).abs() < 1e-9);

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

    let r = 1000.0_f64;
    let c1 = 10e-9_f64;
    let c2 = 10e-9_f64;
    let l = 10e-3_f64;
    let fp_expected = 1.0 / (2.0 * std::f64::consts::PI * (l * (c1 + c2)).sqrt());
    let fb_expected = 1.0 / (2.0 * std::f64::consts::PI * (l * c2).sqrt());

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
    assert!(
        fb.value > fp.value,
        "fbloqueo ({}) debe ser mayor que fpasaje ({})",
        fb.value,
        fp.value
    );
}

#[test]
fn result_input_tolerance_serde_variants() {
    let a: ResultInput =
        serde_json::from_str(r#"{"symbol":"Q","name":"N","unit":"m","formula":"x"}"#).unwrap();
    assert!(a.tolerance.is_none(), "campo ausente debe ser None");

    let b: ResultInput = serde_json::from_str(
        r#"{"symbol":"Q","name":"N","unit":"m","formula":"x","tolerance":null}"#,
    )
    .unwrap();
    assert_eq!(b.tolerance, Some(None), "null debe ser Some(None)");

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

#[tokio::test]
async fn fix_ca_rlc_labels_migrates_stale_rows_without_clobbering_new_ones() {
    let (pool, _dir) = setup().await;
    seed_ca_rlc(&pool).await.unwrap();

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
    sqlx::query(
        "UPDATE practice_quantities SET name = 'Voltaje de la fuente (editado)' \
         WHERE practice_id = 'ca-rlc' AND symbol = 'VRpp'",
    )
    .execute(&pool)
    .await
    .unwrap();
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

    let f_res_exp_pos: (i64,) = sqlx::query_as(
        "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let f_trabajo_pos: (i64,) = sqlx::query_as(
        "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_trabajo'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(f_res_exp_pos.0 < f_trabajo_pos.0);

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

#[tokio::test]
async fn fix_ca_rlc_labels_reorders_a_stray_f_res_exp() {
    let (pool, _dir) = setup().await;
    seed_ca_rlc(&pool).await.unwrap();

    let max_pos: (i64,) = sqlx::query_as(
        "SELECT MAX(position) FROM practice_quantities WHERE practice_id = 'ca-rlc'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE practice_quantities SET position = ?1 \
         WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .bind(max_pos.0 + 1)
    .execute(&pool)
    .await
    .unwrap();

    fix_ca_rlc_labels(&pool).await.unwrap();

    let f_res_exp_pos: (i64,) = sqlx::query_as(
        "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let f_trabajo_pos: (i64,) = sqlx::query_as(
        "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_trabajo'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(f_res_exp_pos.0 < f_trabajo_pos.0);

    sqlx::query(
        "UPDATE practice_quantities SET position = ?1 WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .bind(max_pos.0 + 1)
    .execute(&pool)
    .await
    .unwrap();
    fix_ca_rlc_labels(&pool).await.unwrap();
    let kept: (i64,) = sqlx::query_as(
        "SELECT position FROM practice_quantities WHERE practice_id = 'ca-rlc' AND symbol = 'f_res_exp'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kept.0, max_pos.0 + 1);

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'ca-rlc'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 15);
}

#[tokio::test]
async fn reset_legacy_hidrostatica_is_a_noop_on_a_new_database() {
    let (pool, _dir) = setup().await;
    reset_legacy_hidrostatica(&pool).await.unwrap();
    seed_hidrostatica(&pool).await.unwrap();
    let applied: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM applied_migrations WHERE name = '2026-11-hidrostatica-ranuras'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied.0, 1);
    reset_legacy_hidrostatica(&pool).await.unwrap();
    let qty: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'hidrostatica'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(qty.0 > 0);
}

#[tokio::test]
async fn reset_legacy_hidrostatica_reseeds_the_old_shape() {
    let (pool, _dir) = setup().await;
    let snapshot = |pool: SqlitePool| async move {
        let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
            "SELECT symbol, name, slot_count FROM practice_quantities \
             WHERE practice_id = 'hidrostatica' ORDER BY position",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        rows
    };
    seed_hidrostatica(&pool).await.unwrap();
    let fresh = snapshot(pool.clone()).await;
    assert!(!fresh.is_empty());

    sqlx::query(
        "UPDATE practice_quantities SET slot_count = NULL, symbol = 'dw_regla' \
         WHERE practice_id = 'hidrostatica' AND symbol = 'dw1'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE practice_results SET unit = 'g/cm3' \
         WHERE practice_id = 'hidrostatica' AND symbol = 'rho1'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
         csv_path, analysis_json, submitted_at) \
         VALUES ('s1', 'a', 'g', 'c', 'hidrostatica', '', '', '{}', '2026-10-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO submission_measurements (id, submission_id, quantity_id, value) \
         SELECT 'm1', 's1', id, 4.3 FROM practice_quantities \
         WHERE practice_id = 'hidrostatica' AND symbol = 'dw_regla'",
    )
    .execute(&pool)
    .await
    .unwrap();

    reset_legacy_hidrostatica(&pool).await.unwrap();
    seed_hidrostatica(&pool).await.unwrap();

    assert_eq!(snapshot(pool.clone()).await, fresh);
    let unit: (String,) = sqlx::query_as(
        "SELECT unit FROM practice_results WHERE practice_id = 'hidrostatica' AND symbol = 'rho1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unit.0, "kg/m3");
    let left: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE practice_id = 'hidrostatica'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left.0, 0);

    sqlx::query(
        "UPDATE practice_quantities SET slot_count = NULL \
         WHERE practice_id = 'hidrostatica' AND symbol = 'dw1'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
         csv_path, analysis_json, submitted_at) \
         VALUES ('s2', 'a', 'g', 'c', 'hidrostatica', '', '', '{}', '2026-11-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    reset_legacy_hidrostatica(&pool).await.unwrap();
    let kept: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE id = 's2'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept.0, 1);
}

#[tokio::test]
async fn submissions_off_slots_counts_non_slot_readings() {
    let (pool, _dir) = setup().await;
    seed_hidrostatica(&pool).await.unwrap();
    let (qid,): (String,) = sqlx::query_as(
        "SELECT id FROM practice_quantities WHERE practice_id = 'hidrostatica' AND symbol = 'dw1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for (sid, value) in [("ok", 4.0), ("dec", 4.3), ("alto", 12.0)] {
        sqlx::query(
            "INSERT INTO submissions (id, student_name, group_name, course, practice_id, \
             file_name, csv_path, analysis_json, submitted_at) \
             VALUES (?1, 'a', 'g', 'c', 'hidrostatica', '', '', '{}', '2026-11-01T00:00:00Z')",
        )
        .bind(sid)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO submission_measurements (id, submission_id, quantity_id, value) \
             VALUES (?1, ?1, ?2, ?3)",
        )
        .bind(sid)
        .bind(&qid)
        .bind(value)
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(submissions_off_slots(&pool, &qid, 9).await.unwrap(), 2);
    assert_eq!(submissions_off_slots(&pool, &qid, 12).await.unwrap(), 1);
}

#[test]
fn slugify_folds_accents_and_symbols() {
    assert_eq!(
        slugify("Péndulo de prueba (2026)"),
        "pendulo-de-prueba-2026"
    );
    assert_eq!(slugify("  Tensión superficial  "), "tension-superficial");
    assert_eq!(slugify("¿?¡!"), "practica");
}

#[tokio::test]
async fn practice_lifecycle_create_duplicate_status_and_delete() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let new = |name: &str| NewPracticeInput {
        name: name.into(),
        description: "desc".into(),
        analysis_kind: "estadistico".into(),
    };

    let a = create_practice(&pool, &new("Péndulo de prueba"), "u1")
        .await
        .unwrap();
    assert_eq!(
        (a.id.as_str(), a.status.as_str()),
        ("pendulo-de-prueba", "borrador")
    );
    let b = create_practice(&pool, &new("Péndulo de prueba"), "u1")
        .await
        .unwrap();
    assert_eq!(b.id, "pendulo-de-prueba-2");

    let original = definition(&pool, "fluidos-2").await.unwrap().unwrap();
    let copy = duplicate_practice(&pool, "fluidos-2", "Copia de Fluidos II", "u1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(copy.status, "borrador");
    let copied = definition(&pool, &copy.id).await.unwrap().unwrap();
    let symbols = |d: &PracticeDefinition| -> Vec<String> {
        d.quantities.iter().map(|q| q.symbol.clone()).collect()
    };
    assert_eq!(symbols(&copied), symbols(&original));
    assert_eq!(copied.results.len(), original.results.len());
    assert_eq!(copied.aggregates.len(), original.aggregates.len());
    assert_eq!(copied.x_formula, original.x_formula);
    assert!(copied
        .quantities
        .iter()
        .all(|q| original.quantities.iter().all(|o| o.id != q.id)));
    assert_eq!(
        definition(&pool, "fluidos-2")
            .await
            .unwrap()
            .unwrap()
            .quantities
            .len(),
        original.quantities.len()
    );
    assert!(duplicate_practice(&pool, "no-existe", "x", "u1")
        .await
        .unwrap()
        .is_none());

    assert_eq!(
        set_practice_status(&pool, &a.id, "archivada")
            .await
            .unwrap(),
        StatusChange::NotAllowed("borrador".into())
    );
    assert_eq!(
        set_practice_status(&pool, &a.id, "publicada")
            .await
            .unwrap(),
        StatusChange::Done
    );
    assert_eq!(
        set_practice_status(&pool, &a.id, "archivada")
            .await
            .unwrap(),
        StatusChange::Done
    );
    assert_eq!(
        set_practice_status(&pool, &a.id, "publicada")
            .await
            .unwrap(),
        StatusChange::Done
    );
    assert_eq!(
        set_practice_status(&pool, &a.id, "borrador").await.unwrap(),
        StatusChange::NotAllowed("publicada".into())
    );

    assert_eq!(
        delete_practice(&pool, &a.id).await.unwrap(),
        DeletePractice::NotDraft
    );
    assert_eq!(
        delete_practice(&pool, &copy.id).await.unwrap(),
        DeletePractice::Deleted
    );
    let (left,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1")
            .bind(&copy.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
    assert_eq!(
        delete_practice(&pool, &copy.id).await.unwrap(),
        DeletePractice::NotFound
    );
}

#[tokio::test]
async fn only_published_practices_reach_courses_and_students() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    db::seed_academic(&pool).await.unwrap();
    let course = "fisica-experimental-i-2026";
    let draft = create_practice(
        &pool,
        &NewPracticeInput {
            name: "Borrador".into(),
            description: String::new(),
            analysis_kind: "estadistico".into(),
        },
        "u1",
    )
    .await
    .unwrap();
    let enable = |id: &str| db::SetCoursePractice {
        practice_id: id.into(),
    };
    assert!(
        !db::enable_course_practice(&pool, course, enable(&draft.id))
            .await
            .unwrap()
    );
    set_practice_status(&pool, &draft.id, "publicada")
        .await
        .unwrap();
    assert!(db::enable_course_practice(&pool, course, enable(&draft.id))
        .await
        .unwrap());
    let listed = |pool: SqlitePool| async move {
        db::practices_for_course(&pool, course)
            .await
            .unwrap()
            .into_iter()
            .any(|p| p.id == "borrador")
    };
    assert!(listed(pool.clone()).await);
    set_practice_status(&pool, &draft.id, "archivada")
        .await
        .unwrap();
    assert!(listed(pool.clone()).await);
    let mut summary = db::CourseSummary {
        id: course.into(),
        name: String::new(),
        term: String::new(),
        active: true,
        submission_edit_hours: 0.0,
        acceptance_window_hours: 0.0,
        members: vec![],
        groups: vec![],
        practices: db::practices_for_course(&pool, course).await.unwrap(),
        subgroups: vec![],
        table_assignments: vec![],
    };
    crate::courses::hide_unpublished(&mut summary);
    assert!(summary.practices.iter().all(|p| p.id != "borrador"));
    let student_catalog = db::practices_for_role(&pool, "estudiante").await.unwrap();
    assert!(student_catalog.iter().all(|p| p.id != "borrador"));
    let teacher_catalog = db::practices_for_role(&pool, "docente").await.unwrap();
    assert!(teacher_catalog.iter().any(|p| p.id == "borrador"));
}

#[tokio::test]
async fn legacy_layout_becomes_parts_and_sections() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let parts_of = |def: &PracticeDefinition| -> Vec<String> {
        def.parts.iter().map(|p| p.id.clone()).collect()
    };
    let p1 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(
        parts_of(&p1),
        vec![
            "p1-estadistica-op1",
            "p1-estadistica-op2",
            "p1-estadistica-op3"
        ]
    );
    let cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(cc.parts.len(), 3);
    let section_with = |def: &PracticeDefinition, symbol: &str| -> PracticeSection {
        def.sections
            .iter()
            .find(|s| s.quantities.iter().any(|q| q == symbol))
            .unwrap_or_else(|| panic!("{symbol} sin seccion"))
            .clone()
    };
    assert_eq!(section_with(&cc, "R1").part_id, None);
    assert_eq!(
        section_with(&cc, "Vg_s").part_id.as_deref(),
        Some("p2-cc-serie")
    );
    assert_eq!(
        section_with(&cc, "R").part_id.as_deref(),
        Some("p2-cc-potencia")
    );
    assert!(section_with(&cc, "Vg_s")
        .results
        .contains(&"I_s".to_string()));

    let hidro = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    assert_eq!(hidro.parts.len(), 2);
    assert_eq!(section_with(&hidro, "g_ac").part_id, None);
    assert_eq!(
        section_with(&hidro, "mw1").part_id.as_deref(),
        Some("hidrostatica-ts")
    );

    let fluidos = definition(&pool, "fluidos-2").await.unwrap().unwrap();
    assert!(fluidos.parts.is_empty() && fluidos.sections.is_empty());

    layout::migrate_legacy_layout(&pool).await.unwrap();
    let again = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(again.sections.len(), cc.sections.len());
}

#[tokio::test]
async fn duplicate_keeps_parts_and_sections_with_new_ids() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let copy = duplicate_practice(&pool, "p2-cc", "Copia CC", "u1")
        .await
        .unwrap()
        .unwrap();
    let original = definition(&pool, "p2-cc").await.unwrap().unwrap();
    let copied = definition(&pool, &copy.id).await.unwrap().unwrap();
    assert_eq!(copied.parts.len(), 3);
    assert_eq!(copied.sections.len(), original.sections.len());
    let ids = |d: &PracticeDefinition| -> Vec<String> {
        d.sections.iter().map(|s| s.id.clone()).collect()
    };
    assert!(ids(&copied).iter().all(|id| !ids(&original).contains(id)));
    for (a, b) in original.sections.iter().zip(&copied.sections) {
        assert_eq!(a.quantities, b.quantities);
        assert_eq!(a.results, b.results);
        assert_eq!(a.part_id.is_some(), b.part_id.is_some());
        if let Some(part) = &b.part_id {
            assert!(copied.parts.iter().any(|p| &p.id == part));
        }
    }
}

#[tokio::test]
async fn deleting_parts_and_sections_keeps_the_content() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    let mut without_serie = cc.clone();
    without_serie.parts.retain(|p| p.id != "p2-cc-serie");
    assert!(
        delete_part_saving_kinds(&pool, "p2-cc-serie", &without_serie)
            .await
            .unwrap()
    );
    let after = definition(&pool, "p2-cc").await.unwrap().unwrap();
    let serie = after
        .sections
        .iter()
        .find(|s| s.title == "Circuito serie")
        .unwrap();
    assert_eq!(serie.part_id, None);

    assert!(delete_section(&pool, "p2-cc", &serie.id).await.unwrap());
    let after = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(after.quantities.len(), cc.quantities.len());
    assert!(after
        .sections
        .iter()
        .all(|s| !s.quantities.contains(&"Vg_s".to_string())));

    let other = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    let foreign = &other.sections[0].id;
    let qid = &after.quantities[0].id;
    assert!(
        !set_member_section(&pool, "p2-cc", SectionMember::Quantity, qid, Some(foreign))
            .await
            .unwrap()
    );
    assert!(create_section(
        &pool,
        "p2-cc",
        &SectionInput {
            title: "x".into(),
            part_id: Some("hidrostatica-arq".into()),
        },
    )
    .await
    .unwrap()
    .is_none());
}

#[tokio::test]
async fn shared_results_that_use_one_part_move_into_it() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let p1 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(p1.result_part("Q"), Some("p1-estadistica-op1"));
    assert_eq!(p1.result_part("gamma"), None);
    let cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(cc.result_part("Req"), Some("p2-cc-paralelo"));
    let hidro = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    assert!(hidro
        .results
        .iter()
        .all(|r| hidro.result_part(&r.symbol).is_some()));
}

#[tokio::test]
async fn the_series_part_takes_the_practice_kind_and_the_rest_are_statistical() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    let kinds: Vec<&str> = cc.parts.iter().map(|p| cc.part_kind(p)).collect();
    assert_eq!(kinds, ["estadistico", "estadistico", "curva"]);
    let hidro = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    assert!(hidro
        .parts
        .iter()
        .all(|p| hidro.part_kind(p) == "estadistico"));

    let potencia = cc.parts.iter().find(|p| p.id == "p2-cc-potencia").unwrap();
    let view = cc.part_view(potencia);
    let symbols: Vec<&str> = view.quantities.iter().map(|q| q.symbol.as_str()).collect();
    assert!(symbols.contains(&"R1") && symbols.contains(&"I") && !symbols.contains(&"Vg_s"));
    assert_eq!(view.curves.len(), cc.curves.len());
    let serie = cc.parts.iter().find(|p| p.id == "p2-cc-serie").unwrap();
    assert!(cc.part_view(serie).curves.is_empty());
    let shared = cc.shared_view();
    assert!(shared
        .quantities
        .iter()
        .all(|q| ["R1", "R2", "R3"].contains(&q.symbol.as_str())));
}

#[tokio::test]
async fn shared_results_keep_the_operator_shape_of_the_form() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    set_operator_count(&pool, "p1-estadistica", 2)
        .await
        .unwrap();
    sqlx::query("UPDATE practice_sections SET part_id = NULL WHERE part_id = 'p1-estadistica-op1'")
        .execute(&pool)
        .await
        .unwrap();
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.result_part("g1"), None);
    let id = |sym: &str| {
        def.quantities
            .iter()
            .find(|q| q.symbol == sym)
            .unwrap()
            .id
            .clone()
    };
    let m = |sym: &str, values: Vec<f64>, given_u: Option<f64>, ops: Option<Vec<Vec<f64>>>| {
        crate::computation::MeasurementInput {
            quantity_id: id(sym),
            instrument_id: None,
            scale_id: None,
            values,
            given_u,
            point_replicas: None,
            operator_replicas: ops,
        }
    };
    let measurements = vec![
        m("L", vec![1.0], Some(0.002), None),
        m("t_med", vec![2.0], Some(0.0), None),
        m(
            "T1",
            vec![],
            None,
            Some(vec![vec![2.0, 2.02, 1.98], vec![2.01, 1.99, 2.0]]),
        ),
    ];
    let analysis = crate::computation::analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();
    assert_eq!(analysis.operators.len(), 2);
    for part in &analysis.parts {
        for op in &part.analysis.operators {
            assert!(op
                .quantities
                .iter()
                .all(|q| q.symbol != "T1" && q.symbol != "L"));
        }
    }
    for op in &analysis.operators {
        let g1 = op.derived.iter().find(|d| d.symbol == "g1").unwrap();
        assert!(
            g1.value.is_finite() && (g1.value - 9.87).abs() < 0.1,
            "{}",
            g1.value
        );
    }
}

#[tokio::test]
async fn shared_results_that_use_curve_values_move_into_the_series_part() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    sqlx::query(
        "UPDATE practice_results SET section_id = \
         (SELECT id FROM practice_sections WHERE practice_id = 'p2-cc' AND part_id IS NULL) \
         WHERE practice_id = 'p2-cc' AND symbol IN ('P_max_e', 'RP_max_e')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM applied_migrations WHERE name = '2026-11-mensurandos-compartidos-a-su-parte'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let loose = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(loose.result_part("P_max_e"), None);
    assert_eq!(loose.symbol_part("P_max"), Some("p2-cc-potencia"));
    assert_eq!(loose.symbol_part("R_at_P_max"), Some("p2-cc-potencia"));
    assert_eq!(loose.symbol_part("P"), Some("p2-cc-potencia"));
    assert_eq!(loose.symbol_part("R1"), None);
    assert_eq!(loose.symbol_part("Vg_s"), Some("p2-cc-serie"));

    layout::migrate_shared_results_to_parts(&pool)
        .await
        .unwrap();
    let fixed = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(fixed.result_part("P_max_e"), Some("p2-cc-potencia"));
    assert_eq!(fixed.result_part("RP_max_e"), Some("p2-cc-potencia"));
}

#[tokio::test]
async fn curve_practices_do_not_inherit_a_leftover_operator_count() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let mut cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    cc.operator_count = Some(2);
    assert_eq!(cc.shared_view().operator_count, None);
    assert!(cc
        .parts
        .iter()
        .all(|p| cc.part_view(p).operator_count.is_none()));
    let mut p1 = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    p1.operator_count = Some(2);
    assert_eq!(p1.shared_view().operator_count, Some(2));
}

#[tokio::test]
async fn the_migration_leaves_a_result_unsectioned_when_its_part_has_no_section() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    sqlx::query("DELETE FROM practice_sections WHERE part_id IN ('p2-cc-serie', 'p2-cc-potencia')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE practice_results SET section_id = \
         (SELECT id FROM practice_sections WHERE practice_id = 'p2-cc' AND part_id IS NULL) \
         WHERE practice_id = 'p2-cc' AND symbol = 'P_max_e'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM applied_migrations WHERE name = '2026-11-mensurandos-compartidos-a-su-parte'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let before = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert_eq!(before.result_part("P_max_e"), None);

    layout::migrate_shared_results_to_parts(&pool)
        .await
        .unwrap();
    let after = definition(&pool, "p2-cc").await.unwrap().unwrap();
    assert!(after
        .sections
        .iter()
        .all(|s| !s.results.contains(&"P_max_e".to_string())));
    let series = after.series_part().unwrap().id.clone();
    assert_eq!(after.result_part("P_max_e"), Some(series.as_str()));
}

#[tokio::test]
async fn a_result_used_by_series_artifacts_goes_to_the_series_part() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    create_part(
        &pool,
        "fluidos-1",
        &PartInput {
            title: "Unica".into(),
        },
    )
    .await
    .unwrap();
    let mut def = definition(&pool, "fluidos-1").await.unwrap().unwrap();
    let series = def.series_part().unwrap().id.clone();
    let mut k = def.results[0].clone();
    k.id = "k".into();
    k.symbol = "k".into();
    k.formula = "2".into();
    def.results.push(k);
    assert_eq!(def.result_part("k"), None);
    def.point_results[0].formula = format!("{} * k", def.point_results[0].formula);
    assert_eq!(def.result_part("k"), Some(series.as_str()));
}

#[tokio::test]
async fn the_two_old_p3_practices_become_one() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    for sql in [
        "DELETE FROM applied_migrations WHERE name = '2026-11-p3-unificada'",
        "UPDATE practices SET analysis_kind = 'estadistico', x_formula = NULL, y_formula = NULL \
         WHERE id = 'p3-relajacion'",
        "INSERT INTO practices (id, name, description, analysis_kind) \
         VALUES ('p3-relajacion-desfasaje', 'Desfasaje', '', 'regresion_lineal')",
        "INSERT INTO courses (id, name, term, active, created_at) \
         VALUES ('otro', 'Otro curso', '2026', 1, '2026-01-01')",
        "INSERT INTO course_practices (course_id, practice_id, created_at) \
         VALUES ('otro', 'p3-relajacion-desfasaje', '2026-01-01')",
        "INSERT INTO courses (id, name, term, active, created_at) \
         VALUES ('solo-p2', 'Solo parte 2', '2026', 1, '2026-01-01')",
        "INSERT INTO course_practices (course_id, practice_id, created_at, enabled) \
         VALUES ('solo-p2', 'p3-relajacion', '2026-01-01', 0), \
                ('solo-p2', 'p3-relajacion-desfasaje', '2026-01-01', 1)",
        "INSERT INTO lab_groups (id, course_id, name, created_at) \
         VALUES ('g', 'solo-p2', 'G', '2026-01-01')",
        "INSERT INTO users (id, username, display_name, role, password_hash, created_at, email) \
         VALUES ('u1', 'u1', 'U1', 'estudiante', 'x', '2026-01-01', 'u1@x'), \
                ('u2', 'u2', 'U2', 'estudiante', 'x', '2026-01-01', 'u2@x')",
        "INSERT INTO practice_subgroups (id, course_id, practice_id, group_id, name, created_at) \
         VALUES ('sa1', 'solo-p2', 'p3-relajacion', 'g', 'A', '2026-01-01'), \
                ('sa2', 'solo-p2', 'p3-relajacion-desfasaje', 'g', 'A', '2026-01-01')",
        "INSERT INTO practice_subgroup_members (subgroup_id, user_id, created_at) \
         VALUES ('sa1', 'u1', '2026-01-01'), ('sa2', 'u2', '2026-01-01')",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    seed::run_seeds(&pool).await.unwrap();
    assert!(definition(&pool, "p3-relajacion-desfasaje")
        .await
        .unwrap()
        .is_none());
    let p3 = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
    assert_eq!(p3.analysis_kind.as_deref(), Some("regresion_lineal"));
    assert_eq!(p3.parts.len(), 2);
    assert_eq!(p3.quantities.len(), 8);
    let (enabled,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM course_practices WHERE course_id = 'otro' AND practice_id = 'p3-relajacion'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        enabled, 1,
        "el curso que tenia el desfasaje sigue teniendo P3"
    );
    let (enabled,): (bool,) = sqlx::query_as(
        "SELECT enabled FROM course_practices \
         WHERE course_id = 'solo-p2' AND practice_id = 'p3-relajacion'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        enabled,
        "el curso que solo habilitaba la parte 2 sigue con P3"
    );
    let members: Vec<(String,)> = sqlx::query_as(
        "SELECT user_id FROM practice_subgroup_members WHERE subgroup_id = 'sa1' ORDER BY user_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(members, vec![("u1".into(),), ("u2".into(),)]);
}

#[tokio::test]
async fn an_old_p3_without_desfasaje_is_unified_too() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    for sql in [
        "DELETE FROM applied_migrations WHERE name = '2026-11-p3-unificada'",
        "DELETE FROM practice_quantities WHERE practice_id = 'p3-relajacion' \
         AND symbol IN ('f', 'a', 'b')",
        "UPDATE practices SET analysis_kind = 'estadistico' WHERE id = 'p3-relajacion'",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    seed::run_seeds(&pool).await.unwrap();
    let p3 = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
    assert_eq!(p3.analysis_kind.as_deref(), Some("regresion_lineal"));
    assert_eq!(p3.quantities.len(), 8);
    assert_eq!(p3.parts.len(), 2);
}

#[tokio::test]
async fn reseeding_p3_with_its_parts_still_there_does_not_fail() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    for table in ["practice_results", "practice_quantities"] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE practice_id = 'p3-relajacion'"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    seed::run_seeds(&pool).await.unwrap();
    let p3 = definition(&pool, "p3-relajacion").await.unwrap().unwrap();
    assert_eq!(p3.parts.len(), 2);
    assert_eq!(p3.quantities.len(), 8);
}

#[tokio::test]
async fn seeded_expanded_formulas_become_chained() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let fresh = |pool: SqlitePool| async move {
        let mut formulas = Vec::new();
        for practice in ["ca-rlc", "hidrostatica"] {
            let def = definition(&pool, practice).await.unwrap().unwrap();
            formulas.extend(def.results.into_iter().map(|r| (r.symbol, r.formula)));
        }
        formulas
    };
    let chained = fresh(pool.clone()).await;
    for (practice, expanded) in [
        ("ca-rlc", ca_rlc_expanded()),
        ("hidrostatica", hidro_expanded()),
    ] {
        for (symbol, formula) in expanded {
            sqlx::query(
                "UPDATE practice_results SET formula = ?1 WHERE practice_id = ?2 AND symbol = ?3",
            )
            .bind(formula)
            .bind(practice)
            .bind(symbol)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    sqlx::query("UPDATE practice_results SET formula = 'Vg/R' WHERE practice_id = 'ca-rlc' AND symbol = 'I_teo'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM applied_migrations WHERE name = '2026-11-formulas-encadenadas'")
        .execute(&pool)
        .await
        .unwrap();

    chain_seeded_formulas(&pool).await.unwrap();
    let migrated = fresh(pool.clone()).await;
    let mut expected = chained.clone();
    for (symbol, formula) in &mut expected {
        if symbol == "I_teo" {
            *formula = "Vg/R".into();
        }
    }
    assert_eq!(migrated, expected);
}

#[tokio::test]
async fn a_duplicate_does_not_share_the_source_version() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    assert_eq!(
        definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap()
            .current_version,
        Some(1)
    );
    let copy = duplicate_practice(&pool, "p2-cc", "Copia CC", "u1")
        .await
        .unwrap()
        .unwrap();
    assert!(current_version(&pool, &copy.id).await.unwrap().is_none());
    let published = publish_version(&pool, &copy.id, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(published.number, 1);
    assert_eq!(published.definition.practice_id, copy.id);
}

#[tokio::test]
async fn seeded_submissions_are_pinned_to_their_version() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    crate::db::seed_users(&pool).await.unwrap();
    crate::db::seed_academic(&pool).await.unwrap();
    crate::db::seed_submissions(&pool).await.unwrap();
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT practice_id, practice_version_id FROM submissions")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|(_, v)| v.is_some()), "{rows:?}");
}

#[test]
fn an_old_snapshot_missing_fields_still_reads() {
    let def: PracticeDefinition = serde_json::from_str(
        r#"{"practice_id":"p","status":"publicada","quantities":[{"id":"q","symbol":"L"}],
            "results":[{"id":"r","symbol":"g","formula":"L"}],"parts":[{"id":"a","title":"A"}]}"#,
    )
    .unwrap();
    assert_eq!(def.quantities[0].symbol, "L");
    assert!(def.quantities[0].has_uncertainty);
    assert!(def.results[0].has_uncertainty);
    assert_eq!(def.results[0].tolerance, None);
    assert!(def.quantities[0].repeated && def.quantities[0].per_point);
    assert_eq!(def.parts[0].analysis_kind, None);
    assert!(def.curves.is_empty());
}

#[tokio::test]
async fn publishing_a_draft_is_one_step_and_versions_need_changes() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let copy = duplicate_practice(&pool, "p2-cc", "Copia CC", "u1")
        .await
        .unwrap()
        .unwrap();
    let none = |_: &PracticeDefinition| Vec::new();
    assert!(matches!(
        publish_draft(&pool, &copy.id, None, none).await.unwrap(),
        DraftPublish::Published
    ));
    assert!(matches!(
        publish_draft(&pool, &copy.id, None, none).await.unwrap(),
        DraftPublish::NotDraft
    ));
    let rejected = duplicate_practice(&pool, "p2-cc", "Otra copia", "u1")
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        publish_draft(&pool, &rejected.id, None, |_| vec!["falta algo".into()])
            .await
            .unwrap(),
        DraftPublish::Rejected(_)
    ));
    assert!(current_version(&pool, &rejected.id)
        .await
        .unwrap()
        .is_none());
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_versions WHERE practice_id = ?1")
            .bind(&copy.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let published = published_definition(&pool, &copy.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(published.status, "publicada");
    assert_eq!(published.current_version, Some(1));

    assert!(matches!(
        publish_checked(&pool, &copy.id, None, none).await.unwrap(),
        Publish::Unchanged(1)
    ));
    sqlx::query("UPDATE practice_results SET formula = formula || ' * 1' WHERE practice_id = ?1")
        .bind(&copy.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        publish_checked(&pool, &copy.id, None, none).await.unwrap(),
        Publish::Published { number: 2 }
    ));
    set_practice_status(&pool, &copy.id, "archivada")
        .await
        .unwrap();
    let archived = published_definition(&pool, &copy.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(archived.status, "archivada");
    assert!(matches!(
        publish_draft(&pool, &copy.id, None, |_| vec![
            "borrador a medio editar".into()
        ])
        .await
        .unwrap(),
        DraftPublish::NotDraft
    ));
}

#[tokio::test]
async fn a_damaged_snapshot_is_an_internal_error() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    sqlx::query(
        "UPDATE practice_versions SET definition_json = '{}' WHERE practice_id = 'p1-estadistica'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let err = current_version(&pool, "p1-estadistica").await.unwrap_err();
    assert!(err.downcast_ref::<sqlx::Error>().is_some(), "{err}");
}

fn new_quantity(symbol: &str) -> QuantityInput {
    QuantityInput {
        symbol: symbol.into(),
        name: "Nueva".into(),
        unit: "s".into(),
        repeated: false,
        quantity: None,
        is_given: false,
        replicas_per_point: None,
        per_point: false,
        has_uncertainty: true,
        optional: false,
        default_value: None,
        slot_count: None,
    }
}

#[tokio::test]
async fn a_quantity_in_a_version_is_retired_and_frees_its_symbol() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let practice = "p1-estadistica";
    let fresh = create_quantity(&pool, practice, new_quantity("Tn"))
        .await
        .unwrap();
    assert_eq!(
        remove_quantity(&pool, practice, &fresh.id).await.unwrap(),
        QuantityRemoval::Deleted
    );

    let def = definition(&pool, practice).await.unwrap().unwrap();
    let t2 = def
        .quantities
        .iter()
        .find(|q| q.symbol == "T2")
        .unwrap()
        .clone();
    assert_eq!(
        remove_quantity(&pool, "p2-cc", &t2.id).await.unwrap(),
        QuantityRemoval::NotFound,
        "una magnitud de otra práctica no se toca"
    );
    assert_eq!(
        remove_quantity(&pool, practice, &t2.id).await.unwrap(),
        QuantityRemoval::Retired
    );
    assert_eq!(
        remove_quantity(&pool, practice, &t2.id).await.unwrap(),
        QuantityRemoval::NotFound
    );
    assert!(update_quantity(&pool, &t2.id, new_quantity("T2"))
        .await
        .unwrap()
        .is_none());
    let any_section = definition(&pool, practice).await.unwrap().unwrap().sections[0]
        .id
        .clone();
    assert!(!set_member_section(
        &pool,
        practice,
        SectionMember::Quantity,
        &t2.id,
        Some(&any_section)
    )
    .await
    .unwrap());

    let draft = definition(&pool, practice).await.unwrap().unwrap();
    assert!(draft.quantities.iter().all(|q| q.id != t2.id));
    let version = current_version(&pool, practice).await.unwrap().unwrap();
    assert!(version
        .definition
        .quantities
        .iter()
        .any(|q| q.id == t2.id && q.symbol == "T2"));
    let (symbol, section): (String, Option<String>) =
        sqlx::query_as("SELECT symbol, section_id FROM practice_quantities WHERE id = ?1")
            .bind(&t2.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(symbol, format!("T2~{}", t2.id));
    assert_eq!(section, None);

    assert!(
        !symbol_taken_in_practice(&pool, practice, "T2", None, None, None, None, None)
            .await
            .unwrap()
    );
    create_quantity(&pool, practice, new_quantity("T2"))
        .await
        .unwrap();

    let copy = duplicate_practice(&pool, practice, "Copia", "u1")
        .await
        .unwrap()
        .unwrap();
    let (retired,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM practice_quantities WHERE practice_id = ?1 AND removed = 1",
    )
    .bind(&copy.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retired, 0);
    let copied = definition(&pool, &copy.id).await.unwrap().unwrap();
    assert_eq!(
        copied
            .quantities
            .iter()
            .filter(|q| q.symbol == "T2")
            .count(),
        1
    );
}

#[tokio::test]
async fn reordering_skips_retired_quantities() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let practice = "p1-estadistica";
    let before = definition(&pool, practice).await.unwrap().unwrap();
    let symbols = |d: &PracticeDefinition| -> Vec<String> {
        d.quantities.iter().map(|q| q.symbol.clone()).collect()
    };
    let t2 = before.quantities.iter().find(|q| q.symbol == "T2").unwrap();
    let t3 = before.quantities.iter().find(|q| q.symbol == "T3").unwrap();
    remove_quantity(&pool, practice, &t2.id).await.unwrap();
    assert_eq!(
        move_item(&pool, Orderable::Quantities, practice, &t3.id, true)
            .await
            .unwrap(),
        Some(true)
    );
    let after = definition(&pool, practice).await.unwrap().unwrap();
    let order = symbols(&after);
    let t1_at = order.iter().position(|s| s == "T1").unwrap();
    let t3_at = order.iter().position(|s| s == "T3").unwrap();
    assert!(t3_at < t1_at, "{order:?}");
}

#[tokio::test]
async fn a_quantity_measured_without_a_version_waits_for_the_first_publish() {
    let (pool, _dir) = setup().await;
    let practice = create_practice(
        &pool,
        &NewPracticeInput {
            name: "Heredada".into(),
            description: "desc".into(),
            analysis_kind: "estadistico".into(),
        },
        "u1",
    )
    .await
    .unwrap()
    .id;
    let quantity = create_quantity(&pool, &practice, new_quantity("Tn"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
         csv_path, analysis_json, submitted_at) \
         VALUES ('s1', 'a', 'g', 'c', ?1, '', '', '{}', '2026-10-01T00:00:00Z')",
    )
    .bind(&practice)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO submission_measurements (id, submission_id, quantity_id, value) \
         VALUES ('m1', 's1', ?1, 1.0)",
    )
    .bind(&quantity.id)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        remove_quantity(&pool, &practice, &quantity.id)
            .await
            .unwrap(),
        QuantityRemoval::HasMeasurements
    );
    let def = definition(&pool, &practice).await.unwrap().unwrap();
    assert!(def.quantities.iter().any(|q| q.symbol == "Tn"));

    publish_version(&pool, &practice, None).await.unwrap();
    assert_eq!(
        remove_quantity(&pool, &practice, &quantity.id)
            .await
            .unwrap(),
        QuantityRemoval::Retired
    );
    let def = definition(&pool, &practice).await.unwrap().unwrap();
    assert!(def.quantities.iter().all(|q| q.symbol != "Tn"));
    let pinned = submission_definition(&pool, "s1").await.unwrap().unwrap();
    assert!(pinned
        .quantities
        .iter()
        .any(|q| q.id == quantity.id && q.symbol == "Tn"));
}

#[tokio::test]
async fn restarting_keeps_what_the_teacher_changed() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let rename = |practice: &'static str, from: &'static str, to: &'static str| {
        let pool = pool.clone();
        async move {
            let changed = sqlx::query(
                "UPDATE practice_quantities SET symbol = ?3 WHERE practice_id = ?1 AND symbol = ?2",
            )
            .bind(practice)
            .bind(from)
            .bind(to)
            .execute(&pool)
            .await
            .unwrap()
            .rows_affected();
            assert_eq!(changed, 1, "{from}");
        }
    };
    rename("p1-estadistica", "T1", "Tx").await;
    rename("p2-cc", "Vg_s", "Vfuente").await;
    sqlx::query(
        "UPDATE practice_results SET name = 'Editado', is_final = 0, has_uncertainty = 1 \
         WHERE practice_id = 'p2-cc' AND symbol = 'P_max_e'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let snapshot = |d: PracticeDefinition| serde_json::to_string(&d).unwrap();
    let before_p1 = snapshot(definition(&pool, "p1-estadistica").await.unwrap().unwrap());
    let before_cc = snapshot(definition(&pool, "p2-cc").await.unwrap().unwrap());

    db::migrate(&pool).await.unwrap();
    seed_definitions(&pool).await.unwrap();

    assert_eq!(
        before_p1,
        snapshot(definition(&pool, "p1-estadistica").await.unwrap().unwrap())
    );
    assert_eq!(
        before_cc,
        snapshot(definition(&pool, "p2-cc").await.unwrap().unwrap())
    );
}

#[tokio::test]
async fn the_power_results_have_no_uncertainty_from_the_first_start() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    for symbol in ["P_max_e", "P_max_t", "RP_max_e", "RP_max_t"] {
        let result = cc.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(!result.has_uncertainty, "{symbol}");
    }
}

#[tokio::test]
async fn cosmetic_changes_update_the_current_version_without_a_new_one() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let practice = "p1-estadistica";
    let no_problems = |_: &PracticeDefinition| Vec::new();
    let changes =
        |pool: SqlitePool| async move { draft_changes(&pool, practice).await.unwrap().unwrap() };
    assert_eq!(changes(pool.clone()).await.kind, DraftChangeKind::None);

    sqlx::query(
        "UPDATE practice_results SET name = 'Gravedad editada', tolerance = 3.0 \
         WHERE practice_id = ?1 AND symbol = 'g1'",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(changes(pool.clone()).await.kind, DraftChangeKind::Cosmetic);
    let before = current_version(&pool, practice).await.unwrap().unwrap();
    assert!(matches!(
        publish_checked(&pool, practice, None, no_problems)
            .await
            .unwrap(),
        Publish::UpdatedCurrent { number } if number == before.number
    ));
    let after = current_version(&pool, practice).await.unwrap().unwrap();
    assert_eq!(
        (after.id.as_str(), after.number),
        (before.id.as_str(), before.number)
    );
    assert_eq!(after.definition.current_version, Some(before.number));
    let g1 = after
        .definition
        .results
        .iter()
        .find(|r| r.symbol == "g1")
        .unwrap();
    assert_eq!(
        (g1.name.as_str(), g1.tolerance),
        ("Gravedad editada", Some(3.0))
    );
    assert_eq!(changes(pool.clone()).await.kind, DraftChangeKind::None);

    sqlx::query(
        "UPDATE practice_results SET formula = '2*L' WHERE practice_id = ?1 AND symbol = 'g1'",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    let structural = changes(pool.clone()).await;
    assert_eq!(structural.kind, DraftChangeKind::Structural);
    assert_eq!(structural.changes, ["Cambia la fórmula de g1"]);
    assert!(matches!(
        publish_checked(&pool, practice, None, no_problems)
            .await
            .unwrap(),
        Publish::Published { number } if number == before.number + 1
    ));
}

#[tokio::test]
async fn draft_changes_list_what_differs_and_count_submissions() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let practice = "p1-estadistica";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    let t3 = def.quantities.iter().find(|q| q.symbol == "T3").unwrap();
    sqlx::query("DELETE FROM practice_results WHERE practice_id = ?1 AND formula LIKE '%T3%'")
        .bind(practice)
        .execute(&pool)
        .await
        .unwrap();
    remove_quantity(&pool, practice, &t3.id).await.unwrap();
    create_quantity(&pool, practice, new_quantity("Tn"))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE practice_quantities SET unit = 'ms' WHERE practice_id = ?1 AND symbol = 'T1'",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
         csv_path, analysis_json, submitted_at) \
         VALUES ('s1', 'a', 'g', 'c', ?1, '', '', '{}', '2026-10-01T00:00:00Z')",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();

    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Structural);
    assert_eq!(found.submissions, 1);
    for line in [
        "Se agrega la magnitud Tn",
        "Se quita la magnitud T3",
        "Cambia la magnitud T1",
        "Se quita el mensurando g3",
    ] {
        assert!(
            found.changes.iter().any(|c| c == line),
            "{line}: {:?}",
            found.changes
        );
    }
}

#[tokio::test]
async fn reordering_alone_is_a_structural_change_with_its_own_line() {
    let (pool, _dir) = setup().await;
    seed_definitions(&pool).await.unwrap();
    let practice = "p1-estadistica";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    assert!(move_item(
        &pool,
        Orderable::Quantities,
        practice,
        &def.quantities[1].id,
        true
    )
    .await
    .unwrap()
    .unwrap());
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Structural);
    assert_eq!(
        found.changes,
        ["Cambia el orden o la organización en secciones"]
    );
}

#[tokio::test]
async fn a_practice_without_a_version_has_no_draft_changes() {
    let (pool, _dir) = setup().await;
    let practice = create_practice(
        &pool,
        &NewPracticeInput {
            name: "Nueva".into(),
            description: "desc".into(),
            analysis_kind: "estadistico".into(),
        },
        "u1",
    )
    .await
    .unwrap();
    assert!(draft_changes(&pool, &practice.id).await.unwrap().is_none());
}
