use crate::db::{PracticeQuantity, PracticeResult};
use serde::{Deserialize, Serialize};

fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct QuantityInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub repeated: bool,
    pub quantity: Option<String>,
    #[serde(default)]
    pub is_given: bool,
    #[serde(default)]
    pub replicas_per_point: Option<i64>,
    #[serde(default = "default_true")]
    pub per_point: bool,
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, deserialize_with = "double_option")]
    pub default_value: Option<Option<f64>>,
    #[serde(default)]
    pub slot_count: Option<i64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct ResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    #[serde(default, deserialize_with = "double_option")]
    pub tolerance: Option<Option<f64>>,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PracticeDefinition {
    pub practice_id: String,
    pub status: String,
    pub analysis_kind: Option<String>,
    pub x_formula: Option<String>,
    pub y_formula: Option<String>,
    pub quantities: Vec<PracticeQuantity>,
    pub results: Vec<PracticeResult>,
    pub curves: Vec<PracticeCurve>,
    pub operator_count: Option<i64>,
    pub intermediates: Vec<PracticeIntermediate>,
    pub point_results: Vec<PracticePointResult>,
    pub aggregates: Vec<PracticeAggregate>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PracticeCurve {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub x_formula: String,
    pub y_formula: String,
    pub x_log: bool,
}

#[derive(Debug, Deserialize)]
pub struct CurveInput {
    pub x_formula: String,
    pub y_formula: String,
    #[serde(default)]
    pub x_log: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PracticeIntermediate {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

#[derive(Debug, Deserialize)]
pub struct IntermediateInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PracticePointResult {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

#[derive(Debug, Deserialize)]
pub struct PointResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PracticeAggregate {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    #[serde(default)]
    pub is_final: bool,
}

#[derive(Debug, Deserialize)]
pub struct AggregateInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    #[serde(default)]
    pub is_final: bool,
}

#[derive(Debug, Deserialize)]
pub struct NewPracticeInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub analysis_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct PracticeInfoInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

mod crud;
mod lifecycle;
mod seed;
#[cfg(test)]
mod tests;

pub use crud::*;
pub use lifecycle::*;
pub use seed::seed_definitions;
