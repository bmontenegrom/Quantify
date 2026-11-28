use super::*;
use crate::db;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tempfile::TempDir;

async fn empty_practice(pool: &SqlitePool) {
    sqlx::query("INSERT INTO practices (id, name, description) VALUES ('vacia', 'Vacía', '')")
        .execute(pool)
        .await
        .unwrap();
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
    seed_library(&pool).await.unwrap();
    (pool, dir)
}

async fn set_display_symbol(
    pool: &SqlitePool,
    practice_id: &str,
    symbol: &str,
    display_symbol: &str,
) -> anyhow::Result<bool> {
    let mut updated = 0;
    for table in db::DISPLAY_SYMBOL_TABLES {
        updated += sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET display_symbol = ?1 WHERE practice_id = ?2 AND symbol = ?3"
        )))
        .bind(display_symbol)
        .bind(practice_id)
        .bind(symbol)
        .execute(pool)
        .await?
        .rows_affected();
    }
    Ok(updated > 0)
}

fn sample_quantity() -> QuantityInput {
    QuantityInput {
        display_symbol: None,
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
        ..Default::default()
    }
}

fn sample_result() -> ResultInput {
    ResultInput {
        display_symbol: None,
        compare_with: None,
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
    empty_practice(&pool).await;
    let q = create_quantity(&pool, "vacia", sample_quantity())
        .await
        .unwrap();
    assert_eq!(q.symbol, "l");
    assert_eq!(q.practice_id, "vacia");

    let def = definition(&pool, "vacia").await.unwrap().unwrap();
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
            display_symbol: None,
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
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.symbol, "a");
    assert!(!updated.repeated);

    let with_default = |default_value: Option<Option<f64>>| QuantityInput {
        display_symbol: None,
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
        ..Default::default()
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
    empty_practice(&pool).await;
    let r = create_result(&pool, "vacia", sample_result(), None)
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
    empty_practice(&pool).await;
    let r = create_result(
        &pool,
        "vacia",
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
    empty_practice(&pool).await;
    let q = create_quantity(&pool, "vacia", sample_quantity())
        .await
        .unwrap();

    assert!(
        symbol_taken_in_practice(&pool, "vacia", "l", None, None, None, None, None)
            .await
            .unwrap()
    );
    assert!(
        !symbol_taken_in_practice(&pool, "vacia", "l", Some(&q.id), None, None, None, None)
            .await
            .unwrap()
    );

    let r = create_result(&pool, "vacia", sample_result(), None)
        .await
        .unwrap();

    assert!(
        symbol_taken_in_practice(&pool, "vacia", "l", None, Some(&r.id), None, None, None)
            .await
            .unwrap()
    );
    assert!(
        symbol_taken_in_practice(&pool, "vacia", "Q", Some(&q.id), None, None, None, None)
            .await
            .unwrap()
    );

    create_intermediate(
        &pool,
        "vacia",
        IntermediateInput {
            display_symbol: None,
            symbol: "Iv".into(),
            name: "Iv".into(),
            unit: "u".into(),
            formula: "l".into(),
            show_live: false,
            on_means: false,
        },
    )
    .await
    .unwrap();
    assert!(
        symbol_taken_in_practice(&pool, "vacia", "Iv", None, None, None, None, None)
            .await
            .unwrap()
    );

    create_point_result(
        &pool,
        "vacia",
        PointResultInput {
            display_symbol: None,
            symbol: "Re".into(),
            name: "Re".into(),
            unit: "".into(),
            formula: "L".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        symbol_taken_in_practice(&pool, "vacia", "Re", None, None, None, None, None)
            .await
            .unwrap()
    );

    create_aggregate(
        &pool,
        "vacia",
        AggregateInput {
            display_symbol: None,
            symbol: "Ma".into(),
            name: "Ma".into(),
            unit: "".into(),
            formula: "slope".into(),
            is_final: false,
            has_uncertainty: true,
        },
    )
    .await
    .unwrap();
    assert!(
        symbol_taken_in_practice(&pool, "vacia", "Ma", None, None, None, None, None)
            .await
            .unwrap()
    );

    assert!(
        !symbol_taken_in_practice(&pool, "vacia", "nuevo", None, None, None, None, None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn aggregate_crud_roundtrip() {
    let (pool, _dir) = setup().await;
    let mk = |symbol: &str, formula: &str| AggregateInput {
        display_symbol: None,
        symbol: symbol.into(),
        name: symbol.into(),
        unit: "".into(),
        formula: formula.into(),
        is_final: false,
        has_uncertainty: true,
    };
    let a = create_aggregate(&pool, "p1-estadistica", mk("Re_max", "slope"))
        .await
        .unwrap();
    let b = create_aggregate(&pool, "p1-estadistica", mk("Re_min", "intercept"))
        .await
        .unwrap();
    assert!(b.position > a.position, "la 2da toma la siguiente posición");

    let listed = aggregates_for(&mut pool.acquire().await.unwrap(), "p1-estadistica")
        .await
        .unwrap();
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
        aggregates_for(&mut pool.acquire().await.unwrap(), "p1-estadistica")
            .await
            .unwrap()
            .len(),
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
    assert_eq!(
        curves_for(&mut pool.acquire().await.unwrap(), "p1-estadistica")
            .await
            .unwrap()
            .len(),
        1
    );

    sqlx::query("DELETE FROM practices WHERE id = ?1")
        .bind("p1-estadistica")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        curves_for(&mut pool.acquire().await.unwrap(), "p1-estadistica")
            .await
            .unwrap()
            .len(),
        0
    );
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
            display_symbol: None,
            symbol: " Q ".into(),
            name: "Caudal".into(),
            unit: "m3/s".into(),
            formula: "V/t".into(),
            show_live: true,
            on_means: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(q.symbol, "Q");
    assert!(q.show_live && !q.on_means);
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert_eq!(def.intermediates.len(), 1);
    assert_eq!(def.intermediates[0].formula, "V/t");

    assert!(update_intermediate(
        &pool,
        "p2-cc",
        &q.id,
        IntermediateInput {
            display_symbol: None,
            symbol: "Q".into(),
            name: "x".into(),
            unit: "x".into(),
            formula: "V*t".into(),
            show_live: false,
            on_means: false,
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
            display_symbol: None,
            symbol: "Q".into(),
            name: "Caudal".into(),
            unit: "m3/s".into(),
            formula: "V*t".into(),
            show_live: false,
            on_means: true,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.formula, "V*t");
    assert!(!updated.show_live && updated.on_means);
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert!(!def.intermediates[0].show_live && def.intermediates[0].on_means);

    assert!(create_intermediate(
        &pool,
        "p1-estadistica",
        IntermediateInput {
            display_symbol: None,
            symbol: "Z".into(),
            name: "z".into(),
            unit: "".into(),
            formula: "   ".into(),
            show_live: false,
            on_means: false,
        },
    )
    .await
    .is_err());

    assert!(delete_intermediate(&pool, "p1-estadistica", &q.id)
        .await
        .unwrap());
    assert_eq!(
        intermediates_for(&mut pool.acquire().await.unwrap(), "p1-estadistica")
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn the_library_loads_p1() {
    let (pool, _dir) = setup().await;
    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    assert!(def.parts.is_empty());
    let operators = def.determinations.as_ref().unwrap();
    assert_eq!((operators.count, operators.label.as_str()), (3, "Operador"));
    assert!(!operators.average);
    assert_eq!(operators.display, DeterminationDisplay::Pestanas);
    assert_eq!(def.quantities.len(), 3);
    let by_symbol = |s: &str| def.quantities.iter().find(|q| q.symbol == s).unwrap();
    assert!(by_symbol("L").is_given && !by_symbol("L").per_determination);
    assert!(by_symbol("t_med").is_given);
    assert!(
        !by_symbol("t_med").has_uncertainty,
        "t_med no deberia pedir incertidumbre"
    );
    let t = by_symbol("T");
    assert!(t.repeated && t.per_determination && t.optional);

    let symbols: Vec<&str> = def.results.iter().map(|r| r.symbol.as_str()).collect();
    assert_eq!(symbols, ["gamma", "Q", "g"]);
    assert!(def.results.iter().all(|r| r.is_final));
    let res_by = |s: &str| def.results.iter().find(|r| r.symbol == s).unwrap();
    assert!(!res_by("gamma").has_uncertainty && !res_by("Q").has_uncertainty);
    assert!(res_by("g").has_uncertainty);
    let repeated: Vec<String> = def
        .determination_results()
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert_eq!(repeated, ["Q", "g"], "Q y g se calculan por operador");
}

#[tokio::test]
async fn the_library_loads_hidrostatica() {
    let (pool, _dir) = setup().await;
    let def = definition(&pool, "hidrostatica").await.unwrap().unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("estadistico"));
    assert_eq!(def.parts.len(), 2);
    for part in &def.parts {
        let d = part.determinations.as_ref().unwrap();
        assert_eq!((d.count, d.label.as_str(), d.average), (3, "Medida", true));
    }

    assert_eq!(def.quantities.len(), 26);

    let by_symbol = |s: &str| def.quantities.iter().find(|q| q.symbol == s).unwrap();
    assert_eq!(by_symbol("b_E").default_value, Some(10.0));
    for i in 1..=9 {
        let b = by_symbol(&format!("b{i}"));
        assert!(b.is_given, "b{i} deberia ser dato de catedra");
        assert!(!b.per_determination);
        assert_eq!(b.default_value, Some(i as f64));
        let m = by_symbol(&format!("m_{i}"));
        assert!(!m.is_given && !m.repeated && m.per_determination);
        assert_eq!(m.default_value, Some(0.0));
    }
    assert!(by_symbol("mw").per_determination);
    assert_eq!(by_symbol("dw").slot_count, Some(9));
    assert!(by_symbol("dw").per_determination);
    assert!(!by_symbol("g_ac").has_uncertainty);

    let symbols: Vec<&str> = def.results.iter().map(|r| r.symbol.as_str()).collect();
    assert_eq!(symbols, ["E", "rho", "gamma"]);
    assert!(def.results.iter().all(|r| r.is_final));
    let res_by = |s: &str| def.results.iter().find(|r| r.symbol == s).unwrap();
    assert!(!res_by("gamma").has_uncertainty);
    assert!(res_by("E").has_uncertainty && res_by("rho").has_uncertainty);
    for i in 1..=9 {
        assert!(
            res_by("E").formula.contains(&format!("m_{i}*b{i}")),
            "E sin la ranura {i}"
        );
    }
    let mut averaged = def.averaged_results();
    averaged.sort();
    assert_eq!(averaged, ["E", "gamma", "rho"]);
}

#[tokio::test]
async fn analyze_p1_estadistica_con_operadores_opcionales() {
    let (pool, _dir) = setup().await;
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
            operator_instruments: None,
        };

    let (l, t_med) = (1.0_f64, 2.006_f64);
    let t1_vals = vec![2.0_f64, 2.02, 1.98];
    let t1_mean = t1_vals.iter().sum::<f64>() / t1_vals.len() as f64;
    let measurements = vec![
        mk("L", vec![l], Some(0.002)),
        mk("t_med", vec![t_med], Some(99.0)),
        crate::computation::MeasurementInput {
            operator_replicas: Some(vec![t1_vals, vec![], vec![]]),
            operator_instruments: None,
            ..mk("T", vec![], None)
        },
    ];

    let analysis = crate::computation::analyze(&pool, "p1-estadistica", &measurements)
        .await
        .unwrap();
    assert_eq!(analysis.determinations.len(), 3);
    let first = &analysis.determinations[0];
    assert_eq!(first.label, "Operador 1");
    let t = first.quantities.iter().find(|q| q.symbol == "T").unwrap();
    assert!(t.result.u_c > 0.0, "T sí debe tener incertidumbre real");

    let t_med_q = analysis
        .quantities
        .iter()
        .find(|q| q.symbol == "t_med")
        .unwrap();
    assert_eq!(
        t_med_q.result.u_c, 0.0,
        "t_med sin instrumento/U debe dar u=0 pese a un given_u cargado"
    );

    let in_operator = |k: usize, sym: &str| {
        analysis.determinations[k]
            .derived
            .iter()
            .find(|d| d.symbol == sym)
            .unwrap_or_else(|| panic!("{sym} debe estar en el operador {}", k + 1))
            .value
    };
    let expected_g = 4.0 * std::f64::consts::PI.powi(2) * l / (t1_mean * t1_mean);
    assert!((in_operator(0, "g") - expected_g).abs() < 1e-9);
    let expected_q = std::f64::consts::PI * t_med / (t1_mean * std::f64::consts::LN_2);
    assert!((in_operator(0, "Q") - expected_q).abs() < 1e-9);
    for k in [1, 2] {
        assert!(!in_operator(k, "g").is_finite());
    }
    assert!(
        analysis
            .warnings
            .iter()
            .any(|w| w.starts_with("Operador 2, 3:") && w.contains("(T)")),
        "debe haber una advertencia por los operadores sin datos: {:?}",
        analysis.warnings
    );

    let gamma = analysis
        .derived
        .iter()
        .find(|d| d.symbol == "gamma")
        .unwrap();
    assert!(!gamma.has_uncertainty);
    let expected_gamma = 2.0 * std::f64::consts::LN_2 / t_med;
    assert!((gamma.value - expected_gamma).abs() < 1e-9);
}

#[tokio::test]
async fn deleting_practice_cascades_to_definition() {
    let (pool, _dir) = setup().await;
    empty_practice(&pool).await;
    create_quantity(&pool, "vacia", sample_quantity())
        .await
        .unwrap();
    create_result(&pool, "vacia", sample_result(), None)
        .await
        .unwrap();
    sqlx::query("DELETE FROM practices WHERE id = 'vacia'")
        .execute(&pool)
        .await
        .unwrap();
    let quantities: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_quantities WHERE practice_id = 'vacia'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let results: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM practice_results WHERE practice_id = 'vacia'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(quantities.0, 0);
    assert_eq!(results.0, 0);
}

#[tokio::test]
async fn the_library_loads_p3_relajacion() {
    let (pool, _dir) = setup().await;
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
                operator_instruments: None,
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
async fn the_library_loads_p3_as_two_parts() {
    let (pool, _dir) = setup().await;
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
            operator_instruments: None,
        },
        crate::computation::MeasurementInput {
            quantity_id: id("a"),
            instrument_id: None,
            scale_id: None,
            values: freqs.iter().map(|_| 1.0).collect(),
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        },
        crate::computation::MeasurementInput {
            quantity_id: id("b"),
            instrument_id: None,
            scale_id: None,
            values: b_vals,
            given_u: None,
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
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
    assert!(
        def.aggregates.iter().all(|a| !a.has_uncertainty),
        "los 4 agregados se entregan sin U"
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
        operator_instruments: None,
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
        operator_instruments: None,
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
        operator_instruments: None,
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
        operator_instruments: None,
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
async fn submissions_off_slots_counts_non_slot_readings() {
    let (pool, _dir) = setup().await;
    let (qid,): (String,) = sqlx::query_as(
        "SELECT id FROM practice_quantities WHERE practice_id = 'hidrostatica' AND symbol = 'dw'",
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
    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(submissions_off_slots(&mut conn, &qid, 9).await.unwrap(), 2);
    assert_eq!(submissions_off_slots(&mut conn, &qid, 12).await.unwrap(), 1);
    drop(conn);
    let with_slots = QuantityInput {
        slot_count: Some(9),
        ..sample_quantity()
    };
    let error = update_quantity(&pool, &qid, with_slots).await.unwrap_err();
    assert!(error.to_string().starts_with("Hay 2 entrega(s)"), "{error}");
    let (symbol,): (String,) =
        sqlx::query_as("SELECT symbol FROM practice_quantities WHERE id = ?1")
            .bind(&qid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(symbol, "dw");
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
            help_text: None,
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
    crate::computation::tests::p1_partes(&pool).await;
    let p1 = definition(&pool, "p1-partes").await.unwrap().unwrap();
    assert_eq!(p1.result_part("Q"), Some("p1-partes-op1"));
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
async fn a_result_used_by_series_artifacts_goes_to_the_series_part() {
    let (pool, _dir) = setup().await;
    create_part(
        &pool,
        "fluidos-1",
        &PartInput {
            help_text: None,
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
async fn a_new_part_gets_a_kind_only_when_the_others_have_one() {
    let (pool, _dir) = setup().await;
    let input = PartInput {
        help_text: None,
        title: "Nueva".into(),
    };
    let first = create_part(&pool, "fluidos-1", &input).await.unwrap();
    assert_eq!(first.analysis_kind, None);
    sqlx::query("UPDATE practice_parts SET analysis_kind = 'curva' WHERE id = ?1")
        .bind(&first.id)
        .execute(&pool)
        .await
        .unwrap();
    let second = create_part(&pool, "fluidos-1", &input).await.unwrap();
    assert_eq!(second.analysis_kind.as_deref(), Some("estadistico"));
    assert_eq!(second.position, first.position + 1);
    let saved = definition(&pool, "fluidos-1").await.unwrap().unwrap();
    let stored = saved.parts.iter().find(|p| p.id == second.id).unwrap();
    assert_eq!(stored.analysis_kind.as_deref(), Some("estadistico"));
}

#[tokio::test]
async fn a_duplicate_does_not_share_the_source_version() {
    let (pool, _dir) = setup().await;
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
        publish_checked(&pool, &copy.id, None, None, none)
            .await
            .unwrap(),
        Publish::Unchanged(1)
    ));
    sqlx::query("UPDATE practice_results SET formula = formula || ' * 1' WHERE practice_id = ?1")
        .bind(&copy.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        publish_checked(&pool, &copy.id, None, None, none)
            .await
            .unwrap(),
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
        display_symbol: None,
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
        ..Default::default()
    }
}

#[tokio::test]
async fn a_quantity_in_a_version_is_retired_and_frees_its_symbol() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
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
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
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
    crate::computation::tests::p1_partes(&pool).await;
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
    rename("p1-partes", "T1", "Tx").await;
    rename("p2-cc", "Vg_s", "Vfuente").await;
    sqlx::query(
        "UPDATE practice_results SET name = 'Editado', is_final = 0, has_uncertainty = 1 \
         WHERE practice_id = 'p2-cc' AND symbol = 'P_max_e'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let snapshot = |d: PracticeDefinition| serde_json::to_string(&d).unwrap();
    let before_p1 = snapshot(definition(&pool, "p1-partes").await.unwrap().unwrap());
    let before_cc = snapshot(definition(&pool, "p2-cc").await.unwrap().unwrap());

    db::migrate(&pool).await.unwrap();

    assert_eq!(
        before_p1,
        snapshot(definition(&pool, "p1-partes").await.unwrap().unwrap())
    );
    assert_eq!(
        before_cc,
        snapshot(definition(&pool, "p2-cc").await.unwrap().unwrap())
    );
}

#[tokio::test]
async fn the_power_results_have_no_uncertainty_from_the_first_start() {
    let (pool, _dir) = setup().await;
    let cc = definition(&pool, "p2-cc").await.unwrap().unwrap();
    for symbol in ["P_max_e", "P_max_t", "RP_max_e", "RP_max_t"] {
        let result = cc.results.iter().find(|r| r.symbol == symbol).unwrap();
        assert!(!result.has_uncertainty, "{symbol}");
    }
}

#[tokio::test]
async fn cosmetic_changes_update_the_current_version_without_a_new_one() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
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
    let cosmetic = changes(pool.clone()).await;
    assert_eq!(cosmetic.kind, DraftChangeKind::Cosmetic);
    assert_eq!(cosmetic.changes, ["Cambia la tolerancia de g1"]);
    let before = current_version(&pool, practice).await.unwrap().unwrap();
    assert!(matches!(
        publish_checked(&pool, practice, None, None, no_problems)
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
        publish_checked(&pool, practice, None, None, no_problems)
            .await
            .unwrap(),
        Publish::Published { number } if number == before.number + 1
    ));
}

#[tokio::test]
async fn draft_changes_list_what_differs_and_count_submissions() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
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
async fn reordering_updates_the_current_version_and_records_who() {
    let (pool, _dir) = setup().await;
    let practice = "p1-estadistica";
    let before = current_version(&pool, practice).await.unwrap().unwrap();
    assert!(draft_changes(&pool, practice)
        .await
        .unwrap()
        .unwrap()
        .last_update
        .is_none());
    let moved = before.definition.quantities[1].id.clone();
    assert_eq!(
        move_item(&pool, Orderable::Quantities, practice, &moved, true)
            .await
            .unwrap(),
        Some(true)
    );
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Cosmetic);

    let no_problems = |_: &PracticeDefinition| Vec::new();
    assert!(matches!(
        publish_checked(&pool, practice, Some("docente-x"), None, no_problems)
            .await
            .unwrap(),
        Publish::UpdatedCurrent { number } if number == before.number
    ));
    let after = current_version(&pool, practice).await.unwrap().unwrap();
    assert_eq!(after.definition.quantities[0].id, moved);
    let (who,): (String,) =
        sqlx::query_as("SELECT updated_by FROM practice_version_updates WHERE version_id = ?1")
            .bind(&after.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(who, "docente-x");
    assert!(draft_changes(&pool, practice)
        .await
        .unwrap()
        .unwrap()
        .last_update
        .is_some());
}

#[tokio::test]
async fn reordering_intermediates_stays_structural() {
    let (pool, _dir) = setup().await;
    let practice = "filtros";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    assert!(
        def.intermediates.len() >= 2,
        "{practice} debía tener intermedias"
    );
    sqlx::query("UPDATE practice_intermediates SET position = -position WHERE practice_id = ?1")
        .bind(practice)
        .execute(&pool)
        .await
        .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Structural);
    assert_eq!(found.changes, ["Cambia el orden de las intermedias"]);
}

#[tokio::test]
async fn move_item_reorders_intermediates() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let symbols = |def: &PracticeDefinition| -> Vec<String> {
        def.intermediates
            .iter()
            .map(|it| it.symbol.clone())
            .collect()
    };
    let def = definition(&pool, "filtros").await.unwrap().unwrap();
    assert_eq!(symbols(&def), ["omega", "razon", "phi"]);
    let phi = def.intermediates[2].id.clone();
    assert_eq!(
        move_item(&pool, Orderable::Intermediates, "filtros", &phi, true)
            .await
            .unwrap(),
        Some(true)
    );
    let def = definition(&pool, "filtros").await.unwrap().unwrap();
    assert_eq!(symbols(&def), ["omega", "phi", "razon"]);
}

#[tokio::test]
async fn move_item_reorders_point_results() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let practice = "p1-estadistica";
    let mut ids = Vec::new();
    for symbol in ["Ra", "Rb"] {
        let row = create_point_result(
            &pool,
            practice,
            PointResultInput {
                display_symbol: None,
                symbol: symbol.into(),
                name: symbol.into(),
                unit: "".into(),
                formula: "L".into(),
            },
        )
        .await
        .unwrap();
        ids.push(row.id);
    }
    let no_problems = |_: &PracticeDefinition| Vec::new();
    publish_checked(&pool, practice, None, None, no_problems)
        .await
        .unwrap();
    assert_eq!(
        move_item(&pool, Orderable::PointResults, practice, &ids[1], true)
            .await
            .unwrap(),
        Some(true)
    );
    let def = definition(&pool, practice).await.unwrap().unwrap();
    let symbols: Vec<_> = def
        .point_results
        .iter()
        .map(|r| r.symbol.as_str())
        .collect();
    assert_eq!(symbols, ["Rb", "Ra"]);
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Cosmetic);
    let published = current_version(&pool, practice)
        .await
        .unwrap()
        .unwrap()
        .definition;
    let mut draft = def.clone();
    draft.results[0].formula.push_str(" + 0");
    let comparison = compare_definitions(&published, &draft).unwrap();
    assert_eq!(comparison.kind, DraftChangeKind::Structural);
    assert_eq!(
        comparison.changes,
        [format!("Cambia la fórmula de {}", draft.results[0].symbol)]
    );
}

#[tokio::test]
async fn move_item_reports_an_unknown_formula_row() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    for list in [
        Orderable::Intermediates,
        Orderable::PointResults,
        Orderable::Aggregates,
    ] {
        assert_eq!(
            move_item(&pool, list, "filtros", "no-existe", true)
                .await
                .unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn editing_help_text_updates_the_current_version() {
    let (pool, _dir) = setup().await;
    let practice = "p3-relajacion";
    let before = current_version(&pool, practice).await.unwrap().unwrap();
    let part = before.definition.parts[0].clone();
    let section = before.definition.sections[0].clone();
    update_part(
        &pool,
        practice,
        &part.id,
        &PartInput {
            title: part.title.clone(),
            help_text: Some("Medí con el osciloscopio.".into()),
        },
    )
    .await
    .unwrap();
    update_section(
        &pool,
        practice,
        &section.id,
        &SectionInput {
            title: section.title.clone(),
            part_id: section.part_id.clone(),
            help_text: Some("Anotá la escala.".into()),
        },
    )
    .await
    .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Cosmetic);
    assert_eq!(
        found.changes,
        [
            format!("Cambia la ayuda de la parte {}", part.title),
            format!("Cambia la ayuda de la sección {}", section.title),
        ]
    );
    let no_problems = |_: &PracticeDefinition| Vec::new();
    assert!(matches!(
        publish_checked(&pool, practice, None, None, no_problems)
            .await
            .unwrap(),
        Publish::UpdatedCurrent { number } if number == before.number
    ));
    let after = current_version(&pool, practice).await.unwrap().unwrap();
    assert_eq!(
        after.definition.parts[0].help_text,
        "Medí con el osciloscopio."
    );
    assert_eq!(after.definition.sections[0].help_text, "Anotá la escala.");
}

#[tokio::test]
async fn the_description_waits_for_publish() {
    let (pool, _dir) = setup().await;
    let practice = "p3-relajacion";
    let before = current_version(&pool, practice).await.unwrap().unwrap();
    sqlx::query("UPDATE practices SET description = 'Nueva' WHERE id = ?1")
        .bind(practice)
        .execute(&pool)
        .await
        .unwrap();
    let published = current_version(&pool, practice).await.unwrap().unwrap();
    assert_eq!(
        published.definition.description,
        before.definition.description
    );
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Cosmetic);
    assert_eq!(found.changes, ["Cambia la descripción"]);
    let no_problems = |_: &PracticeDefinition| Vec::new();
    publish_checked(&pool, practice, None, None, no_problems)
        .await
        .unwrap();
    let after = current_version(&pool, practice).await.unwrap().unwrap();
    assert_eq!(after.number, before.number);
    assert_eq!(after.definition.description, "Nueva");
}

#[tokio::test]
async fn renaming_without_help_text_keeps_it() {
    let (pool, _dir) = setup().await;
    let practice = "p3-relajacion";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    let part = def.parts[0].clone();
    let section = def.sections[0].clone();
    for help_text in [Some("Ayuda".to_string()), None] {
        update_part(
            &pool,
            practice,
            &part.id,
            &PartInput {
                title: "Renombrada".into(),
                help_text: help_text.clone(),
            },
        )
        .await
        .unwrap();
        update_section(
            &pool,
            practice,
            &section.id,
            &SectionInput {
                title: "Renombrada".into(),
                part_id: section.part_id.clone(),
                help_text,
            },
        )
        .await
        .unwrap();
    }
    let def = definition(&pool, practice).await.unwrap().unwrap();
    assert_eq!(def.parts[0].title, "Renombrada");
    assert_eq!(def.parts[0].help_text, "Ayuda");
    assert_eq!(def.sections[0].help_text, "Ayuda");
}

#[tokio::test]
async fn reordering_curves_stays_structural() {
    let (pool, _dir) = setup().await;
    let practice = "filtros";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    assert!(def.curves.len() >= 2, "{practice} debía tener curvas");
    move_item(&pool, Orderable::Curves, practice, &def.curves[1].id, true)
        .await
        .unwrap()
        .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Structural);
    assert_eq!(found.changes, ["Cambia el orden de las curvas"]);
}

#[tokio::test]
async fn a_section_move_is_listed_next_to_a_formula_change() {
    let (pool, _dir) = setup().await;
    let practice = "p2-cc";
    sqlx::query(
        "UPDATE practice_results SET section_id = NULL, formula = '2*Vg_s' \
         WHERE practice_id = ?1 AND symbol = 'P_max_e'",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(
        found.changes,
        [
            "Cambia la fórmula de P_max_e",
            "Cambia la organización en secciones"
        ]
    );
}

#[tokio::test]
async fn publishing_rejects_a_draft_that_changed_since_it_was_shown() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
    let no_problems = |_: &PracticeDefinition| Vec::new();
    let formula = |symbol: &'static str, formula: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "UPDATE practice_results SET formula = ?3 WHERE practice_id = ?1 AND symbol = ?2",
            )
            .bind(practice)
            .bind(symbol)
            .bind(formula)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    let before = current_version(&pool, practice).await.unwrap().unwrap();
    formula("g1", "2*L").await;
    let shown = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(shown.kind, DraftChangeKind::Structural);

    formula("g2", "3*L").await;
    assert!(matches!(
        publish_checked(&pool, practice, None, Some(&shown.fingerprint), no_problems)
            .await
            .unwrap(),
        Publish::DraftChanged
    ));
    let after = current_version(&pool, practice).await.unwrap().unwrap();
    assert_eq!(after.number, before.number);

    let shown = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert!(matches!(
        publish_checked(&pool, practice, None, Some(&shown.fingerprint), no_problems)
            .await
            .unwrap(),
        Publish::Published { .. }
    ));
}

#[tokio::test]
async fn renaming_a_sectioned_quantity_does_not_report_a_section_move() {
    let (pool, _dir) = setup().await;
    let practice = "p2-cc";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    let symbol = def
        .sections
        .iter()
        .find_map(|section| section.quantities.first())
        .unwrap()
        .clone();
    sqlx::query(
        "UPDATE practice_quantities SET symbol = 'Renombrada' WHERE practice_id = ?1 AND symbol = ?2",
    )
    .bind(practice)
    .bind(&symbol)
    .execute(&pool)
    .await
    .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.changes, ["Cambia la magnitud Renombrada"]);
}

#[tokio::test]
async fn draft_changes_count_the_submissions_of_the_current_version() {
    let (pool, _dir) = setup().await;
    let practice = "p1-estadistica";
    let first = current_version(&pool, practice).await.unwrap().unwrap();
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
         csv_path, analysis_json, submitted_at, practice_version_id) \
         VALUES ('s1', 'a', 'g', 'c', ?1, '', '', '{}', '2026-10-01T00:00:00Z', ?2)",
    )
    .bind(practice)
    .bind(&first.id)
    .execute(&pool)
    .await
    .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!((found.submissions, found.version_submissions), (1, 1));
    publish_version(&pool, practice, None).await.unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!((found.submissions, found.version_submissions), (1, 0));
}

#[tokio::test]
async fn moving_a_result_to_another_section_stays_structural() {
    let (pool, _dir) = setup().await;
    let practice = "p2-cc";
    sqlx::query(
        "UPDATE practice_results SET section_id = NULL WHERE practice_id = ?1 AND symbol = 'P_max_e'",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Structural);
    assert_eq!(found.changes, ["Cambia la organización en secciones"]);
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

#[tokio::test]
async fn renumbering_positions_with_the_same_order_is_not_a_change() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
    let def = definition(&pool, practice).await.unwrap().unwrap();
    let t2 = def.quantities.iter().find(|q| q.symbol == "T2").unwrap();
    sqlx::query("DELETE FROM practice_results WHERE practice_id = ?1 AND formula LIKE '%T2%'")
        .bind(practice)
        .execute(&pool)
        .await
        .unwrap();
    remove_quantity(&pool, practice, &t2.id).await.unwrap();
    publish_version(&pool, practice, None).await.unwrap();

    let def = definition(&pool, practice).await.unwrap().unwrap();
    let t1 = def.quantities.iter().find(|q| q.symbol == "T1").unwrap();
    for up in [false, true] {
        move_item(&pool, Orderable::Quantities, practice, &t1.id, up)
            .await
            .unwrap()
            .unwrap();
    }
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::None, "{:?}", found.changes);
}

#[tokio::test]
async fn an_unrelated_problem_does_not_block_a_cosmetic_update() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
    let broken = |_: &PracticeDefinition| vec!["un problema anterior".to_string()];
    let set = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(sql)
                .bind(practice)
                .execute(&pool)
                .await
                .unwrap();
        }
    };

    set("UPDATE practice_results SET name = 'Otro' WHERE practice_id = ?1 AND symbol = 'g1'").await;
    assert!(matches!(
        publish_checked(&pool, practice, None, None, broken)
            .await
            .unwrap(),
        Publish::UpdatedCurrent { .. }
    ));

    set("UPDATE practice_results SET formula = '2*L' WHERE practice_id = ?1 AND symbol = 'g1'")
        .await;
    assert!(matches!(
        publish_checked(&pool, practice, None, None, broken)
            .await
            .unwrap(),
        Publish::Rejected(_)
    ));
}

