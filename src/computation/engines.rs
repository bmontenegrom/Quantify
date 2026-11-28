use super::dual::{eval_dual, Dual};
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

fn scale_to_quantity(scale: &InstrumentScale, unit: &str) -> Result<(f64, f64), String> {
    use crate::units::Mismatch;
    crate::units::conversion(&scale.unit, unit).map_err(|mismatch| {
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

type TypeB = Box<dyn Fn(f64) -> f64>;

fn scale_type_b(
    quantity: &PracticeQuantity,
    scale_id: Option<&str>,
    scales: &HashMap<String, InstrumentScale>,
    has_values: bool,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Option<TypeB>> {
    let Some(scale_id) = scale_id else {
        return Ok(None);
    };
    let Some(scale) = scales.get(scale_id) else {
        anyhow::bail!("la escala seleccionada no existe");
    };
    let spec = scale_spec(scale)?;
    let (factor, offset) = scale_to_quantity(scale, &quantity.unit).unwrap_or_else(|w| {
        if has_values {
            warnings.push(format!("\"{}\" ({}): {w}.", quantity.name, quantity.symbol));
        }
        (1.0, 0.0)
    });
    Ok(Some(Box::new(move |mean| {
        uncertainty::type_b(&spec, (mean - offset) / factor) * factor
    })))
}

pub(crate) const ENDPOINT_SUFFIXES: [&str; 4] = ["_first", "_first2", "_last", "_last2"];

fn endpoint_of(symbol: &str) -> Option<(&str, &'static str)> {
    ENDPOINT_SUFFIXES
        .into_iter()
        .find_map(|suffix| Some((symbol.strip_suffix(suffix)?, suffix)))
}

type PointUs<'a> = HashMap<&'a str, Vec<(f64, f64)>>;

fn u_at(point_us: &PointUs, symbol: &str, i: usize) -> (f64, f64) {
    point_us
        .get(symbol)
        .and_then(|v| v.get(i).or_else(|| v.last()))
        .copied()
        .unwrap_or((f64::NAN, f64::NAN))
}

fn point_uncertainties<'a>(
    quantities: &[&'a PracticeQuantity],
    by_quantity: &HashMap<&str, &MeasurementInput>,
    scales: &HashMap<String, InstrumentScale>,
    warnings: &mut Vec<String>,
) -> PointUs<'a> {
    let mut out = HashMap::new();
    for &q in quantities {
        let Some(m) = by_quantity.get(q.id.as_str()) else {
            continue;
        };
        let replicas = m.point_replica_matrix();
        let has_values = replicas.iter().any(|r| !r.is_empty());
        let type_b = match scale_type_b(q, m.instrument_for(None).1, scales, has_values, warnings) {
            Ok(type_b) => type_b,
            Err(e) => {
                warnings.push(format!(
                    "\"{}\" ({}): {e}; los resultados que la usan quedan sin incertidumbre.",
                    q.name, q.symbol
                ));
                out.insert(
                    q.symbol.as_str(),
                    vec![(f64::NAN, f64::NAN); replicas.len().max(1)],
                );
                continue;
            }
        };
        let per_point = replicas
            .iter()
            .map(|values| match &type_b {
                Some(type_b) => uncertainty::measured_with(values, type_b),
                None => uncertainty::measured_quantity(values, None),
            })
            .map(|r| (r.u_c, r.u_b))
            .collect();
        out.insert(q.symbol.as_str(), per_point);
    }
    out
}

struct RunCtx<'a> {
    compiled: &'a CompiledIntermediates<'a>,
    inputs: PointInputs<'a>,
    per_point: std::collections::HashSet<&'a str>,
    on_means: HashMap<&'a str, bool>,
    formulas: &'a HashMap<&'a str, &'a Node>,
    point_us: &'a PointUs<'a>,
    us: &'a HashMap<String, f64>,
    means: Option<&'a HashMap<String, f64>>,
    exact: Option<&'a HashMap<&'a str, f64>>,
    factors: &'a [Vec<(&'a str, f64)>],
}

#[derive(Default)]
struct RunState<'a> {
    reps: HashMap<&'a str, Vec<f64>>,
    failed: std::collections::HashSet<&'a str>,
}

struct RunU {
    u: f64,
    kink: bool,
    missing: bool,
    replicas: Vec<f64>,
}

impl RunU {
    fn checked(&self, value: f64, not_derivable: &mut bool, on_kink: &mut bool) -> f64 {
        if !value.is_finite() || self.missing {
            return f64::NAN;
        }
        *not_derivable |= !self.u.is_finite();
        *on_kink |= self.kink && self.u > 0.0;
        self.u
    }
}

