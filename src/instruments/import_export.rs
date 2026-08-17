use super::{
    insert_instrument, insert_scale, list_instruments, CatalogExport, CreateInstrument,
    InstrumentExport, ScaleInput,
};
use sqlx::SqlitePool;

/// Exporta el catálogo de un curso a una estructura autocontenida (sin ids), apta para
/// reutilizarlo en otro curso vía [`import_course`].
pub async fn export_course(pool: &SqlitePool, course_id: &str) -> anyhow::Result<CatalogExport> {
    let instruments = list_instruments(pool, course_id).await?;
    let exported = instruments
        .into_iter()
        .map(|item| InstrumentExport {
            name: item.instrument.name,
            kind: item.instrument.kind,
            quantity: item.instrument.quantity,
            unit: item.instrument.unit,
            scales: item
                .scales
                .into_iter()
                .map(|s| ScaleInput {
                    label: s.label,
                    full_scale: s.full_scale,
                    step: s.step,
                    appreciation: s.appreciation,
                    internal_res: s.internal_res,
                    internal_res_u: s.internal_res_u,
                    b_model: s.b_model,
                    spec_pct_reading: s.spec_pct_reading,
                    spec_step_coeff: s.spec_step_coeff,
                    spec_fixed: s.spec_fixed,
                    u_cal_pct: Some(s.u_cal_pct),
                    u_cal_fixed: Some(s.u_cal_fixed),
                    unit: s.unit,
                })
                .collect(),
        })
        .collect();
    Ok(CatalogExport {
        instruments: exported,
    })
}

/// Importa un catálogo a un curso destino, recreando instrumentos y escalas con ids nuevos.
/// Corre dentro de una transacción: si algún instrumento o escala falla, no queda nada
/// importado (todo o nada). Devuelve la cantidad de instrumentos importados.
pub async fn import_course(
    pool: &SqlitePool,
    course_id: &str,
    payload: CatalogExport,
) -> anyhow::Result<usize> {
    let count = payload.instruments.len();
    let mut tx = pool.begin().await?;
    for instrument in &payload.instruments {
        let create = CreateInstrument {
            course_id: course_id.to_string(),
            name: instrument.name.clone(),
            kind: instrument.kind.clone(),
            quantity: instrument.quantity.clone(),
            unit: instrument.unit.clone(),
        };
        let inst_id = insert_instrument(&mut tx, &create).await?;
        for (index, scale) in instrument.scales.iter().enumerate() {
            insert_scale(&mut tx, &inst_id, index as i64 + 1, scale).await?;
        }
    }
    tx.commit().await?;
    Ok(count)
}
