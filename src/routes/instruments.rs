//! Handlers de `/api/instruments*`: catálogo de instrumentos y escalas por curso.

use super::{current_user, require_teacher, Health, SharedState};
use crate::db;
use crate::error::AppError;
use crate::instruments::{self, CatalogExport, CreateInstrument, ScaleInput, UpdateInstrument};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

/// Parámetro de query `?course_id=...` para las operaciones de catálogo por curso.
#[derive(Debug, Deserialize)]
pub(super) struct CourseQuery {
    pub(super) course_id: String,
}

/// Cuerpo para importar un catálogo a un curso destino.
#[derive(Debug, Deserialize)]
pub(super) struct ImportRequest {
    course_id: String,
    instruments: Vec<instruments::InstrumentExport>,
}

/// `GET /api/instruments?course_id=...`: lista los instrumentos de un curso con sus escalas.
/// Solo lectura del catálogo (material del curso): accesible a cualquier usuario autenticado,
/// para que el estudiante pueda elegir instrumento/escala al cargar una entrega por formulario.
/// La gestión (alta/edición/baja) sigue siendo solo docente/admin.
pub(super) async fn list_instruments(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<CourseQuery>,
) -> Result<Json<Vec<instruments::InstrumentWithScales>>, AppError> {
    current_user(&state, &headers).await?;
    Ok(Json(
        instruments::list_instruments(&state.pool, &query.course_id).await?,
    ))
}

/// `POST /api/instruments`: crea un instrumento (docente/admin), validando tipo y campos.
pub(super) async fn create_instrument(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<CreateInstrument>,
) -> Result<Json<db::Instrument>, AppError> {
    require_teacher(&state, &headers).await?;
    if input.course_id.trim().is_empty() {
        return Err(AppError::bad_request("course_id requerido"));
    }
    validate_instrument(&input.kind, &input.name, &input.quantity, &input.unit)?;
    Ok(Json(
        instruments::create_instrument(&state.pool, input).await?,
    ))
}

/// `POST /api/instruments/{id}`: actualiza un instrumento (docente/admin).
pub(super) async fn update_instrument(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpdateInstrument>,
) -> Result<Json<db::Instrument>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_instrument(&input.kind, &input.name, &input.quantity, &input.unit)?;
    let updated = instruments::update_instrument(&state.pool, &id, input)
        .await?
        .ok_or_else(|| AppError::not_found("instrumento no encontrado"))?;
    Ok(Json(updated))
}

