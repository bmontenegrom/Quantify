//! Motores de cálculo (estadístico, regresión, curva) y [`analyze`], que lee la definición de la
//! práctica y delega en el motor correspondiente.

use super::formula::{compile_formula, eval_compiled, CONSTANTS};
use super::{
    AggregateComputation, CurveSpec, DerivedComputation, FormAnalysis, MeasurementInput,
    OperatorComputation, PointResultComputation, QuantityComputation, RegressionResult,
    ScatterResult,
};
use crate::analysis;
use crate::db::{InstrumentScale, PracticeQuantity, PracticeResult};
use crate::practices::{PracticeAggregate, PracticeIntermediate, PracticePointResult};
use crate::uncertainty::{self, BModel, ScaleSpec};
use evalexpr::Node;
use std::collections::HashMap;

/// Convierte una escala del catálogo ([`InstrumentScale`]) en la especificación que entiende
/// el motor ([`ScaleSpec`]). Error si el `b_model` guardado no es uno de los modelos conocidos.
///
/// # Ejemplos
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

/// Calcula la incertidumbre de un subconjunto de magnitudes y acumula sus medias/incertidumbres
/// en `means`/`us` (para propagar los mensurandos). `operator` selecciona la serie a usar para las
/// magnitudes repetidas: `Some(i)` toma `operator_replicas[i]` (Motor D); `None` usa `values`.
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
        // Serie a usar: del operador `i` para magnitudes repetidas con operadores; si no, `values`.
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
            // `has_uncertainty = false`: el form no tiene campo U para esta magnitud (solo
            // "Valor"), así que cualquier `given_u` cargado (de una entrega vieja, por ejemplo) se
            // ignora y queda en 0.
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

/// Calcula el [`FormAnalysis`] de una entrega estadística (función pura, sin base de datos).
/// `scales` mapea `scale_id` → escala ya resuelta; `measurements` son las lecturas por magnitud.
///
/// `operator_count` (Motor D): con `≥ 2`, las magnitudes **repetidas** se computan por operador
/// (cada uno con su serie) y los mensurandos derivados se calculan por operador en
/// [`FormAnalysis::operators`]; las magnitudes **compartidas** (dadas/medida única) se calculan una
/// sola vez en `quantities`. Con `None`/`≤ 1` es el comportamiento por defecto (una sola serie).
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
    // Todos los símbolos quedan disponibles al compilar las fórmulas de los mensurandos.
    let symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    let operator_count = operator_count.unwrap_or(0);

    // Comportamiento por defecto (sin operadores): una sola serie por magnitud.
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
        });
    }

    // Con operadores: las repetidas (tipo A) se cargan por operador; las dadas o de medida única se
    // comparten. Cada operador deriva sus mensurandos con su serie + las compartidas, sin promediar.
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
    })
}

/// Serie de puntos `(x, y)` evaluados desde las fórmulas de eje.
type PointSeries = Vec<(f64, f64)>;

/// Empareja las mediciones por punto y evalúa las fórmulas de eje `x_formula`/`y_formula`,
/// devolviendo la serie de puntos `(x, y)`, las advertencias, y el **contexto por punto** (valor de
/// cada magnitud e intermedia en cada punto). Compartido por `regresion_lineal` y `curva`. Las
/// magnitudes con `per_point = false` o `is_given` son escalares compartidos: se difunden a todos los
/// puntos y **no** condicionan la cantidad de puntos. Falla si hay menos de 2 puntos o si un
/// punto produce un valor no finito; el mensaje de "menos de 2 puntos" lo aporta `too_few_msg`.
pub type PointContext = HashMap<String, f64>;