#[tokio::test]
async fn seeded_practices_keep_their_special_symbols_as_data() {
    let (pool, _dir) = setup().await;
    for (practice, symbol, shown) in [
        ("p3-relajacion", "tmedio", "t_1/2"),
        ("p2-cc", "Vg_s", "V_G"),
        ("p2-cc", "VR1_s_t", "VR1"),
        ("ca-rlc", "VRpp", "V_R_PP"),
        ("ca-rlc", "phiR_teo", "phi_R_teo"),
    ] {
        let draft = definition(&pool, practice).await.unwrap().unwrap();
        assert_eq!(
            draft.display_symbols.get(symbol).map(String::as_str),
            Some(shown),
            "{practice} {symbol}"
        );
        let published = current_version(&pool, practice).await.unwrap().unwrap();
        assert_eq!(published.definition.display_symbols, draft.display_symbols);
        let changes = draft_changes(&pool, practice).await.unwrap().unwrap();
        assert_eq!(changes.kind, DraftChangeKind::None, "{practice}");
    }
}

#[tokio::test]
async fn changing_a_display_symbol_is_cosmetic() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
    assert!(set_display_symbol(&pool, practice, "T1", "T_1")
        .await
        .unwrap());
    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Cosmetic);
    assert_eq!(found.changes, vec!["Cambia cómo se muestra T1".to_string()]);
}

