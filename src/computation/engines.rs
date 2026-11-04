use super::formula::{compile_formula, eval_compiled, CONSTANTS};
use super::{
    AggregateComputation, CurveSpec, DerivedComputation, FormAnalysis, MeasurementInput,
    OperatorComputation, PartAnalysis, PointResultComputation, QuantityComputation,
    RegressionResult, ScatterResult,
};
use crate::analysis;
use crate::db::{InstrumentScale, PracticeQuantity, PracticeResult};
use crate::practices::{PracticeAggregate, PracticeIntermediate, PracticePointResult};
use crate::uncertainty::{self, BModel, ScaleSpec};
use evalexpr::Node;
use std::collections::HashMap;

/// Convierte una escala del catálogo ([`InstrumentScale`]) en la especificación que entiende
///
/// ```
/// use quantify::db::InstrumentScale;
/// use quantify::uncertainty::BModel;
/// let escala = InstrumentScale {
///     id: "s1".into(),
///     instrument_id: "i1".into(),
///     label: "200 mm".into(),
///     full_scale: Some(200.0),
///     step: 0.01,
///     appreciation: None,
///     internal_res: None,
///     internal_res_u: None,
///     b_model: "resolucion".into(),
///     spec_pct_reading: None,
///     spec_step_coeff: None,
///     spec_fixed: None,
///     u_cal_pct: 0.0,
///     u_cal_fixed: 0.0,
///     unit: "mm".into(),
///     position: 0,
/// };
/// let spec = quantify::computation::scale_spec(&escala).unwrap();
/// assert!(matches!(spec.b_model, BModel::Resolucion));
/// assert_eq!(spec.step, 0.01);
/// ```
pub fn scale_spec(scale: &InstrumentScale) -> anyhow::Result<ScaleSpec> {
    let b_model = match scale.b_model.as_str() {
        "resolucion" => BModel::Resolucion,
        "apreciacion" => BModel::Apreciacion,
        "fabricante" => BModel::Fabricante,
        other => {
            anyhow::bail!("la escala tiene un modelo de incertidumbre desconocido: {other}")
        }
    };
    Ok(ScaleSpec {
        b_model,
        step: scale.step,
        appreciation: scale.appreciation,
        spec_pct_reading: scale.spec_pct_reading.unwrap_or(0.0),
        spec_step_coeff: scale.spec_step_coeff.unwrap_or(0.0),
        spec_fixed: scale.spec_fixed.unwrap_or(0.0),
        u_cal_pct: scale.u_cal_pct,
        u_cal_fixed: scale.u_cal_fixed,
    })
}

