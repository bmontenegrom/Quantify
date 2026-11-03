use super::{create_instrument, create_scale, CreateInstrument, ScaleInput};
use sqlx::SqlitePool;

const BACKFILL_SCALES: &[&str] = &["0.1 g (calibracion 3 %)"];

pub async fn seed_instruments(pool: &SqlitePool, course_id: &str) -> anyhow::Result<()> {
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
            vec![reso("milesimas", 0.001, None, "s")],
        ),
        (
            instrument("Balanza digital", "digital", "masa", "g"),
            vec![
                reso("0.01 g", 0.01, None, "g"),
                cal(reso("0.1 g (calibracion 3 %)", 0.1, None, "g"), 3.0, 0.0),
            ],
        ),
        (
            instrument("Densimetro", "analogico", "densidad", "g/cm3"),
            vec![cal(
                apre("0.001 g/cm3", 0.001, 0.001, None, "g/cm3"),
                0.0,
                0.001,
            )],
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
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM instruments WHERE course_id = ?1 AND name = ?2")
                .bind(&inst.course_id)
                .bind(inst.name.trim())
                .fetch_optional(pool)
                .await?;
        let (instrument_id, fresh) = match existing {
            Some((id,)) => (id, false),
            None => (create_instrument(pool, inst).await?.id, true),
        };
        for scale in scales {
            if !fresh && !BACKFILL_SCALES.contains(&scale.label.trim()) {
                continue;
            }
            let already: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM instrument_scales WHERE instrument_id = ?1 AND label = ?2",
            )
            .bind(&instrument_id)
            .bind(scale.label.trim())
            .fetch_optional(pool)
            .await?;
            if already.is_none() {
                create_scale(pool, &instrument_id, scale).await?;
            }
        }
    }

    Ok(())
}