#[tokio::test]
async fn display_changes_are_detected_on_symbols_named_like_volatile_keys() {
    let (pool, _dir) = setup().await;
    let practice = "p1-estadistica";
    for symbol in ["position", "status", "current_version"] {
        let input = QuantityInput {
            symbol: symbol.into(),
            ..sample_quantity()
        };
        create_quantity(&pool, practice, input).await.unwrap();
        publish_checked(&pool, practice, None, None, |_| vec![])
            .await
            .unwrap();
        let before = draft_changes(&pool, practice).await.unwrap().unwrap();
        assert_eq!(before.kind, DraftChangeKind::None, "{symbol}");

        assert!(set_display_symbol(&pool, practice, symbol, "X_y")
            .await
            .unwrap());

        let after = draft_changes(&pool, practice).await.unwrap().unwrap();
        assert_eq!(after.kind, DraftChangeKind::Cosmetic, "{symbol}");
        assert_eq!(
            after.changes,
            vec![format!("Cambia cómo se muestra {symbol}")]
        );
        assert_ne!(before.fingerprint, after.fingerprint, "{symbol}");
        let outcome = publish_checked(&pool, practice, None, None, |_| vec![])
            .await
            .unwrap();
        assert!(!matches!(outcome, Publish::Unchanged(_)), "{symbol}");
    }
}

