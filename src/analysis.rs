use anyhow::{anyhow, Context};
use csv::StringRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub name: String,
    pub count: usize,
    pub mean: f64,
    pub std_dev: f64,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub row_count: usize,
    pub numeric_columns: Vec<ColumnStats>,
    pub regression: Option<LinearRegression>,
    pub warnings: Vec<String>,
}

pub fn analyze_csv(input: &str) -> anyhow::Result<AnalysisResult> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(input.as_bytes());
    let headers = reader.headers().context("CSV without headers")?.clone();

    if headers.is_empty() {
        return Err(anyhow!("CSV must contain at least one column"));
    }

    let mut columns: Vec<Vec<f64>> = headers.iter().map(|_| Vec::new()).collect();
    let mut warnings = Vec::new();
    let mut row_count = 0usize;

    for result in reader.records() {
        let record = result.context("invalid CSV row")?;
        row_count += 1;
        parse_record(&headers, &record, row_count, &mut columns, &mut warnings);
    }

    if row_count == 0 {
        return Err(anyhow!("CSV must contain at least one data row"));
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
    let variance = values
        .iter()
        .map(|value| {
            let diff = value - mean;
            diff * diff
        })
        .sum::<f64>()
        / count as f64;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    Some(ColumnStats {
        name: name.to_string(),
        count,
        mean,
        std_dev: variance.sqrt(),
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

    let x_idx = *numeric_indices.get(0)?;
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

fn linear_regression(
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
        .map(|(x, y)| {
            let predicted = slope * x + intercept;
            (y - predicted).powi(2)
        })
        .sum::<f64>();
    let r_squared = if ss_tot == 0.0 {
        1.0
    } else {
        1.0 - ss_res / ss_tot
    };

    Some(LinearRegression {
        x_column: x_name.to_string(),
        y_column: y_name.to_string(),
        slope,
        intercept,
        r_squared,
    })
}

#[cfg(test)]
mod tests {
    use super::analyze_csv;

    #[test]
    fn computes_basic_stats_and_regression() {
        let csv = "x,y\n1,2\n2,4\n3,6\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.row_count, 3);
        assert_eq!(analysis.numeric_columns.len(), 2);
        let regression = analysis.regression.unwrap();
        assert!((regression.slope - 2.0).abs() < 1e-9);
        assert!(regression.r_squared > 0.999);
    }
}
