//! Cálculo de incertidumbres de una entrega cargada por formulario (análisis `estadistico`).
//!
//! Toma las lecturas crudas del estudiante + la definición de la práctica + el catálogo de
//! instrumentos, y produce un [`FormAnalysis`] con incertidumbres tipo A/B/combinada/expandida
//! por magnitud y la propagación de cada mensurando. El cálculo numérico vive en
//! [`crate::uncertainty`]; este módulo lo cablea con la base y evalúa las fórmulas (texto)
//! con `evalexpr`.

use crate::analysis;
use crate::db::{self, AuthUser, InstrumentScale, PracticeQuantity, PracticeResult};
use crate::uncertainty::{self, BModel, QuantityResult, ScaleSpec};
use chrono::Utc;
use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Node, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Lecturas crudas de una magnitud cargadas en el formulario.
#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementInput {
    pub quantity_id: String,
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
    /// Réplicas medidas (una o varias) de la magnitud. En análisis por puntos (regresión/curva)
    /// con una magnitud sin réplicas por punto, es un valor por punto.
    pub values: Vec<f64>,
    /// Incertidumbre expandida U para magnitudes `is_given` (dato de la cátedra).
    pub given_u: Option<f64>,
    /// Solo en análisis por puntos con magnitudes que repiten medición **en cada punto**
    /// (p.ej. tiempo medido varias veces por altura/esfera). Exterior = puntos, interior =
    /// réplicas de ese punto. El motor usa la **media** de cada punto para evaluar los ejes.
    #[serde(default)]
    pub point_replicas: Option<Vec<Vec<f64>>>,
    /// Solo en el camino estadístico con operadores (Motor D) y magnitudes `repeated`: cada
    /// operador trae su propia serie de réplicas. Exterior = operador, interior = réplicas de ese
    /// operador. Las magnitudes compartidas (dadas/medida única) usan `values` y dejan esto en
    /// `None`.
    #[serde(default)]
    pub operator_replicas: Option<Vec<Vec<f64>>>,
}

impl MeasurementInput {
    /// Valor representativo por punto en análisis por puntos: la media de las réplicas de cada
    /// punto si hay `point_replicas`; si no, los `values` tal cual (un valor por punto). Un punto
    /// con réplicas vacías produce `NaN` (lo descarta luego el chequeo de finitud).
    fn point_values(&self) -> Vec<f64> {
        match &self.point_replicas {
            Some(groups) => groups
                .iter()
                .map(|g| {
                    if g.is_empty() {
                        f64::NAN
                    } else {
                        g.iter().sum::<f64>() / g.len() as f64
                    }
                })
                .collect(),
            None => self.values.clone(),
        }
    }
}

/// Cuerpo para crear una entrega por formulario.
#[derive(Debug, Deserialize)]
pub struct FormSubmissionInput {
    pub course_id: String,
    pub group_id: String,
    pub practice_id: String,
    pub measurements: Vec<MeasurementInput>,
    /// Metadatos de depuración por magnitud (bins del histograma + valores descartados).
    /// Se persiste tal cual para que el docente lo vea; opcional.
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    /// Mesa del informe compartido. Si no se envía, se resuelve desde las asignaciones
    /// del alumno. Para docentes/admin es opcional (puede entregar sin mesa asignada).
    #[serde(default)]
    pub table_number: Option<i64>,
}

/// Incertidumbre calculada de una magnitud medida directamente.
#[derive(Debug, Serialize)]
pub struct QuantityComputation {
    pub quantity_id: String,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
    pub values: Vec<f64>,
    pub result: QuantityResult,
}

/// Mensurando derivado calculado por propagación de varianzas.
#[derive(Debug, Serialize)]
pub struct DerivedComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    pub value: f64,
    pub u: f64,
    pub u_expanded: f64,
}