#[tokio::test]
async fn retired_quantities_stay_out_of_the_display_symbols() {
    let (pool, _dir) = setup().await;
    let practice = "p1-estadistica";
    let input = QuantityInput {
        symbol: "Zq".into(),
        ..sample_quantity()
    };
    let quantity = create_quantity(&pool, practice, input).await.unwrap();
    assert!(set_display_symbol(&pool, practice, "Zq", "Z_q")
        .await
        .unwrap());
    publish_checked(&pool, practice, None, None, |_| vec![])
        .await
        .unwrap();
    assert_eq!(
        remove_quantity(&pool, practice, &quantity.id)
            .await
            .unwrap(),
        QuantityRemoval::Retired
    );

    let draft = definition(&pool, practice).await.unwrap().unwrap();
    assert!(!draft
        .display_symbols
        .keys()
        .any(|key| key.starts_with("Zq")));
    publish_checked(&pool, practice, None, None, |_| vec![])
        .await
        .unwrap();
    let published = current_version(&pool, practice).await.unwrap().unwrap();
    assert!(!published
        .definition
        .display_symbols
        .keys()
        .any(|key| key.starts_with("Zq")));
}

#[tokio::test]
async fn the_structural_summary_mentions_display_changes_of_kept_symbols_only() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
    assert!(set_display_symbol(&pool, practice, "T1", "T_uno")
        .await
        .unwrap());
    assert!(set_display_symbol(&pool, practice, "T2", "T_dos")
        .await
        .unwrap());
    publish_checked(&pool, practice, None, None, |_| vec![])
        .await
        .unwrap();

    assert!(set_display_symbol(&pool, practice, "T1", "T_otro")
        .await
        .unwrap());
    sqlx::query(
        "UPDATE practice_quantities SET symbol = 'T2b' WHERE practice_id = ?1 AND symbol = 'T2'",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    let added = QuantityInput {
        symbol: "Zq".into(),
        ..sample_quantity()
    };
    create_quantity(&pool, practice, added).await.unwrap();
    assert!(set_display_symbol(&pool, practice, "Zq", "Z_q")
        .await
        .unwrap());

    let found = draft_changes(&pool, practice).await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Structural);
    let shown: Vec<&String> = found
        .changes
        .iter()
        .filter(|change| change.starts_with("Cambia cómo se muestra"))
        .collect();
    assert_eq!(shown, ["Cambia cómo se muestra T1"], "{:?}", found.changes);
}

