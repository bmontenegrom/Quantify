//! Motor de incertidumbres de Fisica 103.
//!
//! Funciones puras (sin base de datos) para calcular:
//! - incertidumbre tipo A (estadistica): `u_A = s / sqrt(n)`
//! - incertidumbre tipo B (del instrumento), segun el modelo de la escala
//! - incertidumbre combinada `u_c = sqrt(u_A^2 + u_B^2)` y expandida `U = k * u_c` (k = 2)
//! - propagacion de varianzas para determinaciones indirectas (numerica, diferencias finitas)

use serde::{Deserialize, Serialize};

/// Factor de cobertura para la incertidumbre expandida (95 %, convencion del curso).
pub const EXPANSION_K: f64 = 2.0;

const SQRT_3: f64 = 1.732_050_807_568_877_2;
const SQRT_6: f64 = 2.449_489_742_783_178;

/// Modelo de incertidumbre tipo B de una escala de instrumento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BModel {
    /// Digital simple (resolucion finita): `u_B = step / (2*sqrt(3))`.
    Resolucion,
    /// Analogico (apreciacion del operador): `u_B = appreciation / sqrt(6)`.
    Apreciacion,
    /// Tester u osciloscopio: la hoja/tecnica da U expandida (k=2) como
    /// `U_spec = pct*|valor| + coef*step + fijo`, de donde `u_B = U_spec / 2`.
    Fabricante,
}

/// Especificacion de incertidumbre tipo B de una escala concreta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleSpec {
    pub b_model: BModel,
    /// Resolucion (digital), menor division (analogico) o VOLTS/DIV (osciloscopio).
    pub step: f64,
    /// Apreciacion efectiva del operador (analogico); si es `None` se usa `step`.
    pub appreciation: Option<f64>,
    /// Fabricante: porcentaje del valor leido (p. ej. 3.0 = 3 %).
    pub spec_pct_reading: f64,
    /// Fabricante: coeficiente que multiplica `step` (5 = "5 dgt"; 0.1 osciloscopio).
    pub spec_step_coeff: f64,
    /// Fabricante: termino fijo en unidad base (p. ej. 0.001 V = 1 mV).
    pub spec_fixed: f64,
}

/// Resultado de incertidumbre de una magnitud medida directamente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityResult {
    pub n: usize,
    pub mean: f64,
    /// Desviacion estandar muestral.
    pub s: f64,
    pub u_a: f64,
    pub u_b: f64,
    pub u_c: f64,
    pub u_expanded: f64,
}

/// Incertidumbre tipo A. Devuelve `(media, s_muestral, u_A)`.
/// Con `n < 2` la dispersion no es estimable: `s = 0` y `u_A = 0`.
///
/// # Ejemplos
///
/// ```
/// let (media, s, u_a) = quantify::uncertainty::type_a(&[1.0, 2.0, 3.0]);
/// assert_eq!(media, 2.0);
/// assert_eq!(s, 1.0); // varianza muestral = (1 + 0 + 1) / 2 = 1
/// assert!((u_a - 1.0 / 3.0_f64.sqrt()).abs() < 1e-12);
/// ```
pub fn type_a(values: &[f64]) -> (f64, f64, f64) {
    let n = values.len();
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    if n < 2 {
        return (mean, 0.0, 0.0);
    }
    let sum_sq = values
        .iter()
        .map(|value| {
            let d = value - mean;
            d * d
        })
        .sum::<f64>();
    let s = (sum_sq / (n as f64 - 1.0)).sqrt();
    let u_a = s / (n as f64).sqrt();
    (mean, s, u_a)
}

/// Incertidumbre tipo B de una lectura, evaluada en `value` (relevante para `Fabricante`).
///
/// # Ejemplos
///
/// ```
/// use quantify::uncertainty::{type_b, BModel, ScaleSpec};
/// // Tester ±(1% + 5 dgt) con step = 1, leyendo 100 -> U = 6, u_B = 3.
/// let spec = ScaleSpec {
///     b_model: BModel::Fabricante,
///     step: 1.0,
///     appreciation: None,
///     spec_pct_reading: 1.0,
///     spec_step_coeff: 5.0,
///     spec_fixed: 0.0,
/// };
/// assert!((type_b(&spec, 100.0) - 3.0).abs() < 1e-12);
/// ```
pub fn type_b(spec: &ScaleSpec, value: f64) -> f64 {
    match spec.b_model {
        BModel::Resolucion => spec.step / (2.0 * SQRT_3),
        BModel::Apreciacion => spec.appreciation.unwrap_or(spec.step) / SQRT_6,
        BModel::Fabricante => {
            let u_spec = (spec.spec_pct_reading / 100.0) * value.abs()
                + spec.spec_step_coeff * spec.step
                + spec.spec_fixed;
            u_spec / EXPANSION_K
        }
    }
}

