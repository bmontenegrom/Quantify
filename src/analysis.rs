use anyhow::{anyhow, Context};
use csv::StringRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub name: String,
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
    #[serde(default)]
    pub std_dev_sample: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearRegression {
    pub x_column: String,
    pub y_column: String,
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    #[serde(default)]
    pub u_slope: f64,
    #[serde(default)]
    pub u_intercept: f64,
    #[serde(default)]
    pub mean_x: f64,
    #[serde(default)]
    pub u_mean_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub row_count: usize,
    pub numeric_columns: Vec<ColumnStats>,
    pub regression: Option<LinearRegression>,
    pub warnings: Vec<String>,
}

/// Analiza un CSV con encabezados: calcula estadísticos por columna numérica, intenta una
///
/// ```
/// let analisis = quantify::analysis::analyze_csv("x,y\n1,2\n2,4\n3,6\n").unwrap();
/// assert_eq!(analisis.row_count, 3);
/// assert_eq!(analisis.numeric_columns.len(), 2);
/// let regresion = analisis.regression.unwrap();
/// assert!((regresion.slope - 2.0).abs() < 1e-9);
/// ```
pub fn analyze_csv(input: &str) -> anyhow::Result<AnalysisResult> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(input.as_bytes());
    let headers = reader
        .headers()
        .context("el CSV no tiene encabezados")?
        .clone();

    if headers.is_empty() {
        return Err(anyhow!("el CSV debe tener al menos una columna"));
    }

    let mut columns: Vec<Vec<f64>> = headers.iter().map(|_| Vec::new()).collect();
    let mut warnings = Vec::new();
    let mut row_count = 0usize;

    for result in reader.records() {
        let record = result.context("fila de CSV invalida")?;
        row_count += 1;
        parse_record(&headers, &record, row_count, &mut columns, &mut warnings);
    }

    if row_count == 0 {
        return Err(anyhow!("el CSV debe tener al menos una fila de datos"));
    }

    let numeric_columns: Vec<ColumnStats> = headers
        .iter()
        .enumerate()
        .filter_map(|(idx, name)| stats_for_column(name, &columns[idx]))
        .collect();

    let regression = first_regression(&headers, &columns);

    Ok(AnalysisResult {
        row_count,
        numeric_columns,
        regression,
        warnings,
    })
}

fn parse_record(
    headers: &StringRecord,
    record: &StringRecord,
    row_number: usize,
    columns: &mut [Vec<f64>],
    warnings: &mut Vec<String>,
) {
    for (idx, header) in headers.iter().enumerate() {
        let value = record.get(idx).unwrap_or_default().trim();
        if value.is_empty() {
            warnings.push(format!("Fila {row_number}: '{header}' esta vacio"));
            continue;
        }

        match value.replace(',', ".").parse::<f64>() {
            Ok(parsed) if parsed.is_finite() => columns[idx].push(parsed),
            _ => warnings.push(format!(
                "Fila {row_number}: '{header}' no es numerico ({value})"
            )),
        }
    }
}

fn stats_for_column(name: &str, values: &[f64]) -> Option<ColumnStats> {
    if values.is_empty() {
        return None;
    }

    let count = values.len();
    let mean = values.iter().sum::<f64>() / count as f64;
    let sum_sq = values
        .iter()
        .map(|value| {
            let diff = value - mean;
            diff * diff
        })
        .sum::<f64>();
    let variance = sum_sq / count as f64;
    let std_dev_sample = if count > 1 {
        (sum_sq / (count as f64 - 1.0)).sqrt()
    } else {
        0.0
    };
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    Some(ColumnStats {
        name: name.to_string(),
        count,
        mean,
        std_dev: variance.sqrt(),
        std_dev_sample,
        min,
        max,
    })
}