#[tokio::test]
async fn a_later_boot_does_not_rewrite_versions() {
    let (pool, _dir) = setup().await;
    sqlx::query(
        "UPDATE practice_versions \
         SET definition_json = json_remove(definition_json, '$.display_symbols') \
         WHERE practice_id = 'p3-relajacion'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let snapshot = || async {
        let (json,): (String,) = sqlx::query_as(
            "SELECT definition_json FROM practice_versions WHERE practice_id = 'p3-relajacion'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        json
    };
    let before = snapshot().await;

    assert_eq!(before, snapshot().await);
}

#[tokio::test]
async fn display_symbols_are_saved_with_the_element() {
    let (pool, _dir) = setup().await;
    let practice = "p1-estadistica";
    let shown = |symbol: &'static str| {
        let pool = pool.clone();
        async move {
            let def = definition(&pool, practice).await.unwrap().unwrap();
            def.display_symbols.get(symbol).cloned()
        }
    };

    let quantity = QuantityInput {
        symbol: "Zq".into(),
        display_symbol: Some(" Z_q ".into()),
        ..sample_quantity()
    };
    let created = create_quantity(&pool, practice, quantity).await.unwrap();
    assert_eq!(shown("Zq").await.as_deref(), Some("Z_q"));

    let untouched = QuantityInput {
        symbol: "Zq".into(),
        ..sample_quantity()
    };
    update_quantity(&pool, &created.id, untouched)
        .await
        .unwrap();
    assert_eq!(shown("Zq").await.as_deref(), Some("Z_q"));

    let renamed = QuantityInput {
        symbol: "Zr".into(),
        display_symbol: Some("Z_r".into()),
        ..sample_quantity()
    };
    update_quantity(&pool, &created.id, renamed).await.unwrap();
    assert_eq!(shown("Zr").await.as_deref(), Some("Z_r"));

    let cleared = QuantityInput {
        symbol: "Zr".into(),
        display_symbol: Some(String::new()),
        ..sample_quantity()
    };
    update_quantity(&pool, &created.id, cleared).await.unwrap();
    assert_eq!(shown("Zr").await, None);

    let result = ResultInput {
        symbol: "Zs".into(),
        display_symbol: Some("Z_s".into()),
        ..sample_result()
    };
    let saved = create_result(&pool, practice, result, None).await.unwrap();
    assert_eq!(shown("Zs").await.as_deref(), Some("Z_s"));
    for tolerance in [None, Some(Some(1.0))] {
        let edited = ResultInput {
            symbol: "Zs".into(),
            tolerance,
            display_symbol: Some("Z_t".into()),
            ..sample_result()
        };
        update_result(&pool, &saved.id, edited).await.unwrap();
        assert_eq!(shown("Zs").await.as_deref(), Some("Z_t"));
        let kept = ResultInput {
            symbol: "Zs".into(),
            tolerance,
            ..sample_result()
        };
        update_result(&pool, &saved.id, kept).await.unwrap();
        assert_eq!(shown("Zs").await.as_deref(), Some("Z_t"));
        set_display_symbol(&pool, practice, "Zs", "Z_s")
            .await
            .unwrap();
    }

    let intermediate = IntermediateInput {
        display_symbol: Some("I_a".into()),
        symbol: "Ia".into(),
        name: "a".into(),
        unit: "m".into(),
        formula: "2".into(),
        show_live: false,
        on_means: false,
    };
    let row = create_intermediate(&pool, practice, intermediate)
        .await
        .unwrap();
    assert_eq!(shown("Ia").await.as_deref(), Some("I_a"));
    let intermediate = IntermediateInput {
        display_symbol: Some("I_b".into()),
        symbol: "Ia".into(),
        name: "a".into(),
        unit: "m".into(),
        formula: "2".into(),
        show_live: false,
        on_means: false,
    };
    update_intermediate(&pool, practice, &row.id, intermediate)
        .await
        .unwrap();
    assert_eq!(shown("Ia").await.as_deref(), Some("I_b"));

    let point = PointResultInput {
        display_symbol: Some("P_a".into()),
        symbol: "Pa".into(),
        name: "a".into(),
        unit: "m".into(),
        formula: "2".into(),
    };
    let row = create_point_result(&pool, practice, point).await.unwrap();
    assert_eq!(shown("Pa").await.as_deref(), Some("P_a"));
    let point = PointResultInput {
        display_symbol: None,
        symbol: "Pa".into(),
        name: "a".into(),
        unit: "m".into(),
        formula: "2".into(),
    };
    update_point_result(&pool, practice, &row.id, point)
        .await
        .unwrap();
    assert_eq!(shown("Pa").await.as_deref(), Some("P_a"));

    let aggregate = AggregateInput {
        display_symbol: Some("G_a".into()),
        symbol: "Ga".into(),
        name: "a".into(),
        unit: "m".into(),
        formula: "2".into(),
        is_final: false,
        has_uncertainty: true,
    };
    let row = create_aggregate(&pool, practice, aggregate).await.unwrap();
    assert_eq!(shown("Ga").await.as_deref(), Some("G_a"));
    let aggregate = AggregateInput {
        display_symbol: Some("G_b".into()),
        symbol: "Ga".into(),
        name: "a".into(),
        unit: "m".into(),
        formula: "2".into(),
        is_final: false,
        has_uncertainty: true,
    };
    update_aggregate(&pool, practice, &row.id, aggregate)
        .await
        .unwrap();
    assert_eq!(shown("Ga").await.as_deref(), Some("G_b"));
}

#[tokio::test]
async fn a_submission_shows_the_symbols_of_its_version_never_the_draft() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let practice = "p1-partes";
    assert!(set_display_symbol(&pool, practice, "T1", "T_publicado")
        .await
        .unwrap());
    publish_checked(&pool, practice, None, None, |_| vec![])
        .await
        .unwrap();
    let version = current_version(&pool, practice).await.unwrap().unwrap();
    for (id, linked) in [
        ("con-version", Some(version.id.as_str())),
        ("sin-version", None),
    ] {
        sqlx::query(
            "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
             csv_path, analysis_json, submitted_at, practice_version_id) \
             VALUES (?1, 'a', 'g', 'c', ?2, '', '', '{}', '2026-10-01T00:00:00Z', ?3)",
        )
        .bind(id)
        .bind(practice)
        .bind(linked)
        .execute(&pool)
        .await
        .unwrap();
    }
    assert!(set_display_symbol(&pool, practice, "T1", "T_borrador")
        .await
        .unwrap());

    for id in ["con-version", "sin-version"] {
        let symbols = submission_display_symbols(&pool, id).await.unwrap();
        assert_eq!(symbols["T1"], "T_publicado", "{id}");
    }
    assert!(submission_display_symbols(&pool, "no-existe")
        .await
        .unwrap()
        .is_empty());

    sqlx::query(
        "UPDATE practice_versions \
         SET definition_json = json_set(definition_json, '$.display_symbols', 5) WHERE id = ?1",
    )
    .bind(&version.id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(submission_display_symbols(&pool, "con-version")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_submission_without_version_reads_the_published_version_everywhere() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    db::run_startup_seeds(&pool, true, None).await.unwrap();
    let practice = "p1-partes";
    let tolerance = |value: f64| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "UPDATE practice_results SET tolerance = ?1 WHERE practice_id = ?2 AND symbol = 'Q'",
            )
            .bind(value)
            .bind(practice)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    tolerance(1.0).await;
    assert!(set_display_symbol(&pool, practice, "T1", "T_publicado")
        .await
        .unwrap());
    publish_checked(&pool, practice, None, None, |_| vec![])
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, course_id, practice_id, \
         file_name, csv_path, analysis_json, submitted_at) \
         VALUES ('sin-version', 'a', 'g', 'c', (SELECT id FROM courses LIMIT 1), ?1, '', '', '{}', \
         '2026-10-01T00:00:00Z')",
    )
    .bind(practice)
    .execute(&pool)
    .await
    .unwrap();
    tolerance(2.0).await;
    assert!(set_display_symbol(&pool, practice, "T1", "T_borrador")
        .await
        .unwrap());

    let definition = submission_definition(&pool, "sin-version")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(definition.display_symbols["T1"], "T_publicado");
    let detail = db::submission_detail(&pool, "sin-version")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_symbols["T1"], "T_publicado");
    assert_eq!(detail.result_tolerances["Q"], 1.0);
}

fn presentation_of(definition: &PracticeDefinition, symbol: &str) -> (String, String) {
    let q = definition
        .quantities
        .iter()
        .find(|q| q.symbol == symbol)
        .unwrap_or_else(|| panic!("falta {symbol} en {}", definition.practice_id));
    (q.label_mode.clone(), q.input_widget.clone())
}

fn presentation(label: &str, widget: &str) -> (String, String) {
    (label.to_string(), widget.to_string())
}

async fn assert_seeded_presentation(pool: &SqlitePool, read_version: bool) {
    let cases = [
        ("p1-estadistica", "T", "nombre", "cronometro"),
        ("p1-estadistica", "t_med", "nombre", "teclado"),
        ("p3-relajacion", "T_oc", "nombre_simbolo", "teclado"),
        ("p3-relajacion", "tmedio", "nombre", "teclado"),
        ("fluidos-1", "t", "nombre", "cronometro"),
        ("viscosidad", "t", "nombre", "cronometro"),
        ("fluidos-2", "t", "nombre", "teclado"),
        ("p2-cc", "Vg_s", "simbolo", "teclado"),
        ("filtros", "VRpp", "simbolo", "teclado"),
        ("ca-rlc", "a_R", "simbolo", "teclado"),
    ];
    for (practice, symbol, label, widget) in cases {
        let found = if read_version {
            current_version(pool, practice)
                .await
                .unwrap()
                .unwrap()
                .definition
        } else {
            definition(pool, practice).await.unwrap().unwrap()
        };
        assert_eq!(
            presentation_of(&found, symbol),
            presentation(label, widget),
            "{practice}/{symbol}"
        );
    }
}

#[tokio::test]
async fn seeded_quantities_get_their_label_and_input() {
    let (pool, _dir) = setup().await;

    assert_seeded_presentation(&pool, false).await;
    assert_seeded_presentation(&pool, true).await;
}

#[tokio::test]
async fn changing_a_label_or_input_is_cosmetic_and_summarized() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    sqlx::query(
        "UPDATE practice_quantities SET label_mode = 'simbolo', input_widget = 'teclado' \
         WHERE practice_id = 'p1-partes' AND symbol = 'T1'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let found = draft_changes(&pool, "p1-partes").await.unwrap().unwrap();
    assert_eq!(found.kind, DraftChangeKind::Cosmetic);
    assert_eq!(
        found.changes,
        vec![
            "Cambia el rótulo de T1".to_string(),
            "Cambia la entrada de T1".to_string()
        ]
    );
}

