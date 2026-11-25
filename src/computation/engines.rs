use super::formula::{compile_formula, eval_compiled, CONSTANTS};
use super::{
    AggregateComputation, CurveSpec, DerivedComputation, DeterminationComputation, FormAnalysis,
    LiveValue, MeasurementInput, PartAnalysis, PointResultComputation, QuantityComputation,
    RegressionResult, ScatterResult,
};
use crate::analysis;
use crate::db::{InstrumentScale, PracticeQuantity, PracticeResult};
use crate::practices::{PracticeAggregate, PracticeIntermediate, PracticePointResult};
use crate::uncertainty::{self, BModel, ScaleSpec};
use evalexpr::Node;
use std::collections::HashMap;

/// La especificación del tipo B de una escala del catálogo, en la unidad de la escala.
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

fn scale_to_quantity(
    scale: &InstrumentScale,
    quantity: &PracticeQuantity,
) -> Result<(f64, f64), String> {
    use crate::units::Mismatch;
    let unit = &quantity.unit;
    crate::units::conversion(&scale.unit, unit)
        .map(|(factor, offset)| {
            let offset = if quantity.temperature_difference {
                0.0
            } else {
                offset
            };
            (factor, offset)
        })
        .map_err(|mismatch| {
            let why = match mismatch {
                Mismatch::Incompatible => format!(
                    "la escala mide en {} y la magnitud está en {}",
                    scale.unit.trim(),
                    unit.trim()
                ),
                Mismatch::Unknown(u) => format!("no se reconoce la unidad {u}"),
            };
            format!(
                "{why}: la incertidumbre de la escala \"{}\" no se convirtió",
                scale.label
            )
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
        let instrument = measurement.map_or((None, None), |m| m.instrument_for(operator));
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
            match instrument.1 {
                Some(scale_id) => {
                    let Some(scale) = scales.get(scale_id) else {
                        anyhow::bail!("la escala seleccionada no existe");
                    };
                    let spec = scale_spec(scale)?;
                    let (factor, offset) = scale_to_quantity(scale, quantity).unwrap_or_else(|w| {
                        if !values.is_empty() {
                            warnings
                                .push(format!("\"{}\" ({}): {w}.", quantity.name, quantity.symbol));
                        }
                        (1.0, 0.0)
                    });
                    uncertainty::measured_with(&values, |mean| {
                        uncertainty::type_b(&spec, (mean - offset) / factor) * factor
                    })
                }
                None => uncertainty::measured_quantity(&values, None),
            }
        };

        means.insert(quantity.symbol.clone(), result.mean);
        us.insert(quantity.symbol.clone(), result.u_c);
        computed.push(QuantityComputation {
            quantity_id: quantity.id.clone(),
            symbol: quantity.symbol.clone(),
            name: quantity.name.clone(),
            unit: quantity.unit.clone(),
            instrument_id: instrument.0.map(Into::into),
            scale_id: instrument.1.map(Into::into),
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
/// #     default_value: None, slot_count: None, ..Default::default()
/// # };
/// # let m = |id: &str, values: Vec<f64>, given_u: Option<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u,
/// #     point_replicas: None, operator_replicas: None,
/// #     operator_instruments: None,
/// # };
/// let g = PracticeResult {
///     id: "g".into(), practice_id: "p".into(), symbol: "g".into(), name: "g".into(),
///     unit: "m/s2".into(), formula: "4*pi^2*L/T^2".into(), position: 0, tolerance: None,
///     is_final: false, has_uncertainty: true,
///     ..Default::default()
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
    determinations: Option<&crate::practices::Determinations>,
) -> anyhow::Result<FormAnalysis> {
    let mut warnings = Vec::new();
    let by_quantity: HashMap<&str, &MeasurementInput> = measurements
        .iter()
        .map(|m| (m.quantity_id.as_str(), m))
        .collect();
    let symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    if let Some(determinations) = determinations {
        return compute_determinations(
            quantities,
            results,
            scales,
            &by_quantity,
            determinations,
            warnings,
        );
    }

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
    Ok(FormAnalysis {
        quantities: computed,
        regression: None,
        scatters: Vec::new(),
        derived,
        determinations: Vec::new(),
        point_results: Vec::new(),
        aggregates: Vec::new(),
        warnings,
        parts: Vec::new(),
    })
}

/// Reescribe los identificadores de una fórmula; deja intactas las funciones (`math::ln`).
///
/// ```
/// # use quantify::computation::rename_symbols;
/// let renamed = rename_symbols("math::ln(T)*T2 + 1e5*T", |s| (s == "T").then(|| "T__1".to_string()));
/// assert_eq!(renamed, "math::ln(T__1)*T2 + 1e5*T__1");
/// ```
pub fn rename_symbols(formula: &str, rename: impl Fn(&str) -> Option<String>) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let starts = (c.is_ascii_alphabetic() || c == '_')
            && (i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_'));
        if !starts {
            out.push(c);
            i += 1;
            continue;
        }
        let end = (i..chars.len())
            .find(|&j| !(chars[j].is_ascii_alphanumeric() || chars[j] == '_'))
            .unwrap_or(chars.len());
        let ident: String = chars[i..end].iter().collect();
        let namespaced = out.ends_with("::") || chars[end..].starts_with(&[':', ':']);
        match rename(&ident).filter(|_| !namespaced) {
            Some(renamed) => out.push_str(&renamed),
            None => out.push_str(&ident),
        }
        i = end;
    }
    out
}

fn determination_symbol(symbol: &str, k: usize) -> String {
    format!("{symbol}__{k}")
}

pub(crate) fn per_determination_symbols(
    quantities: &[PracticeQuantity],
    results: &[PracticeResult],
) -> Vec<String> {
    let mut symbols: Vec<String> = quantities
        .iter()
        .filter(|q| q.per_determination)
        .map(|q| q.symbol.clone())
        .collect();
    loop {
        let before = symbols.len();
        for r in results {
            if !symbols.contains(&r.symbol)
                && super::formula_symbols(&r.formula)
                    .iter()
                    .any(|s| symbols.contains(s))
            {
                symbols.push(r.symbol.clone());
            }
        }
        if symbols.len() == before {
            return symbols;
        }
    }
}

fn group_warning(grouped: &mut Vec<(String, Vec<usize>)>, text: String, k: Option<usize>) {
    match grouped.iter_mut().find(|(t, _)| *t == text) {
        Some((_, ks)) => ks.extend(k.filter(|k| !ks.contains(k))),
        None => grouped.push((text, k.into_iter().collect())),
    }
}

fn compute_determinations(
    quantities: &[PracticeQuantity],
    results: &[PracticeResult],
    scales: &HashMap<String, InstrumentScale>,
    by_quantity: &HashMap<&str, &MeasurementInput>,
    determinations: &crate::practices::Determinations,
    mut warnings: Vec<String>,
) -> anyhow::Result<FormAnalysis> {
    let count = determinations.count.max(1) as usize;
    let common: Vec<&PracticeQuantity> =
        quantities.iter().filter(|q| !q.per_determination).collect();
    let repeated: Vec<&PracticeQuantity> =
        quantities.iter().filter(|q| q.per_determination).collect();
    let mut means = HashMap::new();
    let mut us = HashMap::new();
    let common_computed = compute_quantities(
        &common,
        by_quantity,
        scales,
        None,
        &mut means,
        &mut us,
        &mut warnings,
    )?;

    let repeated_symbols = per_determination_symbols(quantities, results);
    let per_result = |r: &PracticeResult| repeated_symbols.contains(&r.symbol);

    let label = |k: usize| format!("{} {k}", determinations.label.trim());
    let mut renamed: HashMap<String, (String, usize)> = HashMap::new();
    let mut grouped: Vec<(String, Vec<usize>)> = Vec::new();
    let mut per_quantities = Vec::with_capacity(count);
    let mut allowed: Vec<String> = common.iter().map(|q| q.symbol.clone()).collect();
    for k in 1..=count {
        let mut local_means = HashMap::new();
        let mut local_us = HashMap::new();
        let mut local_warnings = Vec::new();
        let computed = compute_quantities(
            &repeated,
            by_quantity,
            scales,
            Some(k - 1),
            &mut local_means,
            &mut local_us,
            &mut local_warnings,
        )?;
        for w in local_warnings {
            group_warning(&mut grouped, w, Some(k));
        }
        for q in &repeated {
            let symbol = determination_symbol(&q.symbol, k);
            means.insert(symbol.clone(), local_means[&q.symbol]);
            us.insert(symbol.clone(), local_us[&q.symbol]);
            allowed.push(symbol.clone());
            renamed.insert(symbol, (q.symbol.clone(), k));
        }
        per_quantities.push(computed);
    }

    let per_results: Vec<&PracticeResult> = results.iter().filter(|r| per_result(r)).collect();
    let mut expanded: Vec<PracticeResult> =
        results.iter().filter(|r| !per_result(r)).cloned().collect();
    for k in 1..=count {
        for r in &per_results {
            let symbol = determination_symbol(&r.symbol, k);
            renamed.insert(symbol.clone(), (r.symbol.clone(), k));
            expanded.push(PracticeResult {
                symbol,
                formula: rename_symbols(&r.formula, |s| {
                    repeated_symbols
                        .iter()
                        .any(|x| x == s)
                        .then(|| determination_symbol(s, k))
                }),
                ..(*r).clone()
            });
        }
    }
    let mut averaged: HashMap<String, &str> = HashMap::new();
    let mut average_texts: Vec<(String, String)> = Vec::new();
    if determinations.average {
        for r in &per_results {
            let symbol = format!("{}_medio", r.symbol);
            let formula = format!(
                "({})/{count}",
                (1..=count)
                    .map(|k| determination_symbol(&r.symbol, k))
                    .collect::<Vec<_>>()
                    .join(" + ")
            );
            average_texts.push((
                formula.clone(),
                format!("promedio de {} en las {count} determinaciones", r.symbol),
            ));
            averaged.insert(symbol.clone(), &r.symbol);
            expanded.push(PracticeResult {
                symbol,
                name: format!("{} (promedio)", r.name),
                formula,
                ..(*r).clone()
            });
        }
    }

    let mut derive_warnings = Vec::new();
    let mut derived = derive_results(&expanded, &allowed, &means, &us, &mut derive_warnings)?;
    for mut w in derive_warnings {
        for (formula, text) in &average_texts {
            w = w.replace(formula, text);
        }
        let k = std::cell::Cell::new(None);
        let average = std::cell::Cell::new(false);
        let cleaned = rename_symbols(&w, |s| {
            average.set(average.get() || averaged.contains_key(s));
            renamed.get(s).map(|(base, n)| {
                k.set(k.get().or(Some(*n)));
                base.clone()
            })
        });
        group_warning(&mut grouped, cleaned, k.get().filter(|_| !average.get()));
    }
    for (text, ks) in grouped {
        warnings.push(if ks.is_empty() || ks.len() == count {
            text
        } else {
            let list: Vec<String> = ks.iter().map(|k| k.to_string()).collect();
            format!(
                "{} {}: {text}",
                determinations.label.trim(),
                list.join(", ")
            )
        });
    }

    let formulas: HashMap<&str, &str> = results
        .iter()
        .map(|r| (r.symbol.as_str(), r.formula.as_str()))
        .collect();
    let mut by_determination: Vec<Vec<DerivedComputation>> =
        (0..count).map(|_| Vec::new()).collect();
    let mut shared = Vec::new();
    for mut d in derived.drain(..) {
        if let Some(((base, k), formula)) = renamed
            .get(&d.symbol)
            .and_then(|r| Some((r, *formulas.get(r.0.as_str())?)))
        {
            d.formula = formula.to_string();
            d.symbol = base.clone();
            by_determination[k - 1].push(d);
        } else {
            if let Some(base) = averaged.get(&d.symbol) {
                d.formula = format!("promedio de {base} en las {count} determinaciones");
            }
            shared.push(d);
        }
    }

    Ok(FormAnalysis {
        quantities: common_computed,
        regression: None,
        scatters: Vec::new(),
        derived: shared,
        determinations: per_quantities
            .into_iter()
            .zip(by_determination)
            .enumerate()
            .map(|(i, (quantities, derived))| DeterminationComputation {
                label: label(i + 1),
                quantities,
                derived,
            })
            .collect(),
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

struct PointInputs<'a> {
    point_values: HashMap<&'a str, Vec<f64>>,
    point_matrix: HashMap<&'a str, Vec<Vec<f64>>>,
    broadcastable: std::collections::HashSet<&'a str>,
    symbol_to_id: HashMap<&'a str, &'a str>,
}

impl<'a> PointInputs<'a> {
    fn new(quantities: &'a [PracticeQuantity], measurements: &'a [MeasurementInput]) -> Self {
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
                mean(&m.point_values())
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
        Self {
            point_values,
            point_matrix,
            broadcastable,
            symbol_to_id,
        }
    }
}

type CompiledIntermediates<'a> = Vec<(&'a PracticeIntermediate, Node)>;

fn compile_intermediates(
    mut allowed: Vec<String>,
    intermediates: &[PracticeIntermediate],
) -> anyhow::Result<(CompiledIntermediates<'_>, Vec<String>)> {
    let mut compiled = Vec::with_capacity(intermediates.len());
    for it in intermediates {
        compiled.push((it, compile_formula(&it.formula, &allowed)?));
        allowed.push(it.symbol.clone());
    }
    Ok((compiled, allowed))
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

    let inputs = PointInputs::new(quantities, measurements);
    let PointInputs {
        point_values,
        symbol_to_id,
        ..
    } = &inputs;
    let per_point_syms: std::collections::HashSet<&str> =
        per_point_quantity_symbols(quantities).collect();

    let (compiled_intermediates, axis_symbols) =
        compile_intermediates(magnitude_symbols, intermediates)?;
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
            let value = mean(&point_intermediate(
                it,
                tree,
                &inputs,
                &intermediate_values,
                i,
            ));
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

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn point_intermediate(
    it: &PracticeIntermediate,
    tree: &Node,
    inputs: &PointInputs,
    earlier: &HashMap<&str, f64>,
    i: usize,
) -> Vec<f64> {
    let PointInputs {
        point_values,
        point_matrix,
        broadcastable,
        symbol_to_id,
    } = inputs;
    let id_of = |sym: &str| symbol_to_id.get(sym).copied();
    let reps_at = |sym: &str| -> &[f64] {
        id_of(sym)
            .and_then(|id| point_matrix.get(id))
            .and_then(|m| m.get(i).or_else(|| m.last()))
            .map_or(&[][..], |v| v.as_slice())
    };
    let mean_at = |sym: &str| {
        id_of(sym)
            .and_then(|id| point_values.get(id))
            .and_then(|v| v.get(i).or_else(|| v.last()))
            .copied()
            .unwrap_or(f64::NAN)
    };
    let is_broadcastable = |sym: &str| id_of(sym).is_some_and(|id| broadcastable.contains(id));
    let n_reps = if it.on_means {
        1
    } else {
        tree.iter_variable_identifiers()
            .filter(|v| symbol_to_id.contains_key(v) && !is_broadcastable(v))
            .map(|v| reps_at(v).len())
            .max()
            .unwrap_or(0)
            .max(1)
    };
    (0..n_reps)
        .map(|r| {
            let bound: HashMap<&str, f64> = tree
                .iter_variable_identifiers()
                .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v))
                .map(|v| {
                    let value = if !symbol_to_id.contains_key(v) {
                        earlier.get(v).copied().unwrap_or(f64::NAN)
                    } else if it.on_means {
                        mean_at(v)
                    } else if is_broadcastable(v) {
                        reps_at(v).first().copied().unwrap_or(f64::NAN)
                    } else {
                        reps_at(v).get(r).copied().unwrap_or(f64::NAN)
                    };
                    (v, value)
                })
                .collect();
            eval_compiled(tree, &bound)
        })
        .collect()
}

/// Valores por réplica y promedio de los intermedios con `show_live`, calculados como en el ajuste.
///
/// ```
/// # use quantify::computation::{live_values, MeasurementInput};
/// # use quantify::db::PracticeQuantity;
/// # use quantify::practices::PracticeIntermediate;
/// # let q = |symbol: &str| PracticeQuantity {
/// #     id: symbol.into(), symbol: symbol.into(), per_point: true, has_uncertainty: true,
/// #     ..Default::default()
/// # };
/// # let m = |id: &str, values: Vec<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u: None,
/// #     point_replicas: None, operator_replicas: None,
/// #     operator_instruments: None,
/// # };
/// let p = PracticeIntermediate {
///     symbol: "P".into(), formula: "I*I*R".into(), show_live: true, ..Default::default()
/// };
/// let rows = live_values(&[q("R"), q("I")], &[p], &[m("R", vec![100.0]), m("I", vec![0.5])]).unwrap();
/// assert_eq!(rows[0][0].mean, Some(25.0));
/// ```
pub fn live_values(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    measurements: &[MeasurementInput],
) -> anyhow::Result<Vec<Vec<LiveValue>>> {
    let inputs = PointInputs::new(quantities, measurements);
    let magnitude_symbols = quantities.iter().map(|q| q.symbol.clone()).collect();
    let (compiled, _) = compile_intermediates(magnitude_symbols, intermediates)?;
    let n_points = per_point_quantity_symbols(quantities)
        .filter_map(|sym| inputs.symbol_to_id.get(sym))
        .filter_map(|id| inputs.point_values.get(id))
        .map(|values| values.len())
        .min()
        .unwrap_or(0);
    let finite = |v: f64| v.is_finite().then_some(v);
    Ok((0..n_points)
        .map(|i| {
            let mut earlier: HashMap<&str, f64> = HashMap::new();
            let mut row = Vec::new();
            for (it, tree) in &compiled {
                let replicas = point_intermediate(it, tree, &inputs, &earlier, i);
                let value = mean(&replicas);
                earlier.insert(it.symbol.as_str(), value);
                if it.show_live {
                    row.push(LiveValue {
                        symbol: it.symbol.clone(),
                        replicas: replicas.into_iter().map(finite).collect(),
                        mean: finite(value),
                    });
                }
            }
            row
        })
        .collect())
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
/// #     default_value: None, slot_count: None, ..Default::default()
/// # };
/// # let m = |id: &str, values: Vec<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u: None,
/// #     point_replicas: None, operator_replicas: None,
/// #     operator_instruments: None,
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
        determinations: Vec::new(),
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
/// #     default_value: None, slot_count: None, ..Default::default()
/// # };
/// # let m = |id: &str, values: Vec<f64>| MeasurementInput {
/// #     quantity_id: id.into(), instrument_id: None, scale_id: None, values, given_u: None,
/// #     point_replicas: None, operator_replicas: None,
/// #     operator_instruments: None,
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
        determinations: Vec::new(),
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
    let definition: crate::practices::PracticeDefinition =
        serde_json::from_str(&serde_json::to_string(&definition)?)?;
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
    let mut seen = std::collections::HashSet::new();
    for measurement in measurements {
        let Some(quantity) = by_id.get(measurement.quantity_id.as_str()) else {
            anyhow::bail!("una de las mediciones no corresponde a una magnitud de esta practica");
        };
        if !seen.insert(measurement.quantity_id.as_str()) {
            anyhow::bail!(
                "\"{}\" ({}) viene más de una vez en la entrega",
                quantity.name,
                quantity.symbol
            );
        }
        if measurement.operator_replicas.is_some()
            && !(quantity.per_determination
                && definition
                    .quantity_determinations(&quantity.symbol)
                    .is_some())
        {
            anyhow::bail!(
                "\"{}\" ({}) no se carga por determinación: va como un único grupo de lecturas",
                quantity.name,
                quantity.symbol
            );
        }
        if measurement.operator_instruments.is_some()
            && (!quantity.per_determination || quantity.is_given)
        {
            anyhow::bail!(
                "\"{}\" ({}) no lleva un instrumento por determinación",
                quantity.name,
                quantity.symbol
            );
        }
        let instruments = measurement
            .operator_instruments
            .as_ref()
            .map_or(0, Vec::len);
        if instruments > measurement.operator_replicas.as_ref().map_or(0, Vec::len) {
            anyhow::bail!(
                "\"{}\" ({}) trae instrumentos para más determinaciones que lecturas",
                quantity.name,
                quantity.symbol
            );
        }
        if quantity.per_determination {
            if let Some(d) = definition.quantity_determinations(&quantity.symbol) {
                let groups = measurement.operator_replicas.as_ref().map_or(0, Vec::len);
                if !measurement.values.is_empty() || measurement.point_replicas.is_some() {
                    anyhow::bail!(
                        "\"{}\" ({}) se carga en cada determinación ({} 1, {} 2…), no como un único valor",
                        quantity.name,
                        quantity.symbol,
                        d.label.trim(),
                        d.label.trim()
                    );
                }
                if groups > d.count as usize {
                    let place = definition
                        .part_of_quantity(&quantity.symbol)
                        .map_or("la práctica".to_string(), |p| {
                            format!("la parte \"{}\"", p.title)
                        });
                    anyhow::bail!(
                        "\"{}\" ({}) trae lecturas para {groups} determinaciones, pero {place} tiene {}",
                        quantity.name,
                        quantity.symbol,
                        d.count
                    );
                }
            }
        }
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

    for (instrument_id, scale_id) in measurements.iter().flat_map(|m| m.all_instruments()) {
        if let (Some(instrument_id), Some(scale)) =
            (instrument_id, scale_id.and_then(|s| scales.get(s)))
        {
            if scale.instrument_id != instrument_id {
                anyhow::bail!("la escala elegida no pertenece al instrumento seleccionado");
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
        definition.determinations.as_ref(),
    )
}

async fn load_scales(
    pool: &sqlx::SqlitePool,
    measurements: &[MeasurementInput],
) -> anyhow::Result<HashMap<String, InstrumentScale>> {
    let mut scales = HashMap::new();
    for scale_id in measurements
        .iter()
        .flat_map(|m| m.all_instruments())
        .filter_map(|(_, s)| s)
    {
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