/// Resultado de un ajuste lineal `y = slope*x + intercept` sobre una serie de puntos.
/// `x_label`/`y_label` son las fórmulas de eje (texto), para rotular el gráfico.
#[derive(Debug, Serialize)]
pub struct RegressionResult {
    pub points: Vec<(f64, f64)>,
    pub slope: f64,
    pub intercept: f64,
    pub u_slope: f64,
    pub u_intercept: f64,
    pub r_squared: f64,
    pub x_label: String,
    pub y_label: String,
}

/// Serie de puntos sin ajuste (`analysis_kind = "curva"`): se grafica el scatter y se lista la
/// tabla, sin recta ni mensurandos derivados. `x_log` indica eje x logarítmico en el gráfico.
#[derive(Debug, Serialize)]
pub struct ScatterResult {
    pub points: Vec<(f64, f64)>,
    pub x_label: String,
    pub y_label: String,
    pub x_log: bool,
}

/// Cálculo estadístico de un operador (Motor D): sus magnitudes repetidas y los mensurandos
/// derivados con la serie de ese operador (las magnitudes compartidas quedan en
/// [`FormAnalysis::quantities`], se calculan una sola vez).
#[derive(Debug, Serialize)]
pub struct OperatorComputation {
    pub label: String,
    pub quantities: Vec<QuantityComputation>,
    pub derived: Vec<DerivedComputation>,
}

/// Resultado completo del cálculo de una entrega por formulario. Según el `analysis_kind` se
/// llena un camino: `quantities` (estadístico), `regression` (ajuste lineal) o `scatters` (curva:
/// una o varias curvas sobre el mismo barrido). `derived` y `warnings` aplican a los caminos que
/// correspondan.
///
/// Con operadores (Motor D, estadístico): `quantities` lleva solo las magnitudes **compartidas**
/// (dadas/medida única) y `operators` lleva el cálculo **por operador** (sus magnitudes repetidas
/// y sus mensurandos derivados). Sin operadores, `operators` queda vacío y `quantities`/`derived`
/// tienen el cálculo completo (comportamiento por defecto).
#[derive(Debug, Serialize)]
pub struct FormAnalysis {
    pub quantities: Vec<QuantityComputation>,
    pub regression: Option<RegressionResult>,
    pub scatters: Vec<ScatterResult>,
    pub derived: Vec<DerivedComputation>,
    #[serde(default)]
    pub operators: Vec<OperatorComputation>,
    pub warnings: Vec<String>,
}

/// Especificación de una curva a graficar: par de fórmulas de eje y eje x logarítmico opcional.
pub struct CurveSpec<'a> {
    pub x_formula: &'a str,
    pub y_formula: &'a str,
    pub x_log: bool,
}

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
    })
}

/// Constantes disponibles en cualquier fórmula (además de las funciones `math::*` de evalexpr).
const CONSTANTS: [(&str, f64); 2] = [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

/// Compila una fórmula y valida que todas sus variables sean símbolos declarados (los de
/// `allowed`) o constantes conocidas (`pi`, `e`). Devuelve el árbol precompilado.
fn compile_formula(formula: &str, allowed: &[String]) -> anyhow::Result<Node> {
    let tree = build_operator_tree(formula)
        .map_err(|err| anyhow::anyhow!("la formula \"{formula}\" no es valida: {err}"))?;
    for var in tree.iter_variable_identifiers() {
        let is_constant = CONSTANTS.iter().any(|(name, _)| *name == var);
        if !is_constant && !allowed.iter().any(|s| s == var) {
            anyhow::bail!(
                "la formula \"{formula}\" usa el simbolo \"{var}\", que no es una magnitud de la practica"
            );
        }
    }
    Ok(tree)
}

/// Evalúa una fórmula precompilada con los valores dados por símbolo (más las constantes
/// `pi`/`e`). Devuelve `NaN` si la evaluación falla, para no romper la propagación numérica.
fn eval_compiled(tree: &Node, values: &HashMap<&str, f64>) -> f64 {
    let mut context = HashMapContext::new();
    for (name, value) in CONSTANTS {
        let _ = context.set_value(name.to_string(), Value::Float(value));
    }
    for (symbol, value) in values {
        if context
            .set_value((*symbol).to_string(), Value::Float(*value))
            .is_err()
        {
            return f64::NAN;
        }
    }
    tree.eval_float_with_context(&context).unwrap_or(f64::NAN)
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
            let u_exp = measurement.and_then(|m| m.given_u).unwrap_or(0.0);
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
        warnings,
    })
}