#[tokio::test]
async fn renaming_a_quantity_does_not_hide_its_input_change() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    sqlx::query(
        "UPDATE practice_quantities SET symbol = 'T_1', input_widget = 'teclado' \
         WHERE practice_id = 'p1-partes' AND symbol = 'T1'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let found = draft_changes(&pool, "p1-partes").await.unwrap().unwrap();
    assert!(
        found
            .changes
            .contains(&"Cambia la entrada de T_1".to_string()),
        "{:?}",
        found.changes
    );
}

#[tokio::test]
async fn a_copy_keeps_label_and_input_and_a_missing_field_resets_them() {
    let (pool, _dir) = setup().await;
    crate::computation::tests::p1_partes(&pool).await;
    let copy = duplicate_practice(&pool, "p1-partes", "Copia P1", "u1")
        .await
        .unwrap()
        .unwrap();
    let copied = definition(&pool, &copy.id).await.unwrap().unwrap();
    assert_eq!(
        presentation_of(&copied, "T1"),
        presentation("nombre", "cronometro")
    );

    let created = create_quantity(
        &pool,
        &copy.id,
        QuantityInput {
            label_mode: Some("simbolo".into()),
            input_widget: Some("cronometro".into()),
            quantity: Some("tiempo".into()),
            ..new_quantity("t_nuevo")
        },
    )
    .await
    .unwrap();
    assert_eq!(created.label_mode, "simbolo");
    assert_eq!(created.input_widget, "cronometro");

    let kept = update_quantity(
        &pool,
        &created.id,
        QuantityInput {
            quantity: Some("tiempo".into()),
            ..new_quantity("t_nuevo")
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(kept.label_mode, "simbolo");
    assert_eq!(kept.input_widget, "cronometro");

    let not_time = update_quantity(&pool, &created.id, new_quantity("t_nuevo"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(not_time.label_mode, "simbolo");
    assert_eq!(not_time.input_widget, "teclado");
}

async fn compare_with_of(
    pool: &SqlitePool,
    practice: &str,
    symbol: &str,
    in_version: bool,
) -> String {
    let sql = if in_version {
        "SELECT COALESCE(json_extract(r.value, '$.compare_with'), '') \
         FROM practices p JOIN practice_versions v ON v.id = p.current_version_id, \
         json_each(v.definition_json, '$.results') r \
         WHERE p.id = ?1 AND json_extract(r.value, '$.symbol') = ?2"
    } else {
        "SELECT compare_with FROM practice_results WHERE practice_id = ?1 AND symbol = ?2"
    };
    let (value,): (String,) = sqlx::query_as(sql)
        .bind(practice)
        .bind(symbol)
        .fetch_one(pool)
        .await
        .unwrap();
    value
}

async fn assert_seeded_comparisons(pool: &SqlitePool, in_version: bool) {
    for (result, quantity) in [
        ("VR1_s_t", "VR1_s"),
        ("VR3_s_t", "VR3_s"),
        ("VR2_p_t", "VR2_p"),
        ("P_max_t", ""),
        ("RP_max_t", ""),
        ("I_s", ""),
    ] {
        assert_eq!(
            compare_with_of(pool, "p2-cc", result, in_version).await,
            quantity,
            "{result} (versión: {in_version})"
        );
    }
}

#[tokio::test]
async fn seeding_sets_what_each_theoretical_result_compares_with() {
    let (pool, _dir) = setup().await;

    assert_seeded_comparisons(&pool, false).await;
    assert_seeded_comparisons(&pool, true).await;
}

#[tokio::test]
async fn renaming_or_removing_the_quantity_carries_its_comparison() {
    let (pool, _dir) = setup().await;
    let practice = "p1-estadistica";
    let q = create_quantity(&pool, practice, new_quantity("Qa"))
        .await
        .unwrap();
    let result = ResultInput {
        compare_with: Some("Qa".into()),
        symbol: "Qa_t".into(),
        ..sample_result()
    };
    create_result(&pool, practice, result, None).await.unwrap();

    update_quantity(&pool, &q.id, new_quantity("Qb"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(compare_with_of(&pool, practice, "Qa_t", false).await, "Qb");

    remove_quantity(&pool, practice, &q.id).await.unwrap();
    assert_eq!(compare_with_of(&pool, practice, "Qa_t", false).await, "");
}

#[tokio::test]
async fn a_submission_brings_the_comparisons_of_its_version() {
    let (pool, _dir) = setup().await;
    sqlx::query(
        "INSERT INTO submissions (id, student_name, group_name, course, practice_id, file_name, \
         csv_path, analysis_json, submitted_at) \
         VALUES ('s', 'a', 'g', 'c', 'p2-cc', '', '', '{}', '2026-10-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let pairs = submission_comparisons(&pool, "s").await.unwrap();
    assert_eq!(pairs["VR1_s_t"].with, "VR1_s");
    assert_eq!(pairs["VR2_p_t"].with, "VR2_p");
    assert_eq!(pairs["VR1_s_t"].factor, 1.0);
    assert!(!pairs.contains_key("P_max_t"));
    assert!(submission_comparisons(&pool, "no-existe")
        .await
        .unwrap()
        .is_empty());

    sqlx::query(
        "INSERT INTO practice_versions (id, practice_id, number, definition_json, published_at) \
         VALUES ('rota', 'p2-cc', 999, '{\"results\": 1}', '2026-10-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE submissions SET practice_version_id = 'rota' WHERE id = 's'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        submission_comparisons(&pool, "s").await.unwrap().is_empty(),
        "una versión ilegible no rompe el detalle de la entrega"
    );
}

async fn live_symbols(pool: &SqlitePool, practice: &str, in_version: bool) -> Vec<String> {
    let sql = if in_version {
        "SELECT json_extract(i.value, '$.symbol') \
         FROM practices p JOIN practice_versions v ON v.id = p.current_version_id, \
         json_each(v.definition_json, '$.intermediates') i \
         WHERE p.id = ?1 AND json_extract(i.value, '$.show_live') = 1"
    } else {
        "SELECT symbol FROM practice_intermediates WHERE practice_id = ?1 AND show_live = 1"
    };
    sqlx::query_scalar(sql)
        .bind(practice)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn assert_seeded_live_columns(pool: &SqlitePool) {
    for in_version in [false, true] {
        assert_eq!(live_symbols(pool, "p2-cc", in_version).await, ["P"]);
        assert_eq!(live_symbols(pool, "fluidos-1", in_version).await, ["Q"]);
        assert_eq!(live_symbols(pool, "viscosidad", in_version).await, ["v"]);
        assert!(live_symbols(pool, "filtros", in_version).await.is_empty());
    }
    for practice in ["p2-cc", "fluidos-1", "viscosidad"] {
        let found = draft_changes(pool, practice).await.unwrap().unwrap();
        assert_eq!(found.kind, DraftChangeKind::None, "{practice}");
    }
}

#[tokio::test]
async fn seeding_marks_the_live_columns() {
    let (pool, _dir) = setup().await;

    assert_seeded_live_columns(&pool).await;
    let def = definition(&pool, "viscosidad").await.unwrap().unwrap();
    assert_eq!(def.y_formula.as_deref(), Some("dx/t"));
}

fn json_keys(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                out.push(k.clone());
                json_keys(v, out);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| json_keys(v, out)),
        _ => {}
    }
}

#[tokio::test]
async fn export_and_import_keep_every_seeded_practice() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    for practice in db::practices(&pool).await.unwrap() {
        let exported = export_template(&pool, &practice.id, ExportSource::Borrador)
            .await
            .unwrap()
            .unwrap();
        check_template(&exported).unwrap_or_else(|e| panic!("{}: {e}", practice.id));
        let original = serde_json::to_value(&exported).unwrap();
        let mut keys = Vec::new();
        json_keys(&original, &mut keys);
        for internal in ["id", "practice_id", "position", "part_id"] {
            assert!(
                !keys.iter().any(|k| k == internal),
                "{}: exporta {internal}",
                practice.id
            );
        }
        let copy = import_template(&pool, &exported, "u1").await.unwrap();
        assert_ne!(copy.id, practice.id);
        assert_eq!(copy.status, "borrador");
        let again = export_template(&pool, &copy.id, ExportSource::Borrador)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&again).unwrap(),
            original,
            "{}",
            practice.id
        );
        let def = definition(&pool, &copy.id).await.unwrap().unwrap();
        assert_eq!(def.current_version, None);
    }
}

#[tokio::test]
async fn importing_twice_picks_a_free_id() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let exported = export_template(&pool, "p2-cc", ExportSource::Borrador)
        .await
        .unwrap()
        .unwrap();
    let first = import_template(&pool, &exported, "u1").await.unwrap();
    let second = import_template(&pool, &exported, "u1").await.unwrap();
    assert_eq!(second.id, format!("{}-2", first.id));
    assert!(export_template(&pool, "no-existe", ExportSource::Borrador)
        .await
        .unwrap()
        .is_none());
}

#[test]
fn check_template_rejects_what_cannot_be_written() {
    let template = |extra: serde_json::Value| -> PracticeTemplate {
        let mut base = serde_json::json!({
            "schema_version": 1, "name": "P", "analysis_kind": "estadistico",
            "parts": [{"title": "A"}],
            "quantities": [{"symbol": "x", "name": "x", "unit": "", "repeated": false}],
            "results": [{"symbol": "y", "name": "y", "unit": "", "formula": "2*x"}]
        });
        base.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(base).unwrap()
    };
    assert!(check_template(&template(serde_json::json!({}))).is_ok());
    assert!(check_template(&template(serde_json::json!({"analysis_kind": " curva "}))).is_ok());
    let curve_with_statistical_part = serde_json::json!({
        "analysis_kind": "curva",
        "parts": [{"title": "A"}, {"title": "B", "determinations": {"count": 2, "label": "M"}}],
        "quantities": [
            {"symbol": "x", "name": "x", "unit": "", "repeated": true, "per_point": true},
            {"symbol": "t", "name": "t", "unit": "", "repeated": true, "per_determination": true}
        ],
        "sections": [
            {"title": "S", "part": 0, "quantities": ["x"]},
            {"title": "T", "part": 1, "quantities": ["t"]}
        ]
    });
    assert_eq!(
        check_template(&template(curve_with_statistical_part)),
        Ok(())
    );
    for key in ["", "  ", "serie-1"] {
        let parts = serde_json::json!({"parts": [{"title": "A", "key": key}]});
        assert!(check_template(&template(parts)).is_ok(), "{key:?}");
    }
    for (extra, expected) in [
        (serde_json::json!({"schema_version": 2}), "versión 2"),
        (serde_json::json!({"name": " "}), "nombre"),
        (serde_json::json!({"analysis_kind": "otro"}), "\"otro\""),
        (
            serde_json::json!({"analysis_kind": null}),
            "tipo de análisis",
        ),
        (serde_json::json!({"parts": [{"title": " "}]}), "sin título"),
        (
            serde_json::json!({"quantities": [{"symbol": "x", "name": "x", "unit": "", "repeated": true, "per_determination": true}]}),
            "parte con determinaciones",
        ),
        (
            serde_json::json!({"analysis_kind": "curva", "parts": [{"title": "A", "determinations": {"count": 2, "label": "M"}}]}),
            "parte estadística",
        ),
        (
            serde_json::json!({"quantities": [{"symbol": "x", "name": "x", "unit": "", "repeated": false, "fixed": true, "default_value": 1.0}]}),
            "La magnitud \"x\": Solo un dato de cátedra",
        ),
        (
            serde_json::json!({"quantities": [{"symbol": "x", "name": "x", "unit": "", "repeated": false, "is_given": true, "default_u": -1.0}]}),
            "cero o positiva",
        ),
        (
            serde_json::json!({
                "parts": [{"title": "A", "determinations": {"count": 2, "label": "M"}}, {"title": "B"}],
                "quantities": [{"symbol": "x", "name": "x", "unit": "", "repeated": true, "per_determination": true}],
                "sections": [{"title": "S", "part": 1, "quantities": ["x"]}]
            }),
            "no está en una práctica o parte con determinaciones",
        ),
        (
            serde_json::json!({"parts": [{"title": "A", "analysis_kind": "curva", "determinations": {"count": 2, "label": "M"}}]}),
            "parte estadística",
        ),
        (
            serde_json::json!({"parts": [], "analysis_kind": "regresion_lineal", "determinations": {"count": 2, "label": "M"}}),
            "parte estadística",
        ),
        (
            serde_json::json!({"parts": [{"title": "A", "determinations": {"count": 1, "label": "M"}}]}),
            "de 2 a",
        ),
        (
            serde_json::json!({"parts": [{"title": "A", "key": "-"}]}),
            "clave de parte",
        ),
        (
            serde_json::json!({"parts": [{"title": "A", "key": "a-"}]}),
            "clave de parte",
        ),
        (
            serde_json::json!({"parts": [
                {"title": "A", "analysis_kind": "curva"},
                {"title": "B", "analysis_kind": "regresion_lineal"}
            ]}),
            "Solo una parte",
        ),
        (
            serde_json::json!({"results": [
                {"symbol": "y", "name": "y", "unit": "", "formula": "x", "tolerance": -1}
            ]}),
            "negativa",
        ),
        (
            serde_json::json!({"sections": [{"title": "S", "part": 3}]}),
            "parte que no existe",
        ),
        (
            serde_json::json!({"sections": [{"title": "S", "quantities": ["z"]}]}),
            "\"z\"",
        ),
        (
            serde_json::json!({"sections": [
                {"title": "S", "quantities": ["x"]}, {"title": "T", "quantities": ["x"]}
            ]}),
            "más de una sección",
        ),
        (
            serde_json::json!({"aggregates": [{"symbol": "x", "name": "a", "unit": "", "formula": "1"}]}),
            "\"x\" aparece",
        ),
        (
            serde_json::json!({"curves": [{"x_formula": "x", "y_formula": " "}]}),
            "ambos ejes",
        ),
    ] {
        let error = check_template(&template(extra.clone())).unwrap_err();
        assert!(error.contains(expected), "{extra}: {error}");
    }
}

#[tokio::test]
async fn import_mirrors_the_part_kinds_like_the_editor() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let template: PracticeTemplate = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "name": "Partes", "analysis_kind": "curva",
        "parts": [{"title": "A", "analysis_kind": "regresion_lineal"}, {"title": "B"}]
    }))
    .unwrap();
    let practice = import_template(&pool, &template, "u1").await.unwrap();
    let def = definition(&pool, &practice.id).await.unwrap().unwrap();
    assert_eq!(def.analysis_kind.as_deref(), Some("regresion_lineal"));
    let kinds: Vec<_> = def
        .parts
        .iter()
        .map(|p| p.analysis_kind.as_deref())
        .collect();
    assert_eq!(kinds, [Some("regresion_lineal"), Some("estadistico")]);
}

