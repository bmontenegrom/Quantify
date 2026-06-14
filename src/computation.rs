//! Cálculo de incertidumbres de una entrega cargada por formulario (análisis `estadistico`).
//!
//! Toma las lecturas crudas del estudiante + la definición de la práctica + el catálogo de
//! instrumentos, y produce un [`FormAnalysis`] con incertidumbres tipo A/B/combinada/expandida
//! por magnitud y la propagación de cada mensurando. El cálculo numérico vive en
//! [`crate::uncertainty`]; este módulo lo cablea con la base y evalúa las fórmulas (texto)
//! con `evalexpr`.

use crate::analysis;
use crate::db::{self, AuthUser, InstrumentScale, PracticeQuantity, PracticeResult};
use crate::practices::{PracticeAggregate, PracticeIntermediate, PracticePointResult};
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

    /// Matriz punto × réplica (Motor C): las réplicas de cada punto si hay `point_replicas`; si no,
    /// cada valor de `values` como un punto de una sola réplica. Permite evaluar una magnitud
    /// intermedia por réplica antes de promediar.
    fn point_replica_matrix(&self) -> Vec<Vec<f64>> {
        match &self.point_replicas {
            Some(groups) => groups.clone(),
            None => self.values.iter().map(|&v| vec![v]).collect(),
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

/// Magnitud derivada **por punto, post-ajuste** (Motor E): un valor por corrida (mismo orden que
/// los puntos del ajuste). Sin incertidumbre (la técnica las usa con cifras significativas).
#[derive(Debug, Serialize)]
pub struct PointResultComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub values: Vec<f64>,
}

/// Mensurando **agregado** escalar (Motor F): un único valor post-ajuste, sin incertidumbre.
#[derive(Debug, Serialize)]
pub struct AggregateComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub value: f64,
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
    /// Solo regresión (Motor E): magnitudes derivadas por punto (tabla por corrida, p. ej. Reynolds).
    #[serde(default)]
    pub point_results: Vec<PointResultComputation>,
    /// Solo regresión (Motor F): mensurandos agregados escalares post-ajuste (p. ej. Reynolds medio).
    #[serde(default)]
    pub aggregates: Vec<AggregateComputation>,
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

/// Valida que `formula` sea sintácticamente correcta y solo use símbolos de `allowed` (o las
/// constantes `pi`/`e`). Para validar fórmulas en el alta (p. ej. una magnitud intermedia) sin
/// esperar a que falle en el cálculo. Devuelve el error amigable de [`compile_formula`].
pub fn check_formula(formula: &str, allowed: &[String]) -> anyhow::Result<()> {
    compile_formula(formula, allowed).map(|_| ())
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
/// magnitudes con `per_point = false` o `is_given` son escalares compartidos: se difunden a todos
/// los puntos y **no** condicionan la cantidad de puntos. Falla si hay menos de 2 puntos o si un
/// punto produce un valor no finito; el mensaje de "menos de 2 puntos" lo aporta `too_few_msg`.
type PointContext = HashMap<String, f64>;

fn build_points(
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
    let per_point_syms: std::collections::HashSet<&str> = quantities
        .iter()
        .filter(|q| q.per_point && !q.is_given)
        .map(|q| q.symbol.as_str())
        .collect();

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
/// barrido de mediciones; una `x_log` marca eje x logarítmico en esa curva.
pub fn compute_curva(
    quantities: &[PracticeQuantity],
    intermediates: &[PracticeIntermediate],
    curves: &[CurveSpec],
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let mut scatters = Vec::with_capacity(curves.len());
    let mut warnings = Vec::new();
    for curve in curves {
        let (points, mut curve_warnings, _ctx) = build_points(
            quantities,
            intermediates,
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
        point_results: Vec::new(),
        aggregates: Vec::new(),
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
        let mut analysis = compute_curva(
            &definition.quantities,
            &definition.intermediates,
            &curves,
            measurements,
        )?;
        if !definition.results.is_empty() {
            let scalar_qtys: Vec<&PracticeQuantity> = definition
                .quantities
                .iter()
                .filter(|q| !q.per_point || q.is_given)
                .collect();
            if !scalar_qtys.is_empty() {
                let symbols: Vec<String> = scalar_qtys.iter().map(|q| q.symbol.clone()).collect();
                // Pre-validación: si algún mensurando referencia un símbolo por-punto (p. ej.
                // una práctica estadística reutilizada como `curva`), no hay contexto escalar
                // para derivarlos → se omite silenciosamente toda la derivación.
                let all_valid = definition
                    .results
                    .iter()
                    .all(|r| compile_formula(r.formula.trim(), &symbols).is_ok());
                if all_valid {
                    let by_quantity: HashMap<&str, &MeasurementInput> = measurements
                        .iter()
                        .map(|m| (m.quantity_id.as_str(), m))
                        .collect();
                    let mut means = HashMap::new();
                    let mut us = HashMap::new();
                    compute_quantities(
                        &scalar_qtys,
                        &by_quantity,
                        &scales,
                        None,
                        &mut means,
                        &mut us,
                        &mut analysis.warnings,
                    )?;
                    analysis.derived = derive_results(
                        &definition.results,
                        &symbols,
                        &means,
                        &us,
                        &mut analysis.warnings,
                    )?;
                }
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
            per_point: true,
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
            a.warnings.iter().any(|w| w.contains("\"z\"")
                && w.contains("4 punto")
                && w.contains("3 del ajuste")),
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
        let a =
            compute_curva(&quantities, &[], &[curve("px", "py", false)], &measurements).unwrap();
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
        assert!(
            compute_curva(&quantities, &[], &[curve("px", "py", false)], &measurements).is_err()
        );
    }

    #[test]
    fn compute_curva_rejects_non_positive_x_when_log() {
        // Con eje x logarítmico, un x <= 0 es inválido.
        let quantities = vec![quantity("px"), quantity("py")];
        let measurements = vec![
            measurement("px", &[0.0, 10.0]),
            measurement("py", &[1.0, 2.0]),
        ];
        assert!(
            compute_curva(&quantities, &[], &[curve("px", "py", true)], &measurements).is_err()
        );
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
        let a =
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