/// `DELETE /api/instruments/{id}`: elimina un instrumento y sus escalas (docente/admin).
pub(super) async fn delete_instrument(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !instruments::delete_instrument(&state.pool, &id).await? {
        return Err(AppError::not_found("instrumento no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `POST /api/instruments/{id}/scales`: agrega una escala (docente/admin), validando modelo y paso.
pub(super) async fn create_scale(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ScaleInput>,
) -> Result<Json<db::InstrumentScale>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_scale(&input)?;
    Ok(Json(
        instruments::create_scale(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/instruments/{id}/scales/{scale_id}`: actualiza una escala (docente/admin).
pub(super) async fn update_scale(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((_id, scale_id)): Path<(String, String)>,
    Json(input): Json<ScaleInput>,
) -> Result<Json<db::InstrumentScale>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_scale(&input)?;
    let updated = instruments::update_scale(&state.pool, &scale_id, input)
        .await?
        .ok_or_else(|| AppError::not_found("escala no encontrada"))?;
    Ok(Json(updated))
}

/// `DELETE /api/instruments/{id}/scales/{scale_id}`: elimina una escala (docente/admin).
pub(super) async fn delete_scale(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((_id, scale_id)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !instruments::delete_scale(&state.pool, &scale_id).await? {
        return Err(AppError::not_found("escala no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `GET /api/instruments/export?course_id=...`: exporta el catálogo del curso (docente/admin).
pub(super) async fn export_instruments(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<CourseQuery>,
) -> Result<Json<CatalogExport>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(
        instruments::export_course(&state.pool, &query.course_id).await?,
    ))
}

/// `POST /api/instruments/import`: importa un catálogo a un curso destino (docente/admin).
pub(super) async fn import_instruments(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(request): Json<ImportRequest>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if request.course_id.trim().is_empty() {
        return Err(AppError::bad_request("course_id requerido"));
    }
    instruments::import_course(
        &state.pool,
        &request.course_id,
        CatalogExport {
            instruments: request.instruments,
        },
    )
    .await?;
    Ok(Json(Health { status: "ok" }))
}

fn validate_instrument(kind: &str, name: &str, quantity: &str, unit: &str) -> Result<(), AppError> {
    if !matches!(kind.trim(), "analogico" | "digital") {
        return Err(AppError::bad_request("kind debe ser analogico o digital"));
    }
    if name.trim().is_empty() || quantity.trim().is_empty() || unit.trim().is_empty() {
        return Err(AppError::bad_request("datos de instrumento invalidos"));
    }
    Ok(())
}

/// Valida una escala: modelo de incertidumbre válido, paso positivo y campos no vacíos.
fn validate_scale(input: &ScaleInput) -> Result<(), AppError> {
    if !matches!(
        input.b_model.trim(),
        "resolucion" | "apreciacion" | "fabricante"
    ) {
        return Err(AppError::bad_request("b_model invalido"));
    }
    // Rechaza step no positivo y NaN (equivalente a un `> 0.0` negado, pero explícito).
    if input.step <= 0.0 || input.step.is_nan() {
        return Err(AppError::bad_request("step debe ser positivo"));
    }
    // Una escala de fabricante sin ningun termino positivo daria u_B = 0 silenciosamente.
    if input.b_model.trim() == "fabricante"
        && ![
            input.spec_pct_reading,
            input.spec_step_coeff,
            input.spec_fixed,
        ]
        .iter()
        .any(|value| matches!(value, Some(x) if *x > 0.0))
    {
        return Err(AppError::bad_request(
            "una escala de fabricante requiere al menos un termino de spec positivo",
        ));
    }
    if input.label.trim().is_empty() || input.unit.trim().is_empty() {
        return Err(AppError::bad_request("datos de escala invalidos"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construye una escala mínima para los tests de validación.
    fn scale(b_model: &str, step: f64) -> ScaleInput {
        ScaleInput {
            label: "L".into(),
            full_scale: None,
            step,
            appreciation: None,
            internal_res: None,
            internal_res_u: None,
            b_model: b_model.into(),
            spec_pct_reading: None,
            spec_step_coeff: None,
            spec_fixed: None,
            unit: "u".into(),
        }
    }

    #[test]
    fn validate_instrument_accepts_valid_and_rejects_invalid() {
        assert!(validate_instrument("digital", "Tester", "voltaje", "V").is_ok());
        assert!(validate_instrument("analogico", "Regla", "longitud", "mm").is_ok());
        assert!(validate_instrument("otro", "X", "q", "u").is_err());
        assert!(validate_instrument("digital", "  ", "q", "u").is_err());
    }

    #[test]
    fn validate_scale_checks_model_and_step() {
        assert!(validate_scale(&scale("resolucion", 0.1)).is_ok());
        assert!(validate_scale(&scale("apreciacion", 0.5)).is_ok());
        assert!(validate_scale(&scale("raro", 1.0)).is_err());
        assert!(validate_scale(&scale("resolucion", 0.0)).is_err());
    }

    #[test]
    fn validate_scale_fabricante_requires_spec() {
        // Sin ningún término de spec -> error (evita u_B = 0 silencioso).
        assert!(validate_scale(&scale("fabricante", 1.0)).is_err());
        // Con al menos un término positivo -> ok.
        let mut s = scale("fabricante", 1.0);
        s.spec_pct_reading = Some(1.0);
        assert!(validate_scale(&s).is_ok());
    }
}
