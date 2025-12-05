use serde::{Deserialize, Serialize};

/// Tipo general de instrumento físico
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentKind {
    Analogico,
    Digital,
}

/// Categoría más específica (puede servir para la UI o reglas especiales)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentCategory {
    Tester,
    Osciloscopio,
    Balanza,
    Termometro,
    Probeta,
    Generico,
}

/// Magnitud física que mide la escala
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuantityKind {
    Voltaje,
    Corriente,
    Resistencia,
    Temperatura,
    Densidad,
    Masa,
    Volumen,
    Tiempo,
    Capacitancia,
    Inductancia,
    Otra,
}

/// Prefijo SI independiente de la unidad base.
/// El factor se obtiene directamente de un `match`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Prefix {
    None,
    Nano,
    Micro,
    Milli,
    Centi,
    Deci,
    Kilo,
    Mega,
    Giga,
}

impl Prefix {
    pub fn factor(&self) -> f64 {
        match self {
            Prefix::None  => 1.0,
            Prefix::Nano  => 1e-9,
            Prefix::Micro => 1e-6,
            Prefix::Milli => 1e-3,
            Prefix::Centi => 1e-2,
            Prefix::Deci  => 1e-1,
            Prefix::Kilo  => 1e3,
            Prefix::Mega  => 1e6,
            Prefix::Giga  => 1e9,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Prefix::None  => "",
            Prefix::Nano  => "n",
            Prefix::Micro => "µ",
            Prefix::Milli => "m",
            Prefix::Centi => "c",
            Prefix::Deci  => "d",
            Prefix::Kilo  => "k",
            Prefix::Mega  => "M",
            Prefix::Giga  => "G",
        }
    }
}

/// Unidad base (sin prefijo).
/// La "unidad canónica" interna depende de la magnitud:
/// - Voltaje: V
/// - Corriente: A
/// - Resistencia: Ω
/// - Tiempo: s
/// - Temperatura: °C
/// - Masa: kg
/// - Volumen: m³
/// - Densidad: kg/m³
/// - Capacitancia: F
/// - Inductancia: H
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BaseUnit {
    Volt,       // V
    Ohm,        // Ω
    Ampere,     // A
    Second,     // s
    Celsius,    // °C
    Gram,       // g
    Kilogram,   // kg
    Litre,      // L
    CubicMeter, // m³
    KgPerM3,    // kg/m³
    GramPerCm3, // g/cm³
    Farad,      // F
    Henry,      // H
}

impl BaseUnit {
    /// Magnitud asociada a esta unidad base.
    pub fn quantity(&self) -> QuantityKind {
        match self {
            BaseUnit::Volt       => QuantityKind::Voltaje,
            BaseUnit::Ohm        => QuantityKind::Resistencia,
            BaseUnit::Ampere     => QuantityKind::Corriente,
            BaseUnit::Second     => QuantityKind::Tiempo,
            BaseUnit::Celsius    => QuantityKind::Temperatura,
            BaseUnit::Gram
            | BaseUnit::Kilogram => QuantityKind::Masa,
            BaseUnit::Litre
            | BaseUnit::CubicMeter => QuantityKind::Volumen,
            BaseUnit::KgPerM3
            | BaseUnit::GramPerCm3 => QuantityKind::Densidad,
            BaseUnit::Farad      => QuantityKind::Capacitancia,
            BaseUnit::Henry      => QuantityKind::Inductancia,
        }
    }

