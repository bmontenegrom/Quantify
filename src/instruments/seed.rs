use super::{create_instrument, create_scale, CreateInstrument, ScaleInput};
use sqlx::SqlitePool;

/// Siembra un catálogo inicial de instrumentos reales del curso. Idempotente: no hace nada
/// si el curso ya tiene instrumentos. Valores tomados de las hojas de testers y de la técnica
/// de trabajo de Física 103 (a confirmar/afinar por el docente).
pub async fn seed_instruments(pool: &SqlitePool, course_id: &str) -> anyhow::Result<()> {
    // Escala analógica (apreciación).
    let apre = |label: &str, step: f64, appr: f64, full: Option<f64>, unit: &str| ScaleInput {
        label: label.into(),
        full_scale: full,
        step,
        appreciation: Some(appr),
        internal_res: None,
        internal_res_u: None,
        b_model: "apreciacion".into(),
        spec_pct_reading: None,
        spec_step_coeff: None,
        spec_fixed: None,
        u_cal_pct: None,
        u_cal_fixed: None,
        unit: unit.into(),
    };
    // Escala digital simple (resolución).
    let reso = |label: &str, step: f64, full: Option<f64>, unit: &str| ScaleInput {
        label: label.into(),
        full_scale: full,
        step,
        appreciation: None,
        internal_res: None,
        internal_res_u: None,
        b_model: "resolucion".into(),
        spec_pct_reading: None,
        spec_step_coeff: None,
        spec_fixed: None,
        u_cal_pct: None,
        u_cal_fixed: None,
        unit: unit.into(),
    };
    // Escala con especificación de fabricante: U = pct*|v| + coef*step + fijo.
    #[allow(clippy::too_many_arguments)]
    let fab = |label: &str,
               step: f64,
               pct: f64,
               coeff: f64,
               fixed: f64,
               rint: Option<f64>,
               rint_u: Option<f64>,
               full: Option<f64>,
               unit: &str| ScaleInput {
        label: label.into(),
        full_scale: full,
        step,
        appreciation: None,
        internal_res: rint,
        internal_res_u: rint_u,
        b_model: "fabricante".into(),
        spec_pct_reading: Some(pct),
        spec_step_coeff: Some(coeff),
        spec_fixed: Some(fixed),
        u_cal_pct: None,
        u_cal_fixed: None,
        unit: unit.into(),
    };

    // Agrega calibracion (en cuadratura sobre el modelo base) a una escala ya construida:
    // `pct` en % del valor leido, `fixed` en unidad base. Tecnica de Hidrostatica.
    let cal = |scale: ScaleInput, pct: f64, fixed: f64| ScaleInput {
        u_cal_pct: Some(pct),
        u_cal_fixed: Some(fixed),
        ..scale
    };

    let instrument = |name: &str, kind: &str, quantity: &str, unit: &str| CreateInstrument {
        course_id: course_id.to_string(),
        name: name.into(),
        kind: kind.into(),
        quantity: quantity.into(),
        unit: unit.into(),
    };

    let catalog: Vec<(CreateInstrument, Vec<ScaleInput>)> = vec![
        (
            instrument("Regla milimetrada", "analogico", "longitud", "mm"),
            vec![apre("0-300 mm", 1.0, 0.5, Some(300.0), "mm")],
        ),
        (
            instrument("Calibre (Vernier)", "analogico", "longitud", "mm"),
            vec![apre("0-150 mm", 0.05, 0.05, Some(150.0), "mm")],
        ),
        (
            instrument("Cronometro digital", "digital", "tiempo", "s"),
            // Resolucion R = 0.001 s segun la tecnica de Estadistica.
            vec![reso("milesimas", 0.001, None, "s")],
        ),
        (
            instrument("Balanza digital", "digital", "masa", "g"),
            vec![
                reso("0.01 g", 0.01, None, "g"),
                // Tecnica de Hidrostatica: truncamiento R = 0.1 g y calibracion 3 % de la medida.
                cal(reso("0.1 g (calibracion 3 %)", 0.1, None, "g"), 3.0, 0.0),
            ],
        ),
        (
            // Tecnica de Hidrostatica: lectura por apreciacion mas u_calibracion fija de
            // 0.001 g/cm3 (la escala es un papel pegado al vastago: puede estar corrida).
            instrument("Densimetro", "analogico", "densidad", "g/cm3"),
            vec![cal(
                apre("0.001 g/cm3", 0.001, 0.001, None, "g/cm3"),
                0.0,
                0.001,
            )],
        ),
        (
            // Brazos de la balanza de Mohr: solo calibracion (u = 0.5 mm), el operador no
            // aprecia la medida porque las muescas estan grabadas.
            instrument("Balanza de Mohr (brazos)", "analogico", "longitud", "m"),
            vec![cal(reso("muescas", 0.0, None, "m"), 0.0, 0.0005)],
        ),
        (
            instrument("Tester A830L (corriente)", "digital", "corriente", "A"),
            vec![
                fab(
                    "200 uA",
                    0.1e-6,
                    1.0,
                    5.0,
                    0.0,
                    Some(1002.0),
                    Some(10.0),
                    Some(200e-6),
                    "A",
                ),
                fab(
                    "2 mA",
                    1e-6,
                    1.0,
                    5.0,
                    0.0,
                    Some(101.2),
                    Some(1.2),
                    Some(2e-3),
                    "A",
                ),
                fab(
                    "20 mA",
                    10e-6,
                    1.0,
                    5.0,
                    0.0,
                    Some(11.30),
                    Some(0.49),
                    Some(20e-3),
                    "A",
                ),
                fab(
                    "200 mA",
                    100e-6,
                    2.0,
                    5.0,
                    0.0,
                    Some(2.40),
                    Some(0.42),
                    Some(200e-3),
                    "A",
                ),
            ],
        ),
        (
            instrument("Tester EXTECH MN35 (voltaje)", "digital", "voltaje", "V"),
            vec![
                fab(
                    "200 mV",
                    0.1e-3,
                    0.5,
                    2.0,
                    0.0,
                    None,
                    None,
                    Some(200e-3),
                    "V",
                ),
                fab("2 V", 1e-3, 0.5, 2.0, 0.0, None, None, Some(2.0), "V"),
                fab("20 V", 10e-3, 0.5, 2.0, 0.0, None, None, Some(20.0), "V"),
            ],
        ),
        (
            instrument(
                "Tester EXTECH MN35 (resistencia)",
                "digital",
                "resistencia",
                "ohm",
            ),
            vec![
                fab(
                    "200 ohm",
                    0.1,
                    0.8,
                    4.0,
                    0.0,
                    None,
                    None,
                    Some(200.0),
                    "ohm",
                ),
                fab(
                    "2 kohm",
                    1.0,
                    0.8,
                    2.0,
                    0.0,
                    None,
                    None,
                    Some(2000.0),
                    "ohm",
                ),
                fab(
                    "20 kohm",
                    10.0,
                    0.8,
                    2.0,
                    0.0,
                    None,
                    None,
                    Some(20000.0),
                    "ohm",
                ),
                fab(
                    "200 kohm",
                    100.0,
                    0.8,
                    2.0,
                    0.0,
                    None,
                    None,
                    Some(200000.0),
                    "ohm",
                ),
            ],
        ),
        (
            instrument(
                "Osciloscopio GW Instek GDS-1052-U (voltaje)",
                "digital",
                "voltaje",
                "V",
            ),
            // Eje Y (voltaje): U = 3%*V + 0.1*(VOLTS/DIV) + 1 mV (Tecnica de Alterna).
            vec![
                fab("5 V/div", 5.0, 3.0, 0.1, 0.001, None, None, None, "V"),
                fab("2 V/div", 2.0, 3.0, 0.1, 0.001, None, None, None, "V"),
                fab("1 V/div", 1.0, 3.0, 0.1, 0.001, None, None, None, "V"),
                fab("0.5 V/div", 0.5, 3.0, 0.1, 0.001, None, None, None, "V"),
            ],
        ),
        (
            instrument(
                "Osciloscopio GW Instek GDS-1052-U (tiempo)",
                "digital",
                "tiempo",
                "s",
            ),
            // Eje X (tiempo): U = 1% de la medida (Tecnica de RC). Solo termino porcentual.
            vec![fab(
                "tiempo (1% de la medida)",
                1.0,
                1.0,
                0.0,
                0.0,
                None,
                None,
                None,
                "s",
            )],
        ),
        (
            instrument(
                "Osciloscopio GW Instek GDS-1052-U (frecuencia)",
                "digital",
                "frecuencia",
                "Hz",
            ),
            // Frecuencia de pasaje/bloqueo (Tecnica de Filtros): U = 1% de la medida, mismo
            // criterio que el eje X (tiempo). Solo termino porcentual.
            vec![fab(
                "frecuencia (1% de la medida)",
                1.0,
                1.0,
                0.0,
                0.0,
                None,
                None,
                None,
                "Hz",
            )],
        ),
        (
            // Tester UA78A — hoja de especificaciones, tabla "Capacidad".
            // U = pct%·|C| + 1·step (1 dg); unidades SI (F).
            instrument("Tester UA78A (capacidad)", "digital", "capacitancia", "F"),
            vec![
                fab("2 nF", 1e-12, 4.0, 1.0, 0.0, None, None, Some(2e-9), "F"),
                fab("20 nF", 1e-11, 4.0, 1.0, 0.0, None, None, Some(20e-9), "F"),
                fab(
                    "200 nF",
                    1e-10,
                    4.0,
                    1.0,
                    0.0,
                    None,
                    None,
                    Some(200e-9),
                    "F",
                ),
                fab("100 uF", 1e-7, 5.0, 1.0, 0.0, None, None, Some(100e-6), "F"),
            ],
        ),
    ];

    for (inst, scales) in catalog {
        // Aditivo: solo inserta si el instrumento aún no existe en el curso.
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM instruments WHERE course_id = ?1 AND name = ?2")
                .bind(&inst.course_id)
                .bind(inst.name.trim())
                .fetch_optional(pool)
                .await?;
        if existing.is_none() {
            let created = create_instrument(pool, inst).await?;
            for scale in scales {
                create_scale(pool, &created.id, scale).await?;
            }
        }
    }

    Ok(())
}
