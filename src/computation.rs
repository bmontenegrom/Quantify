mod engines;
mod formula;
mod submission;

use serde::{Deserialize, Serialize};

#[cfg(test)]
pub use engines::analyze;
pub(crate) use engines::per_determination_symbols;
pub use engines::{
    analyze_definition, compute, compute_curva, compute_regresion, live_values, rename_symbols,
    scale_spec, PointContext,
};
pub(crate) use formula::{chain_order, formula_symbols};
pub use formula::{check_formula, FUNCTIONS};
pub use submission::{
    check_student_result_symbols, create_form_submission, update_form_submission,
    validate_student_results, FormSubmissionInput,
};

#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementInput {
    pub quantity_id: String,
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
    pub values: Vec<f64>,
    pub given_u: Option<f64>,
    #[serde(default)]
    pub point_replicas: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    pub operator_replicas: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    pub operator_instruments: Option<Vec<InstrumentChoice>>,
}

pub fn with_fixed_values(
    quantities: &[crate::db::PracticeQuantity],
    measurements: &[MeasurementInput],
) -> Vec<MeasurementInput> {
    let fixed: Vec<_> = quantities.iter().filter(|q| q.fixed).collect();
    let mut out: Vec<_> = measurements
        .iter()
        .filter(|m| !fixed.iter().any(|q| q.id == m.quantity_id))
        .cloned()
        .collect();
    out.extend(fixed.iter().filter_map(|q| {
        Some(MeasurementInput {
            quantity_id: q.id.clone(),
            instrument_id: None,
            scale_id: None,
            values: vec![q.default_value?],
            given_u: q.default_u.filter(|_| q.has_uncertainty),
            point_replicas: None,
            operator_replicas: None,
            operator_instruments: None,
        })
    }));
    out
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstrumentChoice {
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
}

impl MeasurementInput {
    /// Instrumento y escala de una determinación, o los de la magnitud si no se eligieron por determinación.
    ///
    /// ```
    /// use quantify::computation::{InstrumentChoice, MeasurementInput};
    /// let choice = |s: Option<&str>| InstrumentChoice { instrument_id: s.map(Into::into), scale_id: s.map(Into::into) };
    /// let m = MeasurementInput {
    ///     quantity_id: "m".into(), instrument_id: Some("comun".into()), scale_id: Some("comun".into()),
    ///     values: vec![], given_u: None, point_replicas: None, operator_replicas: None,
    ///     operator_instruments: Some(vec![choice(None), choice(Some("balanza"))]),
    /// };
    /// assert_eq!(m.instrument_for(Some(0)), (None, None));
    /// assert_eq!(m.instrument_for(Some(1)), (Some("balanza"), Some("balanza")));
    /// assert_eq!(m.instrument_for(Some(2)), (Some("comun"), Some("comun")));
    /// assert_eq!(m.instrument_for(None), (Some("comun"), Some("comun")));
    /// ```
    pub fn instrument_for(&self, operator: Option<usize>) -> (Option<&str>, Option<&str>) {
        match operator.and_then(|i| self.operator_instruments.as_ref()?.get(i)) {
            Some(c) => (c.instrument_id.as_deref(), c.scale_id.as_deref()),
            None => (self.instrument_id.as_deref(), self.scale_id.as_deref()),
        }
    }

    fn all_instruments(&self) -> impl Iterator<Item = (Option<&str>, Option<&str>)> {
        std::iter::once((self.instrument_id.as_deref(), self.scale_id.as_deref())).chain(
            self.operator_instruments
                .iter()
                .flatten()
                .map(|c| (c.instrument_id.as_deref(), c.scale_id.as_deref())),
        )
    }

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

    fn point_replica_matrix(&self) -> Vec<Vec<f64>> {
        match &self.point_replicas {
            Some(groups) => groups.clone(),
            None => self.values.iter().map(|&v| vec![v]).collect(),
        }
    }
}

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

#[derive(Debug, Serialize)]
pub struct DerivedComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    pub value: f64,
    pub u: f64,
    pub u_expanded: f64,
    pub has_uncertainty: bool,
}

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

#[derive(Debug, Serialize)]
pub struct ScatterResult {
    pub points: Vec<(f64, f64)>,
    pub x_label: String,
    pub y_label: String,
    pub x_log: bool,
}

#[derive(Debug, Serialize)]
pub struct DeterminationComputation {
    pub label: String,
    pub quantities: Vec<QuantityComputation>,
    pub derived: Vec<DerivedComputation>,
}

#[derive(Debug, Serialize)]
pub struct PointResultComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct AggregateComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub value: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LiveValue {
    pub symbol: String,
    pub replicas: Vec<Option<f64>>,
    pub mean: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct FormAnalysis {
    pub quantities: Vec<QuantityComputation>,
    pub regression: Option<RegressionResult>,
    pub scatters: Vec<ScatterResult>,
    pub derived: Vec<DerivedComputation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub determinations: Vec<DeterminationComputation>,
    #[serde(default)]
    pub point_results: Vec<PointResultComputation>,
    #[serde(default)]
    pub aggregates: Vec<AggregateComputation>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartAnalysis>,
}

#[derive(Debug, Serialize)]
pub struct PartAnalysis {
    pub part_id: String,
    pub title: String,
    pub analysis_kind: String,
    pub analysis: FormAnalysis,
}

pub struct CurveSpec<'a> {
    pub x_formula: &'a str,
    pub y_formula: &'a str,
    pub x_log: bool,
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod golden_tests;
