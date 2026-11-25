use crate::db::{PracticeQuantity, PracticeResult};
use serde::{Deserialize, Serialize};

fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

fn canonical_magnitude<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(de)?.map(|q| crate::magnitudes::canonical(&q)))
}

pub const LABEL_MODES: [&str; 3] = [DEFAULT_LABEL_MODE, "simbolo", "nombre_simbolo"];
pub const INPUT_WIDGETS: [&str; 2] = [DEFAULT_INPUT_WIDGET, "cronometro"];
const DEFAULT_LABEL_MODE: &str = "nombre";
pub const DEFAULT_INPUT_WIDGET: &str = "teclado";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityInput {
    #[serde(default)]
    pub display_symbol: Option<String>,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub repeated: bool,
    #[serde(default, deserialize_with = "canonical_magnitude")]
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
    #[serde(default)]
    pub label_mode: Option<String>,
    #[serde(default)]
    pub input_widget: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_determination: bool,
}

impl QuantityInput {
    /// Una magnitud por determinación no es por punto, aunque el JSON diga lo contrario.
    ///
    /// ```
    /// let q = quantify::practices::QuantityInput { per_determination: true, ..Default::default() };
    /// assert!(q.per_point && !q.stored_per_point());
    /// ```
    pub fn stored_per_point(&self) -> bool {
        self.per_point && !self.per_determination
    }
}

impl Default for QuantityInput {
    /// Igual que el JSON sin esos campos: `per_point` y `has_uncertainty` arrancan en `true`.
    ///
    /// ```
    /// let q = quantify::practices::QuantityInput::default();
    /// assert!(q.per_point && q.has_uncertainty && !q.is_given);
    /// ```
    fn default() -> Self {
        Self {
            display_symbol: None,
            symbol: String::new(),
            name: String::new(),
            unit: String::new(),
            repeated: false,
            quantity: None,
            is_given: false,
            replicas_per_point: None,
            per_point: true,
            has_uncertainty: true,
            optional: false,
            default_value: None,
            slot_count: None,
            label_mode: None,
            input_widget: None,
            per_determination: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResultInput {
    #[serde(default)]
    pub display_symbol: Option<String>,
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
    #[serde(default)]
    pub compare_with: Option<String>,
}

impl ResultInput {
    /// ```
    /// use quantify::practices::ResultInput;
    /// let input: ResultInput = serde_json::from_value(serde_json::json!({
    ///     "symbol": "V_t", "name": "V", "unit": "V", "formula": "2", "compare_with": " V "
    /// })).unwrap();
    /// assert_eq!(input.compare_with(), Some("V"));
    /// ```
    pub fn compare_with(&self) -> Option<&str> {
        self.compare_with.as_deref().map(str::trim)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PracticeDefinition {
    pub practice_id: String,
    pub description: String,
    pub status: String,
    pub analysis_kind: Option<String>,
    pub x_formula: Option<String>,
    pub y_formula: Option<String>,
    pub quantities: Vec<PracticeQuantity>,
    pub results: Vec<PracticeResult>,
    pub curves: Vec<PracticeCurve>,
    pub intermediates: Vec<PracticeIntermediate>,
    pub point_results: Vec<PracticePointResult>,
    pub aggregates: Vec<PracticeAggregate>,
    pub parts: Vec<PracticePart>,
    pub sections: Vec<PracticeSection>,
    pub display_symbols: std::collections::BTreeMap<String, String>,
    pub current_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinations: Option<Determinations>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeterminationDisplay {
    #[default]
    Bloques,
    Pestanas,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Determinations {
    pub count: i64,
    pub label: String,
    #[serde(default)]
    pub average: bool,
    #[serde(default)]
    pub display: DeterminationDisplay,
}

impl Determinations {
    /// Revisa la cantidad (de 2 a `MAX_DETERMINATIONS`) y el nombre (hasta 40 caracteres).
    ///
    /// ```
    /// let d = |count| quantify::practices::Determinations { count, label: "Medida".into(), average: false, display: Default::default() };
    /// assert!(d(3).validate().is_ok() && d(1).validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=MAX_DETERMINATIONS).contains(&self.count) {
            return Err(format!(
                "La cantidad de determinaciones tiene que ser de 2 a {MAX_DETERMINATIONS}."
            ));
        }
        let label = self.label.trim();
        if label.is_empty() || label.chars().count() > 40 {
            return Err(
                "Las determinaciones necesitan un nombre de hasta 40 caracteres (p. ej. «Medida»)."
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PracticeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinations: Option<Determinations>,
}

impl PracticeConfig {
    /// Lee el `config_json` guardado; uno vacío o ilegible queda sin configuración.
    ///
    /// ```
    /// let c = quantify::practices::PracticeConfig::parse(r#"{"determinations":{"count":3,"label":"Medida"}}"#);
    /// assert_eq!(c.determinations.unwrap().count, 3);
    /// ```
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Lo escribe como `config_json`; sin configuración queda `{}`.
    ///
    /// ```
    /// assert_eq!(quantify::practices::PracticeConfig::default().to_json(), "{}");
    /// ```
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
pub struct PracticePart {
    pub id: String,
    pub practice_id: String,
    pub title: String,
    pub position: i64,
    pub analysis_kind: Option<String>,
    pub help_text: String,
    #[sqlx(skip)]
    pub effective_kind: String,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinations: Option<Determinations>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
#[serde(default)]
pub struct PracticeSection {
    pub id: String,
    pub practice_id: String,
    pub part_id: Option<String>,
    pub title: String,
    pub position: i64,
    pub help_text: String,
    #[sqlx(skip)]
    pub quantities: Vec<String>,
    #[sqlx(skip)]
    pub results: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PartInput {
    pub title: String,
    pub help_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SectionInput {
    pub title: String,
    #[serde(default)]
    pub part_id: Option<String>,
    pub help_text: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
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
    pub show_live: bool,
    pub on_means: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntermediateInput {
    #[serde(default)]
    pub display_symbol: Option<String>,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    #[serde(default)]
    pub show_live: bool,
    #[serde(default)]
    pub on_means: bool,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PointResultInput {
    #[serde(default)]
    pub display_symbol: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct AggregateInput {
    #[serde(default)]
    pub display_symbol: Option<String>,
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

mod changes;
mod crud;
mod display;
mod layout;
mod library;
mod lifecycle;
mod parts;
mod template;
#[cfg(test)]
mod tests;
mod versions;

pub use changes::*;
pub use crud::*;
pub use display::version_symbols;
pub use layout::*;
pub use library::*;
pub use lifecycle::*;
pub use parts::{
    comparison_conversion, is_point_based, ClientDefinition, CompareOptions, Comparison, Resolved,
    ScopedDeterminations,
};
pub use template::*;
pub use versions::*;