/// Combina tipo A y tipo B en cuadratura.
///
/// # Ejemplos
///
/// ```
/// assert_eq!(quantify::uncertainty::combine(3.0, 4.0), 5.0);
/// ```
pub fn combine(u_a: f64, u_b: f64) -> f64 {
    (u_a * u_a + u_b * u_b).sqrt()
}

/// Incertidumbre expandida `U = k * u`.
///
/// # Ejemplos
///
/// ```
/// assert_eq!(quantify::uncertainty::expand(2.5, 2.0), 5.0);
/// ```
pub fn expand(u: f64, k: f64) -> f64 {
    k * u
}

/// Calcula el resultado completo de una magnitud medida con `values` replicas
/// usando la escala `spec` (si hay instrumento asociado). La tipo B se evalua en la media.
///
/// # Ejemplos
///
/// ```
/// // Sin instrumento (sin tipo B): u_c == u_A y U = 2 * u_A.
/// let r = quantify::uncertainty::measured_quantity(&[1.0, 2.0, 3.0], None);
/// assert_eq!(r.n, 3);
/// assert_eq!(r.mean, 2.0);
/// assert_eq!(r.u_b, 0.0);
/// assert!((r.u_expanded - 2.0 / 3.0_f64.sqrt()).abs() < 1e-12);
/// ```
pub fn measured_quantity(values: &[f64], spec: Option<&ScaleSpec>) -> QuantityResult {
    let (mean, s, u_a) = type_a(values);
    let u_b = spec.map(|sp| type_b(sp, mean)).unwrap_or(0.0);
    let u_c = combine(u_a, u_b);
    QuantityResult {
        n: values.len(),
        mean,
        s,
        u_a,
        u_b,
        u_c,
        u_expanded: expand(u_c, EXPANSION_K),
    }
}