    /// Factor desde ESTA unidad base a la unidad canónica de su magnitud.
    ///
    /// Definimos:
    /// - Voltaje: canónica = V
    /// - Resistencia: canónica = Ω
    /// - Corriente: canónica = A
    /// - Tiempo: canónica = s
    /// - Temperatura: canónica = °C
    /// - Masa: canónica = kg
    /// - Volumen: canónica = m³
    /// - Densidad: canónica = kg/m³
    /// - Capacitancia: canónica = F
    /// - Inductancia: canónica = H
    pub fn factor_to_canonical(&self) -> f64 {
        match self {
            // ya son canónicas
            BaseUnit::Volt       => 1.0,
            BaseUnit::Ohm        => 1.0,
            BaseUnit::Ampere     => 1.0,
            BaseUnit::Second     => 1.0,
            BaseUnit::Celsius    => 1.0,
            BaseUnit::Kilogram   => 1.0,
            BaseUnit::CubicMeter => 1.0,
            BaseUnit::KgPerM3    => 1.0,
            BaseUnit::Farad      => 1.0,
            BaseUnit::Henry      => 1.0,

            // conversiones:
            // Masa
            BaseUnit::Gram       => 1e-3,   // 1 g = 1e-3 kg

            // Volumen
            BaseUnit::Litre      => 1e-3,   // 1 L = 1e-3 m³

            // Densidad
            BaseUnit::GramPerCm3 => 1000.0, // 1 g/cm³ = 1000 kg/m³
        }
    }

    /// Símbolo de la unidad base (sin prefijo)
    pub fn symbol(&self) -> &'static str {
        match self {
            BaseUnit::Volt       => "V",
            BaseUnit::Ohm        => "Ω",
            BaseUnit::Ampere     => "A",
            BaseUnit::Second     => "s",
            BaseUnit::Celsius    => "°C",
            BaseUnit::Gram       => "g",
            BaseUnit::Kilogram   => "kg",
            BaseUnit::Litre      => "L",
            BaseUnit::CubicMeter => "m³",
            BaseUnit::KgPerM3    => "kg/m³",
            BaseUnit::GramPerCm3 => "g/cm³",
            BaseUnit::Farad      => "F",
            BaseUnit::Henry      => "H",
        }
    }
}

/// Unidad concreta asociada a una magnitud.
/// Se define por:
/// - quantity: tipo de magnitud (Voltaje, Temperatura, etc.)
/// - base: unidad base (V, Ω, °C, g, L, F, H, ...).
/// - prefix: prefijo (None, Nano, Micro, Milli, Kilo, ...).
///
/// El factor total a la unidad canónica es:
/// canonical = value * prefix.factor() * base.factor_to_canonical()
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UnitDef {
    pub quantity: QuantityKind,
    pub base: BaseUnit,
    pub prefix: Prefix,
}

impl UnitDef {
    /// Opcional: verificación simple de consistencia magnitud/unidad
    pub fn is_consistent(&self) -> bool {
        self.quantity == self.base.quantity()
    }

    /// símbolo completo, ej: mV, kΩ, µA, °C (sin prefijo), nF, mH
    pub fn symbol(&self) -> String {
        format!("{}{}", self.prefix.symbol(), self.base.symbol())
    }

    /// factor total → UNIDAD CANÓNICA
    ///
    /// EJEMPLOS:
    /// - mV    → 1e-3 * 1 = 1e-3 V
    /// - kΩ    → 1e3 * 1 = 1e3 Ω
    /// - g/cm³ → 1 * 1000 = 1000 kg/m³
    /// - mg    → 1e-3 (Gram) * 1e-3 (kilo) = 1e-6 kg
    /// - nF    → 1e-9 F
    /// - mH    → 1e-3 H
    pub fn factor_to_canonical(&self) -> f64 {
        self.prefix.factor() * self.base.factor_to_canonical()
    }

    /// Convierte un valor expresado en esta unidad a la unidad canónica.
    pub fn to_canonical(&self, value: f64) -> f64 {
        value * self.factor_to_canonical()
    }

    /// Convierte un valor expresado en la unidad canónica a esta unidad concreta.
    pub fn from_canonical(&self, canonical_value: f64) -> f64 {
        canonical_value / self.factor_to_canonical()
    }
}