/// Símbolos de las magnitudes medidas por punto (van en la serie, no son escalares compartidos):
/// `per_point == true` y no `is_given`. Usado tanto para condicionar la cantidad de puntos
/// (`build_points`) como para los alias de extremos por punto (`compute_curva`).
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

    // Magnitudes que son **escalares compartidos** (no por punto): se colapsan a un único valor
    // representativo —el mismo que usa `compute_quantities` para los mensurandos: el valor dado
    // para datos de cátedra, la media de las lecturas si es medida única— y se difunde a todos los
    // puntos. Así nunca varían entre puntos aunque lleguen varias lecturas (vía API o entregas
    // viejas reconvertidas de por-punto a compartida).
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

    // Valor por punto (media de réplicas) y matriz punto×réplica (para las intermedias) por magnitud.
    // Los escalares compartidos se reducen a una sola fila/valor (su representativo).
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
    // Magnitudes de un solo valor por punto (sin `point_replicas`) y los escalares compartidos: se
    // difunden a todas las réplicas al evaluar una intermedia. Las replicadas NO se difunden (un
    // conteo distinto entre ellas es dato incompleto → produce un punto no finito que se rechaza).
    let broadcastable: std::collections::HashSet<&str> = measurements
        .iter()
        .filter(|m| m.point_replicas.is_none() || shared_ids.contains(m.quantity_id.as_str()))
        .map(|m| m.quantity_id.as_str())
        .collect();
    let symbol_to_id: HashMap<&str, &str> = quantities
        .iter()
        .map(|q| (q.symbol.as_str(), q.id.as_str()))
        .collect();
    // Magnitudes que se miden por punto (van en la serie): solo estas condicionan la cantidad de
    // puntos. Las `per_point = false` o `is_given` son escalares compartidos que se difunden.
    let per_point_syms: std::collections::HashSet<&str> =
        per_point_quantity_symbols(quantities).collect();

    // Las intermedias (Motor C) se compilan **en orden**: cada una puede usar las magnitudes y las
    // intermedias **anteriores** (a estas las ve como su valor por punto, ya promediado). Sus
    // símbolos quedan disponibles para los ejes.
    let mut allowed = magnitude_symbols.clone();
    let mut compiled_intermediates: Vec<(&PracticeIntermediate, Node)> = Vec::new();
    for it in intermediates {
        let tree = compile_formula(&it.formula, &allowed)?;
        allowed.push(it.symbol.clone());
        compiled_intermediates.push((it, tree));
    }
    let axis_symbols = allowed; // magnitudes + todas las intermedias
    let x_tree = compile_formula(x_formula, &axis_symbols)?;
    let y_tree = compile_formula(y_formula, &axis_symbols)?;
    let intermediate_symbols: std::collections::HashSet<&str> = compiled_intermediates
        .iter()
        .map(|(it, _)| it.symbol.as_str())
        .collect();

    // Intermedias necesarias = las que usan los ejes, más (cierre) las que esas referencian. Como
    // una intermedia solo referencia anteriores, un recorrido inverso basta.
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

    // Cantidad de puntos: solo las magnitudes **medidas por punto** referenciadas por los ejes —o
    // por las intermedias necesarias— condicionan (los escalares compartidos se difunden).
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

    // Valor por punto de una magnitud: el del punto `i`; los escalares (un solo valor) se difunden.
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
        // Contexto del punto: todas las magnitudes (difundiendo escalares) + todas las intermedias
        // (en orden; cada una puede usar las anteriores). Sirve a los ejes y a las derivadas por punto.
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