fn compute_quantities(
    quantities: &[&PracticeQuantity],
    by_quantity: &HashMap<&str, &MeasurementInput>,
    scales: &HashMap<String, InstrumentScale>,
    operator: Option<usize>,
    means: &mut HashMap<String, f64>,
    us: &mut HashMap<String, f64>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<QuantityComputation>> {
    let mut computed = Vec::with_capacity(quantities.len());
    for &quantity in quantities {
        let measurement = by_quantity.get(quantity.id.as_str());
        let values: Vec<f64> = match operator {
            Some(i) => measurement
                .and_then(|m| m.operator_replicas.as_ref())
                .and_then(|ops| ops.get(i))
                .cloned()
                .unwrap_or_default(),
            None => measurement.map(|m| m.values.clone()).unwrap_or_default(),
        };

        let result = if quantity.is_given {
            let value = values.first().copied().unwrap_or(f64::NAN);
            let u_exp = if quantity.has_uncertainty {
                measurement.and_then(|m| m.given_u).unwrap_or(0.0)
            } else {
                0.0
            };
            if value.is_nan() {
                warnings.push(format!(
                    "El dato \"{}\" ({}) no tiene valor cargado.",
                    quantity.name, quantity.symbol
                ));
            }
            uncertainty::measured_given(value, u_exp)
        } else {
            if values.is_empty() {
                warnings.push(format!(
                    "La magnitud \"{}\" ({}) no tiene lecturas cargadas.",
                    quantity.name, quantity.symbol
                ));
            }
            let spec = match measurement.and_then(|m| m.scale_id.as_deref()) {
                Some(scale_id) => match scales.get(scale_id) {
                    Some(scale) => Some(scale_spec(scale)?),
                    None => anyhow::bail!("la escala seleccionada no existe"),
                },
                None => None,
            };
            uncertainty::measured_quantity(&values, spec.as_ref())
        };

        means.insert(quantity.symbol.clone(), result.mean);
        us.insert(quantity.symbol.clone(), result.u_c);
        computed.push(QuantityComputation {
            quantity_id: quantity.id.clone(),
            symbol: quantity.symbol.clone(),
            name: quantity.name.clone(),
            unit: quantity.unit.clone(),
            instrument_id: measurement.and_then(|m| m.instrument_id.clone()),
            scale_id: measurement.and_then(|m| m.scale_id.clone()),
            values,
            result,
        });
    }
    Ok(computed)
}

/// Análisis estadístico: media e incertidumbre de cada magnitud y propagación a los mensurandos.
///
/// ```
/// # use std::collections::HashMap;
/// # use quantify::computation::{compute, MeasurementInput};
/// # use quantify::db::{PracticeQuantity, PracticeResult};
/// # let q = |symbol: &str, is_given: bool| PracticeQuantity {
/// #     id: symbol.into(), practice_id: "p".into(), symbol: symbol.into(), name: symbol.into(),
/// #     unit: String::new(), repeated: !is_given, quantity: None, position: 0, is_given,
/// #     replicas_per_point: None, per_point: false, has_uncertainty: true, optional: false,
/// #     default_value: None, slot_count: None,
/// # };
/// # let m = |id: &str, values: Vec<f64>, given_u: Option<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u,
/// #     point_replicas: None, operator_replicas: None,
/// # };
/// let g = PracticeResult {
///     id: "g".into(), practice_id: "p".into(), symbol: "g".into(), name: "g".into(),
///     unit: "m/s2".into(), formula: "4*pi^2*L/T^2".into(), position: 0, tolerance: None,
///     is_final: false, has_uncertainty: true,
/// };
/// let analysis = compute(
///     &[q("L", true), q("T", false)],
///     &[g],
///     &HashMap::new(),
///     &[m("L", vec![1.0], Some(0.002)), m("T", vec![2.0, 2.0], None)],
///     None,
/// )
/// .unwrap();
/// assert!((analysis.derived[0].value - std::f64::consts::PI.powi(2)).abs() < 1e-9);
/// ```
pub fn compute(
    quantities: &[PracticeQuantity],
    results: &[PracticeResult],
    scales: &HashMap<String, InstrumentScale>,
    measurements: &[MeasurementInput],
    operator_count: Option<i64>,
) -> anyhow::Result<FormAnalysis> {
    let mut warnings = Vec::new();
    let by_quantity: HashMap<&str, &MeasurementInput> = measurements
        .iter()
        .map(|m| (m.quantity_id.as_str(), m))
        .collect();
    let symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    let operator_count = operator_count.unwrap_or(0);

    if operator_count <= 1 {
        let all: Vec<&PracticeQuantity> = quantities.iter().collect();
        let mut means = HashMap::new();
        let mut us = HashMap::new();
        let computed = compute_quantities(
            &all,
            &by_quantity,
            scales,
            None,
            &mut means,
            &mut us,
            &mut warnings,
        )?;
        let derived = derive_results(results, &symbols, &means, &us, &mut warnings)?;
        return Ok(FormAnalysis {
            quantities: computed,
            regression: None,
            scatters: Vec::new(),
            derived,
            operators: Vec::new(),
            point_results: Vec::new(),
            aggregates: Vec::new(),
            warnings,
            parts: Vec::new(),
        });
    }

    let shared: Vec<&PracticeQuantity> = quantities
        .iter()
        .filter(|q| q.is_given || !q.repeated)
        .collect();
    let per_operator: Vec<&PracticeQuantity> = quantities
        .iter()
        .filter(|q| q.repeated && !q.is_given)
        .collect();

    let mut shared_means = HashMap::new();
    let mut shared_us = HashMap::new();
    let shared_computed = compute_quantities(
        &shared,
        &by_quantity,
        scales,
        None,
        &mut shared_means,
        &mut shared_us,
        &mut warnings,
    )?;

    let mut operators = Vec::with_capacity(operator_count as usize);
    for i in 0..operator_count as usize {
        let mut means = shared_means.clone();
        let mut us = shared_us.clone();
        let op_quantities = compute_quantities(
            &per_operator,
            &by_quantity,
            scales,
            Some(i),
            &mut means,
            &mut us,
            &mut warnings,
        )?;
        let derived = derive_results(results, &symbols, &means, &us, &mut warnings)?;
        operators.push(OperatorComputation {
            label: format!("Operador {}", i + 1),
            quantities: op_quantities,
            derived,
        });
    }

    Ok(FormAnalysis {
        quantities: shared_computed,
        regression: None,
        scatters: Vec::new(),
        derived: Vec::new(),
        operators,
        point_results: Vec::new(),
        aggregates: Vec::new(),
        warnings,
        parts: Vec::new(),
    })
}

type PointSeries = Vec<(f64, f64)>;

pub type PointContext = HashMap<String, f64>;

fn per_point_quantity_symbols(quantities: &[PracticeQuantity]) -> impl Iterator<Item = &str> {
    quantities
        .iter()
        .filter(|q| q.per_point && !q.is_given)
        .map(|q| q.symbol.as_str())
}

pub(crate) fn build_points(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    x_formula: &str,
    y_formula: &str,
    measurements: &[MeasurementInput],
    too_few_msg: &str,
) -> anyhow::Result<(PointSeries, Vec<String>, Vec<PointContext>)> {
    let mut warnings = Vec::new();
    let magnitude_symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();

    let given_ids: std::collections::HashSet<&str> = quantities
        .iter()
        .filter(|q| q.is_given)
        .map(|q| q.id.as_str())
        .collect();
    let shared_ids: std::collections::HashSet<&str> = quantities
        .iter()
        .filter(|q| !q.per_point || q.is_given)
        .map(|q| q.id.as_str())
        .collect();
    let shared_repr = |m: &MeasurementInput| -> f64 {
        if given_ids.contains(m.quantity_id.as_str()) {
            m.values.first().copied().unwrap_or(f64::NAN)
        } else {
            let xs = m.point_values();
            if xs.is_empty() {
                f64::NAN
            } else {
                xs.iter().sum::<f64>() / xs.len() as f64
            }
        }
    };

    let point_values: HashMap<&str, Vec<f64>> = measurements
        .iter()
        .map(|m| {
            let vals = if shared_ids.contains(m.quantity_id.as_str()) {
                vec![shared_repr(m)]
            } else {
                m.point_values()
            };
            (m.quantity_id.as_str(), vals)
        })
        .collect();
    let point_matrix: HashMap<&str, Vec<Vec<f64>>> = measurements
        .iter()
        .map(|m| {
            let mat = if shared_ids.contains(m.quantity_id.as_str()) {
                vec![vec![shared_repr(m)]]
            } else {
                m.point_replica_matrix()
            };
            (m.quantity_id.as_str(), mat)
        })
        .collect();
    let broadcastable: std::collections::HashSet<&str> = measurements
        .iter()
        .filter(|m| m.point_replicas.is_none() || shared_ids.contains(m.quantity_id.as_str()))
        .map(|m| m.quantity_id.as_str())
        .collect();
    let symbol_to_id: HashMap<&str, &str> = quantities
        .iter()
        .map(|q| (q.symbol.as_str(), q.id.as_str()))
        .collect();
    let per_point_syms: std::collections::HashSet<&str> =
        per_point_quantity_symbols(quantities).collect();

    let mut allowed = magnitude_symbols.clone();
    let mut compiled_intermediates: Vec<(&PracticeIntermediate, Node)> = Vec::new();
    for it in intermediates {
        let tree = compile_formula(&it.formula, &allowed)?;
        allowed.push(it.symbol.clone());
        compiled_intermediates.push((it, tree));
    }
    let axis_symbols = allowed;
    let x_tree = compile_formula(x_formula, &axis_symbols)?;
    let y_tree = compile_formula(y_formula, &axis_symbols)?;
    let intermediate_symbols: std::collections::HashSet<&str> = compiled_intermediates
        .iter()
        .map(|(it, _)| it.symbol.as_str())
        .collect();

    let axis_refs: std::collections::HashSet<&str> = x_tree
        .iter_variable_identifiers()
        .chain(y_tree.iter_variable_identifiers())
        .collect();
    let mut needed: std::collections::HashSet<&str> = compiled_intermediates
        .iter()
        .filter(|(it, _)| axis_refs.contains(it.symbol.as_str()))
        .map(|(it, _)| it.symbol.as_str())
        .collect();
    for (it, tree) in compiled_intermediates.iter().rev() {
        if needed.contains(it.symbol.as_str()) {
            for v in tree.iter_variable_identifiers() {
                if intermediate_symbols.contains(v) {
                    needed.insert(v);
                }
            }
        }
    }

    let mut conditioning: std::collections::HashSet<&str> = axis_refs
        .iter()
        .copied()
        .filter(|s| per_point_syms.contains(s))
        .collect();
    for (it, tree) in &compiled_intermediates {
        if needed.contains(it.symbol.as_str()) {
            for v in tree.iter_variable_identifiers() {
                if per_point_syms.contains(v) {
                    conditioning.insert(v);
                }
            }
        }
    }

    let lengths: Vec<usize> = conditioning
        .iter()
        .filter_map(|sym| symbol_to_id.get(sym))
        .map(|id| point_values.get(id).map_or(0, |v| v.len()))
        .collect();
    let n_points = lengths.iter().copied().min().unwrap_or(0);
    if lengths.iter().any(|&l| l != n_points) {
        warnings.push(
            "Las magnitudes tienen distinta cantidad de puntos; se usa la menor cantidad comun."
                .into(),
        );
    }
    if n_points < 2 {
        anyhow::bail!("{too_few_msg}");
    }

    let magnitude_at = |id: &str, i: usize| -> f64 {
        point_values
            .get(id)
            .and_then(|v| v.get(i).or_else(|| v.last()))
            .copied()
            .unwrap_or(f64::NAN)
    };

    let mut points = Vec::with_capacity(n_points);
    let mut contexts = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let mut context: PointContext = quantities
            .iter()
            .map(|q| (q.symbol.clone(), magnitude_at(q.id.as_str(), i)))
            .collect();
        let mut intermediate_values: HashMap<&str, f64> = HashMap::new();
        for (it, tree) in &compiled_intermediates {
            let value = point_intermediate(
                tree,
                &point_matrix,
                &symbol_to_id,
                &broadcastable,
                &intermediate_values,
                i,
            );
            intermediate_values.insert(it.symbol.as_str(), value);
            context.insert(it.symbol.clone(), value);
        }

        let bound: HashMap<&str, f64> = context.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let x = eval_compiled(&x_tree, &bound);
        let y = eval_compiled(&y_tree, &bound);
        if !x.is_finite() || !y.is_finite() {
            anyhow::bail!(
                "un punto produjo un valor no finito al evaluar los ejes (revisa las formulas y las lecturas)"
            );
        }
        points.push((x, y));
        contexts.push(context);
    }
    Ok((points, warnings, contexts))
}