impl<'a> RunCtx<'a> {
    fn new(
        quantities: &'a [PracticeQuantity],
        measurements: &'a [MeasurementInput],
        compiled: &'a CompiledIntermediates<'a>,
        formulas: &'a HashMap<&'a str, &'a Node>,
        point_us: &'a PointUs<'a>,
        us: &'a HashMap<String, f64>,
    ) -> Self {
        Self {
            compiled,
            inputs: PointInputs::new(quantities, measurements),
            per_point: per_point_quantity_symbols(quantities).collect(),
            on_means: compiled
                .iter()
                .map(|(it, _)| (it.symbol.as_str(), it.on_means))
                .collect(),
            formulas,
            point_us,
            us,
            means: None,
            exact: None,
            factors: &[],
        }
    }

    fn reached(&self, tree: &Node, state: &RunState, n_reps: usize) -> (bool, bool) {
        let mut stack: Vec<&str> = tree.iter_variable_identifiers().collect();
        let mut seen = std::collections::HashSet::new();
        let (mut unpaired, mut failed) = (false, false);
        while let Some(s) = stack.pop() {
            if !seen.insert(s) {
                continue;
            }
            if let (Some(&on_means), Some(r)) = (self.on_means.get(s), state.reps.get(s)) {
                unpaired |= on_means || (r.len() != n_reps && r.len() != 1);
                failed |= state.failed.contains(s);
            }
            if let Some(t) = self.formulas.get(s) {
                stack.extend(t.iter_variable_identifiers());
            }
        }
        (unpaired, failed)
    }

    fn run_u(
        &self,
        tree: &Node,
        on_means: bool,
        i: usize,
        ctx: &PointContext,
        state: &RunState,
    ) -> RunU {
        let replicas = point_intermediate(on_means, tree, &self.inputs, &state.reps, i);
        let (unpaired, failed) = self.reached(tree, state, replicas.len());
        let per_replica = !on_means && replicas.iter().all(|v| v.is_finite()) && !unpaired;
        let missing = std::cell::Cell::new(failed);
        let varies = |s: &str| {
            per_replica
                && self
                    .inputs
                    .symbol_to_id
                    .get(s)
                    .is_some_and(|id| !self.inputs.broadcastable.contains(id))
        };
        let leaf = |s: &str| {
            let v = self
                .means
                .and_then(|m| m.get(s))
                .or_else(|| ctx.get(s))
                .copied()
                .unwrap_or(f64::NAN);
            let (v, u) = if self.point_us.contains_key(s) {
                let (u_c, u_b) = u_at(self.point_us, s, i);
                (v, if varies(s) { u_b } else { u_c })
            } else if self.per_point.contains(s) {
                (f64::NAN, f64::NAN)
            } else if let Some(&exact) = self.exact.and_then(|e| e.get(s)) {
                (exact, 0.0)
            } else {
                (v, self.us.get(s).copied().unwrap_or(0.0))
            };
            missing.set(missing.get() || !v.is_finite() || !u.is_finite());
            (v, u)
        };
        let propagated = propagate(tree, self.formulas, &leaf, self.factors);
        let u = if per_replica {
            uncertainty::combine(uncertainty::type_a(&replicas).2, propagated.u)
        } else {
            propagated.u
        };
        RunU {
            u,
            kink: propagated.kink,
            missing: missing.get(),
            replicas,
        }
    }
}

struct ReplicaPoints {
    type_a: Vec<f64>,
    varied: std::collections::HashSet<String>,
}

