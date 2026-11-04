mod engines;
mod formula;
mod submission;

use serde::{Deserialize, Serialize};

pub use engines::{analyze, compute, compute_curva, compute_regresion, scale_spec, PointContext};
pub use formula::check_formula;
pub(crate) use formula::{chain_order, formula_symbols};
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
}

impl MeasurementInput {
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
pub struct OperatorComputation {
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

#[derive(Debug, Serialize)]
pub struct FormAnalysis {
    pub quantities: Vec<QuantityComputation>,
    pub regression: Option<RegressionResult>,
    pub scatters: Vec<ScatterResult>,
    pub derived: Vec<DerivedComputation>,
    #[serde(default)]
    pub operators: Vec<OperatorComputation>,
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
