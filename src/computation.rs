//! Cálculo de incertidumbres de una entrega cargada por formulario (análisis `estadistico`).
//!
//! Toma las lecturas crudas del estudiante + la definición de la práctica + el catálogo de
//! instrumentos, y produce un [`FormAnalysis`] con incertidumbres tipo A/B/combinada/expandida
//! por magnitud y la propagación de cada mensurando. El cálculo numérico vive en
//! [`crate::uncertainty`]; este módulo lo cablea con la base y evalúa las fórmulas (texto)
//! con `evalexpr`.

use crate::analysis;
use crate::db::{self, AuthUser, InstrumentScale, PracticeQuantity, PracticeResult};
use crate::uncertainty::{self, BModel, QuantityResult, ScaleSpec};
use chrono::Utc;
use evalexpr::{build_operator_tree, ContextWithMutableVariables, HashMapContext, Node, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Lecturas crudas de una magnitud cargadas en el formulario.
#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementInput {
    pub quantity_id: String,
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
    /// Réplicas medidas (una o varias) de la magnitud.
    pub values: Vec<f64>,
}

/// Cuerpo para crear una entrega por formulario.
#[derive(Debug, Deserialize)]
pub struct FormSubmissionInput {
    pub course_id: String,
    pub group_id: String,
    pub practice_id: String,
    pub measurements: Vec<MeasurementInput>,
}

/// Incertidumbre calculada de una magnitud medida directamente.
#[derive(Debug, Serialize)]
pub struct QuantityComputation {
    pub quantity_id: String,
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub instrument_id: Option<String>,
    pub scale_id: Option<String>,
    pub values: Vec<f64>,
    pub result: QuantityResult,
}

/// Mensurando derivado calculado por propagación de varianzas.
#[derive(Debug, Serialize)]
pub struct DerivedComputation {
    pub symbol: String,
    pub name: String,
    pub unit: String,
    pub formula: String,
    pub value: f64,
    pub u: f64,
    pub u_expanded: f64,
}

/// Resultado de un ajuste lineal `y = slope*x + intercept` sobre una serie de puntos.
/// `x_label`/`y_label` son las fórmulas de eje (texto), para rotular el gráfico.
#[derive(Debug, Serialize)]
pub struct RegressionResult {
    pub points: Vec<(f64, f64)>,
    pub slope: f64,
    pub intercept: f64,
    pub u_slope: f64,
    pub u_intercept: f64,
    pub r_squared: f64,
    pub x_label: String,
    pub y_label: String,
}

/// Resultado completo del cálculo de una entrega por formulario. En el camino estadístico se
/// llenan `quantities` (incertidumbres por magnitud); en el de regresión, `regression` (ajuste).
/// `derived` y `warnings` se usan en ambos.
#[derive(Debug, Serialize)]
pub struct FormAnalysis {
    pub quantities: Vec<QuantityComputation>,
    pub regression: Option<RegressionResult>,
    pub derived: Vec<DerivedComputation>,
    pub warnings: Vec<String>,
}

/// Convierte una escala del catálogo ([`InstrumentScale`]) en la especificación que entiende
/// el motor ([`ScaleSpec`]). Error si el `b_model` guardado no es uno de los modelos conocidos.
pub fn scale_spec(scale: &InstrumentScale) -> anyhow::Result<ScaleSpec> {
    let b_model = match scale.b_model.as_str() {
        "resolucion" => BModel::Resolucion,
        "apreciacion" => BModel::Apreciacion,
        "fabricante" => BModel::Fabricante,
        other => {
            anyhow::bail!("la escala tiene un modelo de incertidumbre desconocido: {other}")
        }
    };
    Ok(ScaleSpec {
        b_model,
        step: scale.step,
        appreciation: scale.appreciation,
        spec_pct_reading: scale.spec_pct_reading.unwrap_or(0.0),
        spec_step_coeff: scale.spec_step_coeff.unwrap_or(0.0),
        spec_fixed: scale.spec_fixed.unwrap_or(0.0),
    })
}