fn point_intermediate(
    tree: &Node,
    point_matrix: &HashMap<&str, Vec<Vec<f64>>>,
    symbol_to_id: &HashMap<&str, &str>,
    broadcastable: &std::collections::HashSet<&str>,
    earlier: &HashMap<&str, f64>,
    i: usize,
) -> f64 {
    let id_of = |sym: &str| symbol_to_id.get(sym).copied();
    let reps_at = |sym: &str| -> &[f64] {
        id_of(sym)
            .and_then(|id| point_matrix.get(id))
            .and_then(|m| m.get(i).or_else(|| m.last()))
            .map_or(&[][..], |v| v.as_slice())
    };
    let is_broadcastable = |sym: &str| id_of(sym).is_some_and(|id| broadcastable.contains(id));
    let n_reps = tree
        .iter_variable_identifiers()
        .filter(|v| symbol_to_id.contains_key(v) && !is_broadcastable(v))
        .map(|v| reps_at(v).len())
        .max()
        .unwrap_or(0)
        .max(1);
    let mut sum = 0.0;
    for r in 0..n_reps {
        let bound: HashMap<&str, f64> = tree
            .iter_variable_identifiers()
            .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v))
            .map(|v| {
                if symbol_to_id.contains_key(v) {
                    let reps = reps_at(v);
                    let value = if is_broadcastable(v) {
                        reps.first().copied().unwrap_or(f64::NAN)
                    } else {
                        reps.get(r).copied().unwrap_or(f64::NAN)
                    };
                    (v, value)
                } else {
                    (v, earlier.get(v).copied().unwrap_or(f64::NAN))
                }
            })
            .collect();
        sum += eval_compiled(tree, &bound);
    }
    sum / n_reps as f64
}