/// Serie de puntos `(x, y)` evaluados desde las fórmulas de eje.
type PointSeries = Vec<(f64, f64)>;

/// Empareja las mediciones por punto y evalúa las fórmulas de eje `x_formula`/`y_formula`,
/// devolviendo la serie de puntos `(x, y)` junto con las advertencias acumuladas. Compartido por
/// los caminos `regresion_lineal` y `curva`. Falla si hay menos de 2 puntos o si un punto produce
/// un valor no finito; el mensaje de "menos de 2 puntos" lo aporta `too_few_msg`.
fn build_points(
    quantities: &[PracticeQuantity],
    x_formula: &str,
    y_formula: &str,
    measurements: &[MeasurementInput],
    too_few_msg: &str,
) -> anyhow::Result<(PointSeries, Vec<String>)> {
    let mut warnings = Vec::new();
    let symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    // Valor representativo por punto de cada magnitud (media de réplicas por punto si las hay).
    let point_values: HashMap<&str, Vec<f64>> = measurements
        .iter()
        .map(|m| (m.quantity_id.as_str(), m.point_values()))
        .collect();

    let x_tree = compile_formula(x_formula, &symbols)?;
    let y_tree = compile_formula(y_formula, &symbols)?;

    // Solo las magnitudes que aparecen en las fórmulas de eje condicionan los puntos. Las
    // auxiliares (p. ej. un dato de cátedra usado en otra parte de la práctica, o una magnitud
    // que no se grafica) se ignoran: no exigen mediciones ni arrastran el conteo de puntos.
    let referenced: std::collections::HashSet<&str> = x_tree
        .iter_variable_identifiers()
        .chain(y_tree.iter_variable_identifiers())
        .collect();
    let axis_quantities: Vec<&PracticeQuantity> = quantities
        .iter()
        .filter(|q| referenced.contains(q.symbol.as_str()))
        .collect();

    // Cantidad de puntos = mínimo de puntos entre las magnitudes de eje (deben venir parejas).
    let lengths: Vec<usize> = axis_quantities
        .iter()
        .map(|q| point_values.get(q.id.as_str()).map_or(0, |v| v.len()))
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

    let mut points = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let bound: HashMap<&str, f64> = axis_quantities
            .iter()
            .filter_map(|q| {
                point_values
                    .get(q.id.as_str())
                    .map(|v| (q.symbol.as_str(), v[i]))
            })
            .collect();
        let x = eval_compiled(&x_tree, &bound);
        let y = eval_compiled(&y_tree, &bound);
        if !x.is_finite() || !y.is_finite() {
            anyhow::bail!(
                "un punto produjo un valor no finito al evaluar los ejes (revisa las formulas y las lecturas)"
            );
        }
        points.push((x, y));
    }
    Ok((points, warnings))
}