/// Valor de una magnitud intermedia en el punto `i`: evalúa su fórmula para cada réplica del punto
/// y promedia. Las magnitudes de **un solo valor por punto** (`broadcastable`) —y las intermedias
/// anteriores (`earlier`, ya promediadas por punto)— se difunden a todas las réplicas. Las
/// magnitudes **replicadas** NO se difunden: si una tiene menos réplicas que el punto (dato
/// incompleto/desparejo), la réplica faltante produce `NaN` y el punto se rechaza aguas arriba. Una
/// fórmula sin magnitudes replicadas (solo magnitudes de un valor, intermedias o constantes) se
/// evalúa una vez.
fn point_intermediate(
    tree: &Node,
    point_matrix: &HashMap<&str, Vec<Vec<f64>>>,
    symbol_to_id: &HashMap<&str, &str>,
    broadcastable: &std::collections::HashSet<&str>,
    earlier: &HashMap<&str, f64>,
    i: usize,
) -> f64 {
    let id_of = |sym: &str| symbol_to_id.get(sym).copied();
    // Réplicas de la magnitud en el punto `i`. Un escalar compartido tiene una sola fila: se
    // difunde a todos los puntos cayendo a la última fila (su único valor) para `i` fuera de rango.
    let reps_at = |sym: &str| -> &[f64] {
        id_of(sym)
            .and_then(|id| point_matrix.get(id))
            .and_then(|m| m.get(i).or_else(|| m.last()))
            .map_or(&[][..], |v| v.as_slice())
    };
    let is_broadcastable = |sym: &str| id_of(sym).is_some_and(|id| broadcastable.contains(id));
    // Réplicas del punto: el máximo entre las magnitudes **replicadas** de la fórmula; al menos 1.
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
            // Las constantes (`pi`, `e`) las precarga `eval_compiled`: no bindear acá (si no, las
            // pisaríamos con NaN al tratarlas como intermedia anterior).
            .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v))
            .map(|v| {
                if symbol_to_id.contains_key(v) {
                    let reps = reps_at(v);
                    let value = if is_broadcastable(v) {
                        // Magnitud de un solo valor por punto: se difunde a todas las réplicas.
                        reps.first().copied().unwrap_or(f64::NAN)
                    } else {
                        // Magnitud replicada: réplica r exacta (sin difundir → NaN si falta).
                        reps.get(r).copied().unwrap_or(f64::NAN)
                    };
                    (v, value)
                } else {
                    // Intermedia anterior: escalar por punto, difundido a todas las réplicas.
                    (v, earlier.get(v).copied().unwrap_or(f64::NAN))
                }
            })
            .collect();
        sum += eval_compiled(tree, &bound);
    }
    sum / n_reps as f64
}