/// Regresión lineal sobre los puntos que dan las fórmulas de eje, con mensurandos y agregados.
///
/// ```
/// # use std::collections::HashMap;
/// # use quantify::computation::{compute_regresion, MeasurementInput};
/// # use quantify::db::PracticeQuantity;
/// # let q = |symbol: &str| PracticeQuantity {
/// #     id: symbol.into(), practice_id: "p".into(), symbol: symbol.into(), name: symbol.into(),
/// #     unit: String::new(), repeated: false, quantity: None, position: 0, is_given: false,
/// #     replicas_per_point: None, per_point: true, has_uncertainty: true, optional: false,
/// #     default_value: None, slot_count: None,
/// # };
/// # let m = |id: &str, values: Vec<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u: None,
/// #     point_replicas: None, operator_replicas: None,
/// # };
/// let analysis = compute_regresion(
///     &[q("x"), q("y")], &[], &[], &[], &[], &HashMap::new(), "x", "y",
///     &[m("x", vec![1.0, 2.0, 3.0]), m("y", vec![2.0, 4.0, 6.0])],
/// )
/// .unwrap();
/// assert!((analysis.regression.unwrap().slope - 2.0).abs() < 1e-12);
/// ```
#[allow(clippy::too_many_arguments)]
pub fn compute_regresion(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    results: &[PracticeResult],
    point_results: &[PracticePointResult],
    aggregates: &[PracticeAggregate],
    scales: &HashMap<String, InstrumentScale>,
    x_formula: &str,
    y_formula: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let (points, mut warnings, contexts) = build_points(
        quantities,
        intermediates,
        x_formula,
        y_formula,
        measurements,
        "se necesitan al menos 2 puntos para el ajuste lineal",
    )?;

    let fit = analysis::linear_regression("x", "y", &points)
        .ok_or_else(|| anyhow::anyhow!("no se pudo ajustar la recta (¿todos los x iguales?)"))?;

    let by_quantity: HashMap<&str, &MeasurementInput> = measurements
        .iter()
        .map(|m| (m.quantity_id.as_str(), m))
        .collect();
    let shared: Vec<&PracticeQuantity> = quantities
        .iter()
        .filter(|q| !q.per_point || q.is_given)
        .collect();
    let mut means: HashMap<String, f64> = HashMap::new();
    let mut us: HashMap<String, f64> = HashMap::new();
    compute_quantities(
        &shared,
        &by_quantity,
        scales,
        None,
        &mut means,
        &mut us,
        &mut warnings,
    )?;
    means.insert("slope".into(), fit.slope);
    means.insert("intercept".into(), fit.intercept);
    us.insert("slope".into(), fit.u_slope);
    us.insert("intercept".into(), fit.u_intercept);
    let mut allowed: Vec<String> = shared.iter().map(|q| q.symbol.clone()).collect();
    allowed.push("slope".into());
    allowed.push("intercept".into());
    let derived = derive_results(results, &allowed, &means, &us, &mut warnings)?;

    let mut extras: HashMap<&str, f64> = HashMap::new();
    extras.insert("slope", fit.slope);
    extras.insert("intercept", fit.intercept);
    for d in &derived {
        extras.insert(d.symbol.as_str(), d.value);
    }
    let mut pr_allowed: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    pr_allowed.extend(intermediates.iter().map(|it| it.symbol.clone()));
    pr_allowed.extend(derived.iter().map(|d| d.symbol.clone()));
    pr_allowed.push("slope".into());
    pr_allowed.push("intercept".into());
    let mut point_results_out = Vec::with_capacity(point_results.len());
    for pr in point_results {
        let tree = match compile_formula(&pr.formula, &pr_allowed) {
            Ok(tree) => tree,
            Err(e) => {
                warnings.push(skipped("la derivada por punto", &pr.symbol, &e));
                continue;
            }
        };
        let values: Vec<f64> = contexts
            .iter()
            .map(|ctx| {
                let mut bound: HashMap<&str, f64> =
                    ctx.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                bound.extend(&extras);
                eval_compiled(&tree, &bound)
            })
            .collect();
        point_results_out.push(PointResultComputation {
            symbol: pr.symbol.clone(),
            name: pr.name.clone(),
            unit: pr.unit.clone(),
            values,
        });
    }

    let mut agg_values: HashMap<String, f64> = means.clone();
    for d in &derived {
        agg_values.insert(d.symbol.clone(), d.value);
    }
    let n_points = contexts.len();
    let last = n_points - 1;
    const ENDPOINT_SUFFIXES: [&str; 4] = ["_first", "_first2", "_last", "_last2"];

    let mut series_len: HashMap<&str, usize> = HashMap::new();
    for q in quantities.iter().filter(|q| q.per_point && !q.is_given) {
        let sym = &q.symbol;
        let series = by_quantity
            .get(q.id.as_str())
            .map(|m| m.point_values())
            .unwrap_or_default();
        let n = series.len();
        series_len.insert(sym.as_str(), n);
        let at = |i: usize| series.get(i).copied().unwrap_or(f64::NAN);
        agg_values.insert(format!("{sym}_first"), at(0));
        agg_values.insert(format!("{sym}_first2"), at(1));
        agg_values.insert(
            format!("{sym}_last"),
            if n == 0 { f64::NAN } else { series[n - 1] },
        );
        agg_values.insert(
            format!("{sym}_last2"),
            if n < 2 { f64::NAN } else { series[n - 2] },
        );
    }
    for it in intermediates {
        let sym = &it.symbol;
        let at = |i: usize| contexts[i].get(sym).copied().unwrap_or(f64::NAN);
        agg_values.insert(format!("{sym}_first"), at(0));
        agg_values.insert(format!("{sym}_first2"), at(1));
        agg_values.insert(format!("{sym}_last"), at(last));
        agg_values.insert(format!("{sym}_last2"), at(last - 1));
    }
    let mut agg_allowed: Vec<String> = agg_values.keys().cloned().collect();
    let mut aggregates_out = Vec::with_capacity(aggregates.len());
    let mut referenced_endpoints: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for agg in aggregates {
        let tree = match compile_formula(&agg.formula, &agg_allowed) {
            Ok(tree) => tree,
            Err(e) => {
                warnings.push(skipped("el mensurando agregado", &agg.symbol, &e));
                continue;
            }
        };
        for v in tree.iter_variable_identifiers() {
            referenced_endpoints.insert(v.to_string());
        }
        let bound: HashMap<&str, f64> = agg_values.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let value = eval_compiled(&tree, &bound);
        if !value.is_finite() {
            warnings.push(format!(
                "El mensurando agregado \"{}\" ({} = {}) no dio un valor finito; revisa la formula y las lecturas (p. ej. division por cero).",
                agg.name, agg.symbol, agg.formula
            ));
        }
        agg_values.insert(agg.symbol.clone(), value);
        agg_allowed.push(agg.symbol.clone());
        aggregates_out.push(AggregateComputation {
            symbol: agg.symbol.clone(),
            name: agg.name.clone(),
            unit: agg.unit.clone(),
            value,
        });
    }
    for q in quantities.iter().filter(|q| q.per_point && !q.is_given) {
        let sym = q.symbol.as_str();
        let n = series_len.get(sym).copied().unwrap_or(0);
        if n == n_points {
            continue;
        }
        let used = ENDPOINT_SUFFIXES
            .iter()
            .any(|suf| referenced_endpoints.contains(&format!("{sym}{suf}")));
        if used {
            warnings.push(format!(
                "Un mensurando agregado usa un extremo de \"{sym}\", que tiene {n} punto(s) frente a {n_points} del ajuste; el extremo se toma de la serie completa de \"{sym}\". Revisa que las cantidades de puntos coincidan."
            ));
        }
    }

    Ok(FormAnalysis {
        quantities: Vec::new(),
        regression: Some(RegressionResult {
            points,
            slope: fit.slope,
            intercept: fit.intercept,
            u_slope: fit.u_slope,
            u_intercept: fit.u_intercept,
            r_squared: fit.r_squared,
            x_label: x_formula.to_string(),
            y_label: y_formula.to_string(),
        }),
        scatters: Vec::new(),
        derived,
        operators: Vec::new(),
        point_results: point_results_out,
        aggregates: aggregates_out,
        warnings,
        parts: Vec::new(),
    })
}