/// Calcula el [`FormAnalysis`] de una práctica `regresion_lineal`: empareja las mediciones por
/// punto, evalúa las fórmulas de eje `x_formula`/`y_formula` en cada punto, ajusta una recta
/// (`analysis::linear_regression`) y deriva los mensurandos desde `slope`/`intercept`.
pub fn compute_regresion(
    quantities: &[PracticeQuantity],
    results: &[PracticeResult],
    x_formula: &str,
    y_formula: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let (points, mut warnings) = build_points(
        quantities,
        x_formula,
        y_formula,
        measurements,
        "se necesitan al menos 2 puntos para el ajuste lineal",
    )?;

    let fit = analysis::linear_regression("x", "y", &points)
        .ok_or_else(|| anyhow::anyhow!("no se pudo ajustar la recta (¿todos los x iguales?)"))?;

    // Mensurandos derivados de la pendiente/intercepto.
    let means: HashMap<String, f64> = [
        ("slope".to_string(), fit.slope),
        ("intercept".to_string(), fit.intercept),
    ]
    .into();
    let us: HashMap<String, f64> = [
        ("slope".to_string(), fit.u_slope),
        ("intercept".to_string(), fit.u_intercept),
    ]
    .into();
    let allowed = vec!["slope".to_string(), "intercept".to_string()];
    let derived = derive_results(results, &allowed, &means, &us, &mut warnings)?;

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
        warnings,
    })
}

/// Calcula el [`FormAnalysis`] de una práctica `curva`: para cada curva empareja las mediciones
/// por punto y evalúa su par de fórmulas de eje, produciendo una serie de puntos **sin ajuste**
/// (scatter + tabla) en `scatters`. No deriva mensurandos. Todas las curvas comparten el mismo
/// barrido de mediciones; una `x_log` marca eje x logarítmico en esa curva.
pub fn compute_curva(
    quantities: &[PracticeQuantity],
    curves: &[CurveSpec],
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let mut scatters = Vec::with_capacity(curves.len());
    let mut warnings = Vec::new();
    for curve in curves {
        let (points, mut curve_warnings) = build_points(
            quantities,
            curve.x_formula,
            curve.y_formula,
            measurements,
            "se necesitan al menos 2 puntos para graficar la curva",
        )?;

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

    Ok(FormAnalysis {
        quantities: Vec::new(),
        regression: None,
        scatters,
        derived: Vec::new(),
        operators: Vec::new(),
        warnings,
    })
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
            &definition.results,
            x_formula,
            y_formula,
            measurements,
        );
    }

    // Camino de curva (scatter sin ajuste): una o varias curvas sobre el mismo barrido (Motor B).
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
        return compute_curva(&definition.quantities, &curves, measurements);
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