fn per_replica_points(
    tree: &Node,
    deps: &HashMap<&str, &Node>,
    leaf_point: &HashMap<String, (&str, usize, f64)>,
    run_leaves: &HashMap<String, (usize, bool)>,
    inputs: &PointInputs,
    leaf: &dyn Fn(&str) -> (f64, f64),
) -> ReplicaPoints {
    let mut stack: Vec<(&str, bool, Option<usize>)> = tree
        .iter_variable_identifiers()
        .map(|v| (v, false, None))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut points: std::collections::BTreeMap<usize, Vec<(&str, &[f64])>> = Default::default();
    let mut on_means = std::collections::HashSet::new();
    while let Some((s, under_means, run)) = stack.pop() {
        if !seen.insert((s, under_means, run)) {
            continue;
        }
        if let Some(&(id, k, _)) = leaf_point.get(s) {
            let g = run.unwrap_or(k);
            if under_means {
                on_means.insert(g);
            }
            let reps = inputs
                .point_matrix
                .get(id)
                .and_then(|m| m.get(k))
                .filter(|_| !inputs.broadcastable.contains(id))
                .map_or(&[][..], Vec::as_slice);
            let group = points.entry(g).or_default();
            if !group.iter().any(|(l, _)| *l == s) {
                group.push((s, reps));
            }
        }
        let (under_means, run) = match run_leaves.get(s) {
            Some(&(i, means)) => (under_means || means, Some(i)),
            None => (under_means, run),
        };
        if let Some(t) = deps.get(s) {
            stack.extend(t.iter_variable_identifiers().map(|v| (v, under_means, run)));
        }
    }
    let mut groups_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for (&g, leaves) in &points {
        for (l, _) in leaves {
            groups_of.entry(l).or_default().push(g);
        }
    }
    on_means.extend(groups_of.values().filter(|gs| gs.len() > 1).flatten());
    let mut out = ReplicaPoints {
        type_a: Vec::new(),
        varied: std::collections::HashSet::new(),
    };
    for (k, leaves) in points {
        let n = leaves.iter().map(|(_, r)| r.len()).max().unwrap_or(0);
        if n == 0
            || on_means.contains(&k)
            || leaves.iter().any(|(_, r)| !r.is_empty() && r.len() != n)
        {
            continue;
        }
        let values: Vec<f64> = (0..n)
            .map(|r| {
                let at_r = |s: &str| match leaves.iter().find(|(l, _)| *l == s) {
                    Some((_, reps)) if !reps.is_empty() => (reps[r], 0.0),
                    _ => (leaf(s).0, 0.0),
                };
                propagate(tree, deps, &at_r, &[]).value
            })
            .collect();
        if values.iter().all(|v| v.is_finite()) {
            out.type_a.push(uncertainty::type_a(&values).2);
            out.varied.extend(
                leaves
                    .iter()
                    .filter(|(_, r)| !r.is_empty())
                    .map(|(l, _)| l.to_string()),
            );
        }
    }
    out
}

fn u_warning(
    (the, of_the): (&str, &str),
    subject: &str,
    not_derivable: bool,
    on_kink: bool,
) -> Option<String> {
    if not_derivable {
        Some(format!(
            "La incertidumbre {of_the} {subject} no se puede calcular en alguna corrida: la fórmula no es derivable ahí (p. ej. una raíz en 0) o falta la de una magnitud."
        ))
    } else if on_kink {
        Some(format!(
            "En alguna corrida, {the} {subject} cae justo en un quiebre de la fórmula (un valor absoluto en 0 o un mínimo/máximo empatado): la incertidumbre se calculó con una sola de las ramas (la de mayor incertidumbre), así que puede no ser exacta."
        ))
    } else {
        None
    }
}

