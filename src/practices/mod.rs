//! Definición de prácticas: magnitudes de entrada y mensurandos derivados.
//!
//! Las definiciones son **globales por práctica** (no por curso). Una vez definida P1
//! con sus magnitudes y fórmulas, cualquier curso que habilite P1 usa la misma definición.
//! El cálculo de incertidumbres (Fase 4) lee esta definición para saber qué medir y qué derivar.

use crate::db::{PracticeQuantity, PracticeResult};
use serde::{Deserialize, Serialize};

/// Deserializador para `Option<Option<T>>` que distingue campo ausente de `null` explícito.
///
/// El derive estándar de serde mapea tanto "ausente" como `null` a `None`, por lo que
/// `Option<Option<T>>` no puede representar las tres variantes. Este helper envuelve
/// cualquier valor presente (incluso `null`) en `Some(...)`, preservando la semántica:
/// - campo ausente → `None`
/// - `null` explícito → `Some(None)`
/// - valor numérico → `Some(Some(v))`
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// Datos para crear o actualizar una magnitud de entrada de una práctica.
#[derive(Debug, Deserialize)]
pub struct QuantityInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// `true` si el estudiante toma varias réplicas (tipo A); `false` para medida única.
    pub repeated: bool,
    /// Magnitud física para sugerir instrumentos compatibles (opcional).
    pub quantity: Option<String>,
    /// `true` si es un dato dado por la cátedra (valor ± U directo, sin instrumento ni réplicas).
    #[serde(default)]
    pub is_given: bool,
    /// Réplicas por punto (grilla) para magnitudes `repeated` en regresión/curva. `None` = sin grilla.
    #[serde(default)]
    pub replicas_per_point: Option<i64>,
    /// En regresión/curva: `true` = se mide por punto (tabla de la serie); `false` = escalar
    /// compartido (Motor E). Default `true` (comportamiento previo).
    #[serde(default = "default_true")]
    pub per_point: bool,
    /// `false` solo tiene efecto combinado con `is_given`: pide únicamente "Valor" (sin
    /// instrumento ni campo U), computado con U = 0. Default `true` (comportamiento previo).
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
    /// `true` si puede quedar sin lecturas sin bloquear el envío del formulario.
    #[serde(default)]
    pub optional: bool,
    /// Valor inicial que muestra el formulario. `None` = campo vacío (comportamiento previo).
    #[serde(default)]
    pub default_value: Option<f64>,
}

/// Default `true` para campos booleanos opcionales (p. ej. `per_point`).
fn default_true() -> bool {
    true
}

/// Datos para crear o actualizar un mensurando derivado de una práctica.
#[derive(Debug, Deserialize)]
pub struct ResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    /// Expresión matemática usando los símbolos de las magnitudes de la práctica.
    pub formula: String,
    /// Tolerancia máxima aceptable como |Δ%|.
    ///
    /// `None` (campo ausente en el JSON) = no modificar la tolerancia existente.
    /// `Some(None)` (campo presente con valor `null`) = borrar la tolerancia.
    /// `Some(Some(v))` = fijar la tolerancia a `v`.
    #[serde(default, deserialize_with = "double_option")]
    pub tolerance: Option<Option<f64>>,
    /// `true` si es el resultado central que el alumno debe entregar para esta práctica.
    #[serde(default)]
    pub is_final: bool,
    /// `false` oculta la ±U de este mensurando en toda la UI. Default `true` (comportamiento
    /// previo). Reemplaza el Set hardcodeado `RESULTS_WITHOUT_U` del frontend.
    #[serde(default = "default_true")]
    pub has_uncertainty: bool,
}

/// Definición completa de una práctica: tipo de análisis, magnitudes y mensurandos.
#[derive(Debug, Serialize)]
pub struct PracticeDefinition {
    pub practice_id: String,
    pub analysis_kind: Option<String>,
    /// Solo `regresion_lineal`: expresiones por punto de los ejes `x` e `y` del ajuste.
    pub x_formula: Option<String>,
    pub y_formula: Option<String>,
    pub quantities: Vec<PracticeQuantity>,
    pub results: Vec<PracticeResult>,
    /// Solo `curva`: curvas a graficar sobre el mismo barrido (una o varias, p. ej. en Filtros).
    pub curves: Vec<PracticeCurve>,
    /// Solo estadístico (Motor D): cantidad de operadores que cargan su propia serie. `None` o ≤1
    /// = sin operadores (comportamiento por defecto, una sola serie por magnitud).
    pub operator_count: Option<i64>,
    /// Solo regresión/curva (Motor C): magnitudes intermedias por punto (promedio del derivado por
    /// réplica), disponibles como símbolos en las fórmulas de eje.
    pub intermediates: Vec<PracticeIntermediate>,
    /// Solo `regresion_lineal` (Motor E): magnitudes derivadas por punto, post-ajuste (tabla por
    /// corrida, p. ej. Reynolds).
    pub point_results: Vec<PracticePointResult>,
    /// Solo `regresion_lineal` (Motor F): mensurandos agregados escalares, post-ajuste (un valor,
    /// con acceso a los extremos de cada magnitud por punto: `X_first`/`X_first2`/`X_last`/`X_last2`).
    pub aggregates: Vec<PracticeAggregate>,
}

/// Una curva de una práctica `curva`: un par de fórmulas de eje sobre el barrido común, con eje x
/// logarítmico opcional. `position` ordena las curvas en el gráfico.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticeCurve {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub x_formula: String,
    pub y_formula: String,
    pub x_log: bool,
}

/// Datos para crear o actualizar una curva de una práctica `curva`.
#[derive(Debug, Deserialize)]
pub struct CurveInput {
    pub x_formula: String,
    pub y_formula: String,
    #[serde(default)]
    pub x_log: bool,
}

/// Magnitud intermedia por punto (Motor C) de una práctica de regresión/curva: su `formula` se
/// evalúa por réplica de cada punto y se promedia, quedando disponible como símbolo en los ejes.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticeIntermediate {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Datos para crear o actualizar una magnitud intermedia por punto.
#[derive(Debug, Deserialize)]
pub struct IntermediateInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Magnitud derivada **por punto, post-ajuste** (Motor E) de una práctica `regresion_lineal`: su
/// `formula` se evalúa en cada punto con las magnitudes/intermedias del punto + `slope`/`intercept`
/// + los mensurandos derivados, produciendo una columna por corrida (p. ej. Reynolds).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticePointResult {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Datos para crear o actualizar una magnitud derivada por punto.
#[derive(Debug, Deserialize)]
pub struct PointResultInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
}

/// Mensurando **agregado** escalar (Motor F) de una práctica `regresion_lineal`: su `formula` se
/// evalúa una vez tras el ajuste y puede usar escalares compartidos, `slope`/`intercept`, los
/// mensurandos, los agregados anteriores, y los extremos de cada magnitud por punto (`X_first`,
/// `X_first2`, `X_last`, `X_last2`). Un valor, sin incertidumbre.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PracticeAggregate {
    pub id: String,
    pub practice_id: String,
    pub position: i64,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    /// `true` si es un resultado central que el alumno debe entregar (para comparar contra el
    /// valor automático): habilita el campo en "Mis cálculos" igual que `PracticeResult::is_final`.
    #[serde(default)]
    pub is_final: bool,
}

/// Datos para crear o actualizar un mensurando agregado.
#[derive(Debug, Deserialize)]
pub struct AggregateInput {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    #[serde(default)]
    pub is_final: bool,
}

mod crud;
mod seed;
#[cfg(test)]
mod tests;

pub use crud::*;
pub use seed::seed_definitions;
