//! Módulo de regresión lineal no ponderada (mínimos cuadrados).
//!
//! Ajusta una recta y = a x + b a un conjunto de puntos (xᵢ, yᵢ).
//!
//! Devuelve:
//! - pendiente a
//! - ordenada al origen b
//! - incertidumbre en la pendiente σ_a
//! - incertidumbre en la ordenada σ_b
//! - coeficiente de determinación R²
//!
//! Fórmulas estándar de mínimos cuadrados:
//!   Δ = n Σx² − (Σx)²
//!   a = (n Σxy − Σx Σy) / Δ
//!   b = (Σy Σx² − Σx Σxy) / Δ
//!   σ² = Σ(y − y_fit)² / (n − 2)
//!   σ_a² = n σ² / Δ
//!   σ_b² = σ² Σx² / Δ
//!   R² = 1 − SS_res / SS_tot

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearRegressionResult {
    /// Cantidad de puntos usados en el ajuste
    pub n: usize,
    /// Pendiente a
    pub slope: f64,
    /// Ordenada al origen b
    pub intercept: f64,
    /// Incertidumbre (error estándar) en la pendiente σ_a
    pub slope_err: f64,
    /// Incertidumbre (error estándar) en la ordenada σ_b
    pub intercept_err: f64,
    /// Coeficiente de determinación R²
    pub r2: f64,
    /// Media de x
    pub x_mean: f64,
    /// Media de y
    pub y_mean: f64,
}

impl LinearRegressionResult {
    /// Evalúa la recta ajustada en un valor de x
    pub fn predict(&self, x: f64) -> f64 {
        self.slope * x + self.intercept
    }
}

/// Regresión lineal NO ponderada (mínimos cuadrados).
///
/// xs y ys deben tener la misma longitud.
/// Devuelve None si:
/// - hay menos de 2 puntos
/// - o el denominador del ajuste se hace ~0 (todos los x casi iguales).
pub fn linear_regression(xs: &[f64], ys: &[f64]) -> Option<LinearRegressionResult> {
    let n = xs.len();
    if n < 2 || ys.len() != n {
        return None;
    }

    // Sumas necesarias
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xx = 0.0;
    let mut sum_xy = 0.0;

    for (&x, &y) in xs.iter().zip(ys.iter()) {
        sum_x += x;
        sum_y += y;
        sum_xx += x * x;
        sum_xy += x * y;
    }

    let n_f = n as f64;
    let delta = n_f * sum_xx - sum_x * sum_x;

    // Si Δ ≈ 0, no se puede ajustar (x todos iguales o casi)
    if delta.abs() < 1e-20 {
        return None;
    }

    // Pendiente y ordenada
    let slope = (n_f * sum_xy - sum_x * sum_y) / delta;
    let intercept = (sum_y * sum_xx - sum_x * sum_xy) / delta;

    // Estadísticas adicionales
    let x_mean = sum_x / n_f;
    let y_mean = sum_y / n_f;

    // Sumas de cuadrados
    let mut ss_res = 0.0; // suma de cuadrados de residuos
    let mut ss_tot = 0.0; // suma total de cuadrados (respecto a ȳ)

    for (&x, &y) in xs.iter().zip(ys.iter()) {
        let y_fit = slope * x + intercept;
        let resid = y - y_fit;
        ss_res += resid * resid;

        let dy = y - y_mean;
        ss_tot += dy * dy;
    }

    // Coeficiente de determinación
    let r2 = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    // Varianza residual y errores en a y b (n > 2)
    let (slope_err, intercept_err) = if n > 2 {
        let sigma2 = ss_res / (n_f - 2.0); // estimador de la varianza de los residuos
        let slope_err = (n_f * sigma2 / delta).sqrt();
        let intercept_err = (sigma2 * sum_xx / delta).sqrt();
        (slope_err, intercept_err)
    } else {
        // Con solo 2 puntos, la recta pasa exactamente por ellos → ss_res = 0
        // pero las fórmulas de error no tienen sentido (n−2 = 0), devolvemos 0.
        (0.0, 0.0)
    };

    Some(LinearRegressionResult {
        n,
        slope,
        intercept,
        slope_err,
        intercept_err,
        r2,
        x_mean,
        y_mean,
    })
}

/// Calcula los residuos yᵢ − y_fitᵢ dados xs, ys y un resultado de regresión.
pub fn residuals(result: &LinearRegressionResult, xs: &[f64], ys: &[f64]) -> Option<Vec<f64>> {
    if xs.len() != ys.len() {
        return None;
    }
    let res: Vec<f64> = xs
        .iter()
        .zip(ys.iter())
        .map(|(&x, &y)| y - result.predict(x))
        .collect();
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_line_exact() {
        // y = 2 x + 1 exacta
        let xs = [0.0, 1.0, 2.0, 3.0];
        let ys = [1.0, 3.0, 5.0, 7.0];

        let res = linear_regression(&xs, &ys).unwrap();

        assert!((res.slope - 2.0).abs() < 1e-10);
        assert!((res.intercept - 1.0).abs() < 1e-10);
        assert!(res.r2 > 0.999999);
        assert_eq!(res.n, 4);
    }

    #[test]
    fn test_constant_y() {
        // y = 5 (R^2 queda 0 porque ss_tot = 0, definimos r2 = 0 en ese caso)
        let xs = [0.0, 1.0, 2.0, 3.0];
        let ys = [5.0, 5.0, 5.0, 5.0];

        let res = linear_regression(&xs, &ys).unwrap();

        assert!(res.r2 >= 0.0);
        assert_eq!(res.n, 4);
    }
}