/// Calcula el [`FormAnalysis`] de una práctica `regresion_lineal`: empareja las mediciones por
/// punto, evalúa las fórmulas de eje en cada punto, ajusta una recta y deriva los mensurandos.
///
/// Los mensurandos (`results`) se derivan de `slope`/`intercept` **y de los escalares compartidos**
/// (magnitudes con `per_point = false` o `is_given`), con propagación de incertidumbre — p. ej.
/// μ = slope·(π·ρ·g·R⁴)/(8·L). Las magnitudes derivadas **por punto** (`point_results`, Motor E) se
/// evalúan tras el ajuste con el contexto de cada punto + slope/intercept + los mensurandos
/// (p. ej. el número de Reynolds por corrida).
///
/// Los mensurandos **agregados** (`aggregates`, Motor F) se evalúan una vez tras el ajuste, en orden
/// (encadenables), con acceso a los escalares compartidos, slope/intercept, los mensurandos, los
/// agregados anteriores y los **extremos** de cada magnitud/intermedia por punto: `{sym}_first`,
/// `{sym}_first2`, `{sym}_last`, `{sym}_last2` (p. ej. Reynolds máx/mín con el primer/último par).
/// Para las magnitudes por punto el extremo se toma de su **serie medida completa** (no del último
/// punto ajustado); si esa serie tiene distinta cantidad de puntos que el ajuste, se agrega un aviso.
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

    // Escalares compartidos (valor ± u) disponibles para los mensurandos, junto a slope/intercept.
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

    // Derivadas por punto (post-ajuste): contexto del punto + slope/intercept + mensurandos.
    let mut extras: HashMap<&str, f64> = HashMap::new();
    extras.insert("slope", fit.slope);
    extras.insert("intercept", fit.intercept);
    for d in &derived {
        extras.insert(d.symbol.as_str(), d.value);
    }
    let mut pr_allowed: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    pr_allowed.extend(intermediates.iter().map(|it| it.symbol.clone()));
    pr_allowed.extend(results.iter().map(|r| r.symbol.clone()));
    pr_allowed.push("slope".into());
    pr_allowed.push("intercept".into());
    let mut point_results_out = Vec::with_capacity(point_results.len());
    for pr in point_results {
        let tree = compile_formula(&pr.formula, &pr_allowed)?;
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

    // Mensurandos agregados (Motor F): un valor escalar post-ajuste. Símbolos disponibles: escalares
    // compartidos (en `means`) + slope/intercept + los mensurandos derivados + los extremos de cada
    // magnitud/intermedia por punto + los agregados anteriores (encadenable).
    let mut agg_values: HashMap<String, f64> = means.clone();
    for d in &derived {
        agg_values.insert(d.symbol.clone(), d.value);
    }
    let n_points = contexts.len(); // >= 2: build_points garantiza al menos 2 puntos.
    let last = n_points - 1;
    const ENDPOINT_SUFFIXES: [&str; 4] = ["_first", "_first2", "_last", "_last2"];

    // Alias de extremos para magnitudes por punto: se leen desde la serie cruda de cada
    // magnitud (no desde `contexts`, que puede repetir el último valor o truncar si la
    // magnitud no está en el conjunto de condicionamiento de los ejes). Guardamos el largo de
    // cada serie para avisar después si un extremo referenciado proviene de una serie
    // desalineada con el ajuste.
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
    // Intermedias: no tienen serie independiente; se usan los contextos de la regresión (siempre
    // alineados con el ajuste, así que no condicionan el aviso de desalineamiento).
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
    // Alias de extremo realmente usados por alguna fórmula de agregado: solo sobre estos avisamos
    // un eventual desalineamiento de puntos (así no metemos ruido por magnitudes no referenciadas).
    let mut referenced_endpoints: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for agg in aggregates {
        let tree = compile_formula(&agg.formula, &agg_allowed)?;
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
    // Aviso: un agregado usa un extremo (`X_first`/`X_last`/...) de una magnitud por punto cuya
    // serie tiene distinta cantidad de puntos que el ajuste. El extremo se toma de la serie
    // completa de esa magnitud (no del último punto ajustado), así que conviene revisar la carga.
    // Cubre el caso que `build_points` no avisa: una magnitud por punto que no entra a los ejes.
    // Se recorre `quantities` en orden (no el `HashMap`) para que los avisos salgan determinísticos.
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
    })
}

/// Calcula el [`FormAnalysis`] de una práctica `curva`: para cada curva empareja las mediciones
/// por punto y evalúa su par de fórmulas de eje, produciendo una serie de puntos **sin ajuste**
/// (scatter + tabla) en `scatters`. No deriva mensurandos. Todas las curvas comparten el mismo
/// barrido de mediciones; una `x_log` marca eje x logarítmico en esa curva. Devuelve además los
/// contextos por punto del barrido (los de la primera curva: todas comparten las mediciones), que
/// el llamador usa para los alias de extremos (`{S}_max` / `{T}_at_{S}_max`).
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
        // Varias curvas comparten el mismo barrido: evita repetir el mismo aviso (p. ej. el mismo
        // punto no finito) una vez por curva.
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
    };
    Ok((analysis, contexts))
}