/// Constantes disponibles en cualquier fórmula (además de las funciones `math::*` de evalexpr).
const CONSTANTS: [(&str, f64); 2] = [("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

/// Compila una fórmula y valida que todas sus variables sean símbolos declarados (los de
/// `allowed`) o constantes conocidas (`pi`, `e`). Devuelve el árbol precompilado.
fn compile_formula(formula: &str, allowed: &[String]) -> anyhow::Result<Node> {
    let tree = build_operator_tree(formula)
        .map_err(|err| anyhow::anyhow!("la formula \"{formula}\" no es valida: {err}"))?;
    for var in tree.iter_variable_identifiers() {
        let is_constant = CONSTANTS.iter().any(|(name, _)| *name == var);
        if !is_constant && !allowed.iter().any(|s| s == var) {
            anyhow::bail!(
                "la formula \"{formula}\" usa el simbolo \"{var}\", que no es una magnitud de la practica"
            );
        }
    }
    Ok(tree)
}

/// Evalúa una fórmula precompilada con los valores dados por símbolo (más las constantes
/// `pi`/`e`). Devuelve `NaN` si la evaluación falla, para no romper la propagación numérica.
fn eval_compiled(tree: &Node, values: &HashMap<&str, f64>) -> f64 {
    let mut context = HashMapContext::new();
    for (name, value) in CONSTANTS {
        let _ = context.set_value(name.to_string(), Value::Float(value));
    }
    for (symbol, value) in values {
        if context
            .set_value((*symbol).to_string(), Value::Float(*value))
            .is_err()
        {
            return f64::NAN;
        }
    }
    tree.eval_float_with_context(&context).unwrap_or(f64::NAN)
}

/// Calcula el [`FormAnalysis`] de una entrega (función pura, sin base de datos).
/// `scales` mapea `scale_id` → escala ya resuelta; `measurements` son las lecturas por magnitud.
pub fn compute(
    quantities: &[PracticeQuantity],
    results: &[PracticeResult],
    scales: &HashMap<String, InstrumentScale>,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let mut warnings = Vec::new();
    let by_quantity: HashMap<&str, &MeasurementInput> = measurements
        .iter()
        .map(|m| (m.quantity_id.as_str(), m))
        .collect();

    let mut computed = Vec::with_capacity(quantities.len());
    // Media de cada símbolo, para la propagación de los mensurandos.
    let mut means_by_symbol: HashMap<String, f64> = HashMap::new();
    let mut u_by_symbol: HashMap<String, f64> = HashMap::new();

    for quantity in quantities {
        let measurement = by_quantity.get(quantity.id.as_str());
        let values: Vec<f64> = measurement.map(|m| m.values.clone()).unwrap_or_default();
        if values.is_empty() {
            warnings.push(format!(
                "La magnitud \"{}\" ({}) no tiene lecturas cargadas.",
                quantity.name, quantity.symbol
            ));
        }
        let spec = match measurement.and_then(|m| m.scale_id.as_deref()) {
            Some(scale_id) => match scales.get(scale_id) {
                Some(scale) => Some(scale_spec(scale)?),
                None => anyhow::bail!("la escala seleccionada no existe"),
            },
            None => None,
        };
        let result = uncertainty::measured_quantity(&values, spec.as_ref());
        means_by_symbol.insert(quantity.symbol.clone(), result.mean);
        u_by_symbol.insert(quantity.symbol.clone(), result.u_c);
        computed.push(QuantityComputation {
            quantity_id: quantity.id.clone(),
            symbol: quantity.symbol.clone(),
            name: quantity.name.clone(),
            unit: quantity.unit.clone(),
            instrument_id: measurement.and_then(|m| m.instrument_id.clone()),
            scale_id: measurement.and_then(|m| m.scale_id.clone()),
            values,
            result,
        });
    }

    let symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    let derived = derive_results(
        results,
        &symbols,
        &means_by_symbol,
        &u_by_symbol,
        &mut warnings,
    )?;

    Ok(FormAnalysis {
        quantities: computed,
        regression: None,
        derived,
        warnings,
    })
}

/// Calcula el [`FormAnalysis`] de una práctica `regresion_lineal`: empareja las mediciones por
/// punto, evalúa las fórmulas de eje `x_formula`/`y_formula` en cada punto, ajusta una recta
/// (`analysis::linear_regression`) y deriva los mensurandos desde `slope`/`intercept`.
pub fn compute_regresion(
    quantities: &[PracticeQuantity],
    results: &[PracticeResult],
    x_formula: &str,
    y_formula: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let mut warnings = Vec::new();
    let symbols: Vec<String> = quantities.iter().map(|q| q.symbol.clone()).collect();
    let by_quantity: HashMap<&str, &MeasurementInput> = measurements
        .iter()
        .map(|m| (m.quantity_id.as_str(), m))
        .collect();

    // Cantidad de puntos = mínimo de réplicas entre las magnitudes (deben venir parejas).
    let lengths: Vec<usize> = quantities
        .iter()
        .map(|q| by_quantity.get(q.id.as_str()).map_or(0, |m| m.values.len()))
        .collect();
    let n_points = lengths.iter().copied().min().unwrap_or(0);
    if lengths.iter().any(|&l| l != n_points) {
        warnings.push(
            "Las magnitudes tienen distinta cantidad de puntos; se usa la menor cantidad comun."
                .into(),
        );
    }
    if n_points < 2 {
        anyhow::bail!("se necesitan al menos 2 puntos para el ajuste lineal");
    }

    let x_tree = compile_formula(x_formula, &symbols)?;
    let y_tree = compile_formula(y_formula, &symbols)?;

    let mut points = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let bound: HashMap<&str, f64> = quantities
            .iter()
            .filter_map(|q| {
                by_quantity
                    .get(q.id.as_str())
                    .map(|m| (q.symbol.as_str(), m.values[i]))
            })
            .collect();
        let x = eval_compiled(&x_tree, &bound);
        let y = eval_compiled(&y_tree, &bound);
        if !x.is_finite() || !y.is_finite() {
            anyhow::bail!(
                "un punto produjo un valor no finito al evaluar los ejes (revisa las formulas y las lecturas)"
            );
        }
        points.push((x, y));
    }

    let fit = analysis::linear_regression("x", "y", &points)
        .ok_or_else(|| anyhow::anyhow!("no se pudo ajustar la recta (¿todos los x iguales?)"))?;

    // Mensurandos derivados de la pendiente/intercepto.
    let means: HashMap<String, f64> = [
        ("slope".to_string(), fit.slope),
        ("intercept".to_string(), fit.intercept),
    ]
    .into();
    let us: HashMap<String, f64> = [
        ("slope".to_string(), fit.u_slope),
        ("intercept".to_string(), fit.u_intercept),
    ]
    .into();
    let allowed = vec!["slope".to_string(), "intercept".to_string()];
    let derived = derive_results(results, &allowed, &means, &us, &mut warnings)?;

    Ok(FormAnalysis {
        quantities: Vec::new(),
        regression: Some(RegressionResult {
            points,
            slope: fit.slope,
            intercept: fit.intercept,
            u_slope: fit.u_slope,
            u_intercept: fit.u_intercept,
            r_squared: fit.r_squared,
            x_label: x_formula.to_string(),
            y_label: y_formula.to_string(),
        }),
        derived,
        warnings,
    })
}

/// Calcula los mensurandos derivados por propagación de varianzas: cada fórmula se evalúa y
/// propaga usando los valores/incertidumbres de los símbolos disponibles (`means_by_symbol` /
/// `u_by_symbol`). Sirve tanto para el camino estadístico (símbolos = magnitudes) como para el
/// de regresión (símbolos = `slope`/`intercept`). Acumula advertencias por valores no finitos.
fn derive_results(
    results: &[PracticeResult],
    allowed: &[String],
    means_by_symbol: &HashMap<String, f64>,
    u_by_symbol: &HashMap<String, f64>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<DerivedComputation>> {
    let mut derived = Vec::with_capacity(results.len());
    for result in results {
        let tree = compile_formula(&result.formula, allowed)?;
        // Variables que la fórmula realmente usa (sin constantes), en orden estable.
        let vars: Vec<String> = tree
            .iter_variable_identifiers()
            .filter(|v| !CONSTANTS.iter().any(|(name, _)| name == v))
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let means: Vec<f64> = vars
            .iter()
            .map(|v| *means_by_symbol.get(v).unwrap_or(&0.0))
            .collect();
        let us: Vec<f64> = vars
            .iter()
            .map(|v| *u_by_symbol.get(v).unwrap_or(&0.0))
            .collect();
        let (value, u) = uncertainty::propagate(
            |x: &[f64]| {
                let bound: HashMap<&str, f64> = vars
                    .iter()
                    .map(|s| s.as_str())
                    .zip(x.iter().copied())
                    .collect();
                eval_compiled(&tree, &bound)
            },
            &means,
            &us,
        );
        if !value.is_finite() || !u.is_finite() {
            warnings.push(format!(
                "El mensurando \"{}\" ({} = {}) no dio un valor finito; revisa la formula y las lecturas (p. ej. division por cero).",
                result.name, result.symbol, result.formula
            ));
        }
        derived.push(DerivedComputation {
            symbol: result.symbol.clone(),
            name: result.name.clone(),
            unit: result.unit.clone(),
            formula: result.formula.clone(),
            value,
            u,
            u_expanded: uncertainty::expand(u, uncertainty::EXPANSION_K),
        });
    }
    Ok(derived)
}

/// Lee la definición de la práctica y las escalas referidas por las mediciones, y calcula el
/// [`FormAnalysis`]. Reúne los datos de la base y delega en [`compute`].
pub async fn analyze(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
    measurements: &[MeasurementInput],
) -> anyhow::Result<FormAnalysis> {
    let definition = crate::practices::definition(pool, practice_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la practica no existe"))?;

    // Toda medición debe corresponder a una magnitud de esta práctica (evita insertar filas
    // colgadas y da un error claro en vez de una violación de clave foránea).
    let valid_ids: std::collections::HashSet<&str> = definition
        .quantities
        .iter()
        .map(|q| q.id.as_str())
        .collect();
    for measurement in measurements {
        if !valid_ids.contains(measurement.quantity_id.as_str()) {
            anyhow::bail!("una de las mediciones no corresponde a una magnitud de esta practica");
        }
    }

    let scales = load_scales(pool, measurements).await?;

    // Si se eligió instrumento y escala, la escala debe pertenecer a ese instrumento.
    for measurement in measurements {
        if let (Some(instrument_id), Some(scale_id)) =
            (&measurement.instrument_id, &measurement.scale_id)
        {
            if let Some(scale) = scales.get(scale_id) {
                if scale.instrument_id != *instrument_id {
                    anyhow::bail!("la escala elegida no pertenece al instrumento seleccionado");
                }
            }
        }
    }

    // Camino de regresión lineal: requiere las fórmulas de eje definidas.
    if definition.analysis_kind.as_deref() == Some("regresion_lineal") {
        let (Some(x_formula), Some(y_formula)) = (
            definition.x_formula.as_deref(),
            definition.y_formula.as_deref(),
        ) else {
            anyhow::bail!(
                "la practica es de regresion pero no tiene definidas las formulas de los ejes"
            );
        };
        return compute_regresion(
            &definition.quantities,
            &definition.results,
            x_formula,
            y_formula,
            measurements,
        );
    }

    compute(
        &definition.quantities,
        &definition.results,
        &scales,
        measurements,
    )
}

/// Carga, por id, las escalas referidas por las mediciones (las que traen `scale_id`).
async fn load_scales(
    pool: &sqlx::SqlitePool,
    measurements: &[MeasurementInput],
) -> anyhow::Result<HashMap<String, InstrumentScale>> {
    let mut scales = HashMap::new();
    for measurement in measurements {
        let Some(scale_id) = measurement.scale_id.as_deref() else {
            continue;
        };
        if scales.contains_key(scale_id) {
            continue;
        }
        let scale = sqlx::query_as::<_, InstrumentScale>(
            "SELECT id, instrument_id, label, full_scale, step, appreciation, internal_res, \
             internal_res_u, b_model, spec_pct_reading, spec_step_coeff, spec_fixed, unit, position \
             FROM instrument_scales WHERE id = ?1",
        )
        .bind(scale_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la escala seleccionada no existe"))?;
        scales.insert(scale_id.to_string(), scale);
    }
    Ok(scales)
}

/// Crea una entrega por formulario: calcula el análisis, inserta la entrega y sus mediciones
/// en una transacción, y devuelve el detalle. El usuario ya fue validado por el handler.
pub async fn create_form_submission(
    pool: &sqlx::SqlitePool,
    user: &AuthUser,
    input: FormSubmissionInput,
) -> anyhow::Result<db::SubmissionDetail> {
    let analysis = analyze(pool, &input.practice_id, &input.measurements).await?;
    let analysis_json = serde_json::to_string(&analysis)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    // Inserta la entrega resolviendo nombres denormalizados (igual que la variante CSV).
    let inserted = sqlx::query(
        r#"
        INSERT INTO submissions (
            id, student_name, group_name, course, practice_id, file_name, csv_path,
            analysis_json, status, submitted_at, submitted_by_user_id, course_id, group_id, entry_mode
        )
        SELECT
            ?1,
            u.display_name,
            g.name,
            c.name,
            ?5,
            '(formulario)',
            '',
            ?6,
            'pendiente',
            ?7,
            u.id,
            c.id,
            g.id,
            'form'
        FROM users u, lab_groups g, courses c
        WHERE u.id = ?2 AND g.id = ?3 AND c.id = ?4
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&input.group_id)
    .bind(&input.course_id)
    .bind(&input.practice_id)
    .bind(&analysis_json)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // El INSERT...SELECT no inserta nada si el curso/grupo (o usuario) no existe.
    if inserted.rows_affected() == 0 {
        anyhow::bail!("el curso o el grupo indicados no existen");
    }

    for measurement in &input.measurements {
        for (index, value) in measurement.values.iter().enumerate() {
            sqlx::query(
                "INSERT INTO submission_measurements \
                 (id, submission_id, quantity_id, instrument_id, scale_id, replicate_index, value) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&id)
            .bind(&measurement.quantity_id)
            .bind(measurement.instrument_id.as_deref())
            .bind(measurement.scale_id.as_deref())
            .bind(index as i64)
            .bind(*value)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;

    db::submission_detail(pool, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no se pudo leer la entrega recien creada"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    async fn setup() -> (SqlitePool, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("t.db").to_string_lossy());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        db::migrate(&pool).await.unwrap();
        db::seed_practices(&pool).await.unwrap();
        db::seed_users(&pool).await.unwrap();
        crate::practices::seed_definitions(&pool).await.unwrap();
        (pool, dir)
    }

    fn quantity(symbol: &str) -> PracticeQuantity {
        PracticeQuantity {
            id: format!("q-{symbol}"),
            practice_id: "p1-estadistica".into(),
            symbol: symbol.into(),
            name: symbol.into(),
            unit: "mm".into(),
            repeated: true,
            quantity: None,
            position: 0,
        }
    }

    fn measurement(symbol: &str, values: &[f64]) -> MeasurementInput {
        MeasurementInput {
            quantity_id: format!("q-{symbol}"),
            instrument_id: None,
            scale_id: None,
            values: values.to_vec(),
        }
    }

    fn fab_scale() -> InstrumentScale {
        InstrumentScale {
            id: "s1".into(),
            instrument_id: "i1".into(),
            label: "x".into(),
            full_scale: None,
            step: 1.0,
            appreciation: None,
            internal_res: None,
            internal_res_u: None,
            b_model: "fabricante".into(),
            spec_pct_reading: Some(1.0),
            spec_step_coeff: Some(5.0),
            spec_fixed: Some(0.0),
            unit: "A".into(),
            position: 1,
        }
    }

    #[test]
    fn scale_spec_maps_models_and_options() {
        let spec = scale_spec(&fab_scale()).unwrap();
        assert_eq!(spec.b_model, BModel::Fabricante);
        assert_eq!(spec.spec_pct_reading, 1.0);
        assert_eq!(spec.spec_step_coeff, 5.0);
        assert_eq!(spec.spec_fixed, 0.0);

        let mut bad = fab_scale();
        bad.b_model = "otro".into();
        assert!(scale_spec(&bad).is_err());
    }

    #[test]
    fn compile_formula_rejects_unknown_symbol() {
        let symbols = vec!["l".to_string(), "a".to_string()];
        assert!(compile_formula("l*a", &symbols).is_ok());
        // 'z' no es una magnitud declarada.
        assert!(compile_formula("l*z", &symbols).is_err());
        // paréntesis sin cerrar -> sintaxis inválida.
        assert!(compile_formula("(l*a", &symbols).is_err());
    }

    #[test]
    fn compute_propagates_q_l_a_b() {
        // Q = l*a + l*b con medias 2,3,4 e incertidumbres dadas -> valor 14, u_Q 0.9
        // (mismo caso que el test analítico de uncertainty::propagate).
        // Para forzar u_c = 0.1/0.2/0.2 sin tipo B usamos lecturas con esa s/sqrt(n).
        let quantities = vec![quantity("l"), quantity("a"), quantity("b")];
        let results = vec![PracticeResult {
            id: "r1".into(),
            practice_id: "p1-estadistica".into(),
            symbol: "Q".into(),
            name: "Area".into(),
            unit: "mm2".into(),
            formula: "l*a + l*b".into(),
            position: 0,
        }];
        let measurements = vec![
            measurement("l", &[2.0]),
            measurement("a", &[3.0]),
            measurement("b", &[4.0]),
        ];
        let analysis = compute(&quantities, &results, &HashMap::new(), &measurements).unwrap();
        assert_eq!(analysis.quantities.len(), 3);
        let q_l = &analysis.quantities[0];
        assert_eq!(q_l.symbol, "l");
        assert!(close(q_l.result.mean, 2.0, 1e-12));
        // Una sola lectura -> u_A = 0, sin escala -> u_B = 0 -> u_c = 0.
        assert!(close(q_l.result.u_c, 0.0, 1e-12));
        assert_eq!(analysis.derived.len(), 1);
        assert!(close(analysis.derived[0].value, 14.0, 1e-9));
        // u_c todas cero -> u_Q = 0.
        assert!(close(analysis.derived[0].u, 0.0, 1e-9));
    }

    #[test]
    fn compute_propagates_uncertainty_to_measurand() {
        // l con réplicas [9, 11] -> media 10, s = √2, u_A = s/√2 = 1.0; a=2, b=3 (sin u).
        // Q = l*a + l*b = 50; ∂Q/∂l = a+b = 5 -> u_Q = 5 * 1.0 = 5.0.
        let quantities = vec![quantity("l"), quantity("a"), quantity("b")];
        let results = vec![PracticeResult {
            id: "r1".into(),
            practice_id: "p1-estadistica".into(),
            symbol: "Q".into(),
            name: "Area".into(),
            unit: "mm2".into(),
            formula: "l*a + l*b".into(),
            position: 0,
        }];
        let measurements = vec![
            measurement("l", &[9.0, 11.0]),
            measurement("a", &[2.0]),
            measurement("b", &[3.0]),
        ];
        let analysis = compute(&quantities, &results, &HashMap::new(), &measurements).unwrap();
        let q_l = &analysis.quantities[0];
        assert!(close(q_l.result.u_a, 1.0, 1e-12));
        let q = &analysis.derived[0];
        assert!(close(q.value, 50.0, 1e-9));
        assert!(close(q.u, 5.0, 1e-6));
        assert!(close(q.u_expanded, 10.0, 1e-6));
    }

    #[test]
    fn compute_warns_on_missing_readings() {
        let quantities = vec![quantity("l")];
        let analysis = compute(&quantities, &[], &HashMap::new(), &[]).unwrap();
        assert_eq!(analysis.warnings.len(), 1);
        assert!(analysis.warnings[0].contains("no tiene lecturas"));
    }

    #[tokio::test]
    async fn analyze_uses_type_a_with_replicas() {
        let (pool, _dir) = setup().await;
        // P1 sembrada: l/a/b. Cargo réplicas de l con dispersión conocida.
        let def = crate::practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        let l_id = def
            .quantities
            .iter()
            .find(|q| q.symbol == "l")
            .unwrap()
            .id
            .clone();
        let measurements = vec![MeasurementInput {
            quantity_id: l_id,
            instrument_id: None,
            scale_id: None,
            values: vec![10.0, 12.0, 11.0],
        }];
        let analysis = analyze(&pool, "p1-estadistica", &measurements)
            .await
            .unwrap();
        let q_l = analysis
            .quantities
            .iter()
            .find(|q| q.symbol == "l")
            .unwrap();
        assert_eq!(q_l.result.n, 3);
        assert!(close(q_l.result.mean, 11.0, 1e-12));
        assert!(q_l.result.u_a > 0.0);
    }

    #[tokio::test]
    async fn create_form_submission_persists_and_reads_back() {
        let (pool, _dir) = setup().await;
        // Usuario docente (puede entregar sin estar en grupo); curso/grupo de prueba.
        let course = db::create_course(
            &pool,
            db::CreateCourse {
                name: "Curso".into(),
                term: "2026".into(),
            },
        )
        .await
        .unwrap();
        let group = db::create_group(
            &pool,
            &course.id,
            db::CreateGroup {
                name: "Grupo 1".into(),
                table_count: Some(4),
                group_type: None,
            },
        )
        .await
        .unwrap();
        let user = db::users(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|u| u.email == "docente@quantify.local")
            .unwrap();
        let def = crate::practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        let l_id = def
            .quantities
            .iter()
            .find(|q| q.symbol == "l")
            .unwrap()
            .id
            .clone();
        let input = FormSubmissionInput {
            course_id: course.id.clone(),
            group_id: group.id.clone(),
            practice_id: "p1-estadistica".into(),
            measurements: vec![MeasurementInput {
                quantity_id: l_id,
                instrument_id: None,
                scale_id: None,
                values: vec![5.0, 5.2, 4.9],
            }],
        };
        let detail = create_form_submission(&pool, &user, input).await.unwrap();
        assert_eq!(detail.entry_mode, "form");
        // El analysis es el FormAnalysis serializado (tiene "quantities").
        assert!(detail.analysis.get("quantities").is_some());
    }

    #[tokio::test]
    async fn analyze_rejects_foreign_quantity_id() {
        let (pool, _dir) = setup().await;
        let measurements = vec![MeasurementInput {
            quantity_id: "no-pertenece".into(),
            instrument_id: None,
            scale_id: None,
            values: vec![1.0],
        }];
        assert!(analyze(&pool, "p1-estadistica", &measurements)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn create_form_submission_rejects_unknown_course_and_rolls_back() {
        let (pool, _dir) = setup().await;
        let user = db::users(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|u| u.email == "docente@quantify.local")
            .unwrap();
        let def = crate::practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        let l_id = def
            .quantities
            .iter()
            .find(|q| q.symbol == "l")
            .unwrap()
            .id
            .clone();
        let input = FormSubmissionInput {
            course_id: "curso-fantasma".into(),
            group_id: "grupo-fantasma".into(),
            practice_id: "p1-estadistica".into(),
            measurements: vec![MeasurementInput {
                quantity_id: l_id,
                instrument_id: None,
                scale_id: None,
                values: vec![1.0],
            }],
        };
        assert!(create_form_submission(&pool, &user, input).await.is_err());
        // Rollback: no debe quedar ninguna entrega ni medición.
        let subs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM submissions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(subs.0, 0);
    }

    fn result(symbol: &str, formula: &str) -> PracticeResult {
        PracticeResult {
            id: format!("r-{symbol}"),
            practice_id: "p".into(),
            symbol: symbol.into(),
            name: symbol.into(),
            unit: "u".into(),
            formula: formula.into(),
            position: 0,
        }
    }

    #[test]
    fn compute_regresion_fits_known_line() {
        // y = 2x + 1 con ejes triviales (x = px, y = py).
        let quantities = vec![quantity("px"), quantity("py")];
        let results = vec![result("m", "slope"), result("b0", "intercept")];
        let measurements = vec![
            measurement("px", &[0.0, 1.0, 2.0, 3.0]),
            measurement("py", &[1.0, 3.0, 5.0, 7.0]),
        ];
        let a = compute_regresion(&quantities, &results, "px", "py", &measurements).unwrap();
        let reg = a.regression.unwrap();
        assert!(close(reg.slope, 2.0, 1e-9));
        assert!(close(reg.intercept, 1.0, 1e-9));
        assert!(close(reg.r_squared, 1.0, 1e-9));
        assert_eq!(reg.points.len(), 4);
        // Los mensurandos derivan de slope/intercept.
        assert!(close(
            a.derived.iter().find(|d| d.symbol == "m").unwrap().value,
            2.0,
            1e-9
        ));
        assert!(close(
            a.derived.iter().find(|d| d.symbol == "b0").unwrap().value,
            1.0,
            1e-9
        ));
    }

    #[test]
    fn compute_regresion_uses_pi_and_sqrt_in_axis_formulas() {
        // x = 2*pi*f ; y = math::sqrt(a). f=[1,2,3], a=[4,9,16] -> x=2pi*{1,2,3}, y={2,3,4}.
        // y crece 1 por unidad de f, x crece 2pi por unidad de f -> slope = 1/(2pi), intercept = 1.
        let quantities = vec![quantity("f"), quantity("a")];
        let results = vec![result("tau", "slope")];
        let measurements = vec![
            measurement("f", &[1.0, 2.0, 3.0]),
            measurement("a", &[4.0, 9.0, 16.0]),
        ];
        let analysis = compute_regresion(
            &quantities,
            &results,
            "2*pi*f",
            "math::sqrt(a)",
            &measurements,
        )
        .unwrap();
        let reg = analysis.regression.unwrap();
        assert!(close(reg.slope, 1.0 / (2.0 * std::f64::consts::PI), 1e-9));
        assert!(close(reg.intercept, 1.0, 1e-9));
        // Las etiquetas de eje conservan las fórmulas para rotular el gráfico.
        assert_eq!(reg.x_label, "2*pi*f");
        assert_eq!(reg.y_label, "math::sqrt(a)");
        assert!(close(
            analysis
                .derived
                .iter()
                .find(|d| d.symbol == "tau")
                .unwrap()
                .value,
            1.0 / (2.0 * std::f64::consts::PI),
            1e-9
        ));
    }

    #[test]
    fn compute_regresion_needs_at_least_two_points() {
        let quantities = vec![quantity("px"), quantity("py")];
        let measurements = vec![measurement("px", &[1.0]), measurement("py", &[2.0])];
        assert!(compute_regresion(&quantities, &[], "px", "py", &measurements).is_err());
    }
}