/// Curvas sin ajuste: un gráfico de puntos por cada curva, sobre el mismo barrido.
///
/// ```
/// # use quantify::computation::{compute_curva, CurveSpec, MeasurementInput};
/// # use quantify::db::PracticeQuantity;
/// # let q = |symbol: &str| PracticeQuantity {
/// #     id: symbol.into(), practice_id: "p".into(), symbol: symbol.into(), name: symbol.into(),
/// #     unit: String::new(), repeated: false, quantity: None, position: 0, is_given: false,
/// #     replicas_per_point: None, per_point: true, has_uncertainty: true, optional: false,
/// #     default_value: None, slot_count: None,
/// # };
/// # let m = |id: &str, values: Vec<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u: None,
/// #     point_replicas: None, operator_replicas: None,
/// # };
/// let curve = CurveSpec { x_formula: "R", y_formula: "I*I*R", x_log: false };
/// let (analysis, _) = compute_curva(
///     &[q("R"), q("I")], &[], &[curve],
///     &[m("R", vec![1.0, 2.0]), m("I", vec![1.0, 0.5])],
/// )
/// .unwrap();
/// assert_eq!(analysis.scatters[0].points, vec![(1.0, 1.0), (2.0, 0.5)]);
/// ```
pub fn compute_curva(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    curves: &[CurveSpec],
    measurements: &[MeasurementInput],
) -> anyhow::Result<(FormAnalysis, Vec<PointContext>)> {
    let mut scatters = Vec::with_capacity(curves.len());
    let mut warnings = Vec::new();
    let mut contexts: Vec<PointContext> = Vec::new();
    for curve in curves {
        let (points, mut curve_warnings, ctx) = build_points(
            quantities,
            intermediates,
            curve.x_formula,
            curve.y_formula,
            measurements,
            "se necesitan al menos 2 puntos para graficar la curva",
        )?;
        if contexts.is_empty() {
            contexts = ctx;
        }

        if curve.x_log && points.iter().any(|(x, _)| *x <= 0.0) {
            anyhow::bail!("el eje x es logaritmico pero un punto tiene x <= 0");
        }

        scatters.push(ScatterResult {
            points,
            x_label: curve.x_formula.to_string(),
            y_label: curve.y_formula.to_string(),
            x_log: curve.x_log,
        });
        for w in curve_warnings.drain(..) {
            if !warnings.contains(&w) {
                warnings.push(w);
            }
        }
    }

    let analysis = FormAnalysis {
        quantities: Vec::new(),
        regression: None,
        scatters,
        derived: Vec::new(),
        operators: Vec::new(),
        point_results: Vec::new(),
        aggregates: Vec::new(),
        warnings,
        parts: Vec::new(),
    };
    Ok((analysis, contexts))
}

