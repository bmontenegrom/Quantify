//! Cálculo de incertidumbres de una entrega cargada por formulario (análisis `estadistico`).
//!
//! Toma las lecturas crudas del estudiante + la definición de la práctica + el catálogo de
//! instrumentos, y produce un [`FormAnalysis`] con incertidumbres tipo A/B/combinada/expandida
//! por magnitud y la propagación de cada mensurando. El cálculo numérico vive en
//! [`crate::uncertainty`]; este módulo lo cablea con la base y evalúa las fórmulas (texto)
//! con `evalexpr`.
//!
//! Dividido en [`formula`] (compilar/evaluar fórmulas), [`engines`] (los motores de cálculo +
//! `analyze`) y [`submission`] (alta/edición de entregas); este archivo solo tiene los DTOs
//! compartidos y los re-exports que mantienen el path público `computation::X` de siempre.

mod engines;
mod formula;
mod submission;

use serde::{Deserialize, Serialize};

pub use engines::{analyze, compute, compute_curva, compute_regresion, scale_spec, PointContext};
pub use formula::check_formula;
pub use submission::{
    check_student_result_symbols, create_form_submission, update_form_submission,
    validate_student_results, FormSubmissionInput,
};

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
    pub result: crate::uncertainty::QuantityResult,
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
    /// `false` si el mensurando se muestra sin ±U en toda la UI (ver `PracticeResult::has_uncertainty`).
    pub has_uncertainty: bool,
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

#[cfg(test)]
mod tests;