/// Persiste las mediciones de una entrega en `submission_measurements`. Cubre los tres modos
/// (según `point_based`, que indica si la entrega es por puntos — regresión/curva):
/// - **estadístico** (`point_based = false`): réplicas de una magnitud → `point_index = 0`,
///   `replicate_index = réplica`.
/// - **estadístico con operadores** (`operator_replicas`): `operator_index = operador`,
///   `replicate_index = réplica` (point_index = 0).
/// - **por puntos sin réplicas** (`point_based = true`, `values`): un valor por punto →
///   `point_index = punto`, `replicate_index = 0`.
/// - **por puntos con réplicas** (`point_replicas`): `point_index = punto`, `replicate_index = réplica`.
///
/// Los índices explícitos (`operator_index`/`point_index`, en vez de meter todo en
/// `replicate_index`) permiten reconstruir la serie al editar agrupando por operador/punto. Los
/// datos de cátedra (`given_u`) guardan su único valor con la U en `value_u`.
async fn insert_measurements(
    conn: &mut sqlx::SqliteConnection,
    submission_id: &str,
    measurements: &[MeasurementInput],
    point_based: bool,
) -> anyhow::Result<()> {
    for measurement in measurements {
        // Filas (operator_index, point_index, replicate_index, value, value_u) según el modo.
        let rows: Vec<(i64, i64, i64, f64, Option<f64>)> =
            if let Some(operators) = &measurement.operator_replicas {
                operators
                    .iter()
                    .enumerate()
                    .flat_map(|(o, reps)| {
                        reps.iter()
                            .enumerate()
                            .map(move |(r, &v)| (o as i64, 0i64, r as i64, v, None))
                    })
                    .collect()
            } else if let Some(groups) = &measurement.point_replicas {
                groups
                    .iter()
                    .enumerate()
                    .flat_map(|(p, reps)| {
                        reps.iter()
                            .enumerate()
                            .map(move |(r, &v)| (0i64, p as i64, r as i64, v, None))
                    })
                    .collect()
            } else if measurement.given_u.is_some() {
                measurement
                    .values
                    .first()
                    .map(|&v| vec![(0i64, 0i64, 0i64, v, measurement.given_u)])
                    .unwrap_or_default()
            } else if point_based {
                // Un valor por punto: el índice va en point_index (replicate_index = 0).
                measurement
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (0i64, i as i64, 0i64, v, None))
                    .collect()
            } else {
                // Réplicas estadísticas: un solo punto (0), el índice va en replicate_index.
                measurement
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (0i64, 0i64, i as i64, v, None))
                    .collect()
            };
        for (operator_index, point_index, replicate_index, value, value_u) in rows {
            sqlx::query(
                "INSERT INTO submission_measurements \
                 (id, submission_id, quantity_id, instrument_id, scale_id, \
                  operator_index, point_index, replicate_index, value, value_u) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(submission_id)
            .bind(&measurement.quantity_id)
            .bind(measurement.instrument_id.as_deref())
            .bind(measurement.scale_id.as_deref())
            .bind(operator_index)
            .bind(point_index)
            .bind(replicate_index)
            .bind(value)
            .bind(value_u)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// `true` si la práctica analiza **por puntos** (regresión o curva). Determina el layout de
/// persistencia (el índice del punto va en `point_index`, no en `replicate_index`). Se deriva del
/// `analysis_kind` declarado en la práctica —**no** del resultado del cálculo— para que el formato
/// almacenado no dependa de que el ajuste haya producido salida con los datos cargados (p.ej. un
/// punto único o valores no finitos dejarían `regression`/`scatter` en `None` sin dejar de ser una
/// entrega por puntos).
async fn is_point_based_practice(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
) -> anyhow::Result<bool> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT analysis_kind FROM practices WHERE id = ?1")
            .bind(practice_id)
            .fetch_optional(pool)
            .await?;
    Ok(matches!(
        row.and_then(|r| r.0).as_deref(),
        Some("regresion_lineal") | Some("curva")
    ))
}

/// Crea una entrega por formulario: calcula el análisis, inserta la entrega y sus mediciones
/// en una transacción, y devuelve el detalle. El usuario ya fue validado por el handler.
pub async fn create_form_submission(
    pool: &sqlx::SqlitePool,
    user: &AuthUser,
    input: FormSubmissionInput,
) -> anyhow::Result<db::SubmissionDetail> {
    let is_teacher = matches!(user.role.as_str(), "docente" | "admin");

    // Resolver mesa: prioridad input > practice_table_assignments > user_default_tables.
    // Para docentes/admin la mesa es opcional (pueden entregar sin mesa asignada).
    let table_number = if let Some(t) = input.table_number {
        Some(t)
    } else if !is_teacher {
        db::resolve_user_table(pool, &user.id, &input.group_id, &input.practice_id).await?
    } else {
        None
    };

    // Para alumnos: la mesa es obligatoria.
    if !is_teacher && table_number.is_none() {
        anyhow::bail!(
            "No tenés una mesa asignada para esta práctica. \
             Pedile al docente que te asigne una mesa."
        );
    }

    // Si hay mesa asignada, verificar que no exista ya un informe para (práctica, grupo, mesa).
    if let Some(t) = table_number {
        // Validar rango de la mesa.
        let table_count: Option<(i64,)> =
            sqlx::query_as("SELECT table_count FROM lab_groups WHERE id = ?1")
                .bind(&input.group_id)
                .fetch_optional(pool)
                .await?;
        if let Some((count,)) = table_count {
            if t < 1 || t > count {
                anyhow::bail!("El número de mesa {t} no es válido para este grupo (1..={count})");
            }
        }

        if db::find_existing_report(pool, &input.practice_id, &input.group_id, t)
            .await?
            .is_some()
        {
            anyhow::bail!(
                "Ya existe un informe para la mesa {t} en esta práctica. \
                 Si sos parte de esa mesa, aceptá la invitación desde tus notificaciones."
            );
        }
    }

    let analysis = analyze(pool, &input.practice_id, &input.measurements).await?;
    let point_based = is_point_based_practice(pool, &input.practice_id).await?;
    let analysis_json = serde_json::to_string(&analysis)?;
    let meta_json = match &input.meta {
        Some(value) => Some(serde_json::to_string(value)?),
        None => None,
    };

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    // Inserta la entrega resolviendo nombres denormalizados (igual que la variante CSV).
    let inserted = sqlx::query(
        r#"
        INSERT INTO submissions (
            id, student_name, group_name, course, practice_id, file_name, csv_path,
            analysis_json, status, submitted_at, submitted_by_user_id, course_id, group_id,
            entry_mode, measurement_meta_json, table_number
        )
        SELECT
            ?1,
            u.display_name,
            g.name,
            c.name,
            ?5,
            '(formulario)',
            '',
            ?6,
            'pendiente',
            ?7,
            u.id,
            c.id,
            g.id,
            'form',
            ?8,
            ?9
        FROM users u, lab_groups g, courses c
        WHERE u.id = ?2 AND g.id = ?3 AND c.id = ?4
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&input.group_id)
    .bind(&input.course_id)
    .bind(&input.practice_id)
    .bind(&analysis_json)
    .bind(now)
    .bind(&meta_json)
    .bind(table_number)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // Captura la violación del índice único (carrera entre dos alumnos de la misma mesa).
        if e.to_string().contains("UNIQUE constraint failed") {
            anyhow::anyhow!(
                "Otro integrante ya creó el informe de esta mesa. \
                 Aceptá la invitación desde tus notificaciones."
            )
        } else {
            anyhow::Error::from(e)
        }
    })?;

    // El INSERT...SELECT no inserta nada si el curso/grupo (o usuario) no existe.
    if inserted.rows_affected() == 0 {
        anyhow::bail!("el curso o el grupo indicados no existen");
    }

    // Insertar al creador como owner del informe.
    sqlx::query(
        r#"
        INSERT INTO report_members (submission_id, user_id, role, status, invited_at, accepted_at)
        VALUES (?1, ?2, 'owner', 'accepted', ?3, ?3)
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    insert_measurements(&mut tx, &id, &input.measurements, point_based).await?;
    tx.commit().await?;

    // Invitar a los demás alumnos de la mesa (fuera de la tx para no bloquear).
    if let Some(t) = table_number {
        let _ = db::invite_table_members(
            pool,
            &id,
            &input.group_id,
            &input.practice_id,
            t,
            &user.id,
            now,
        )
        .await;
    }

    db::submission_detail(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no se pudo leer la entrega recien creada"))
}

/// Reemplaza las lecturas y recalcula el análisis de una entrega por formulario existente
/// (edición dentro de la ventana permitida). No cambia `submitted_at` ni la práctica: la
/// validación de propiedad/ventana ocurre en la capa de rutas. Transaccional.
pub async fn update_form_submission(
    pool: &sqlx::SqlitePool,
    submission_id: &str,
    practice_id: &str,
    measurements: &[MeasurementInput],
    meta: Option<&serde_json::Value>,
) -> anyhow::Result<db::SubmissionDetail> {
    let analysis = analyze(pool, practice_id, measurements).await?;
    let point_based = is_point_based_practice(pool, practice_id).await?;
    let analysis_json = serde_json::to_string(&analysis)?;
    let meta_json = match meta {
        Some(value) => Some(serde_json::to_string(value)?),
        None => None,
    };

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE submissions SET analysis_json = ?2, measurement_meta_json = ?3 WHERE id = ?1",
    )
    .bind(submission_id)
    .bind(&analysis_json)
    .bind(&meta_json)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM submission_measurements WHERE submission_id = ?1")
        .bind(submission_id)
        .execute(&mut *tx)
        .await?;

    insert_measurements(&mut tx, submission_id, measurements, point_based).await?;
    tx.commit().await?;

    db::submission_detail(pool, submission_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no se pudo leer la entrega editada"))
}

#[cfg(test)]
mod tests {
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
        }];
        let measurements = vec![
            measurement("l", &[2.0]),
            measurement("a", &[3.0]),
            measurement("b", &[4.0]),
        ];
        let analysis =
            compute(&quantities, &results, &HashMap::new(), &measurements, None).unwrap();
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
        }];
        let measurements = vec![
            measurement("l", &[9.0, 11.0]),
            measurement("a", &[2.0]),
            measurement("b", &[3.0]),
        ];
        let analysis =
            compute(&quantities, &results, &HashMap::new(), &measurements, None).unwrap();
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
            .find(|q| q.symbol == "T")
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
            .find(|q| q.symbol == "T")
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
            .find(|q| q.symbol == "T")
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
                // Operador 0: [1.0, 1.1] ; operador 1: [2.0, 2.1, 2.2].
                operator_replicas: Some(vec![vec![1.0, 1.1], vec![2.0, 2.1, 2.2]]),
            }],
            meta: None,
            table_number: None,
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
        // P1 como curva: T (un valor por punto) vs t_med (réplicas por punto).
        crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
            .await
            .unwrap();
        crate::practices::create_curve(
            &pool,
            "p1-estadistica",
            crate::practices::CurveInput {
                x_formula: "T".into(),
                y_formula: "t_med".into(),
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
                    quantity_id: qid("T"),
                    instrument_id: None,
                    scale_id: None,
                    values: vec![1.0, 2.0, 3.0],
                    given_u: None,
                    point_replicas: None,
                    operator_replicas: None,
                },
                MeasurementInput {
                    quantity_id: qid("t_med"),
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
        };
        let detail = create_form_submission(&pool, &user, input).await.unwrap();
        let rows = db::measurements_for(&pool, &detail.id).await.unwrap();

        // T: un valor por punto → point_index 0,1,2 y replicate_index 0.
        let mut t_rows: Vec<_> = rows.iter().filter(|m| m.quantity_id == qid("T")).collect();
        t_rows.sort_by_key(|m| m.point_index);
        assert_eq!(t_rows.len(), 3);
        for (i, m) in t_rows.iter().enumerate() {
            assert_eq!(m.point_index, i as i64);
            assert_eq!(m.replicate_index, 0);
            assert!(close(m.value, (i + 1) as f64, 1e-9));
        }

        // t_med: réplicas por punto → point_index = punto, replicate_index = réplica.
        let tmed_rows: Vec<_> = rows
            .iter()
            .filter(|m| m.quantity_id == qid("t_med"))
            .collect();
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
                x_formula: "T".into(),
                y_formula: "t_med".into(),
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
                    quantity_id: qid("T"),
                    instrument_id: None,
                    scale_id: None,
                    values: vec![1.0, 2.0, 3.0],
                    given_u: None,
                    point_replicas: None,
                    operator_replicas: None,
                },
                MeasurementInput {
                    quantity_id: qid("t_med"),
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
        };
        let detail = create_form_submission(&pool, &user, input).await.unwrap();
        let rows = db::measurements_for(&pool, &detail.id).await.unwrap();

        // Cada punto guarda su cantidad propia de réplicas (1 + 3 + 2 = 6), con replicate_index
        // contiguo desde 0 dentro del punto.
        let tmed_rows: Vec<_> = rows
            .iter()
            .filter(|m| m.quantity_id == qid("t_med"))
            .collect();
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
            .find(|q| q.symbol == "T")
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
            .find(|q| q["symbol"] == "T")
            .unwrap();
        assert!((q_t["result"]["mean"].as_f64().unwrap() - 11.0).abs() < 1e-9);
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
            .find(|q| q.symbol == "T")
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
        let a = compute_regresion(&quantities, &results, "px", "py", &measurements).unwrap();
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
            &results,
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
    fn compute_regresion_needs_at_least_two_points() {
        let quantities = vec![quantity("px"), quantity("py")];
        let measurements = vec![measurement("px", &[1.0]), measurement("py", &[2.0])];
        assert!(compute_regresion(&quantities, &[], "px", "py", &measurements).is_err());
    }

    #[test]
    fn compute_curva_builds_scatter_without_fit() {
        // Curva sin ajuste: evalúa los ejes y produce los puntos, sin slope/intercept ni derivados.
        let quantities = vec![quantity("px"), quantity("py")];
        let measurements = vec![
            measurement("px", &[1.0, 2.0, 3.0]),
            measurement("py", &[4.0, 9.0, 16.0]),
        ];
        let a = compute_curva(&quantities, &[curve("px", "py", false)], &measurements).unwrap();
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
        let a = compute_curva(
            &quantities,
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
        assert!(compute_curva(&quantities, &[curve("px", "py", false)], &measurements).is_err());
    }

    #[test]
    fn compute_curva_rejects_non_positive_x_when_log() {
        // Con eje x logarítmico, un x <= 0 es inválido.
        let quantities = vec![quantity("px"), quantity("py")];
        let measurements = vec![
            measurement("px", &[0.0, 10.0]),
            measurement("py", &[1.0, 2.0]),
        ];
        assert!(compute_curva(&quantities, &[curve("px", "py", true)], &measurements).is_err());
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
        let a = compute_curva(&quantities, &[curve("px", "py", false)], &measurements).unwrap();
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
        let a = compute_regresion(&quantities, &[], "px", "py", &measurements).unwrap();
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
    async fn analyze_routes_curva_to_scatter() {
        let (pool, _dir) = setup().await;
        // Configuramos P1 como curva con ejes sobre sus propias magnitudes (T vs t_med).
        crate::practices::set_analysis_kind(&pool, "p1-estadistica", "curva")
            .await
            .unwrap();
        crate::practices::create_curve(
            &pool,
            "p1-estadistica",
            crate::practices::CurveInput {
                x_formula: "T".into(),
                y_formula: "t_med".into(),
                x_log: false,
            },
        )
        .await
        .unwrap();
        let def = crate::practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        // L es magnitud auxiliar (no está en los ejes T vs t_med): se omite a propósito y
        // build_points debe ignorarla sin exigirle mediciones.
        let measurements = vec![
            measurement_for(&def, "T", &[1.0, 2.0, 3.0]),
            measurement_for(&def, "t_med", &[4.0, 5.0, 6.0]),
        ];
        let analysis = analyze(&pool, "p1-estadistica", &measurements)
            .await
            .unwrap();
        assert!(analysis.regression.is_none());
        assert!(analysis.derived.is_empty());
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
                x_formula: "T".into(),
                y_formula: "t_med".into(),
                x_log: false,
            },
        )
        .await
        .unwrap();
        crate::practices::create_curve(
            &pool,
            "p1-estadistica",
            crate::practices::CurveInput {
                x_formula: "t_med".into(),
                y_formula: "T".into(),
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
            measurement_for(&def, "T", &[1.0, 2.0, 3.0]),
            measurement_for(&def, "t_med", &[4.0, 5.0, 6.0]),
        ];
        let analysis = analyze(&pool, "p1-estadistica", &measurements)
            .await
            .unwrap();
        assert_eq!(analysis.scatters.len(), 2);
        assert_eq!(analysis.scatters[0].x_label, "T");
        assert_eq!(analysis.scatters[0].y_label, "t_med");
        assert_eq!(
            analysis.scatters[0].points,
            vec![(1.0, 4.0), (2.0, 5.0), (3.0, 6.0)]
        );
        assert_eq!(analysis.scatters[1].x_label, "t_med");
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
}