fn intermediate_uncertainties<'a>(
    run: &RunCtx<'a>,
    contexts: &[PointContext],
    warnings: &mut Vec<String>,
) -> (Vec<PointResultComputation>, Vec<RunState<'a>>) {
    let compiled = run.compiled;
    let mut out: Vec<PointResultComputation> = compiled
        .iter()
        .map(|(it, _)| PointResultComputation {
            symbol: it.symbol.clone(),
            name: it.name.clone(),
            unit: it.unit.clone(),
            values: Vec::new(),
            u: Vec::new(),
            u_expanded: Vec::new(),
        })
        .collect();
    let mut not_derivable = vec![false; compiled.len()];
    let mut on_kink = vec![false; compiled.len()];
    let mut runs: HashMap<&str, usize> = HashMap::new();
    for (it, tree) in compiled {
        let own = tree
            .iter_variable_identifiers()
            .filter_map(|v| {
                runs.get(v).copied().or_else(|| {
                    run.per_point.contains(v).then(|| {
                        run.inputs
                            .symbol_to_id
                            .get(v)
                            .and_then(|id| run.inputs.point_values.get(id))
                            .map_or(0, Vec::len)
                    })
                })
            })
            .min()
            .unwrap_or(contexts.len());
        runs.insert(it.symbol.as_str(), own.min(contexts.len()));
    }
    let mut states = Vec::with_capacity(contexts.len());
    for (i, ctx) in contexts.iter().enumerate() {
        let mut state = RunState::default();
        for (k, (it, tree)) in compiled.iter().enumerate() {
            let symbol = it.symbol.as_str();
            if i >= runs[symbol] {
                let replicas = point_intermediate(it.on_means, tree, &run.inputs, &state.reps, i);
                state.reps.insert(symbol, replicas);
                continue;
            }
            let r = run.run_u(tree, it.on_means, i, ctx, &state);
            let value = ctx.get(&it.symbol).copied().unwrap_or(f64::NAN);
            let u = r.checked(value, &mut not_derivable[k], &mut on_kink[k]);
            if !u.is_finite() {
                state.failed.insert(symbol);
            }
            state.reps.insert(symbol, r.replicas);
            out[k].values.push(value);
            out[k].u.push(u);
            out[k]
                .u_expanded
                .push(uncertainty::expand(u, uncertainty::EXPANSION_K));
        }
        states.push(state);
    }
    for (k, (it, _)) in compiled.iter().enumerate() {
        let subject = format!(
            "intermedia \"{}\" ({} = {})",
            it.name, it.symbol, it.formula
        );
        warnings.extend(u_warning(
            ("la", "de la"),
            &subject,
            not_derivable[k],
            on_kink[k],
        ));
    }
    out.retain(|r| !r.values.is_empty());
    (out, states)
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
            match scale_type_b(quantity, instrument.1, scales, !values.is_empty(), warnings)? {
                Some(type_b) => uncertainty::measured_with(&values, type_b),
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
    let (derived, _) = derive_results(results, &symbols, &means, &us, &[], &mut warnings)?;
    Ok(FormAnalysis {
        quantities: computed,
        regression: None,
        scatters: Vec::new(),
        derived,
        determinations: Vec::new(),
        intermediates: Vec::new(),
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
    let (mut derived, _) =
        derive_results(&expanded, &allowed, &means, &us, &[], &mut derive_warnings)?;
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
        intermediates: Vec::new(),
        point_results: Vec::new(),
        aggregates: Vec::new(),
        warnings,
        parts: Vec::new(),
    })
}

type PointSeries = Vec<(f64, f64)>;
type BuiltPoints = (PointSeries, Vec<String>, Vec<PointContext>, [Node; 2]);

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
) -> anyhow::Result<BuiltPoints> {
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
        let mut intermediate_values: HashMap<&str, Vec<f64>> = HashMap::new();
        for (it, tree) in &compiled_intermediates {
            let replicas = point_intermediate(it.on_means, tree, &inputs, &intermediate_values, i);
            context.insert(it.symbol.clone(), mean(&replicas));
            intermediate_values.insert(it.symbol.as_str(), replicas);
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
    Ok((points, warnings, contexts, [x_tree, y_tree]))
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn point_intermediate(
    on_means: bool,
    tree: &Node,
    inputs: &PointInputs,
    earlier: &HashMap<&str, Vec<f64>>,
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
    let n_reps = if on_means {
        1
    } else {
        let vars = || tree.iter_variable_identifiers();
        vars()
            .filter(|v| symbol_to_id.contains_key(v) && !is_broadcastable(v))
            .map(|v| reps_at(v).len())
            .max()
            .or_else(|| vars().filter_map(|v| earlier.get(v).map(Vec::len)).max())
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
                        match earlier.get(v) {
                            Some(reps) if !on_means && reps.len() == n_reps => reps[r],
                            Some(reps) => mean(reps),
                            None => f64::NAN,
                        }
                    } else if on_means {
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
            let mut earlier: HashMap<&str, Vec<f64>> = HashMap::new();
            let mut row = Vec::new();
            for (it, tree) in &compiled {
                let replicas = point_intermediate(it.on_means, tree, &inputs, &earlier, i);
                let value = mean(&replicas);
                if it.show_live {
                    row.push(LiveValue {
                        symbol: it.symbol.clone(),
                        replicas: replicas.iter().copied().map(finite).collect(),
                        mean: finite(value),
                    });
                }
                earlier.insert(it.symbol.as_str(), replicas);
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
    let (points, mut warnings, contexts, axis_trees) = build_points(
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
    let mut allowed: Vec<String> = shared.iter().map(|q| q.symbol.clone()).collect();
    allowed.push("slope".into());
    allowed.push("intercept".into());
    let line = [
        vec![
            ("slope", fit.u_slope),
            ("intercept", -fit.mean_x * fit.u_slope),
        ],
        vec![("intercept", fit.u_mean_y)],
    ];
    let (derived, result_trees) =
        derive_results(results, &allowed, &means, &us, &line, &mut warnings)?;

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
    let quantity_symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    let (compiled_intermediates, _) = compile_intermediates(quantity_symbols, intermediates)?;
    let exact: HashMap<&str, f64> = derived
        .iter()
        .filter(|d| !d.has_uncertainty)
        .map(|d| (d.symbol.as_str(), d.value))
        .collect();
    let formulas: HashMap<&str, &Node> = compiled_intermediates
        .iter()
        .map(|(it, tree)| (it.symbol.as_str(), tree))
        .chain(
            result_trees
                .iter()
                .filter(|(s, _)| !exact.contains_key(s.as_str()))
                .map(|(s, tree)| (s.as_str(), tree)),
        )
        .collect();
    let mut compiled_points = Vec::with_capacity(point_results.len());
    for pr in point_results {
        match compile_formula(&pr.formula, &pr_allowed) {
            Ok(tree) => compiled_points.push((pr, tree)),
            Err(e) => warnings.push(skipped("la derivada por punto", &pr.symbol, &e)),
        }
    }
    let reach = |roots: Vec<&str>| {
        let mut stack: Vec<String> = roots.into_iter().map(str::to_string).collect();
        let mut seen: Vec<String> = Vec::new();
        while let Some(symbol) = stack.pop() {
            if seen.contains(&symbol) {
                continue;
            }
            if let Some(tree) = formulas.get(symbol.as_str()) {
                stack.extend(tree.iter_variable_identifiers().map(str::to_string));
            }
            seen.push(symbol);
        }
        seen
    };
    let aggregate_roots: Vec<String> = aggregates
        .iter()
        .filter(|a| a.has_uncertainty)
        .flat_map(|a| super::formula_symbols(&a.formula))
        .map(|s| match endpoint_of(&s) {
            Some((base, _)) => base.to_string(),
            None => s,
        })
        .collect();
    let used = reach(
        compiled_points
            .iter()
            .flat_map(|(_, tree)| tree.iter_variable_identifiers())
            .chain(aggregate_roots.iter().map(String::as_str))
            .chain(intermediates.iter().map(|it| it.symbol.as_str()))
            .chain(axis_trees.iter().flat_map(Node::iter_variable_identifiers))
            .collect(),
    );
    let used: Vec<&str> = used.iter().map(String::as_str).collect();
    let per_point: Vec<&PracticeQuantity> = quantities
        .iter()
        .filter(|q| q.per_point && !q.is_given)
        .collect();
    let used_per_point: Vec<&PracticeQuantity> = per_point
        .iter()
        .copied()
        .filter(|q| used.contains(&q.symbol.as_str()))
        .collect();
    let point_us = point_uncertainties(&used_per_point, &by_quantity, scales, &mut warnings);
    let run = RunCtx {
        means: Some(&means),
        exact: Some(&exact),
        factors: &line,
        ..RunCtx::new(
            quantities,
            measurements,
            &compiled_intermediates,
            &formulas,
            &point_us,
            &us,
        )
    };
    let (intermediates_out, mut states) =
        intermediate_uncertainties(&run, &contexts, &mut warnings);
    for state in &mut states {
        state
            .reps
            .extend(extras.iter().map(|(&s, &v)| (s, vec![v])));
    }
    let mut axis_flags = [(false, false); 2];
    let mut u_points = Vec::with_capacity(points.len());
    for (i, ((ctx, state), &(x, y))) in contexts.iter().zip(&states).zip(&points).enumerate() {
        let axis_u = |tree: &Node, value: f64, (nd, kink): &mut (bool, bool)| {
            let r = run.run_u(tree, false, i, ctx, state);
            uncertainty::expand(r.checked(value, nd, kink), uncertainty::EXPANSION_K)
        };
        let [fx, fy] = &mut axis_flags;
        u_points.push((axis_u(&axis_trees[0], x, fx), axis_u(&axis_trees[1], y, fy)));
    }
    for ((axis, formula), (not_derivable, on_kink)) in [("x", x_formula), ("y", y_formula)]
        .into_iter()
        .zip(axis_flags)
    {
        let subject = format!("eje {axis} ({formula})");
        warnings.extend(u_warning(("el", "del"), &subject, not_derivable, on_kink));
    }
    let mut point_results_out = Vec::with_capacity(point_results.len());
    for (pr, tree) in &compiled_points {
        let (mut values, mut u) = (Vec::new(), Vec::new());
        let (mut not_derivable, mut on_kink) = (false, false);
        for (i, (ctx, state)) in contexts.iter().zip(&states).enumerate() {
            let mut bound: HashMap<&str, f64> = ctx.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            bound.extend(&extras);
            let value = eval_compiled(tree, &bound);
            let r = run.run_u(tree, false, i, ctx, state);
            u.push(r.checked(value, &mut not_derivable, &mut on_kink));
            values.push(value);
        }
        let subject = format!(
            "derivada por punto \"{}\" ({} = {})",
            pr.name, pr.symbol, pr.formula
        );
        warnings.extend(u_warning(("la", "de la"), &subject, not_derivable, on_kink));
        point_results_out.push(PointResultComputation {
            symbol: pr.symbol.clone(),
            name: pr.name.clone(),
            unit: pr.unit.clone(),
            u_expanded: u
                .iter()
                .map(|&u| uncertainty::expand(u, uncertainty::EXPANSION_K))
                .collect(),
            values,
            u,
        });
    }

    let mut agg_values: HashMap<String, f64> = means.clone();
    for d in &derived {
        agg_values.insert(d.symbol.clone(), d.value);
    }
    let n_points = contexts.len();
    let last = n_points - 1;

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
    let point_leaf = |s: &str, k: usize| format!("{s}__p{k}");
    let run_leaf = |s: &str, i: usize| format!("{s}__c{i}");
    let endpoint_index = |suffix: &str, n: usize| match suffix {
        "_first" => (n > 0).then_some(0),
        "_first2" => (n > 1).then_some(1),
        "_last" => n.checked_sub(1),
        _ => n.checked_sub(2),
    };
    let mut point_leaves: HashMap<String, (f64, f64)> = HashMap::new();
    let mut leaf_point: HashMap<String, (&str, usize, f64)> = HashMap::new();
    for q in &per_point {
        let series = by_quantity
            .get(q.id.as_str())
            .map(|m| m.point_values())
            .unwrap_or_default();
        for (k, &value) in series.iter().enumerate() {
            let (u_c, u_b) = u_at(&point_us, &q.symbol, k);
            point_leaves.insert(point_leaf(&q.symbol, k), (value, u_c));
            leaf_point.insert(point_leaf(&q.symbol, k), (q.id.as_str(), k, u_b));
        }
        if series.is_empty() {
            point_leaves.insert(point_leaf(&q.symbol, 0), (f64::NAN, f64::NAN));
        }
    }
    let is_intermediate = |s: &str| intermediates.iter().any(|i| i.symbol == s);
    let at_run = |s: &str, i: usize| -> Option<String> {
        match series_len.get(s) {
            Some(&n) => Some(point_leaf(s, i.min(n.saturating_sub(1)))),
            None => is_intermediate(s).then(|| run_leaf(s, i)),
        }
    };
    let mut owned_trees: Vec<(String, Node)> = Vec::new();
    let mut run_leaves: HashMap<String, (usize, bool)> = HashMap::new();
    let mut runs: Vec<usize> = vec![0, 1, last - 1, last];
    runs.sort_unstable();
    runs.dedup();
    for (it, _) in &compiled_intermediates {
        for &i in &runs {
            let renamed = rename_symbols(&it.formula, |s| at_run(s, i));
            run_leaves.insert(run_leaf(&it.symbol, i), (i, it.on_means));
            match super::parse(&renamed) {
                Ok(tree) => owned_trees.push((run_leaf(&it.symbol, i), tree)),
                Err(_) => {
                    point_leaves.insert(run_leaf(&it.symbol, i), (f64::NAN, f64::NAN));
                }
            }
        }
    }
    let canonical = |s: &str| -> Option<String> {
        let (base, suffix) = endpoint_of(s)?;
        match series_len.get(base) {
            Some(&n) => endpoint_index(suffix, n).map(|k| point_leaf(base, k)),
            None if is_intermediate(base) => {
                endpoint_index(suffix, n_points).map(|i| run_leaf(base, i))
            }
            None => None,
        }
    };
    let mut agg_allowed: Vec<String> = agg_values.keys().cloned().collect();
    let mut aggregates_out = Vec::with_capacity(aggregates.len());
    let mut referenced_endpoints: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut exact_aggregates: std::collections::HashSet<String> = std::collections::HashSet::new();
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
        let deps: HashMap<&str, &Node> = formulas
            .iter()
            .map(|(&s, &t)| (s, t))
            .chain(owned_trees.iter().map(|(s, t)| (s.as_str(), t)))
            .collect();
        let leaf = |s: &str| {
            if let Some(&point) = point_leaves.get(s) {
                point
            } else if let Some(&v) = exact.get(s) {
                (v, 0.0)
            } else if exact_aggregates.contains(s) {
                (agg_values.get(s).copied().unwrap_or(f64::NAN), 0.0)
            } else {
                (
                    agg_values.get(s).copied().unwrap_or(0.0),
                    us.get(s).copied().unwrap_or(0.0),
                )
            }
        };
        let canonical_tree = super::parse(&rename_symbols(&agg.formula, canonical)).ok();
        let propagated = match (&canonical_tree, agg.has_uncertainty) {
            (_, false) => Propagated {
                value,
                u: 0.0,
                kink: false,
            },
            (Some(canonical_tree), true) => {
                let by_point = per_replica_points(
                    canonical_tree,
                    &deps,
                    &leaf_point,
                    &run_leaves,
                    &run.inputs,
                    &leaf,
                );
                let leaf_b = |s: &str| match leaf_point.get(s) {
                    Some(&(_, _, u_b)) if by_point.varied.contains(s) => (leaf(s).0, u_b),
                    _ => leaf(s),
                };
                let mut propagated = propagate(canonical_tree, &deps, &leaf_b, &line);
                propagated.u = by_point
                    .type_a
                    .iter()
                    .fold(propagated.u, |u, &a| uncertainty::combine(u, a));
                propagated
            }
            (None, true) => Propagated {
                value,
                u: f64::NAN,
                kink: false,
            },
        };
        let u = if value.is_finite() {
            propagated.u
        } else {
            f64::NAN
        };
        if !value.is_finite() {
            warnings.push(format!(
                "El mensurando agregado \"{}\" ({} = {}) no dio un valor finito; revisa la formula y las lecturas (p. ej. division por cero).",
                agg.name, agg.symbol, agg.formula
            ));
        } else if !u.is_finite() {
            warnings.push(format!(
                "La incertidumbre del mensurando agregado \"{}\" ({} = {}) no se puede calcular con estos valores: la fórmula no es derivable ahí (p. ej. una raíz en 0) o falta la de una magnitud.",
                agg.name, agg.symbol, agg.formula
            ));
        } else if propagated.kink && u > 0.0 {
            warnings.push(format!(
                "El mensurando agregado \"{}\" ({} = {}) cae justo en un quiebre de la fórmula (un valor absoluto en 0 o un mínimo/máximo empatado): la incertidumbre se calculó con una sola de las ramas (la de mayor incertidumbre), así que puede no ser exacta.",
                agg.name, agg.symbol, agg.formula
            ));
        }
        agg_values.insert(agg.symbol.clone(), value);
        agg_allowed.push(agg.symbol.clone());
        match (agg.has_uncertainty, canonical_tree) {
            (false, _) => {
                exact_aggregates.insert(agg.symbol.clone());
            }
            (true, Some(canonical_tree)) => owned_trees.push((agg.symbol.clone(), canonical_tree)),
            (true, None) => {
                point_leaves.insert(agg.symbol.clone(), (value, f64::NAN));
            }
        }
        aggregates_out.push(AggregateComputation {
            symbol: agg.symbol.clone(),
            name: agg.name.clone(),
            unit: agg.unit.clone(),
            value,
            u,
            u_expanded: uncertainty::expand(u, uncertainty::EXPANSION_K),
            has_uncertainty: agg.has_uncertainty,
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
            u_points,
            x_label: x_formula.to_string(),
            y_label: y_formula.to_string(),
        }),
        scatters: Vec::new(),
        derived,
        determinations: Vec::new(),
        intermediates: intermediates_out,
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
        let (points, mut curve_warnings, ctx, _) = build_points(
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
        intermediates: Vec::new(),
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

type CompiledResults = Vec<(String, Node)>;

struct Propagated {
    value: f64,
    u: f64,
    kink: bool,
}

fn propagate(
    tree: &Node,
    formulas: &HashMap<&str, &Node>,
    leaf: &dyn Fn(&str) -> (f64, f64),
    factors: &[Vec<(&str, f64)>],
) -> Propagated {
    fn visit<'a>(
        name: &str,
        formulas: &HashMap<&'a str, &'a Node>,
        seen: &mut Vec<&'a str>,
        order: &mut Vec<&'a str>,
    ) {
        let Some((&key, tree)) = formulas.get_key_value(name) else {
            return;
        };
        if seen.contains(&key) {
            return;
        }
        seen.push(key);
        for v in tree.iter_variable_identifiers() {
            visit(v, formulas, seen, order);
        }
        order.push(key);
    }
    let (mut seen, mut order) = (Vec::new(), Vec::new());
    for v in tree.iter_variable_identifiers() {
        visit(v, formulas, &mut seen, &mut order);
    }
    let vars: Vec<&str> = order
        .iter()
        .map(|name| formulas[name])
        .chain(std::iter::once(tree))
        .flat_map(|t| t.iter_variable_identifiers())
        .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v) && !formulas.contains_key(v))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let (means, us): (Vec<f64>, Vec<f64>) = vars.iter().map(|v| leaf(v)).unzip();
    let position = |symbol: &str| vars.iter().position(|v| *v == symbol);
    let factor_loads: Vec<Vec<(usize, f64)>> = factors
        .iter()
        .map(|factor| {
            factor
                .iter()
                .filter_map(|&(s, load)| Some((position(s)?, load)))
                .collect()
        })
        .collect();
    let n = vars.len();
    let weigh = |d: &[f64]| uncertainty::propagated_u(d, &us, &factor_loads);
    let mut plain: HashMap<&str, f64> = vars.iter().copied().zip(means.iter().copied()).collect();
    let mut duals: HashMap<&str, Dual> = vars
        .iter()
        .zip(&means)
        .enumerate()
        .map(|(k, (&s, &v))| (s, Dual::variable(v, k, n)))
        .collect();
    for &name in &order {
        let dep = formulas[name];
        plain.insert(name, eval_compiled(dep, &plain));
        duals.insert(name, eval_dual(dep, &duals, n, &weigh));
    }
    let value = eval_compiled(tree, &plain);
    let outcome = eval_dual(tree, &duals, n, &weigh);
    let u = if value.is_finite() {
        uncertainty::propagated_u(&outcome.d, &us, &factor_loads)
    } else {
        f64::NAN
    };
    Propagated {
        value,
        u,
        kink: outcome.kink,
    }
}

fn derive_results(
    results: &[PracticeResult],
    allowed: &[String],
    means_by_symbol: &HashMap<String, f64>,
    u_by_symbol: &HashMap<String, f64>,
    factors: &[Vec<(&str, f64)>],
    warnings: &mut Vec<String>,
) -> anyhow::Result<(Vec<DerivedComputation>, CompiledResults)> {
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

    let mut computed: Vec<Option<DerivedComputation>> = (0..results.len()).map(|_| None).collect();
    let mut notes: Vec<Vec<String>> = vec![Vec::new(); results.len()];
    {
        let formulas: HashMap<&str, &Node> = results
            .iter()
            .enumerate()
            .filter(|(i, r)| !blocked.contains(i) && r.has_uncertainty)
            .filter_map(|(i, r)| Some((r.symbol.as_str(), trees[i].as_ref()?)))
            .collect();
        let mut exact: HashMap<&str, f64> = HashMap::new();
        for &i in &order {
            let result = &results[i];
            let Some(tree) = trees[i].as_ref() else {
                continue;
            };
            let leaf = |s: &str| match exact.get(s) {
                Some(&value) => (value, 0.0),
                None => (
                    means_by_symbol.get(s).copied().unwrap_or(0.0),
                    u_by_symbol.get(s).copied().unwrap_or(0.0),
                ),
            };
            let Propagated { value, u, kink } = propagate(tree, &formulas, &leaf, factors);
            let note = &mut notes[i];
            if !value.is_finite() {
                note.push(format!(
                    "El mensurando \"{}\" ({} = {}) no dio un valor finito; revisa la formula y las lecturas (p. ej. division por cero).",
                    result.name, result.symbol, result.formula
                ));
            } else if result.has_uncertainty && !u.is_finite() {
                note.push(format!(
                    "La incertidumbre del mensurando \"{}\" ({} = {}) no se puede calcular con estos valores: la fórmula no es derivable ahí (p. ej. una raíz en 0).",
                    result.name, result.symbol, result.formula
                ));
            } else if result.has_uncertainty && kink && u > 0.0 {
                note.push(format!(
                    "El mensurando \"{}\" ({} = {}) cae justo en un quiebre de la fórmula (un valor absoluto en 0 o un mínimo/máximo empatado): la incertidumbre se calculó con una sola de las ramas (la de mayor incertidumbre), así que puede no ser exacta.",
                    result.name, result.symbol, result.formula
                ));
            }
            if !result.has_uncertainty {
                exact.insert(result.symbol.as_str(), value);
            }
            computed[i] = Some(DerivedComputation {
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
    }
    warnings.extend(notes.into_iter().flatten());
    let compiled: CompiledResults = trees
        .into_iter()
        .zip(&computed)
        .zip(results)
        .filter_map(|((tree, done), r)| Some((r.symbol.clone(), tree.filter(|_| done.is_some())?)))
        .collect();
    let derived = computed.into_iter().flatten().collect();
    Ok((derived, compiled))
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
        let (compiled_intermediates, _) = compile_intermediates(
            definition
                .quantities
                .iter()
                .map(|q| q.symbol.clone())
                .collect(),
            &definition.intermediates,
        )?;
        let used_per_point: Vec<&PracticeQuantity> = definition
            .quantities
            .iter()
            .filter(|q| q.per_point && !q.is_given)
            .filter(|q| {
                compiled_intermediates
                    .iter()
                    .any(|(_, tree)| tree.iter_variable_identifiers().any(|v| v == q.symbol))
            })
            .collect();
        let point_us = point_uncertainties(
            &used_per_point,
            &by_quantity,
            scales,
            &mut analysis.warnings,
        );
        let formulas: HashMap<&str, &Node> = compiled_intermediates
            .iter()
            .map(|(it, tree)| (it.symbol.as_str(), tree))
            .collect();
        let run = RunCtx::new(
            &definition.quantities,
            measurements,
            &compiled_intermediates,
            &formulas,
            &point_us,
            &us,
        );
        (analysis.intermediates, _) =
            intermediate_uncertainties(&run, &contexts, &mut analysis.warnings);
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
            &[],
            &mut analysis.warnings,
        )?
        .0;
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