#[tokio::test]
async fn export_picks_the_draft_or_the_published_version() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let info = PracticeInfoInput {
        name: "Corriente continua".into(),
        description: "Descripción sin publicar".into(),
    };
    update_practice(&pool, "p2-cc", &info).await.unwrap();
    let draft = export_template(&pool, "p2-cc", ExportSource::Borrador)
        .await
        .unwrap()
        .unwrap();
    let published = export_template(&pool, "p2-cc", ExportSource::Publicada)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(draft.description, "Descripción sin publicar");
    assert_ne!(published.description, draft.description);
    let copy = import_template(&pool, &published, "u1").await.unwrap();
    assert!(export_template(&pool, &copy.id, ExportSource::Publicada)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn the_library_loads_and_exports_unchanged() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/practices");
    let update = std::env::var("QUANTIFY_UPDATE_TEMPLATES").is_ok_and(|v| v == "1");
    let mut seeded: Vec<String> = db::practices(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    seeded.sort();
    let mut keys: Vec<String> = LIBRARY.iter().map(|(k, _)| k.to_string()).collect();
    keys.sort();
    assert_eq!(
        keys, seeded,
        "cada práctica de una base nueva tiene que venir de LIBRARY"
    );
    let mut stale = Vec::new();
    for (key, json) in LIBRARY {
        let exported = export_template(&pool, key, ExportSource::Borrador)
            .await
            .unwrap()
            .unwrap();
        if update {
            let pretty = serde_json::to_string_pretty(&exported).unwrap() + "\n";
            std::fs::write(dir.join(format!("{key}.json")), pretty).unwrap();
            continue;
        }
        let file: serde_json::Value = serde_json::from_str(json).unwrap();
        if file != serde_json::to_value(&exported).unwrap() {
            stale.push(key);
        }
        let template = library_template(key).unwrap().unwrap();
        import_template(&pool, &template, "u1")
            .await
            .unwrap_or_else(|e| panic!("{key}: no se puede importar: {e:#}"));
    }
    assert!(
        stale.is_empty(),
        "estas plantillas cambian al importarlas y exportarlas (normalizalas con QUANTIFY_UPDATE_TEMPLATES=1 cargo test library): {stale:?}"
    );
}

#[tokio::test]
async fn part_keys_that_collide_across_practices_fall_back_to_a_fresh_id() {
    let (pool, _dir) = setup().await;
    let template = |name: &str, key: &str| -> PracticeTemplate {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "name": name, "analysis_kind": "estadistico",
            "parts": [{"key": key, "title": "Parte"}]
        }))
        .unwrap()
    };
    let first = import_template(&pool, &template("A B", "c"), "u1")
        .await
        .unwrap();
    let second = import_template(&pool, &template("A", "b-c"), "u1")
        .await
        .unwrap();
    assert_eq!((first.id.as_str(), second.id.as_str()), ("a-b", "a"));
    let first_parts = definition(&pool, "a-b").await.unwrap().unwrap().parts;
    let second_parts = definition(&pool, "a").await.unwrap().unwrap().parts;
    assert_eq!(first_parts[0].id, "a-b-c");
    assert_ne!(second_parts[0].id, "a-b-c");
}

#[tokio::test]
async fn a_part_with_determinations_is_analyzed_and_round_trips() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let template: PracticeTemplate = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "name": "Con medidas", "analysis_kind": "estadistico",
        "parts": [{"title": "A", "determinations": {"count": 2, "label": "Medida", "average": true}}],
        "sections": [{"title": "S", "part": 0, "quantities": ["T"], "results": ["f"]}],
        "quantities": [{"symbol": "T", "name": "T", "unit": "s", "repeated": true,
                        "per_determination": true}],
        "results": [{"symbol": "f", "name": "f", "unit": "1/s", "formula": "1/T"}]
    }))
    .unwrap();
    let practice = import_template(&pool, &template, "u1").await.unwrap();
    let exported = export_template(&pool, &practice.id, ExportSource::Borrador)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(exported.parts[0].determinations.as_ref().unwrap().count, 2);
    assert!(exported.quantities[0].per_determination);
    let def = definition(&pool, &practice.id).await.unwrap().unwrap();
    let measurements = vec![crate::computation::MeasurementInput {
        quantity_id: def.quantities[0].id.clone(),
        instrument_id: None,
        scale_id: None,
        values: vec![],
        given_u: None,
        point_replicas: None,
        operator_replicas: Some(vec![vec![2.0, 2.0], vec![4.0, 4.0]]),
        operator_instruments: None,
    }];
    let analysis = crate::computation::analyze(&pool, &practice.id, &measurements)
        .await
        .unwrap();
    let part = &analysis.parts[0].analysis;
    assert_eq!(part.determinations.len(), 2);
    assert!((part.determinations[1].derived[0].value - 0.25).abs() < 1e-12);
    let average = part.derived.iter().find(|d| d.symbol == "f_medio").unwrap();
    assert!((average.value - 0.375).abs() < 1e-12);
}

