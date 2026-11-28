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

#[derive(Debug, Deserialize)]
pub(super) struct CourseQuery {
    pub(super) course_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ImportRequest {
    course_id: String,
    instruments: Vec<instruments::InstrumentExport>,
}

pub(super) async fn list_instruments(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<CourseQuery>,
) -> Result<Json<Vec<instruments::InstrumentWithScales>>, AppError> {
    let user = current_user(&state, &headers).await?;
    if !crate::courses::user_sees_course(&state.pool, &user, &query.course_id).await? {
        return Err(AppError::forbidden("No estás inscripto en ese curso."));
    }
    Ok(Json(
        instruments::list_instruments(&state.pool, &query.course_id).await?,
    ))
}

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

pub(super) async fn import_instruments(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(request): Json<ImportRequest>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if request.course_id.trim().is_empty() {
        return Err(AppError::bad_request("course_id requerido"));
    }
    validate_import(&request.instruments)?;
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
    if name.trim().is_empty()
        || crate::magnitudes::canonical(quantity).is_empty()
        || unit.trim().is_empty()
    {
        return Err(AppError::bad_request("datos de instrumento invalidos"));
    }
    Ok(())
}

fn validate_import(items: &[instruments::InstrumentExport]) -> Result<(), AppError> {
    for item in items {
        validate_instrument(&item.kind, &item.name, &item.quantity, &item.unit)?;
        item.scales.iter().try_for_each(validate_scale)?;
    }
    Ok(())
}

fn validate_scale(input: &ScaleInput) -> Result<(), AppError> {
    if !matches!(
        input.b_model.trim(),
        "resolucion" | "apreciacion" | "fabricante"
    ) {
        return Err(AppError::bad_request("b_model invalido"));
    }
    if input.step <= 0.0 || input.step.is_nan() {
        return Err(AppError::bad_request("step debe ser positivo"));
    }
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

pub(super) async fn list_magnitudes(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<&'static str>>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(
        crate::magnitudes::MAGNITUDES
            .iter()
            .map(|(name, _)| *name)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            u_cal_pct: None,
            u_cal_fixed: None,
            unit: "u".into(),
        }
    }

    #[test]
    fn validate_instrument_accepts_valid_and_rejects_invalid() {
        assert!(validate_instrument("digital", "Tester", "voltaje", "V").is_ok());
        assert!(validate_instrument("analogico", "Regla", "longitud", "mm").is_ok());
        assert!(validate_instrument("otro", "X", "q", "u").is_err());
        assert!(validate_instrument("digital", "  ", "q", "u").is_err());
        assert!(validate_instrument("digital", "X", "\u{301}", "u").is_err());
    }

    #[test]
    fn validate_scale_checks_model_and_step() {
        assert!(validate_scale(&scale("resolucion", 0.1)).is_ok());
        assert!(validate_scale(&scale("apreciacion", 0.5)).is_ok());
        assert!(validate_scale(&scale("raro", 1.0)).is_err());
        assert!(validate_scale(&scale("resolucion", 0.0)).is_err());
    }

    #[test]
    fn validate_import_checks_every_instrument_and_scale() {
        let item = |scales| instruments::InstrumentExport {
            name: "Tester".into(),
            kind: "digital".into(),
            quantity: "voltaje".into(),
            unit: "V".into(),
            scales,
        };
        assert!(validate_import(&[item(vec![scale("resolucion", 0.1)])]).is_ok());
        assert!(validate_import(&[item(vec![scale("resolucion", 0.0)])]).is_err());
        let mut nameless = item(vec![]);
        nameless.quantity = "  ".into();
        assert!(validate_import(&[item(vec![]), nameless]).is_err());
    }

    #[test]
    fn validate_scale_fabricante_requires_spec() {
        assert!(validate_scale(&scale("fabricante", 1.0)).is_err());
        let mut s = scale("fabricante", 1.0);
        s.spec_pct_reading = Some(1.0);
        assert!(validate_scale(&s).is_ok());
    }
}
