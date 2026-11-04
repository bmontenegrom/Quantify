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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
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
    pub parts: Vec<PracticePart>,
    pub sections: Vec<PracticeSection>,
    pub current_version: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
pub struct PracticePart {
    pub id: String,
    pub practice_id: String,
    pub title: String,
    pub position: i64,
    pub analysis_kind: Option<String>,
    #[sqlx(skip)]
    pub effective_kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
pub struct PracticeSection {
    pub id: String,
    pub practice_id: String,
    pub part_id: Option<String>,
    pub title: String,
    pub position: i64,
    #[sqlx(skip)]
    pub quantities: Vec<String>,
    #[sqlx(skip)]
    pub results: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PartInput {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct SectionInput {
    pub title: String,
    #[serde(default)]
    pub part_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
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
mod layout;
mod lifecycle;
mod parts;
mod seed;
#[cfg(test)]
mod tests;
mod versions;

pub use crud::*;
pub use layout::*;
pub use lifecycle::*;
pub use parts::is_point_based;
pub use seed::seed_definitions;
pub use versions::*;