#[test]
fn resolved_offers_input_units_per_quantity_and_final_result() {
    let quantity = |symbol: &str, unit: &str, slots: Option<i64>| crate::db::PracticeQuantity {
        symbol: symbol.into(),
        unit: unit.into(),
        slot_count: slots,
        ..Default::default()
    };
    let result = |symbol: &str, unit: &str, is_final: bool| crate::db::PracticeResult {
        symbol: symbol.into(),
        unit: unit.into(),
        formula: "1".into(),
        is_final,
        ..Default::default()
    };
    let def = PracticeDefinition {
        quantities: vec![
            quantity("L", "cm", None),
            quantity("T", "K", None),
            quantity("d", "cm", Some(9)),
        ],
        results: vec![
            result("dT", "K", true),
            result("A", "m2", true),
            result("x", "cm", false),
        ],
        ..Default::default()
    };
    let units = def.resolved().input_units;
    let labels = |s: &str| -> Vec<String> { units[s].iter().map(|u| u.label.clone()).collect() };
    assert!(labels("L").contains(&"mm".to_string()) && labels("L").contains(&"m".to_string()));
    assert!(
        labels("T").contains(&"°C".to_string()),
        "una temperatura cargada ofrece °C"
    );
    assert_eq!(labels("d"), ["cm"], "una ranura no se convierte");
    assert!(
        !labels("dT").contains(&"°C".to_string()),
        "un resultado no ofrece otro desplazamiento"
    );
    assert!(labels("A").contains(&"cm2".to_string()));
    assert!(
        !units.contains_key("x"),
        "un resultado no final no se informa"
    );
}

#[tokio::test]
async fn a_per_determination_quantity_only_accepts_groups() {
    let (pool, _dir) = crate::computation::tests::setup().await;
    let template: PracticeTemplate = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "name": "Con medidas", "analysis_kind": "estadistico",
        "determinations": {"count": 2, "label": "Medida", "average": true},
        "quantities": [{"symbol": "T", "name": "T", "unit": "s", "repeated": true,
                        "per_determination": true}],
        "results": [{"symbol": "f", "name": "f", "unit": "1/s", "formula": "1/T"}]
    }))
    .unwrap();
    let practice = import_template(&pool, &template, "u1").await.unwrap();
    let def = definition(&pool, &practice.id).await.unwrap().unwrap();
    let analyze =
        |values: Vec<f64>, points: Option<Vec<Vec<f64>>>, groups: Option<Vec<Vec<f64>>>| {
            let measurements = vec![crate::computation::MeasurementInput {
                quantity_id: def.quantities[0].id.clone(),
                instrument_id: None,
                scale_id: None,
                values,
                given_u: None,
                point_replicas: points,
                operator_replicas: groups,
                operator_instruments: None,
            }];
            let pool = pool.clone();
            let id = practice.id.clone();
            async move { crate::computation::analyze(&pool, &id, &measurements).await }
        };
    assert!(analyze(vec![], None, Some(vec![vec![2.0], vec![4.0]]))
        .await
        .is_ok());
    let twice = crate::computation::MeasurementInput {
        quantity_id: def.quantities[0].id.clone(),
        instrument_id: None,
        scale_id: None,
        values: vec![],
        given_u: None,
        point_replicas: None,
        operator_replicas: Some(vec![vec![2.0], vec![4.0]]),
        operator_instruments: None,
    };
    let repeated = crate::computation::analyze(&pool, &practice.id, &[twice.clone(), twice]).await;
    assert!(repeated.unwrap_err().to_string().contains("más de una vez"));
    for (values, points, groups) in [
        (vec![2.0], None, None),
        (vec![], Some(vec![vec![2.0], vec![4.0]]), None),
        (vec![], None, Some(vec![vec![2.0], vec![4.0], vec![5.0]])),
    ] {
        assert!(analyze(values, points, groups).await.is_err());
    }
}

#[test]
fn only_count_and_average_make_determinations_structural() {
    let with = |d: Determinations| PracticeDefinition {
        determinations: Some(d),
        ..Default::default()
    };
    let base = Determinations {
        count: 3,
        label: "Medida".into(),
        average: true,
        display: DeterminationDisplay::Bloques,
    };
    let kind = |d: Determinations| compare_definitions(&with(base.clone()), &with(d)).unwrap();
    let looks = kind(Determinations {
        label: "Serie".into(),
        display: DeterminationDisplay::Pestanas,
        ..base.clone()
    });
    assert_eq!(looks.kind, DraftChangeKind::Cosmetic);
    assert!(looks.changes[0].contains("Cambia cómo se muestran"));
    assert_eq!(
        kind(Determinations {
            count: 4,
            ..base.clone()
        })
        .kind,
        DraftChangeKind::Structural
    );
    let added = compare_definitions(&PracticeDefinition::default(), &with(base.clone())).unwrap();
    assert_eq!(added.kind, DraftChangeKind::Structural);
    assert_eq!(added.changes, ["Cambian las determinaciones"]);
    assert_eq!(
        kind(Determinations {
            average: false,
            ..base.clone()
        })
        .kind,
        DraftChangeKind::Structural
    );
}

#[test]
fn a_symbol_named_medio_belongs_to_its_base_only_when_it_is_an_undefined_average() {
    use crate::practices::{
        Determinations, PracticeAggregate, PracticeDefinition, PracticePart, PracticeSection,
    };
    let result = |symbol: &str, formula: &str| crate::db::PracticeResult {
        symbol: symbol.into(),
        formula: formula.into(),
        ..Default::default()
    };
    let def = |aggregate: bool, average: bool| PracticeDefinition {
        parts: vec![
            PracticePart {
                id: "stat".into(),
                determinations: Some(Determinations {
                    count: 2,
                    label: "Medida".into(),
                    average,
                    display: Default::default(),
                }),
                ..Default::default()
            },
            PracticePart {
                id: "reg".into(),
                ..Default::default()
            },
        ],
        sections: vec![PracticeSection {
            part_id: Some("stat".into()),
            quantities: vec!["T".into()],
            results: vec!["x".into()],
            ..Default::default()
        }],
        quantities: vec![crate::db::PracticeQuantity {
            symbol: "T".into(),
            per_determination: true,
            ..Default::default()
        }],
        results: vec![result("x", "1/T"), result("k", "2*x_medio")],
        aggregates: if aggregate {
            vec![PracticeAggregate {
                symbol: "x_medio".into(),
                ..Default::default()
            }]
        } else {
            vec![]
        },
        ..Default::default()
    };
    assert_eq!(def(false, true).result_part("k"), Some("stat"));
    assert_eq!(def(false, false).result_part("k"), None);
    assert_eq!(def(true, true).result_part("k"), None);
    let resolved = def(false, true).resolved();
    assert_eq!(
        resolved.symbol_parts.get("k").map(String::as_str),
        Some("stat")
    );
    assert_eq!(resolved.determinations["x"].scope, "stat");
    assert_eq!(resolved.determinations["T"].scope, "stat");
    assert!(!resolved.determinations.contains_key("k"));
}

#[tokio::test]
async fn seeded_magnitudes_are_already_canonical() {
    let (pool, _dir) = setup().await;
    let used: Vec<(String,)> = sqlx::query_as(
        "SELECT quantity FROM practice_quantities WHERE quantity IS NOT NULL \
         UNION SELECT quantity FROM instruments",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(used.iter().any(|(q,)| q == "voltaje"));
    for (q,) in used {
        assert_eq!(crate::magnitudes::canonical(&q), q);
    }
}

#[tokio::test]
async fn a_saved_magnitude_is_stored_canonical() {
    let (pool, _dir) = setup().await;
    let q = create_quantity(
        &pool,
        "p1-estadistica",
        QuantityInput {
            quantity: Some("Diferencia de Potencial".into()),
            ..sample_quantity()
        },
    )
    .await
    .unwrap();
    assert_eq!(q.quantity.as_deref(), Some("voltaje"));
}

#[tokio::test]
async fn a_fixed_given_uses_the_definition_and_ignores_the_client() {
    let (pool, _dir) = setup().await;
    let l = definition(&pool, "p1-estadistica")
        .await
        .unwrap()
        .unwrap()
        .quantities
        .into_iter()
        .find(|q| q.symbol == "L")
        .unwrap();
    let saved = update_quantity(
        &pool,
        &l.id,
        QuantityInput {
            symbol: "L".into(),
            name: l.name.clone(),
            unit: l.unit.clone(),
            quantity: l.quantity.clone(),
            is_given: true,
            per_point: false,
            fixed: true,
            default_value: Some(Some(1.0)),
            default_u: Some(Some(0.002)),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(saved.fixed);
    assert_eq!(
        (saved.default_value, saved.default_u),
        (Some(1.0), Some(0.002))
    );

    let def = definition(&pool, "p1-estadistica").await.unwrap().unwrap();
    let sent = crate::computation::MeasurementInput {
        quantity_id: l.id.clone(),
        values: vec![5.0],
        given_u: Some(99.0),
        ..Default::default()
    };
    let mut unset = def.quantities.clone();
    unset.iter_mut().for_each(|q| q.default_value = None);
    let kept = crate::computation::with_fixed_values(&unset, std::slice::from_ref(&sent));
    assert_eq!(kept[0].values, vec![5.0]);
    for client in [vec![sent], vec![]] {
        let used = crate::computation::with_fixed_values(&def.quantities, &client);
        assert_eq!(used.len(), 1);
        assert_eq!(
            (used[0].values.clone(), used[0].given_u),
            (vec![1.0], Some(0.002))
        );
    }

    let exported = export_template(&pool, "p1-estadistica", ExportSource::Borrador)
        .await
        .unwrap()
        .unwrap();
    let copy = import_template(&pool, &exported, "u1").await.unwrap();
    let copied = definition(&pool, &copy.id).await.unwrap().unwrap();
    let l = copied.quantities.iter().find(|q| q.symbol == "L").unwrap();
    assert!(l.fixed);
    assert_eq!(l.default_u, Some(0.002));
}
