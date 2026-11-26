use serde::{Deserialize, Serialize};

pub const EXPANSION_K: f64 = 2.0;

const SQRT_3: f64 = 1.732_050_807_568_877_2;
const SQRT_6: f64 = 2.449_489_742_783_178;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BModel {
    Resolucion,
    Apreciacion,
    Fabricante,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleSpec {
    pub b_model: BModel,
    pub step: f64,
    pub appreciation: Option<f64>,
    pub spec_pct_reading: f64,
    pub spec_step_coeff: f64,
    pub spec_fixed: f64,
    #[serde(default)]
    pub u_cal_pct: f64,
    #[serde(default)]
    pub u_cal_fixed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantityResult {
    pub n: usize,
    pub mean: f64,
    pub s: f64,
    pub u_a: f64,
    pub u_b: f64,
    pub u_c: f64,
    pub u_expanded: f64,
}

/// Incertidumbre tipo A. Devuelve `(media, s_muestral, u_A)`.
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
///     u_cal_pct: 0.0,
///     u_cal_fixed: 0.0,
/// };
/// assert!((type_b(&spec, 100.0) - 3.0).abs() < 1e-12);
///
/// // Balanza de Hidrostatica: truncamiento R = 0.1 g en cuadratura con 3 % de calibracion.
/// let balanza = ScaleSpec {
///     b_model: BModel::Resolucion,
///     step: 0.1,
///     appreciation: None,
///     spec_pct_reading: 0.0,
///     spec_step_coeff: 0.0,
///     spec_fixed: 0.0,
///     u_cal_pct: 3.0,
///     u_cal_fixed: 0.0,
/// };
/// let base = 0.1 / (2.0 * 3.0_f64.sqrt());
/// let esperado = (base * base + (0.03 * 10.0) * (0.03 * 10.0)).sqrt();
/// assert!((type_b(&balanza, 10.0) - esperado).abs() < 1e-12);
/// ```
pub fn type_b(spec: &ScaleSpec, value: f64) -> f64 {
    let base = match spec.b_model {
        BModel::Resolucion => spec.step / (2.0 * SQRT_3),
        BModel::Apreciacion => spec.appreciation.unwrap_or(spec.step) / SQRT_6,
        BModel::Fabricante => {
            let u_spec = (spec.spec_pct_reading / 100.0) * value.abs()
                + spec.spec_step_coeff * spec.step
                + spec.spec_fixed;
            u_spec / EXPANSION_K
        }
    };
    let u_cal = (spec.u_cal_pct / 100.0) * value.abs();
    (base * base + u_cal * u_cal + spec.u_cal_fixed * spec.u_cal_fixed).sqrt()
}

/// Combina tipo A y tipo B en cuadratura.
///
/// ```
/// assert_eq!(quantify::uncertainty::combine(3.0, 4.0), 5.0);
/// ```
pub fn combine(u_a: f64, u_b: f64) -> f64 {
    (u_a * u_a + u_b * u_b).sqrt()
}

/// Incertidumbre expandida `U = k * u`.
///
/// ```
/// assert_eq!(quantify::uncertainty::expand(2.5, 2.0), 5.0);
/// ```
pub fn expand(u: f64, k: f64) -> f64 {
    k * u
}

/// Calcula el resultado completo de una magnitud medida con `values` replicas
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
    measured_with(values, |mean| {
        spec.map(|sp| type_b(sp, mean)).unwrap_or(0.0)
    })
}

/// Como [`measured_quantity`], con el tipo B calculado por `u_b` a partir de la media.
///
/// ```
/// let r = quantify::uncertainty::measured_with(&[2.0, 2.0], |media| media / 100.0);
/// assert_eq!((r.mean, r.u_b), (2.0, 0.02));
/// ```
pub fn measured_with(values: &[f64], u_b: impl Fn(f64) -> f64) -> QuantityResult {
    let (mean, s, u_a) = type_a(values);
    let u_b = u_b(mean);
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

/// Resultado de una magnitud dada por la cátedra como `valor ± U` (incertidumbre expandida).
///
/// ```
/// use quantify::uncertainty::{measured_given, EXPANSION_K};
/// let r = measured_given(10.0, 2.0);
/// assert_eq!(r.mean, 10.0);
/// assert_eq!(r.u_expanded, 2.0);
/// assert!((r.u_c - 1.0).abs() < 1e-12); // u_c = U/k = 2/2
/// assert_eq!(r.u_a, 0.0);
/// ```
pub fn measured_given(value: f64, u_expanded: f64) -> QuantityResult {
    let u_c = u_expanded / EXPANSION_K;
    QuantityResult {
        n: 1,
        mean: value,
        s: 0.0,
        u_a: 0.0,
        u_b: u_c,
        u_c,
        u_expanded,
    }
}

/// Propagacion de varianzas para `Q = f(x_1, ..., x_n)` por diferencias finitas
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
    propagate_correlated(f, means, us, &[])
}

/// Como [`propagate`], con fuentes compartidas: cada factor es una fuente independiente de
/// desviación 1 que mueve varias entradas a la vez, `x_i += carga_i·z`. Suma el cuadrado de la
/// derivada direccional de cada factor: sin restas entre términos, no pierde lo que queda cuando
/// las correlaciones casi se cancelan.
///
/// ```
/// // x e y comparten una fuente de u = 1: x - y no la ve, x + y la ve dos veces.
/// use quantify::uncertainty::propagate_correlated;
/// let shared = [vec![(0, 1.0), (1, 1.0)]];
/// let (_, u_diff) = propagate_correlated(|x| x[0] - x[1], &[3.0, 1.0], &[0.0, 0.0], &shared);
/// let (_, u_sum) = propagate_correlated(|x| x[0] + x[1], &[3.0, 1.0], &[0.0, 0.0], &shared);
/// assert!(u_diff.abs() < 1e-9);
/// assert!((u_sum - 2.0).abs() < 1e-6);
/// ```
pub fn propagate_correlated<F>(
    f: F,
    means: &[f64],
    us: &[f64],
    factors: &[Vec<(usize, f64)>],
) -> (f64, f64)
where
    F: Fn(&[f64]) -> f64,
{
    let value = f(means);
    let mut variance = 0.0;
    let reach = |i: usize, scale: f64| 1e-3 * means[i].abs().max(scale.abs());
    for (i, &u) in us.iter().enumerate() {
        variance += (central(&f, means, &[(i, 1.0)], 0.0, reach(i, u)) * u).powi(2);
    }
    for factor in factors {
        let loads: Vec<(usize, f64)> = factor.iter().copied().filter(|&(_, l)| l != 0.0).collect();
        if loads.is_empty() {
            continue;
        }
        let rounding = loads
            .iter()
            .map(|&(i, l)| {
                let partial = central(&f, means, &[(i, 1.0)], 0.0, reach(i, l));
                partial.abs() * f64::EPSILON * means[i].abs()
            })
            .sum::<f64>();
        let max_step = loads
            .iter()
            .map(|&(i, l)| reach(i, l) / l.abs())
            .fold(f64::INFINITY, f64::min);
        variance += central(&f, means, &loads, rounding, max_step).powi(2);
    }
    (value, variance.sqrt())
}

fn central<F>(
    f: &F,
    means: &[f64],
    loads: &[(usize, f64)],
    input_rounding: f64,
    max_step: f64,
) -> f64
where
    F: Fn(&[f64]) -> f64,
{
    let start = loads
        .iter()
        .filter(|&&(i, _)| means[i] != 0.0)
        .map(|&(i, l)| 1e-6 * means[i].abs() / l.abs())
        .reduce(f64::min)
        .unwrap_or(1e-6);
    let shifted = |t: f64| {
        let mut x = means.to_vec();
        for &(i, l) in loads {
            x[i] += t * l;
        }
        f(&x)
    };
    let derivative = |t: f64| {
        let (plus, minus) = (shifted(t), shifted(-t));
        let noise = (f64::EPSILON * (plus.abs() + minus.abs()) / 2.0 + input_rounding) / t;
        ((plus - minus) / (2.0 * t), noise)
    };
    let (first, _) = derivative(start);
    let mut t = start;
    while t <= max_step.max(start) {
        let (d, noise) = derivative(t);
        if d.is_finite() && noise <= 1e-3 * d.abs() {
            let (d_wider, _) = derivative(2.0 * t);
            if (d_wider - d).abs() <= 1e-2 * d.abs() {
                return d;
            }
        }
        t *= 10.0;
    }
    first
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
        assert!(close(s, 1.0, 1e-12));
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
            u_cal_pct: 0.0,
            u_cal_fixed: 0.0,
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
            u_cal_pct: 0.0,
            u_cal_fixed: 0.0,
        };
        assert!(close(type_b(&spec, 10.0), 1.0 / SQRT_6, 1e-15));
    }

    #[test]
    fn type_b_fabricante_tester() {
        let spec = ScaleSpec {
            b_model: BModel::Fabricante,
            step: 1.0,
            appreciation: None,
            spec_pct_reading: 1.0,
            spec_step_coeff: 5.0,
            spec_fixed: 0.0,
            u_cal_pct: 0.0,
            u_cal_fixed: 0.0,
        };
        assert!(close(type_b(&spec, 100.0), 3.0, 1e-12));
    }

    #[test]
    fn type_b_fabricante_osciloscopio() {
        let spec = ScaleSpec {
            b_model: BModel::Fabricante,
            step: 2.0,
            appreciation: None,
            spec_pct_reading: 3.0,
            spec_step_coeff: 0.1,
            spec_fixed: 0.001,
            u_cal_pct: 0.0,
            u_cal_fixed: 0.0,
        };
        assert!(close(type_b(&spec, 5.0), 0.1755, 1e-12));
    }

    #[test]
    fn type_b_calibracion_se_suma_en_cuadratura() {
        let spec = ScaleSpec {
            b_model: BModel::Apreciacion,
            step: 0.001,
            appreciation: Some(0.001),
            spec_pct_reading: 0.0,
            spec_step_coeff: 0.0,
            spec_fixed: 0.0,
            u_cal_pct: 0.0,
            u_cal_fixed: 0.001,
        };
        let base = 0.001 / SQRT_6;
        let esperado = (base * base + 0.001 * 0.001_f64).sqrt();
        assert!(close(type_b(&spec, 0.998), esperado, 1e-15));
        assert!(esperado > base && esperado > 0.001);
        assert!(esperado < base + 0.001);

        let brazos = ScaleSpec {
            step: 0.0,
            appreciation: None,
            b_model: BModel::Resolucion,
            u_cal_fixed: 0.0005,
            ..spec.clone()
        };
        assert!(close(type_b(&brazos, 0.09), 0.0005, 1e-15));

        let sin_cal = ScaleSpec {
            u_cal_pct: 0.0,
            u_cal_fixed: 0.0,
            ..spec
        };
        assert!(close(type_b(&sin_cal, 0.998), base, 1e-15));
    }

    #[test]
    fn combine_and_expand() {
        assert!(close(combine(3.0, 4.0), 5.0, 1e-12));
        assert!(close(expand(2.5, EXPANSION_K), 5.0, 1e-12));
    }

    #[test]
    fn propagate_matches_analytic_for_product_sum() {
        let f = |x: &[f64]| x[0] * x[1] + x[0] * x[2];
        let means = [2.0, 3.0, 4.0];
        let us = [0.1, 0.2, 0.2];
        let (value, u_q) = propagate(f, &means, &us);
        assert!(close(value, 14.0, 1e-9));
        assert!(close(u_q, 0.9, 1e-6));
    }

    #[test]
    fn propagate_handles_a_mean_that_is_almost_zero() {
        let (_, u_q) = propagate(|x| x[0] + x[1], &[1e-17, 1.0], &[0.3, 0.4]);
        assert!(close(u_q, 0.5, 5e-4), "{u_q}");
    }

    #[test]
    fn propagate_stays_inside_the_domain_when_u_exceeds_the_mean() {
        let (_, u_q) = propagate(|x| x[0].sqrt(), &[1e-8], &[0.1]);
        assert!((u_q - 500.0).abs() / 500.0 < 1e-3, "{u_q}");
    }

    #[test]
    fn curvature_far_away_is_not_taken_for_a_slope() {
        let (_, u_q) = propagate(|x| 1e12 + (x[0] - 1.0).powi(3), &[1.0], &[0.01]);
        assert_eq!(u_q, 0.0);
    }

    #[test]
    fn a_zero_mean_with_a_tiny_u_still_moves_the_output() {
        let (_, u_q) = propagate(|x| 1.0 + x[0], &[0.0], &[1e-12]);
        assert!((u_q - 1e-12).abs() / 1e-12 < 1e-6, "{u_q}");
    }

    #[test]
    fn every_loaded_input_moves_with_its_factor() {
        let factors = [vec![(0, 0.1), (1, -0.3)]];
        let (_, u_q) = propagate_correlated(
            |x| (x[0] - 1.0) + x[1],
            &[1.0, 1e-17],
            &[0.0, 0.0],
            &factors,
        );
        assert!((u_q - 0.2).abs() / 0.2 < 1e-3, "{u_q}");
    }

    #[test]
    fn a_large_u_does_not_step_across_a_singularity() {
        let (_, u_q) = propagate(|x| 1.0 / x[0], &[1e-8], &[0.1]);
        assert!((u_q - 1e15).abs() / 1e15 < 1e-6, "{u_q}");
    }

    #[test]
    fn a_tiny_loading_does_not_stretch_the_step_of_the_others() {
        let f = |x: &[f64]| (2.0 - x[1]) / x[0];
        let (m, b) = (0.997, 1.004);
        let factors = [vec![(0, 0.005), (1, -1e-12)]];
        let (_, u_q) = propagate_correlated(f, &[m, b], &[0.0, 0.0], &factors);
        let analytic = (2.0 - b) / (m * m) * 0.005;
        assert!(
            (u_q - analytic).abs() / analytic < 1e-6,
            "{u_q} vs {analytic}"
        );
    }

    #[test]
    fn propagate_is_accurate_for_tiny_nonlinear_magnitudes() {
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
            u_cal_pct: 0.0,
            u_cal_fixed: 0.0,
        };
        let r = measured_quantity(&[1.0, 2.0, 3.0], Some(&spec));
        assert!(close(r.u_b, 0.0, 1e-15));
        assert!(close(r.u_c, 1.0 / 3.0_f64.sqrt(), 1e-12));
        assert!(close(r.u_expanded, 2.0 / 3.0_f64.sqrt(), 1e-12));
    }
}