/// Modelos de incertidumbre que queremos soportar.
/// Se calculan SIEMPRE en la misma unidad que `value` y la escala
/// (ej: V, mV, kΩ, °C, s, ms, nF, mH, etc.).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum UncertaintyModel {
    /// Instrumento analógico simple:
    /// componentes de lectura y calibración en UNIDADES ABSOLUTAS.
    /// Se combinan en cuadratura: u = sqrt(u_lect^2 + u_cal^2).
    AnalogComponents {
        lectura_abs: Option<f64>,
        calibracion_abs: Option<f64>,
    },

    /// Instrumento digital tipo tester:
    /// ±(percent * |valor| + digits * resolution)
    ///
    /// - percent: fracción (1% -> 0.01)
    /// - digits: cantidad de dígitos (cuentas) a sumar multiplicados por la resolución.
    PercentPlusDigits {
        percent: f64,
        digits: f64,
    },

    /// Osciloscopio (por ej. tensión vertical):
    /// ±(percent * |valor| + vdiv_coeff * VOLTS/DIV + constant)
    ///
    /// - percent: fracción (3% -> 0.03)
    /// - vdiv_coeff: factor multiplicativo del valor VOLTS/DIV
    /// - constant: constante adicional (por ej. 1 mV => 0.001 V si la unidad es V).
    OscilloscopeVoltage {
        percent: f64,
        vdiv_coeff: f64,
        constant: f64,
    },
}

/// Escala de un instrumento.
/// Por ejemplo: "V DC 20 V", "V AC 200 mV", "0–5 A", "0–100 °C", "0–200 g", "0–200 nF", etc.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Scale {
    /// Id (podés usar un String tipo "tester_v_20" o similar)
    pub id: String,
    pub name: String,

    pub quantity: QuantityKind,

    /// Rango mínimo y máximo en la UNIDAD DE LA ESCALA (no en canónica).
    pub min: f64,
    pub max: f64,

    /// Unidad usada en esta escala (incluye base + prefijo).
    pub unit: UnitDef,

    /// Resolución mínima de indicación en la UNIDAD DE LA ESCALA.
    /// Ejemplo: para una escala de 20 V con resolución 0.01 V → resolution = 0.01.
    pub resolution: Option<f64>,

    /// Paso principal de la escala, si aplica.
    /// Por ejemplo, en un osciloscopio: VOLTS/DIV.
    pub main_step: Option<f64>,

    /// Modelo de incertidumbre para esta escala.
    pub uncertainty_model: UncertaintyModel,
}

/// Instrumento completo, con sus escalas.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Instrument {
    pub id: String,
    pub name: String,
    pub kind: InstrumentKind,
    pub category: InstrumentCategory,
    pub description: Option<String>,
    pub scales: Vec<Scale>,
}

impl Instrument {
    /// Busca una escala por id.
    pub fn find_scale(&self, scale_id: &str) -> Option<&Scale> {
        self.scales.iter().find(|s| s.id == scale_id)
    }
}

/// Funciones de ayuda asociadas al modelo de incertidumbre
impl UncertaintyModel {
    /// Calcula la incertidumbre ABSOLUTA (mismo tipo de unidades que `value` y la escala).
    pub fn compute_absolute_uncertainty(&self, value: f64, scale: &Scale) -> Option<f64> {
        match *self {
            UncertaintyModel::AnalogComponents {
                lectura_abs,
                calibracion_abs,
            } => {
                let mut sum_sq = 0.0;
                if let Some(u_lect) = lectura_abs {
                    sum_sq += u_lect * u_lect;
                }
                if let Some(u_cal) = calibracion_abs {
                    sum_sq += u_cal * u_cal;
                }
                if sum_sq == 0.0 {
                    None
                } else {
                    Some(sum_sq.sqrt())
                }
            }

            UncertaintyModel::PercentPlusDigits { percent, digits } => {
                let resolution = scale.resolution?;
                let term_percent = percent * value.abs();
                let term_digits = digits * resolution;
                let u = term_percent + term_digits;
                Some(u)
            }

            UncertaintyModel::OscilloscopeVoltage {
                percent,
                vdiv_coeff,
                constant,
            } => {
                let volts_per_div = scale.main_step?;
                let u = percent * value.abs() + vdiv_coeff * volts_per_div + constant;
                Some(u)
            }
        }
    }

    /// Incertidumbre relativa (u / |value|) si tiene sentido.
    pub fn compute_relative_uncertainty(&self, value: f64, scale: &Scale) -> Option<f64> {
        let u = self.compute_absolute_uncertainty(value, scale)?;
        if value == 0.0 {
            None
        } else {
            Some(u / value.abs())
        }
    }
}