/// Propagacion de varianzas para `Q = f(x_1, ..., x_n)` por diferencias finitas
/// centradas. `means` son los valores medios de las variables de entrada y `us`
/// sus incertidumbres estandar. Devuelve `(valor, u_Q)`.
///
/// `u_Q^2 = sum_i (df/dx_i)^2 * u_i^2`, con `df/dx_i ~= (f(x_i+h) - f(x_i-h)) / (2h)`.
///
/// # Ejemplos
///
/// ```
/// // Q = l * a ; con incertidumbres nulas, u_Q = 0 y el valor es el producto.
/// let (valor, u_q) = quantify::uncertainty::propagate(|x| x[0] * x[1], &[2.0, 3.0], &[0.0, 0.0]);
/// assert_eq!(valor, 6.0);
/// assert!(u_q.abs() < 1e-9);
/// ```
pub fn propagate<F>(f: F, means: &[f64], us: &[f64]) -> (f64, f64)
where
    F: Fn(&[f64]) -> f64,
{
    let value = f(means);
    let mut variance = 0.0;
    let mut x = means.to_vec();
    for i in 0..means.len() {
        // Paso relativo al valor: preciso también para magnitudes muy chicas (p. ej.
        // capacitancias ~1e-8). Un piso absoluto grande perturbaría tanto que el punto
        // evaluado podría cambiar de signo. Para x = 0 se usa un paso absoluto pequeño.
        let h = if means[i] == 0.0 {
            1e-6
        } else {
            means[i].abs() * 1e-6
        };
        let original = x[i];
        x[i] = original + h;
        let f_plus = f(&x);
        x[i] = original - h;
        let f_minus = f(&x);
        x[i] = original;
        let derivative = (f_plus - f_minus) / (2.0 * h);
        variance += (derivative * us[i]).powi(2);
    }
    (value, variance.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn type_a_sample_std_and_mean() {
        let (mean, s, u_a) = type_a(&[1.0, 2.0, 3.0]);
        assert!(close(mean, 2.0, 1e-12));
        assert!(close(s, 1.0, 1e-12)); // var muestral = (1+0+1)/2 = 1
        assert!(close(u_a, 1.0 / 3.0_f64.sqrt(), 1e-12));
    }

    #[test]
    fn type_a_single_value_has_no_dispersion() {
        let (mean, s, u_a) = type_a(&[5.0]);
        assert!(close(mean, 5.0, 1e-12));
        assert_eq!(s, 0.0);
        assert_eq!(u_a, 0.0);
    }

    #[test]
    fn type_b_resolucion_rectangular() {
        let spec = ScaleSpec {
            b_model: BModel::Resolucion,
            step: 0.01,
            appreciation: None,
            spec_pct_reading: 0.0,
            spec_step_coeff: 0.0,
            spec_fixed: 0.0,
        };
        assert!(close(type_b(&spec, 1.23), 0.01 / (2.0 * SQRT_3), 1e-15));
    }

    #[test]
    fn type_b_apreciacion_triangular() {
        let spec = ScaleSpec {
            b_model: BModel::Apreciacion,
            step: 1.0,
            appreciation: Some(1.0),
            spec_pct_reading: 0.0,
            spec_step_coeff: 0.0,
            spec_fixed: 0.0,
        };
        assert!(close(type_b(&spec, 10.0), 1.0 / SQRT_6, 1e-15));
    }

    #[test]
    fn type_b_fabricante_tester() {
        // pct=1.0 %, coef=5 dgt, step=1.0, fijo=0, valor=100 -> U=1+5=6 -> u_B=3
        let spec = ScaleSpec {
            b_model: BModel::Fabricante,
            step: 1.0,
            appreciation: None,
            spec_pct_reading: 1.0,
            spec_step_coeff: 5.0,
            spec_fixed: 0.0,
        };
        assert!(close(type_b(&spec, 100.0), 3.0, 1e-12));
    }

    #[test]
    fn type_b_fabricante_osciloscopio() {
        // GDS-1052-U: 3% + 0.1*(VOLTS/DIV) + 1 mV; V/div=2, valor=5
        // U = 0.03*5 + 0.1*2 + 0.001 = 0.15 + 0.2 + 0.001 = 0.351 -> u_B = 0.1755
        let spec = ScaleSpec {
            b_model: BModel::Fabricante,
            step: 2.0,
            appreciation: None,
            spec_pct_reading: 3.0,
            spec_step_coeff: 0.1,
            spec_fixed: 0.001,
        };
        assert!(close(type_b(&spec, 5.0), 0.1755, 1e-12));
    }

    #[test]
    fn combine_and_expand() {
        assert!(close(combine(3.0, 4.0), 5.0, 1e-12));
        assert!(close(expand(2.5, EXPANSION_K), 5.0, 1e-12));
    }

    #[test]
    fn propagate_matches_analytic_for_product_sum() {
        // Q = l*a + l*b ; dQ/dl = a+b, dQ/da = l, dQ/db = l
        let f = |x: &[f64]| x[0] * x[1] + x[0] * x[2];
        let means = [2.0, 3.0, 4.0];
        let us = [0.1, 0.2, 0.2];
        let (value, u_q) = propagate(f, &means, &us);
        assert!(close(value, 14.0, 1e-9));
        // u_Q^2 = 7^2*0.01 + 4*0.04 + 4*0.04 = 0.49+0.16+0.16 = 0.81 -> 0.9
        assert!(close(u_q, 0.9, 1e-6));
    }

    #[test]
    fn propagate_is_accurate_for_tiny_nonlinear_magnitudes() {
        // f = 1/x con x = 1e-8 (escala de una capacitancia). df/dx = -1/x^2 = -1e16.
        // u_q = |df/dx| * u_x = 1e16 * 1e-10 = 1e6.
        // Con un paso absoluto (piso 1.0) el punto evaluado cambiaba de signo y daba basura;
        // con paso relativo la propagacion es correcta.
        let (value, u_q) = propagate(|x| 1.0 / x[0], &[1e-8], &[1e-10]);
        assert!(close(value, 1e8, 1.0));
        assert!((u_q - 1e6).abs() / 1e6 < 1e-3);
    }

    #[test]
    fn measured_quantity_combines_a_and_b() {
        let spec = ScaleSpec {
            b_model: BModel::Resolucion,
            step: 0.0,
            appreciation: None,
            spec_pct_reading: 0.0,
            spec_step_coeff: 0.0,
            spec_fixed: 0.0,
        };
        // Sin tipo B (step=0): u_c == u_A, U = 2*u_A
        let r = measured_quantity(&[1.0, 2.0, 3.0], Some(&spec));
        assert!(close(r.u_b, 0.0, 1e-15));
        assert!(close(r.u_c, 1.0 / 3.0_f64.sqrt(), 1e-12));
        assert!(close(r.u_expanded, 2.0 / 3.0_f64.sqrt(), 1e-12));
    }
}