fn first_regression(headers: &StringRecord, columns: &[Vec<f64>]) -> Option<LinearRegression> {
    let numeric_indices: Vec<_> = columns
        .iter()
        .enumerate()
        .filter(|(_, values)| values.len() >= 2)
        .map(|(idx, _)| idx)
        .collect();

    let x_idx = *numeric_indices.first()?;
    let y_idx = *numeric_indices.get(1)?;
    let paired: Vec<(f64, f64)> = columns[x_idx]
        .iter()
        .copied()
        .zip(columns[y_idx].iter().copied())
        .collect();

    linear_regression(
        headers.get(x_idx).unwrap_or("x"),
        headers.get(y_idx).unwrap_or("y"),
        &paired,
    )
}

/// Ajuste de mínimos cuadrados de `y = slope*x + intercept` sobre los puntos dados.
///
/// ```
/// let puntos = [(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)];
/// let ajuste = quantify::analysis::linear_regression("x", "y", &puntos).unwrap();
/// assert!((ajuste.slope - 2.0).abs() < 1e-9);
/// assert!((ajuste.intercept - 1.0).abs() < 1e-9);
/// assert!((ajuste.r_squared - 1.0).abs() < 1e-9);
/// ```
pub fn linear_regression(
    x_name: &str,
    y_name: &str,
    points: &[(f64, f64)],
) -> Option<LinearRegression> {
    let n = points.len();
    if n < 2 {
        return None;
    }

    let n_f = n as f64;
    let sum_x = points.iter().map(|(x, _)| x).sum::<f64>();
    let sum_y = points.iter().map(|(_, y)| y).sum::<f64>();
    let mean_x = sum_x / n_f;
    let mean_y = sum_y / n_f;

    let ss_xx = points
        .iter()
        .map(|(x, _)| {
            let dx = x - mean_x;
            dx * dx
        })
        .sum::<f64>();
    if ss_xx == 0.0 {
        return None;
    }

    let ss_xy = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let slope = ss_xy / ss_xx;
    let intercept = mean_y - slope * mean_x;

    let ss_tot = points
        .iter()
        .map(|(_, y)| (y - mean_y).powi(2))
        .sum::<f64>();
    let ss_res = points
        .iter()
        .map(|(x, y)| (y - mean_y - slope * (x - mean_x)).powi(2))
        .sum::<f64>();
    let r_squared = if ss_tot == 0.0 {
        1.0
    } else {
        1.0 - ss_res / ss_tot
    };

    let (u_slope, u_mean_y) = if n >= 3 {
        let s2 = ss_res / (n_f - 2.0);
        ((s2 / ss_xx).sqrt(), (s2 / n_f).sqrt())
    } else {
        (0.0, 0.0)
    };
    let u_intercept = u_mean_y.hypot(mean_x * u_slope);

    Some(LinearRegression {
        x_column: x_name.to_string(),
        y_column: y_name.to_string(),
        slope,
        intercept,
        r_squared,
        u_slope,
        u_intercept,
        mean_x,
        u_mean_y,
    })
}

/// Derivadas de la pendiente y la ordenada del ajuste respecto de un parámetro, dadas las derivadas de cada punto.
///
/// ```
/// let (db, da) = quantify::analysis::line_sensitivity(&[(1.0, 2.0), (2.0, 4.0), (3.0, 6.0)], &[(0.0, 1.0), (0.0, 2.0), (0.0, 3.0)]);
/// assert!((db - 1.0).abs() < 1e-12 && da.abs() < 1e-12);
/// ```
pub fn line_sensitivity(points: &[(f64, f64)], d_points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let mean = |pairs: &[(f64, f64)]| {
        let (sx, sy) = pairs
            .iter()
            .fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x, sy + y));
        (sx / n, sy / n)
    };
    let ((mx, my), (mdx, mdy)) = (mean(points), mean(d_points));
    let (mut sxx, mut sxy, mut dsxx, mut dsxy) = (0.0, 0.0, 0.0, 0.0);
    for (&(x, y), &(dx, dy)) in points.iter().zip(d_points) {
        let (cx, cy, cdx, cdy) = (x - mx, y - my, dx - mdx, dy - mdy);
        sxx += cx * cx;
        sxy += cx * cy;
        dsxx += 2.0 * cx * cdx;
        dsxy += cdx * cy + cx * cdy;
    }
    let slope = sxy / sxx;
    let db = (dsxy - slope * dsxx) / sxx;
    (db, mdy - db * mx - slope * mdx)
}