fn skipped(what: &str, symbol: &str, error: &anyhow::Error) -> String {
    format!("No se calculó {what} {symbol}: {error}. Revisá su fórmula en la práctica.")
}

fn derive_results(
    results: &[PracticeResult],
    allowed: &[String],
    means_by_symbol: &HashMap<String, f64>,
    u_by_symbol: &HashMap<String, f64>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<DerivedComputation>> {
    let mut scope = allowed.to_vec();
    scope.extend(results.iter().map(|r| r.symbol.clone()));
    let index: HashMap<&str, usize> = results
        .iter()
        .enumerate()
        .map(|(i, r)| (r.symbol.as_str(), i))
        .collect();
    let mut trees: Vec<Option<Node>> = Vec::with_capacity(results.len());
    for result in results {
        match compile_formula(&result.formula, &scope) {
            Ok(tree) => trees.push(Some(tree)),
            Err(e) => {
                warnings.push(skipped("el mensurando", &result.symbol, &e));
                trees.push(None);
            }
        }
    }
    let vars_of = |i: usize| -> Vec<String> {
        trees[i].as_ref().map_or_else(Vec::new, |tree| {
            tree.iter_variable_identifiers()
                .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v))
                .map(|s| s.to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        })
    };
    let uses: Vec<Vec<usize>> = (0..results.len())
        .map(|i| {
            if trees[i].is_none() {
                return vec![i];
            }
            vars_of(i)
                .iter()
                .filter_map(|v| index.get(v.as_str()).copied())
                .collect()
        })
        .collect();
    let (order, blocked) = super::formula::chain_order(&uses);
    for &i in &blocked {
        if trees[i].is_some() {
            warnings.push(format!(
                "No se calculó el mensurando {}: depende de sí mismo o de un mensurando que no se pudo calcular. Revisá su fórmula en la práctica.",
                results[i].symbol
            ));
        }
    }

    let mut derived = Vec::with_capacity(results.len());
    for (i, result) in results.iter().enumerate() {
        if blocked.contains(&i) {
            continue;
        }
        let Some(tree) = trees[i].as_ref() else {
            continue;
        };
        let mut needed = vec![false; results.len()];
        let mut stack = uses[i].clone();
        while let Some(d) = stack.pop() {
            if !needed[d] {
                needed[d] = true;
                stack.extend(uses[d].iter().copied());
            }
        }
        let chain: Vec<usize> = order.iter().copied().filter(|&d| needed[d]).collect();
        let vars: Vec<String> = chain
            .iter()
            .chain(std::iter::once(&i))
            .flat_map(|&d| vars_of(d))
            .filter(|v| !index.contains_key(v.as_str()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let means: Vec<f64> = vars
            .iter()
            .map(|v| *means_by_symbol.get(v).unwrap_or(&0.0))
            .collect();
        let us: Vec<f64> = vars
            .iter()
            .map(|v| *u_by_symbol.get(v).unwrap_or(&0.0))
            .collect();
        let (value, u) = uncertainty::propagate(
            |x: &[f64]| {
                let mut bound: HashMap<&str, f64> = vars
                    .iter()
                    .map(|s| s.as_str())
                    .zip(x.iter().copied())
                    .collect();
                for &d in &chain {
                    if let Some(dep) = trees[d].as_ref() {
                        let v = eval_compiled(dep, &bound);
                        bound.insert(results[d].symbol.as_str(), v);
                    }
                }
                eval_compiled(tree, &bound)
            },
            &means,
            &us,
        );
        if !value.is_finite() || !u.is_finite() {
            warnings.push(format!(
                "El mensurando \"{}\" ({} = {}) no dio un valor finito; revisa la formula y las lecturas (p. ej. division por cero).",
                result.name, result.symbol, result.formula
            ));
        }
        derived.push(DerivedComputation {
            symbol: result.symbol.clone(),
            name: result.name.clone(),
            unit: result.unit.clone(),
            has_uncertainty: result.has_uncertainty,
            formula: result.formula.clone(),
            value,
            u,
            u_expanded: uncertainty::expand(u, uncertainty::EXPANSION_K),
        });
    }
    Ok(derived)
}

#[cfg(test)]
pub async fn analyze(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let definition = crate::practices::definition(pool, practice_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la practica no existe"))?;
    analyze_definition(pool, &definition, measurements).await
}

pub async fn analyze_definition(
    pool: &sqlx::SqlitePool,
    definition: &crate::practices::PracticeDefinition,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let by_id: HashMap<&str, &PracticeQuantity> = definition
        .quantities
        .iter()
        .map(|q| (q.id.as_str(), q))
        .collect();
    for measurement in measurements {
        let Some(quantity) = by_id.get(measurement.quantity_id.as_str()) else {
            anyhow::bail!("una de las mediciones no corresponde a una magnitud de esta practica");
        };
        if let Some(n) = quantity.slot_count {
            let nested = [&measurement.point_replicas, &measurement.operator_replicas];
            let all = measurement
                .values
                .iter()
                .chain(nested.into_iter().flatten().flatten().flatten());
            for &v in all {
                if v.fract() != 0.0 || !(1.0..=n as f64).contains(&v) {
                    anyhow::bail!(
                        "\"{}\" ({}) tiene que ser una de las ranuras, un entero de 1 a {n} (se cargó {v})",
                        quantity.name,
                        quantity.symbol
                    );
                }
            }
        }
    }

    let scales = load_scales(pool, measurements).await?;

    for measurement in measurements {
        if let (Some(instrument_id), Some(scale_id)) =
            (&measurement.instrument_id, &measurement.scale_id)
        {
            if let Some(scale) = scales.get(scale_id) {
                if scale.instrument_id != *instrument_id {
                    anyhow::bail!("la escala elegida no pertenece al instrumento seleccionado");
                }
            }
        }
    }

    if definition.parts.is_empty() {
        return run_kind(definition, &scales, measurements);
    }
    let shared = definition.shared_view();
    let mut analysis = run_kind(&shared, &scales, &measurements_of(&shared, measurements))?;
    let shared_symbols: Vec<&str> = shared
        .quantities
        .iter()
        .map(|q| q.symbol.as_str())
        .collect();
    let shared_results: Vec<&str> = shared.results.iter().map(|r| r.symbol.as_str()).collect();
    for part in &definition.parts {
        if !definition.part_has_content(part) {
            continue;
        }
        let view = definition.part_view(part);
        let mut part_analysis = match run_kind(
            &view,
            &scales,
            &measurements_of(&view, measurements),
        ) {
            Ok(part_analysis) => part_analysis,
            Err(e) => {
                analysis.warnings.push(format!(
                        "No se pudo calcular la parte \"{}\": {e}. Revisá su configuración en la práctica.",
                        part.title
                    ));
                continue;
            }
        };
        part_analysis
            .quantities
            .retain(|q| !shared_symbols.contains(&q.symbol.as_str()));
        part_analysis
            .derived
            .retain(|d| !shared_results.contains(&d.symbol.as_str()));
        for operator in &mut part_analysis.operators {
            operator
                .quantities
                .retain(|q| !shared_symbols.contains(&q.symbol.as_str()));
            operator
                .derived
                .retain(|d| !shared_results.contains(&d.symbol.as_str()));
        }
        part_analysis
            .warnings
            .retain(|w| !analysis.warnings.contains(w));
        analysis.parts.push(PartAnalysis {
            part_id: part.id.clone(),
            title: part.title.clone(),
            analysis_kind: definition.part_kind(part).to_string(),
            analysis: part_analysis,
        });
    }
    Ok(analysis)
}

fn run_kind(
    definition: &crate::practices::PracticeDefinition,
    scales: &HashMap<String, InstrumentScale>,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let mut analysis = run_kind_raw(definition, scales, measurements)?;
    let mut seen = std::collections::HashSet::new();
    analysis.warnings.retain(|w| seen.insert(w.clone()));
    Ok(analysis)
}

fn measurements_of(
    definition: &crate::practices::PracticeDefinition,
    measurements: &[MeasurementInput],
) -> Vec<MeasurementInput> {
    measurements
        .iter()
        .filter(|m| definition.quantities.iter().any(|q| q.id == m.quantity_id))
        .cloned()
        .collect()
}

fn run_kind_raw(
    definition: &crate::practices::PracticeDefinition,
    scales: &HashMap<String, InstrumentScale>,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    if definition.analysis_kind.as_deref() == Some("regresion_lineal") {
        let (Some(x_formula), Some(y_formula)) = (
            definition.x_formula.as_deref(),
            definition.y_formula.as_deref(),
        ) else {
            anyhow::bail!(
                "la practica es de regresion pero no tiene definidas las formulas de los ejes"
            );
        };
        return compute_regresion(
            &definition.quantities,
            &definition.intermediates,
            &definition.results,
            &definition.point_results,
            &definition.aggregates,
            scales,
            x_formula,
            y_formula,
            measurements,
        );
    }

    if definition.analysis_kind.as_deref() == Some("curva") {
        if definition.curves.is_empty() {
            anyhow::bail!("la practica es de curva pero no tiene curvas definidas");
        }
        let curves: Vec<CurveSpec> = definition
            .curves
            .iter()
            .map(|c| CurveSpec {
                x_formula: &c.x_formula,
                y_formula: &c.y_formula,
                x_log: c.x_log,
            })
            .collect();
        let (mut analysis, contexts) = compute_curva(
            &definition.quantities,
            &definition.intermediates,
            &curves,
            measurements,
        )?;
        let scalar_qtys: Vec<&PracticeQuantity> = definition
            .quantities
            .iter()
            .filter(|q| !q.per_point || q.is_given)
            .collect();
        let mut symbols: Vec<String> = scalar_qtys.iter().map(|q| q.symbol.clone()).collect();
        let by_quantity: HashMap<&str, &MeasurementInput> = measurements
            .iter()
            .map(|m| (m.quantity_id.as_str(), m))
            .collect();
        let mut means = HashMap::new();
        let mut us = HashMap::new();
        analysis.quantities = compute_quantities(
            &scalar_qtys,
            &by_quantity,
            scales,
            None,
            &mut means,
            &mut us,
            &mut analysis.warnings,
        )?;
        let per_point_syms: Vec<String> = per_point_quantity_symbols(&definition.quantities)
            .map(String::from)
            .chain(definition.intermediates.iter().map(|it| it.symbol.clone()))
            .collect();
        for s in &per_point_syms {
            let mut best: Option<(usize, f64)> = None;
            for (i, ctx) in contexts.iter().enumerate() {
                if let Some(&v) = ctx.get(s) {
                    if v.is_finite() && best.is_none_or(|(_, bv)| v > bv) {
                        best = Some((i, v));
                    }
                }
            }
            let Some((idx, max_value)) = best else {
                continue;
            };
            let max_symbol = format!("{s}_max");
            means.insert(max_symbol.clone(), max_value);
            us.insert(max_symbol.clone(), 0.0);
            symbols.push(max_symbol);
            for t in &per_point_syms {
                if t == s {
                    continue;
                }
                if let Some(&tv) = contexts[idx].get(t) {
                    let at_symbol = format!("{t}_at_{s}_max");
                    means.insert(at_symbol.clone(), tv);
                    us.insert(at_symbol.clone(), 0.0);
                    symbols.push(at_symbol);
                }
            }
        }
        analysis.derived = derive_results(
            &definition.results,
            &symbols,
            &means,
            &us,
            &mut analysis.warnings,
        )?;
        return Ok(analysis);
    }

    compute(
        &definition.quantities,
        &definition.results,
        scales,
        measurements,
        definition.operator_count,
    )
}

async fn load_scales(
    pool: &sqlx::SqlitePool,
    measurements: &[MeasurementInput],
) -> anyhow::Result<HashMap<String, InstrumentScale>> {
    let mut scales = HashMap::new();
    for measurement in measurements {
        let Some(scale_id) = measurement.scale_id.as_deref() else {
            continue;
        };
        if scales.contains_key(scale_id) {
            continue;
        }
        let scale = crate::instruments::find_scale(pool, scale_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("la escala seleccionada no existe"))?;
        scales.insert(scale_id.to_string(), scale);
    }
    Ok(scales)
}
