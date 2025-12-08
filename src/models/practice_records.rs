use serde::{Deserialize, Serialize};

use crate::models::instrument::{Instrument, Measurement};

/// Tipo de práctica de laboratorio
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PracticeKind {
    PenduloSimple,
    Generica,
    // más adelante: Osciloscopio, DensidadLiquido, etc.
}

/// Resultado del test de normalidad (ej. χ²) para los períodos
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NormalityTestResult {
    /// Método usado (por ejemplo: "chi_square", "ks", etc.)
    pub method: String,

    /// ¿Se acepta la hipótesis de normalidad?
    pub passed: bool,

    /// p-value si lo calculás (opcional)
    pub p_value: Option<f64>,

    /// Detalles/texto explicativo (opcional)
    pub details: Option<String>,
}

/// Estadísticos específicos de una serie de datos del péndulo
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendulumSeriesStats {
    /// Número total de mediciones de tiempo registradas (antes de combinarlas en períodos)
    pub n_raw_times: usize,

    /// Número de períodos efectivos usados (luego de descartar, etc.)
    pub n_periods_used: usize,

    /// Período medio
    pub mean_period: f64,

    /// Desviación estándar de los períodos
    pub sigma: f64,

    /// Desviación estándar de la media (Sn)
    pub sn: f64,

    /// Resultado del test de normalidad, si se pudo aplicar
    pub normality: Option<NormalityTestResult>,
}

/// Una serie de datos dentro de la práctica del péndulo.
/// Por ejemplo: "Serie 1 (L ≈ 0.8 m)", "Serie 2 (L ≈ 1.0 m)", etc.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendulumSeriesData {
    /// Etiqueta legible de la serie (para mostrar en UI y en el JSON)
    pub label: String,

    /// Medidas registradas para esta serie (períodos o tiempos ya procesados),
    /// usando tu tipo Measurement (value + unidad + incertidumbres registradas por el alumno).
    pub measurements: Vec<Measurement>,

    /// Estadísticos para ESTA serie
    pub stats: PendulumSeriesStats,
}

/// Datos de una corrida de la práctica del péndulo.
///
/// - Puede haber más de una serie de datos (`series: Vec<PendulumSeriesData>`).
/// - Puede haber más de un instrumento utilizado (`instruments: Vec<Instrument>`).
/// - No se exporta ningún "valor correcto" de incertidumbre: solo modelos
///   en los instrumentos y las incertidumbres que grabó el alumno en las medidas.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendulumPracticeData {
    /// Identificador de la práctica (ej. "pend_2025_grupo_3")
    pub id: String,

    /// Título o nombre legible
    pub title: String,

    /// Tipo de práctica (acá será siempre `PenduloSimple`)
    pub kind: PracticeKind,

    /// Fecha de creación/exportación (ISO8601, ej. "2025-12-08T14:30:00Z")
    pub created_at_utc: String,

    /// Notas generales (por el docente o por el grupo)
    pub notes: Option<String>,

    /// Instrumentos utilizados en la práctica.
    pub instruments: Vec<Instrument>,

    /// Todas las series de datos de la práctica del péndulo.
    pub series: Vec<PendulumSeriesData>,
}

/// Datos de una práctica genérica (sin análisis estadístico específico)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenericPracticeData {
    pub id: String,
    pub title: String,
    pub kind: PracticeKind,     // aquí será Generica
    pub created_at_utc: String,
    pub notes: Option<String>,

    /// Instrumentos utilizados (0, 1 o varios).
    pub instruments: Vec<Instrument>,

    /// Lista de medidas asociadas a esta práctica.
    pub measurements: Vec<Measurement>,
}

/// Registro de práctica exportable a JSON.
/// Se usa un enum etiquetado para que en el JSON aparezca un campo `practice_type`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "practice_type", rename_all = "snake_case")]
pub enum PracticeRecord {
    PenduloSimple(PendulumPracticeData),
    Generica(GenericPracticeData),
    // más adelante otras variantes específicas
}

/// Serializa una práctica a un String JSON "lindo" (pretty-print)
pub fn practice_to_json(precord: &PracticeRecord) -> Result<String, String> {
    serde_json::to_string_pretty(precord)
        .map_err(|e| format!("Error serializando práctica a JSON: {}", e))
}
