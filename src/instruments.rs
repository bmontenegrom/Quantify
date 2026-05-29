//! Catálogo de instrumentos por curso: operaciones de lectura/escritura y export/import.
//!
//! Los tipos de fila ([`Instrument`], [`InstrumentScale`]) y el esquema viven en [`crate::db`];
//! este módulo concentra las operaciones CRUD y la portabilidad del catálogo entre cursos.

use crate::db::{Instrument, InstrumentScale};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Datos para crear un instrumento en un curso.
#[derive(Debug, Deserialize)]
pub struct CreateInstrument {
    pub course_id: String,
    pub name: String,
    pub kind: String,
    pub quantity: String,
    pub unit: String,
}

/// Datos para actualizar un instrumento existente (no cambia el curso).
#[derive(Debug, Deserialize)]
pub struct UpdateInstrument {
    pub name: String,
    pub kind: String,
    pub quantity: String,
    pub unit: String,
}

/// Datos para crear o actualizar una escala de instrumento. La `position` se asigna
/// automáticamente al crear y se conserva al actualizar.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScaleInput {
    pub label: String,
    pub full_scale: Option<f64>,
    pub step: f64,
    pub appreciation: Option<f64>,
    pub internal_res: Option<f64>,
    pub internal_res_u: Option<f64>,
    pub b_model: String,
    pub spec_pct_reading: Option<f64>,
    pub spec_step_coeff: Option<f64>,
    pub spec_fixed: Option<f64>,
    pub unit: String,
}

/// Instrumento junto con sus escalas, para listar el catálogo.
#[derive(Debug, Serialize)]
pub struct InstrumentWithScales {
    #[serde(flatten)]
    pub instrument: Instrument,
    pub scales: Vec<InstrumentScale>,
}

/// Instrumento serializable para export/import (sin ids internos ni curso).
#[derive(Debug, Serialize, Deserialize)]
pub struct InstrumentExport {
    pub name: String,
    pub kind: String,
    pub quantity: String,
    pub unit: String,
    pub scales: Vec<ScaleInput>,
}

/// Catálogo exportable de un curso: lista autocontenida de instrumentos con sus escalas.
#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogExport {
    pub instruments: Vec<InstrumentExport>,
}

/// Columnas de un instrumento usadas en los `SELECT` (sin `created_at`).
const INSTRUMENT_COLS: &str = "id, course_id, name, kind, quantity, unit";

/// Columnas de una escala usadas en los `SELECT` (sin `created_at`).
const SCALE_COLS: &str = "id, instrument_id, label, full_scale, step, appreciation, internal_res, \
    internal_res_u, b_model, spec_pct_reading, spec_step_coeff, spec_fixed, unit, position";