/// Ajuste de relajacion exponencial V(t) = V0 * exp(-t / tau).
///
/// ```
/// // V(t) = 10 * exp(-t / 2) -> tau = 2.
/// let puntos: Vec<(f64, f64)> = (0..6)
///     .map(|i| (i as f64, 10.0 * (-(i as f64) / 2.0).exp()))
///     .collect();
/// let (tau, _u_tau) = quantify::analysis::relaxation_tau(&puntos).unwrap();
/// assert!((tau - 2.0).abs() < 1e-9);
/// ```
pub fn relaxation_tau(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let linearized: Vec<(f64, f64)> = points
        .iter()
        .filter(|(_, v)| *v > 0.0)
        .map(|(t, v)| (*t, v.ln()))
        .collect();

    let fit = linear_regression("t", "ln_V", &linearized)?;
    if fit.slope == 0.0 {
        return None;
    }
    let tau = -1.0 / fit.slope;
    let u_tau = fit.u_slope / (fit.slope * fit.slope);
    Some((tau, u_tau))
}

#[cfg(test)]
mod tests {
    use super::{analyze_csv, linear_regression, relaxation_tau};

    #[test]
    fn intercept_splits_into_mean_height_and_slope() {
        let points = [(1.0, 1.1), (2.0, 1.9), (3.0, 3.2), (4.0, 3.9)];
        let fit = linear_regression("x", "y", &points).unwrap();
        assert_eq!(fit.mean_x, 2.5);
        let s2 = points
            .iter()
            .map(|(x, y)| (y - fit.slope * x - fit.intercept).powi(2))
            .sum::<f64>()
            / 2.0;
        assert!((fit.u_mean_y - (s2 / 4.0).sqrt()).abs() < 1e-12);
        let rebuilt = fit.u_mean_y.powi(2) + (fit.mean_x * fit.u_slope).powi(2);
        assert!((rebuilt - fit.u_intercept.powi(2)).abs() < 1e-12);
    }

    #[test]
    fn residuals_survive_a_large_x_offset() {
        let ys = [0.21, 0.39, 0.62, 0.79, 1.02];
        let near: Vec<(f64, f64)> = ys.iter().enumerate().map(|(i, &y)| (i as f64, y)).collect();
        let far: Vec<(f64, f64)> = near.iter().map(|&(x, y)| (x + 1e12, y)).collect();
        let (a, b) = (
            linear_regression("x", "y", &near).unwrap(),
            linear_regression("x", "y", &far).unwrap(),
        );
        assert!((a.u_mean_y - b.u_mean_y).abs() < 1e-9 * a.u_mean_y);
        assert!((a.u_slope - b.u_slope).abs() < 1e-9 * a.u_slope);
    }

    #[test]
    fn two_points_have_no_dispersion() {
        let fit = linear_regression("x", "y", &[(0.0, 1.0), (1.0, 3.0)]).unwrap();
        assert_eq!(fit.u_mean_y, 0.0);
    }

    #[test]
    fn computes_basic_stats_and_regression() {
        let csv = "x,y\n1,2\n2,4\n3,6\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.row_count, 3);
        assert_eq!(analysis.numeric_columns.len(), 2);
        let column = &analysis.numeric_columns[0];
        assert!((column.std_dev_sample - 1.0).abs() < 1e-12);
        let regression = analysis.regression.unwrap();
        assert!((regression.slope - 2.0).abs() < 1e-9);
        assert!(regression.r_squared > 0.999);
        assert!(regression.u_slope < 1e-9);
    }

    #[test]
    fn recovers_tau_from_exponential_decay() {
        let tau_real = 2.0;
        let points: Vec<(f64, f64)> = (0..6)
            .map(|i| {
                let t = i as f64;
                (t, 10.0 * (-t / tau_real).exp())
            })
            .collect();
        let (tau, u_tau) = relaxation_tau(&points).unwrap();
        assert!((tau - tau_real).abs() < 1e-9);
        assert!(u_tau < 1e-6);
    }
}