/// Calcula los mensurandos derivados por propagación de varianzas: cada fórmula se evalúa y
/// propaga usando los valores/incertidumbres de los símbolos disponibles (`means_by_symbol` /
/// `u_by_symbol`). Sirve tanto para el camino estadístico (símbolos = magnitudes) como para el
/// de regresión (símbolos = `slope`/`intercept`). Acumula advertencias por valores no finitos.
fn derive_results(
    results: &[PracticeResult],
    allowed: &[String],
    means_by_symbol: &HashMap<String, f64>,
    u_by_symbol: &HashMap<String, f64>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<DerivedComputation>> {
    let mut derived = Vec::with_capacity(results.len());
    for result in results {
        let tree = compile_formula(&result.formula, allowed)?;
        // Variables que la fórmula realmente usa (sin constantes), en orden estable.
        let vars: Vec<String> = tree
            .iter_variable_identifiers()
            .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v))
            .map(|s| s.to_string())
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
                let bound: HashMap<&str, f64> = vars
                    .iter()
                    .map(|s| s.as_str())
                    .zip(x.iter().copied())
                    .collect();
                eval_compiled(&tree, &bound)
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

/// Lee la definición de la práctica y las escalas referidas por las mediciones, y calcula el
/// [`FormAnalysis`]. Reúne los datos de la base y delega en [`compute`].
pub async fn analyze(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let definition = crate::practices::definition(pool, practice_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la practica no existe"))?;

    // Toda medición debe corresponder a una magnitud de esta práctica (evita insertar filas
    // colgadas y da un error claro en vez de una violación de clave foránea).
    let valid_ids: std::collections::HashSet<&str> = definition
        .quantities
        .iter()
        .map(|q| q.id.as_str())
        .collect();
    for measurement in measurements {
        if !valid_ids.contains(measurement.quantity_id.as_str()) {
            anyhow::bail!("una de las mediciones no corresponde a una magnitud de esta practica");
        }
    }

    let scales = load_scales(pool, measurements).await?;

    // Si se eligió instrumento y escala, la escala debe pertenecer a ese instrumento.
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

    // Camino de regresión lineal: requiere las fórmulas de eje definidas.
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
            &scales,
            x_formula,
            y_formula,
            measurements,
        );
    }

    // Camino de curva (scatter sin ajuste): una o varias curvas sobre el mismo barrido (Motor B).
    // Los mensurandos derivados escalares (de magnitudes no-por-punto) se calculan igual que en el
    // camino estadístico: reduce las escalares, propaga incertidumbres, agrega a `derived`.
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
        // Magnitudes escalares (no-por-punto o dadas): siempre se computan y exponen, con su
        // incertidumbre de instrumento, para que el frontend pueda mostrarlas y compararlas.
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
            &scales,
            None,
            &mut means,
            &mut us,
            &mut analysis.warnings,
        )?;
        // Alias de extremos por punto (análogos a `_first`/`_last` de la regresión): para cada
        // símbolo por punto `S` (magnitudes medidas por punto + intermedias), `{S}_max` es su
        // máximo sobre los puntos y `{T}_at_{S}_max` el valor de `T` en ese mismo punto. Van con
        // u = 0 (son lecturas de la tabla, no medidas con incertidumbre propia), de modo que un
        // mensurando como `P_max_e = P_max` resulta con U = 0 y el frontend lo muestra sin ±U.
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
        if !definition.results.is_empty() {
            // Filtra result por result: los que no compilan con los símbolos escalares y alias
            // disponibles emiten un warning individual en lugar de silenciar todo el bloque.
            let scalar_results: Vec<PracticeResult> = definition
                .results
                .iter()
                .filter(|r| {
                    if compile_formula(r.formula.trim(), &symbols).is_ok() {
                        true
                    } else {
                        analysis.warnings.push(format!(
                            "mensurando '{}': la fórmula no puede evaluarse con las magnitudes escalares disponibles",
                            r.symbol
                        ));
                        false
                    }
                })
                .cloned()
                .collect();
            if !scalar_results.is_empty() {
                analysis.derived = derive_results(
                    &scalar_results,
                    &symbols,
                    &means,
                    &us,
                    &mut analysis.warnings,
                )?;
            }
        }
        return Ok(analysis);
    }

    compute(
        &definition.quantities,
        &definition.results,
        &scales,
        measurements,
        definition.operator_count,
    )
}

/// Carga, por id, las escalas referidas por las mediciones (las que traen `scale_id`).
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
        let scale = sqlx::query_as::<_, InstrumentScale>(
            "SELECT id, instrument_id, label, full_scale, step, appreciation, internal_res, \
             internal_res_u, b_model, spec_pct_reading, spec_step_coeff, spec_fixed, unit, position \
             FROM instrument_scales WHERE id = ?1",
        )
        .bind(scale_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la escala seleccionada no existe"))?;
        scales.insert(scale_id.to_string(), scale);
    }
    Ok(scales)
}