/// Lista los instrumentos de un curso (ordenados por nombre) con sus escalas.
pub async fn list_instruments(
    pool: &SqlitePool,
    course_id: &str,
) -> anyhow::Result<Vec<InstrumentWithScales>> {
    let instruments = sqlx::query_as::<_, Instrument>(&format!(
        "SELECT {INSTRUMENT_COLS} FROM instruments WHERE course_id = ?1 ORDER BY name"
    ))
    .bind(course_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::with_capacity(instruments.len());
    for instrument in instruments {
        let scales = scales_for_instrument(pool, &instrument.id).await?;
        result.push(InstrumentWithScales { instrument, scales });
    }
    Ok(result)
}

/// Lee las escalas de un instrumento ordenadas por posición.
async fn scales_for_instrument(
    pool: &SqlitePool,
    instrument_id: &str,
) -> anyhow::Result<Vec<InstrumentScale>> {
    Ok(sqlx::query_as::<_, InstrumentScale>(&format!(
        "SELECT {SCALE_COLS} FROM instrument_scales WHERE instrument_id = ?1 ORDER BY position, label"
    ))
    .bind(instrument_id)
    .fetch_all(pool)
    .await?)
}

/// Recupera un instrumento por id.
async fn fetch_instrument(pool: &SqlitePool, id: &str) -> anyhow::Result<Instrument> {
    Ok(sqlx::query_as::<_, Instrument>(&format!(
        "SELECT {INSTRUMENT_COLS} FROM instruments WHERE id = ?1"
    ))
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Recupera una escala por id.
async fn fetch_scale(pool: &SqlitePool, id: &str) -> anyhow::Result<InstrumentScale> {
    Ok(sqlx::query_as::<_, InstrumentScale>(&format!(
        "SELECT {SCALE_COLS} FROM instrument_scales WHERE id = ?1"
    ))
    .bind(id)
    .fetch_one(pool)
    .await?)
}

/// Inserta un instrumento con el ejecutor dado y devuelve el id generado. Permite reutilizar
/// la misma lógica sobre el pool o dentro de una transacción (export/import).
async fn insert_instrument(
    conn: &mut SqliteConnection,
    input: &CreateInstrument,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO instruments (id, course_id, name, kind, quantity, unit, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(input.course_id.trim())
    .bind(input.name.trim())
    .bind(input.kind.trim())
    .bind(input.quantity.trim())
    .bind(input.unit.trim())
    .bind(Utc::now())
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

/// Inserta una escala en la posición dada usando el ejecutor, y devuelve el id generado.
async fn insert_scale(
    conn: &mut SqliteConnection,
    instrument_id: &str,
    position: i64,
    input: &ScaleInput,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO instrument_scales (id, instrument_id, label, full_scale, step, appreciation, \
         internal_res, internal_res_u, b_model, spec_pct_reading, spec_step_coeff, spec_fixed, \
         unit, position, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )
    .bind(&id)
    .bind(instrument_id)
    .bind(input.label.trim())
    .bind(input.full_scale)
    .bind(input.step)
    .bind(input.appreciation)
    .bind(input.internal_res)
    .bind(input.internal_res_u)
    .bind(input.b_model.trim())
    .bind(input.spec_pct_reading)
    .bind(input.spec_step_coeff)
    .bind(input.spec_fixed)
    .bind(input.unit.trim())
    .bind(position)
    .bind(Utc::now())
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

/// Crea un instrumento en un curso y lo devuelve.
pub async fn create_instrument(
    pool: &SqlitePool,
    input: CreateInstrument,
) -> anyhow::Result<Instrument> {
    // La conexión se libera al cerrar el bloque, antes del fetch (evita bloquear pools chicos).
    let id = {
        let mut conn = pool.acquire().await?;
        insert_instrument(&mut conn, &input).await?
    };
    fetch_instrument(pool, &id).await
}

/// Actualiza un instrumento por id. Devuelve `None` si no existe.
pub async fn update_instrument(
    pool: &SqlitePool,
    id: &str,
    input: UpdateInstrument,
) -> anyhow::Result<Option<Instrument>> {
    let result = sqlx::query(
        "UPDATE instruments SET name = ?2, kind = ?3, quantity = ?4, unit = ?5 WHERE id = ?1",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(input.kind.trim())
    .bind(input.quantity.trim())
    .bind(input.unit.trim())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_instrument(pool, id).await?))
}

/// Elimina un instrumento y sus escalas. Devuelve `true` si existía.
/// Borra las escalas explícitamente (no dependemos de `PRAGMA foreign_keys`).
pub async fn delete_instrument(pool: &SqlitePool, id: &str) -> anyhow::Result<bool> {
    sqlx::query("DELETE FROM instrument_scales WHERE instrument_id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    let result = sqlx::query("DELETE FROM instruments WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Agrega una escala a un instrumento, asignándole la siguiente posición disponible.
pub async fn create_scale(
    pool: &SqlitePool,
    instrument_id: &str,
    input: ScaleInput,
) -> anyhow::Result<InstrumentScale> {
    let position: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM instrument_scales WHERE instrument_id = ?1",
    )
    .bind(instrument_id)
    .fetch_one(pool)
    .await?;
    let id = {
        let mut conn = pool.acquire().await?;
        insert_scale(&mut conn, instrument_id, position.0, &input).await?
    };
    fetch_scale(pool, &id).await
}

/// Actualiza una escala por id (conserva su posición). Devuelve `None` si no existe.
pub async fn update_scale(
    pool: &SqlitePool,
    scale_id: &str,
    input: ScaleInput,
) -> anyhow::Result<Option<InstrumentScale>> {
    let result = sqlx::query(
        "UPDATE instrument_scales SET label = ?2, full_scale = ?3, step = ?4, appreciation = ?5, \
         internal_res = ?6, internal_res_u = ?7, b_model = ?8, spec_pct_reading = ?9, \
         spec_step_coeff = ?10, spec_fixed = ?11, unit = ?12 WHERE id = ?1",
    )
    .bind(scale_id)
    .bind(input.label.trim())
    .bind(input.full_scale)
    .bind(input.step)
    .bind(input.appreciation)
    .bind(input.internal_res)
    .bind(input.internal_res_u)
    .bind(input.b_model.trim())
    .bind(input.spec_pct_reading)
    .bind(input.spec_step_coeff)
    .bind(input.spec_fixed)
    .bind(input.unit.trim())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(fetch_scale(pool, scale_id).await?))
}

/// Elimina una escala por id. Devuelve `true` si existía.
pub async fn delete_scale(pool: &SqlitePool, scale_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM instrument_scales WHERE id = ?1")
        .bind(scale_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

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

/// Siembra un catálogo inicial de instrumentos reales del curso. Idempotente: no hace nada
/// si el curso ya tiene instrumentos. Valores tomados de las hojas de testers y de la técnica
/// de trabajo de Física 103 (a confirmar/afinar por el docente).
pub async fn seed_instruments(pool: &SqlitePool, course_id: &str) -> anyhow::Result<()> {
    if !list_instruments(pool, course_id).await?.is_empty() {
        return Ok(());
    }

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
        unit: unit.into(),
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
            vec![reso("0.01 g", 0.01, None, "g")],
        ),
        (
            instrument("Tester A830L (corriente)", "digital", "corriente", "A"),
            vec![
                fab("200 uA", 0.1e-6, 1.0, 5.0, 0.0, Some(1002.0), Some(10.0), Some(200e-6), "A"),
                fab("2 mA", 1e-6, 1.0, 5.0, 0.0, Some(101.2), Some(1.2), Some(2e-3), "A"),
                fab("20 mA", 10e-6, 1.0, 5.0, 0.0, Some(11.30), Some(0.49), Some(20e-3), "A"),
                fab("200 mA", 100e-6, 2.0, 5.0, 0.0, Some(2.40), Some(0.42), Some(200e-3), "A"),
            ],
        ),
        (
            instrument("Tester EXTECH MN35 (voltaje)", "digital", "voltaje", "V"),
            vec![
                fab("200 mV", 0.1e-3, 0.5, 2.0, 0.0, None, None, Some(200e-3), "V"),
                fab("2 V", 1e-3, 0.5, 2.0, 0.0, None, None, Some(2.0), "V"),
                fab("20 V", 10e-3, 0.5, 2.0, 0.0, None, None, Some(20.0), "V"),
            ],
        ),
        (
            instrument("Tester EXTECH MN35 (resistencia)", "digital", "resistencia", "ohm"),
            vec![
                fab("200 ohm", 0.1, 0.8, 4.0, 0.0, None, None, Some(200.0), "ohm"),
                fab("2 kohm", 1.0, 0.8, 2.0, 0.0, None, None, Some(2000.0), "ohm"),
                fab("20 kohm", 10.0, 0.8, 2.0, 0.0, None, None, Some(20000.0), "ohm"),
                fab("200 kohm", 100.0, 0.8, 2.0, 0.0, None, None, Some(200000.0), "ohm"),
            ],
        ),
        (
            instrument("Osciloscopio GW Instek GDS-1052-U (voltaje)", "digital", "voltaje", "V"),
            // Eje Y (voltaje): U = 3%*V + 0.1*(VOLTS/DIV) + 1 mV (Tecnica de Alterna).
            vec![
                fab("5 V/div", 5.0, 3.0, 0.1, 0.001, None, None, None, "V"),
                fab("2 V/div", 2.0, 3.0, 0.1, 0.001, None, None, None, "V"),
                fab("1 V/div", 1.0, 3.0, 0.1, 0.001, None, None, None, "V"),
                fab("0.5 V/div", 0.5, 3.0, 0.1, 0.001, None, None, None, "V"),
            ],
        ),
        (
            instrument("Osciloscopio GW Instek GDS-1052-U (tiempo)", "digital", "tiempo", "s"),
            // Eje X (tiempo): U = 1% de la medida (Tecnica de RC). Solo termino porcentual.
            vec![fab("tiempo (1% de la medida)", 1.0, 1.0, 0.0, 0.0, None, None, None, "s")],
        ),
    ];

    for (inst, scales) in catalog {
        let created = create_instrument(pool, inst).await?;
        for scale in scales {
            create_scale(pool, &created.id, scale).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::TempDir;

    /// Pool temporal migrado, con un curso creado; devuelve `(pool, dir, course_id)`.
    async fn setup() -> (SqlitePool, TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("t.db").to_string_lossy());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        db::migrate(&pool).await.unwrap();
        let course = db::create_course(
            &pool,
            db::CreateCourse {
                name: "Curso".into(),
                term: "2026".into(),
            },
        )
        .await
        .unwrap();
        (pool, dir, course.id)
    }

    fn sample_instrument(course_id: &str) -> CreateInstrument {
        CreateInstrument {
            course_id: course_id.to_string(),
            name: "Tester A830L".into(),
            kind: "digital".into(),
            quantity: "corriente".into(),
            unit: "A".into(),
        }
    }

    fn sample_scale() -> ScaleInput {
        // Escala tipo fabricante: ±(1% + 5 dgt).
        ScaleInput {
            label: "200 uA".into(),
            full_scale: Some(200e-6),
            step: 0.1e-6,
            appreciation: None,
            internal_res: Some(1002.0),
            internal_res_u: Some(10.0),
            b_model: "fabricante".into(),
            spec_pct_reading: Some(1.0),
            spec_step_coeff: Some(5.0),
            spec_fixed: Some(0.0),
            unit: "A".into(),
        }
    }

    #[tokio::test]
    async fn create_and_list_instruments() {
        let (pool, _dir, course_id) = setup().await;
        let created = create_instrument(&pool, sample_instrument(&course_id))
            .await
            .unwrap();
        assert_eq!(created.name, "Tester A830L");

        let listed = list_instruments(&pool, &course_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].instrument.id, created.id);
        assert!(listed[0].scales.is_empty());
    }

    #[tokio::test]
    async fn update_and_delete_instrument() {
        let (pool, _dir, course_id) = setup().await;
        let created = create_instrument(&pool, sample_instrument(&course_id))
            .await
            .unwrap();

        let updated = update_instrument(
            &pool,
            &created.id,
            UpdateInstrument {
                name: "Tester nuevo".into(),
                kind: "digital".into(),
                quantity: "voltaje".into(),
                unit: "V".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.name, "Tester nuevo");
        assert_eq!(updated.quantity, "voltaje");

        assert!(update_instrument(
            &pool,
            "inexistente",
            UpdateInstrument {
                name: "x".into(),
                kind: "digital".into(),
                quantity: "q".into(),
                unit: "u".into(),
            },
        )
        .await
        .unwrap()
        .is_none());

        assert!(delete_instrument(&pool, &created.id).await.unwrap());
        assert!(!delete_instrument(&pool, &created.id).await.unwrap());
        assert!(list_instruments(&pool, &course_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn scales_crud_and_position() {
        let (pool, _dir, course_id) = setup().await;
        let inst = create_instrument(&pool, sample_instrument(&course_id))
            .await
            .unwrap();

        let s1 = create_scale(&pool, &inst.id, sample_scale()).await.unwrap();
        let mut second = sample_scale();
        second.label = "2 mA".into();
        let s2 = create_scale(&pool, &inst.id, second).await.unwrap();
        assert_eq!(s1.position, 1);
        assert_eq!(s2.position, 2);

        let updated = update_scale(
            &pool,
            &s1.id,
            ScaleInput {
                label: "200 uA mod".into(),
                full_scale: Some(200e-6),
                step: 0.1e-6,
                appreciation: None,
                internal_res: Some(1002.0),
                internal_res_u: Some(10.0),
                b_model: "fabricante".into(),
                spec_pct_reading: Some(2.0),
                spec_step_coeff: Some(5.0),
                spec_fixed: Some(0.0),
                unit: "A".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.label, "200 uA mod");
        assert_eq!(updated.spec_pct_reading, Some(2.0));
        assert_eq!(updated.position, 1); // posición conservada

        let listed = list_instruments(&pool, &course_id).await.unwrap();
        assert_eq!(listed[0].scales.len(), 2);

        assert!(delete_scale(&pool, &s2.id).await.unwrap());
        assert!(!delete_scale(&pool, &s2.id).await.unwrap());
        let listed = list_instruments(&pool, &course_id).await.unwrap();
        assert_eq!(listed[0].scales.len(), 1);
    }

    #[tokio::test]
    async fn delete_instrument_removes_its_scales() {
        let (pool, _dir, course_id) = setup().await;
        let inst = create_instrument(&pool, sample_instrument(&course_id))
            .await
            .unwrap();
        let scale = create_scale(&pool, &inst.id, sample_scale()).await.unwrap();

        assert!(delete_instrument(&pool, &inst.id).await.unwrap());
        // La escala ya no debe existir.
        assert!(!delete_scale(&pool, &scale.id).await.unwrap());
    }

    #[tokio::test]
    async fn export_import_roundtrip_into_other_course() {
        let (pool, _dir, course_id) = setup().await;
        let inst = create_instrument(&pool, sample_instrument(&course_id))
            .await
            .unwrap();
        create_scale(&pool, &inst.id, sample_scale()).await.unwrap();

        let exported = export_course(&pool, &course_id).await.unwrap();
        assert_eq!(exported.instruments.len(), 1);
        assert_eq!(exported.instruments[0].scales.len(), 1);

        // Importar a un curso destino distinto.
        let dest = db::create_course(
            &pool,
            db::CreateCourse {
                name: "Destino".into(),
                term: "2027".into(),
            },
        )
        .await
        .unwrap();
        let imported = import_course(&pool, &dest.id, exported).await.unwrap();
        assert_eq!(imported, 1);

        let dest_list = list_instruments(&pool, &dest.id).await.unwrap();
        assert_eq!(dest_list.len(), 1);
        assert_eq!(dest_list[0].instrument.course_id, dest.id);
        assert_eq!(dest_list[0].instrument.name, "Tester A830L");
        assert_eq!(dest_list[0].scales.len(), 1);
        assert_eq!(dest_list[0].scales[0].b_model, "fabricante");
        assert_eq!(dest_list[0].scales[0].spec_step_coeff, Some(5.0));
    }

    #[tokio::test]
    async fn import_rolls_back_on_error() {
        let (pool, _dir, course_id) = setup().await;
        // El segundo instrumento trae una escala con b_model invalido (viola el CHECK).
        let mut bad_scale = sample_scale();
        bad_scale.b_model = "invalido".into();
        let payload = CatalogExport {
            instruments: vec![
                InstrumentExport {
                    name: "Bueno".into(),
                    kind: "digital".into(),
                    quantity: "tiempo".into(),
                    unit: "s".into(),
                    scales: vec![],
                },
                InstrumentExport {
                    name: "Malo".into(),
                    kind: "digital".into(),
                    quantity: "tiempo".into(),
                    unit: "s".into(),
                    scales: vec![bad_scale],
                },
            ],
        };

        let result = import_course(&pool, &course_id, payload).await;
        assert!(result.is_err());
        // Rollback total: no debe quedar ni siquiera el primer instrumento.
        assert!(list_instruments(&pool, &course_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn seed_instruments_populates_and_is_idempotent() {
        let (pool, _dir, course_id) = setup().await;
        seed_instruments(&pool, &course_id).await.unwrap();
        let first = list_instruments(&pool, &course_id).await.unwrap();
        assert!(first.len() >= 5);

        // Volver a sembrar no duplica.
        seed_instruments(&pool, &course_id).await.unwrap();
        let second = list_instruments(&pool, &course_id).await.unwrap();
        assert_eq!(first.len(), second.len());

        // El tester A830L tiene escalas de fabricante con resistencia interna.
        let a830 = first
            .iter()
            .find(|i| i.instrument.name.contains("A830L"))
            .unwrap();
        assert!(a830
            .scales
            .iter()
            .any(|s| s.b_model == "fabricante" && s.internal_res.is_some()));

        // El osciloscopio mide tiempo con incertidumbre tipo fabricante (1% de la medida).
        let osc_t = first
            .iter()
            .find(|i| i.instrument.name.contains("Osciloscopio") && i.instrument.quantity == "tiempo")
            .unwrap();
        assert_eq!(osc_t.scales[0].spec_pct_reading, Some(1.0));
        assert_eq!(osc_t.scales[0].spec_step_coeff, Some(0.0));
    }
}
