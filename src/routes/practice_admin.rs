use super::{current_user, ensure_submission_access, require_teacher, Health, SharedState};
use crate::{
    computation, db,
    error::AppError,
    practices::{
        self, AggregateInput, CurveInput, IntermediateInput, PointResultInput, QuantityInput,
        ResultInput,
    },
};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct SetAnalysisKindBody {
    analysis_kind: String,
}

async fn ensure_practice(state: &SharedState, practice_id: &str) -> Result<(), AppError> {
    practices::practice(&state.pool, practice_id)
        .await?
        .map(|_| ())
        .ok_or_else(|| AppError::not_found("practica no encontrada"))
}

async fn load_definition(
    state: &SharedState,
    practice_id: &str,
) -> Result<practices::PracticeDefinition, AppError> {
    practices::definition(&state.pool, practice_id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))
}

const ANALYSIS_KINDS: [&str; 3] = ["estadistico", "regresion_lineal", "curva"];

fn validate_practice_name(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::bad_request("La practica necesita un nombre."));
    }
    Ok(())
}

pub(super) async fn create_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<practices::NewPracticeInput>,
) -> Result<Json<db::Practice>, AppError> {
    let user = require_teacher(&state, &headers).await?;
    validate_practice_name(&input.name)?;
    if !ANALYSIS_KINDS.contains(&input.analysis_kind.trim()) {
        return Err(AppError::bad_request(
            "analysis_kind debe ser estadistico, regresion_lineal o curva",
        ));
    }
    Ok(Json(
        practices::create_practice(&state.pool, &input, &user.id).await?,
    ))
}

pub(super) async fn update_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<practices::PracticeInfoInput>,
) -> Result<Json<db::Practice>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_practice_name(&input.name)?;
    practices::update_practice(&state.pool, &id, &input)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found("practica no encontrada"))
}

#[derive(Debug, Deserialize)]
pub(super) struct DuplicateBody {
    #[serde(default)]
    name: Option<String>,
}

pub(super) async fn duplicate_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<DuplicateBody>,
) -> Result<Json<db::Practice>, AppError> {
    let user = require_teacher(&state, &headers).await?;
    let source = practices::practice(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    let name = body
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("Copia de {}", source.name));
    practices::duplicate_practice(&state.pool, &id, &name, &user.id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found("practica no encontrada"))
}

#[derive(Debug, Deserialize)]
pub(super) struct StatusBody {
    status: String,
}

fn publish_problems(def: &practices::PracticeDefinition) -> Vec<String> {
    let mut problems = structure_problems(def);
    problems.extend(
        formula_problems(def)
            .into_iter()
            .map(|(element, message)| format!("{element}: {message}")),
    );
    problems
}

fn structure_problems(def: &practices::PracticeDefinition) -> Vec<String> {
    let mut problems = Vec::new();
    if def.quantities.is_empty() {
        problems.push("no tiene magnitudes de entrada".to_string());
    }
    if def.parts.is_empty() {
        problems.extend(kind_problems(def));
        return problems;
    }
    if def.results.is_empty()
        && def.curves.is_empty()
        && def.point_results.is_empty()
        && def.aggregates.is_empty()
    {
        problems.push("no tiene mensurandos".to_string());
    }
    if let Some(series) = def.stray_series() {
        problems.push(format!(
            "tiene magnitudes por punto fuera de la parte \"{}\", que es la de la serie",
            series.title
        ));
    }
    for part in def
        .parts
        .iter()
        .filter(|p| def.part_has_content(p) && def.part_kind(p) != "estadistico")
    {
        problems.extend(
            kind_problems(&def.part_view(part))
                .into_iter()
                .map(|p| format!("la parte \"{}\" {p}", part.title)),
        );
    }
    problems
}

fn kind_problems(def: &practices::PracticeDefinition) -> Vec<String> {
    let mut problems = Vec::new();
    match def.analysis_kind.as_deref() {
        Some("curva") if def.curves.is_empty() => {
            problems.push("es de tipo curva y no tiene curvas".to_string());
        }
        Some("regresion_lineal") if def.x_formula.is_none() || def.y_formula.is_none() => {
            problems.push("es de regresion y le faltan las formulas de los ejes".to_string());
        }
        Some("curva") => {}
        _ if def.results.is_empty() => {
            problems.push("no tiene mensurandos".to_string());
        }
        _ => {}
    }
    problems
}

pub(super) async fn set_practice_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<db::Practice>, AppError> {
    let user = require_teacher(&state, &headers).await?;
    let status = body.status.trim();
    if status == "publicada" {
        match practices::publish_draft(&state.pool, &id, Some(&user.id), publish_problems).await? {
            practices::DraftPublish::Published => {
                return practices::practice(&state.pool, &id)
                    .await?
                    .map(Json)
                    .ok_or_else(|| AppError::not_found("practica no encontrada"));
            }
            practices::DraftPublish::Rejected(problems) => {
                return Err(AppError::bad_request(format!(
                    "Todavia no se puede publicar: {}.",
                    problems.join("; ")
                )));
            }
            practices::DraftPublish::NotFound => {
                return Err(AppError::not_found("practica no encontrada"));
            }
            practices::DraftPublish::NotDraft => {
                if let Some(version) = practices::current_version(&state.pool, &id).await? {
                    let problems = publish_problems(&version.definition);
                    if !problems.is_empty() {
                        return Err(AppError::bad_request(format!(
                            "No se puede restaurar la versión {}: {}.",
                            version.number,
                            problems.join("; ")
                        )));
                    }
                }
            }
        }
    }
    match practices::set_practice_status(&state.pool, &id, status).await? {
        practices::StatusChange::Done => practices::practice(&state.pool, &id)
            .await?
            .map(Json)
            .ok_or_else(|| AppError::not_found("practica no encontrada")),
        practices::StatusChange::NotFound => Err(AppError::not_found("practica no encontrada")),
        practices::StatusChange::NotAllowed(current) => Err(AppError::bad_request(format!(
            "Una practica {current} no puede pasar a {status}."
        ))),
    }
}

#[derive(Debug, Serialize)]
pub(super) struct PublishedVersion {
    number: i64,
    new_version: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct PublishVersionBody {
    #[serde(default)]
    expected_fingerprint: Option<String>,
}

pub(super) async fn publish_practice_version(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<PublishVersionBody>,
) -> Result<Json<PublishedVersion>, AppError> {
    let user = require_teacher(&state, &headers).await?;
    match practices::publish_checked(
        &state.pool,
        &id,
        Some(&user.id),
        body.expected_fingerprint.as_deref(),
        publish_problems,
    )
    .await?
    {
        practices::Publish::Published { number } => Ok(Json(PublishedVersion {
            number,
            new_version: true,
        })),
        practices::Publish::UpdatedCurrent { number } => Ok(Json(PublishedVersion {
            number,
            new_version: false,
        })),
        practices::Publish::DraftChanged => Err(AppError::bad_request(
            "El borrador cambió desde que se mostró el aviso: revisá el resumen y volvé a publicar.",
        )),
        practices::Publish::Rejected(problems) => Err(AppError::bad_request(format!(
            "Todavía no se puede publicar una versión nueva: {}.",
            problems.join("; ")
        ))),
        practices::Publish::Unchanged(number) => Err(AppError::bad_request(format!(
            "No hay cambios desde la versión {number}: no hace falta publicar otra."
        ))),
        practices::Publish::IsDraft => Err(AppError::bad_request(
            "La práctica es un borrador: publicala para crear su primera versión.",
        )),
        practices::Publish::NotFound => Err(AppError::not_found("practica no encontrada")),
    }
}

pub(super) async fn practice_draft_changes(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Option<practices::DraftChanges>>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(practices::draft_changes(&state.pool, &id).await?))
}

pub(super) async fn delete_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    match practices::delete_practice(&state.pool, &id).await? {
        practices::DeletePractice::Deleted => Ok(Json(Health { status: "ok" })),
        practices::DeletePractice::NotFound => Err(AppError::not_found("practica no encontrada")),
        practices::DeletePractice::NotDraft => Err(AppError::bad_request(
            "Solo se puede eliminar un borrador; una practica publicada se archiva.",
        )),
        practices::DeletePractice::HasSubmissions => Err(AppError::bad_request(
            "La practica tiene entregas: no se puede eliminar.",
        )),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct DefinitionQuery {
    #[serde(default)]
    published: bool,
}

pub(super) async fn practice_definition(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<DefinitionQuery>,
) -> Result<Json<practices::PracticeDefinition>, AppError> {
    let user = current_user(&state, &headers).await?;
    ensure_student_can_see(&state, &user, &id, &["publicada", "archivada"]).await?;
    if matches!(user.role.as_str(), "docente" | "admin") && !query.published {
        return Ok(Json(load_definition(&state, &id).await?));
    }
    Ok(Json(published_definition(&state, &id).await?))
}

async fn published_definition(
    state: &SharedState,
    practice_id: &str,
) -> Result<practices::PracticeDefinition, AppError> {
    practices::published_definition(&state.pool, practice_id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))
}

async fn ensure_student_can_see(
    state: &SharedState,
    user: &db::AuthUser,
    practice_id: &str,
    allowed: &[&str],
) -> Result<(), AppError> {
    if matches!(user.role.as_str(), "docente" | "admin") {
        return Ok(());
    }
    match practices::practice(&state.pool, practice_id).await? {
        Some(p) if allowed.contains(&p.status.as_str()) => Ok(()),
        _ => Err(AppError::not_found("practica no encontrada")),
    }
}

#[derive(serde::Deserialize)]
pub(super) struct AnalyzePreviewBody {
    measurements: Vec<computation::MeasurementInput>,
    #[serde(default)]
    submission_id: Option<String>,
    #[serde(default)]
    practice_version: Option<i64>,
    #[serde(default)]
    draft: bool,
}

pub(super) async fn analyze_preview(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AnalyzePreviewBody>,
) -> Result<Json<computation::FormAnalysis>, AppError> {
    let user = current_user(&state, &headers).await?;
    ensure_student_can_see(&state, &user, &id, &["publicada"]).await?;
    if body.draft {
        require_teacher(&state, &headers).await?;
    }
    let definition = match &body.submission_id {
        _ if body.draft => load_definition(&state, &id).await?,
        Some(submission_id) => {
            ensure_submission_access(&state, &user, submission_id).await?;
            practices::submission_definition(&state.pool, submission_id)
                .await?
                .filter(|d| d.practice_id == id)
                .ok_or_else(|| AppError::not_found("entrega no encontrada"))?
        }
        None if body.practice_version.is_some() => {
            practices::version_for_form(&state.pool, &id, body.practice_version)
                .await
                .map_err(AppError::from_domain_or_db)?
                .definition
        }
        None => published_definition(&state, &id).await?,
    };
    let analysis = computation::analyze_definition(&state.pool, &definition, &body.measurements)
        .await
        .map_err(AppError::from_domain_or_db)?;
    Ok(Json(analysis))
}

pub(super) async fn set_practice_analysis_kind(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<SetAnalysisKindBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let kind = parse_analysis_kind(&body.analysis_kind)?;
    let before = load_definition(&state, &id).await?;
    if !before.parts.is_empty() {
        return Err(AppError::bad_request(
            "La práctica tiene partes: el tipo de análisis se elige en cada parte, en \"Partes y secciones\".",
        ));
    }
    let mut after = before.clone();
    after.analysis_kind = Some(kind.to_string());
    guard_change(&before, &after, "cambiar el tipo de analisis")?;
    if !practices::set_analysis_kind(&state.pool, &id, kind).await? {
        return Err(AppError::not_found("practica no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

#[derive(Debug, Deserialize)]
pub(super) struct RegressionFormulasBody {
    x_formula: String,
    y_formula: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct OperatorCountBody {
    count: i64,
}

pub(super) async fn set_practice_operator_count(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<OperatorCountBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if body.count > 20 {
        return Err(AppError::bad_request(
            "La cantidad de operadores no puede superar 20.",
        ));
    }
    if !practices::set_operator_count(&state.pool, &id, body.count).await? {
        return Err(AppError::not_found("practica no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn set_practice_regression_formulas(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RegressionFormulasBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    check_axes(&def, &body.x_formula, &body.y_formula)?;
    let mut after = def.clone();
    let axis = |f: &str| Some(f.trim().to_string()).filter(|f| !f.is_empty());
    after.x_formula = axis(&body.x_formula);
    after.y_formula = axis(&body.y_formula);
    guard_change(&def, &after, "guardar los ejes")?;
    if !practices::set_regression_formulas(&state.pool, &id, &body.x_formula, &body.y_formula)
        .await?
    {
        return Err(AppError::not_found("practica no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn create_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<QuantityInput>,
) -> Result<Json<db::PracticeQuantity>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_quantity(&input)?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    Ok(Json(
        practices::create_quantity(&state.pool, &id, input).await?,
    ))
}

pub(super) async fn update_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((practice_id, qid)): Path<(String, String)>,
    Json(input): Json<QuantityInput>,
) -> Result<Json<db::PracticeQuantity>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_quantity(&input)?;
    if let Some(n) = input.slot_count {
        let bad = practices::submissions_off_slots(&state.pool, &qid, n).await?;
        if bad > 0 {
            return Err(AppError::bad_request(format!(
                "Hay {bad} entrega(s) con valores de esta magnitud que no son una ranura de 1 a {n}: \
                 no se pueden poner las ranuras sin dejarlas imposibles de editar."
            )));
        }
    }
    guard_symbol(
        &state.pool,
        &practice_id,
        &input.symbol,
        Some(&qid),
        None,
        None,
        None,
        None,
    )
    .await?;
    let before = load_definition(&state, &practice_id).await?;
    let mut after = before.clone();
    let q = after
        .quantities
        .iter_mut()
        .find(|q| q.id == qid)
        .ok_or_else(|| AppError::not_found("magnitud no encontrada en la practica"))?;
    let old_symbol = std::mem::replace(&mut q.symbol, input.symbol.trim().to_string());
    q.per_point = input.per_point;
    q.is_given = input.is_given;
    let new_symbol = q.symbol.clone();
    for section in &mut after.sections {
        for symbol in &mut section.quantities {
            if *symbol == old_symbol {
                symbol.clone_from(&new_symbol);
            }
        }
    }
    guard_change(&before, &after, "guardar la magnitud")?;
    let updated = practices::update_quantity(&state.pool, &qid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud no encontrada"))?;
    Ok(Json(updated))
}

#[derive(Debug, Serialize)]
pub(super) struct RemovedQuantity {
    retired: bool,
}

pub(super) async fn delete_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, qid)): Path<(String, String)>,
) -> Result<Json<RemovedQuantity>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.quantities.retain(|q| q.id != qid);
    if after.quantities.len() == before.quantities.len() {
        return Err(AppError::not_found("magnitud no encontrada en la practica"));
    }
    guard_change(&before, &after, "eliminar la magnitud")?;
    match practices::remove_quantity(&state.pool, &id, &qid).await? {
        practices::QuantityRemoval::NotFound => Err(AppError::not_found("magnitud no encontrada")),
        practices::QuantityRemoval::Retired => Ok(Json(RemovedQuantity { retired: true })),
        practices::QuantityRemoval::HasMeasurements => Err(AppError::bad_request(
            "La magnitud tiene mediciones guardadas en entregas sin versión, así que no se puede borrar. Publicá la práctica primero.",
        )),
        practices::QuantityRemoval::Deleted => Ok(Json(RemovedQuantity { retired: false })),
    }
}

pub(super) async fn create_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<CurveInput>,
) -> Result<Json<practices::PracticeCurve>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_curve(&def, &input)?;
    Ok(Json(
        practices::create_curve(&state.pool, &id, input).await?,
    ))
}

pub(super) async fn update_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, cid)): Path<(String, String)>,
    Json(input): Json<CurveInput>,
) -> Result<Json<practices::PracticeCurve>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_curve(&def, &input)?;
    let updated = practices::update_curve(&state.pool, &id, &cid, input)
        .await?
        .ok_or_else(|| AppError::not_found("curva no encontrada"))?;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub(super) struct MoveBody {
    #[serde(default)]
    up: bool,
}

async fn move_in_list(
    state: &SharedState,
    headers: &HeaderMap,
    list: practices::Orderable,
    practice_id: &str,
    item_id: &str,
    up: bool,
) -> Result<Json<Health>, AppError> {
    require_teacher(state, headers).await?;
    if practices::move_item(&state.pool, list, practice_id, item_id, up)
        .await?
        .is_none()
    {
        return Err(AppError::not_found("elemento no encontrado en la practica"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn move_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, cid)): Path<(String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Health>, AppError> {
    move_in_list(
        &state,
        &headers,
        practices::Orderable::Curves,
        &id,
        &cid,
        body.up,
    )
    .await
}

pub(super) async fn move_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, qid)): Path<(String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Health>, AppError> {
    move_in_list(
        &state,
        &headers,
        practices::Orderable::Quantities,
        &id,
        &qid,
        body.up,
    )
    .await
}

pub(super) async fn move_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, rid)): Path<(String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Health>, AppError> {
    move_in_list(
        &state,
        &headers,
        practices::Orderable::Results,
        &id,
        &rid,
        body.up,
    )
    .await
}

pub(super) async fn create_intermediate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<IntermediateInput>,
) -> Result<Json<practices::PracticeIntermediate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_intermediate(&def, &input, None)?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    Ok(Json(
        practices::create_intermediate(&state.pool, &id, input).await?,
    ))
}

pub(super) async fn update_intermediate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, iid)): Path<(String, String)>,
    Json(input): Json<IntermediateInput>,
) -> Result<Json<practices::PracticeIntermediate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_intermediate(&def, &input, Some(&iid))?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        Some(&iid),
        None,
        None,
    )
    .await?;
    let mut after = def.clone();
    let item = after
        .intermediates
        .iter_mut()
        .find(|x| x.id == iid)
        .ok_or_else(|| AppError::not_found("magnitud intermedia no encontrada"))?;
    item.symbol = input.symbol.trim().to_string();
    item.formula = input.formula.trim().to_string();
    guard_change(&def, &after, "guardar la magnitud intermedia")?;
    let updated = practices::update_intermediate(&state.pool, &id, &iid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud intermedia no encontrada"))?;
    Ok(Json(updated))
}

pub(super) async fn delete_intermediate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, iid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.intermediates.retain(|x| x.id != iid);
    if after.intermediates.len() == before.intermediates.len() {
        return Err(AppError::not_found("magnitud intermedia no encontrada"));
    }
    guard_change(&before, &after, "eliminar la magnitud intermedia")?;
    if !practices::delete_intermediate(&state.pool, &id, &iid).await? {
        return Err(AppError::not_found("magnitud intermedia no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

fn validate_intermediate(
    def: &practices::PracticeDefinition,
    input: &IntermediateInput,
    exclude_id: Option<&str>,
) -> Result<(), AppError> {
    validate_display_symbol(&input.display_symbol)?;
    let view = def.point_view();
    let def = &view;
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        return Err(AppError::bad_request(
            "La magnitud intermedia necesita un simbolo y una formula.",
        ));
    }
    validate_symbol_format(symbol)?;
    validate_symbol_not_reserved(symbol)?;
    let self_pos = exclude_id.and_then(|id| {
        def.intermediates
            .iter()
            .find(|it| it.id == id)
            .map(|it| it.position)
    });
    let mut allowed: Vec<String> = def.quantities.iter().map(|q| q.symbol.clone()).collect();
    for it in &def.intermediates {
        if Some(it.id.as_str()) == exclude_id {
            continue;
        }
        if self_pos.is_none_or(|p| it.position < p) {
            allowed.push(it.symbol.clone());
        }
    }
    computation::check_formula(formula, &allowed)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(())
}

pub(super) async fn create_point_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PointResultInput>,
) -> Result<Json<practices::PracticePointResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_point_result(&def, &input)?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    Ok(Json(
        practices::create_point_result(&state.pool, &id, input).await?,
    ))
}

pub(super) async fn update_point_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    Json(input): Json<PointResultInput>,
) -> Result<Json<practices::PracticePointResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_point_result(&def, &input)?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        Some(&pid),
        None,
    )
    .await?;
    let updated = practices::update_point_result(&state.pool, &id, &pid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud derivada por punto no encontrada"))?;
    Ok(Json(updated))
}

pub(super) async fn delete_point_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::delete_point_result(&state.pool, &id, &pid).await? {
        return Err(AppError::not_found(
            "magnitud derivada por punto no encontrada",
        ));
    }
    Ok(Json(Health { status: "ok" }))
}

fn validate_point_result(
    def: &practices::PracticeDefinition,
    input: &PointResultInput,
) -> Result<(), AppError> {
    validate_display_symbol(&input.display_symbol)?;
    let view = def.point_view();
    let def = &view;
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        return Err(AppError::bad_request(
            "La magnitud derivada por punto necesita un simbolo y una formula.",
        ));
    }
    validate_symbol_format(symbol)?;
    validate_symbol_not_reserved(symbol)?;
    let mut allowed: Vec<String> = def.quantities.iter().map(|q| q.symbol.clone()).collect();
    allowed.extend(def.intermediates.iter().map(|it| it.symbol.clone()));
    allowed.extend(def.results.iter().map(|r| r.symbol.clone()));
    allowed.push("slope".into());
    allowed.push("intercept".into());
    computation::check_formula(formula, &allowed)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(())
}

pub(super) async fn create_aggregate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AggregateInput>,
) -> Result<Json<practices::PracticeAggregate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_aggregate(&def, &input, None)?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    Ok(Json(
        practices::create_aggregate(&state.pool, &id, input).await?,
    ))
}

pub(super) async fn update_aggregate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, aid)): Path<(String, String)>,
    Json(input): Json<AggregateInput>,
) -> Result<Json<practices::PracticeAggregate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_aggregate(&def, &input, Some(&aid))?;
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        Some(&aid),
    )
    .await?;
    let mut after = def.clone();
    let item = after
        .aggregates
        .iter_mut()
        .find(|x| x.id == aid)
        .ok_or_else(|| AppError::not_found("mensurando agregado no encontrado"))?;
    item.symbol = input.symbol.trim().to_string();
    item.formula = input.formula.trim().to_string();
    guard_change(&def, &after, "guardar el agregado")?;
    let updated = practices::update_aggregate(&state.pool, &id, &aid, input)
        .await?
        .ok_or_else(|| AppError::not_found("mensurando agregado no encontrado"))?;
    Ok(Json(updated))
}

pub(super) async fn delete_aggregate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, aid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.aggregates.retain(|x| x.id != aid);
    if after.aggregates.len() == before.aggregates.len() {
        return Err(AppError::not_found("mensurando agregado no encontrado"));
    }
    guard_change(&before, &after, "eliminar el agregado")?;
    if !practices::delete_aggregate(&state.pool, &id, &aid).await? {
        return Err(AppError::not_found("mensurando agregado no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

fn validate_aggregate(
    def: &practices::PracticeDefinition,
    input: &AggregateInput,
    exclude_id: Option<&str>,
) -> Result<(), AppError> {
    validate_display_symbol(&input.display_symbol)?;
    let view = def.point_view();
    let def = &view;
    let symbol = input.symbol.trim();
    let formula = input.formula.trim();
    if symbol.is_empty() || formula.is_empty() {
        return Err(AppError::bad_request(
            "El mensurando agregado necesita un simbolo y una formula.",
        ));
    }
    validate_symbol_format(symbol)?;
    validate_symbol_not_reserved(symbol)?;
    let mut allowed: Vec<String> = def
        .quantities
        .iter()
        .filter(|q| !q.per_point || q.is_given)
        .map(|q| q.symbol.clone())
        .collect();
    allowed.extend(def.results.iter().map(|r| r.symbol.clone()));
    allowed.push("slope".into());
    allowed.push("intercept".into());
    let self_pos = exclude_id.and_then(|id| {
        def.aggregates
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.position)
    });
    for a in &def.aggregates {
        if Some(a.id.as_str()) == exclude_id {
            continue;
        }
        if self_pos.is_none_or(|p| a.position < p) {
            allowed.push(a.symbol.clone());
        }
    }
    let endpoint_bases = def
        .quantities
        .iter()
        .filter(|q| q.per_point && !q.is_given)
        .map(|q| q.symbol.clone())
        .chain(def.intermediates.iter().map(|it| it.symbol.clone()));
    for base in endpoint_bases {
        for suffix in ["first", "first2", "last", "last2"] {
            allowed.push(format!("{base}_{suffix}"));
        }
    }
    computation::check_formula(formula, &allowed)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(())
}

fn validate_curve(def: &practices::PracticeDefinition, input: &CurveInput) -> Result<(), AppError> {
    if input.x_formula.trim().is_empty() || input.y_formula.trim().is_empty() {
        return Err(AppError::bad_request(
            "La curva necesita las formulas de ambos ejes (x e y).",
        ));
    }
    check_axes(def, &input.x_formula, &input.y_formula)
}

fn cycle_problems(def: &practices::PracticeDefinition) -> Vec<(String, String)> {
    let uses: Vec<Vec<usize>> = def
        .results
        .iter()
        .map(|r| {
            computation::formula_symbols(&r.formula)
                .iter()
                .filter_map(|s| def.results.iter().position(|o| o.symbol == *s))
                .collect()
        })
        .collect();
    let (_, blocked) = computation::chain_order(&uses);
    blocked
        .into_iter()
        .map(|i| {
            (
                format!("el mensurando {}", def.results[i].symbol),
                "se usa a sí mismo a través de otros mensurandos (forma un ciclo, o depende de uno que lo forma)"
                    .to_string(),
            )
        })
        .collect()
}

fn formula_problems(def: &practices::PracticeDefinition) -> Vec<(String, String)> {
    let mut problems = cycle_problems(def);
    if def.parts.is_empty() {
        problems.extend(formula_problems_in(def));
        return problems;
    }
    problems.extend(formula_problems_in(&def.shared_view()).into_iter().map(
        |(element, message)| {
            (
                element,
                format!(
                    "{message} (está en los datos compartidos, que solo pueden usar datos \
                     compartidos: pasalo a una sección de la parte que corresponda)"
                ),
            )
        },
    ));
    for part in def.parts.iter().filter(|p| def.part_has_content(p)) {
        problems.extend(formula_problems_in(&def.part_view(part)).into_iter().map(
            |(element, message)| {
                (
                    element,
                    format!(
                        "{message} (está en la parte \"{}\", que solo ve lo compartido y lo suyo)",
                        part.title
                    ),
                )
            },
        ));
    }
    problems
}

fn formula_problems_in(def: &practices::PracticeDefinition) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    let mut note = |what: &str, symbol: &str, result: Result<(), AppError>| {
        if let Err(e) = result {
            problems.push((format!("{what} {symbol}"), e.message().to_string()));
        }
    };
    for r in &def.results {
        let input = ResultInput {
            display_symbol: None,
            symbol: r.symbol.clone(),
            name: r.name.clone(),
            unit: r.unit.clone(),
            formula: r.formula.clone(),
            tolerance: None,
            is_final: r.is_final,
            has_uncertainty: r.has_uncertainty,
        };
        note("el mensurando", &r.symbol, validate_result(def, &input));
    }
    let kind = def.analysis_kind.as_deref();
    if kind == Some("regresion_lineal") {
        if let (Some(x), Some(y)) = (&def.x_formula, &def.y_formula) {
            note("el ajuste", "lineal", check_axes(def, x, y));
        }
    }
    if kind == Some("curva") {
        for c in &def.curves {
            note(
                "la curva",
                &c.y_formula,
                check_axes(def, &c.x_formula, &c.y_formula),
            );
        }
    }
    if matches!(kind, Some("regresion_lineal" | "curva")) {
        for it in &def.intermediates {
            let input = IntermediateInput {
                display_symbol: None,
                symbol: it.symbol.clone(),
                name: it.name.clone(),
                unit: it.unit.clone(),
                formula: it.formula.clone(),
            };
            note(
                "la intermedia",
                &it.symbol,
                validate_intermediate(def, &input, Some(&it.id)),
            );
        }
    }
    if kind == Some("regresion_lineal") {
        for pr in &def.point_results {
            let input = PointResultInput {
                display_symbol: None,
                symbol: pr.symbol.clone(),
                name: pr.name.clone(),
                unit: pr.unit.clone(),
                formula: pr.formula.clone(),
            };
            note(
                "la derivada por punto",
                &pr.symbol,
                validate_point_result(def, &input),
            );
        }
        for a in &def.aggregates {
            let input = AggregateInput {
                display_symbol: None,
                symbol: a.symbol.clone(),
                name: a.name.clone(),
                unit: a.unit.clone(),
                formula: a.formula.clone(),
                is_final: a.is_final,
            };
            note(
                "el agregado",
                &a.symbol,
                validate_aggregate(def, &input, Some(&a.id)),
            );
        }
    }
    problems
}

fn guard_series_part(
    before: &practices::PracticeDefinition,
    after: &practices::PracticeDefinition,
    what: &str,
) -> Result<(), AppError> {
    let after_parts = after.series_parts();
    let before_parts = before.series_parts();
    if after_parts.len() > 1 && !after_parts.iter().all(|p| before_parts.contains(p)) {
        return Err(AppError::bad_request(format!(
            "No se puede {what}: las magnitudes por punto forman una sola serie y tienen que \
             estar en secciones de la misma parte. Para pasarlas a otra parte, dejá sus \
             secciones primero sin parte (compartidas) y después asignales la nueva."
        )));
    }
    if let Some(series) = after.stray_series() {
        if before.stray_series().is_none() {
            return Err(AppError::bad_request(format!(
                "No se puede {what}: las magnitudes por punto tienen que estar en la parte de la \
                 serie (\"{}\"). Para cambiar de parte la serie, dejá sus secciones sin parte, \
                 elegí la nueva parte de la serie y después asignáselas.",
                series.title
            )));
        }
    }
    Ok(())
}

fn guard_change(
    before: &practices::PracticeDefinition,
    after: &practices::PracticeDefinition,
    what: &str,
) -> Result<(), AppError> {
    guard_series_part(before, after, what)?;
    if after.status == "publicada" {
        let before_missing = structure_problems(before);
        if let Some(missing) = structure_problems(after)
            .into_iter()
            .find(|p| !before_missing.contains(p))
        {
            return Err(AppError::bad_request(format!(
                "No se puede {what}: la practica esta publicada y quedaria sin lo necesario ({missing}). \
                 Archivala si ya no se usa."
            )));
        }
    }
    let already_broken: Vec<String> = formula_problems(before)
        .into_iter()
        .map(|(element, _)| element)
        .collect();
    match formula_problems(after)
        .into_iter()
        .find(|(element, _)| !already_broken.contains(element))
    {
        Some((element, message)) => Err(AppError::bad_request(format!(
            "No se puede {what}: {element}: {message}"
        ))),
        None => Ok(()),
    }
}

fn axis_scope(def: &practices::PracticeDefinition) -> Vec<String> {
    let def = def.point_view();
    def.quantities
        .iter()
        .map(|q| q.symbol.clone())
        .chain(def.intermediates.iter().map(|it| it.symbol.clone()))
        .collect()
}

fn check_axes(def: &practices::PracticeDefinition, x: &str, y: &str) -> Result<(), AppError> {
    let allowed = axis_scope(def);
    for (axis, formula) in [("x", x.trim()), ("y", y.trim())] {
        if formula.is_empty() {
            continue;
        }
        computation::check_formula(formula, &allowed)
            .map_err(|e| AppError::bad_request(format!("Eje {axis}: {e}")))?;
    }
    Ok(())
}

fn result_scope(def: &practices::PracticeDefinition) -> Vec<String> {
    if def.parts.is_empty() {
        return result_scope_in(def);
    }
    let mut allowed = result_scope_in(&def.shared_view());
    for part in &def.parts {
        allowed.extend(result_scope_in(&def.part_view(part)));
    }
    allowed.sort();
    allowed.dedup();
    allowed
}

fn result_scope_in(def: &practices::PracticeDefinition) -> Vec<String> {
    let shared = def
        .quantities
        .iter()
        .filter(|q| !q.per_point || q.is_given)
        .map(|q| q.symbol.clone())
        .chain(def.results.iter().map(|r| r.symbol.clone()));
    match def.analysis_kind.as_deref() {
        Some("regresion_lineal") => shared
            .chain(["slope".to_string(), "intercept".to_string()])
            .collect(),
        Some("curva") => {
            let per_point: Vec<&str> = def
                .quantities
                .iter()
                .filter(|q| q.per_point && !q.is_given)
                .map(|q| q.symbol.as_str())
                .chain(def.intermediates.iter().map(|it| it.symbol.as_str()))
                .collect();
            let mut allowed: Vec<String> = shared.collect();
            for s in &per_point {
                allowed.push(format!("{s}_max"));
                allowed.extend(
                    per_point
                        .iter()
                        .filter(|t| *t != s)
                        .map(|t| format!("{t}_at_{s}_max")),
                );
            }
            allowed
        }
        _ => def
            .quantities
            .iter()
            .map(|q| q.symbol.clone())
            .chain(def.results.iter().map(|r| r.symbol.clone()))
            .collect(),
    }
}

pub(super) async fn delete_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, cid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.curves.retain(|c| c.id != cid);
    guard_change(&before, &after, "eliminar la curva")?;
    if !practices::delete_curve(&state.pool, &id, &cid).await? {
        return Err(AppError::not_found("curva no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

#[derive(Debug, Deserialize)]
pub(super) struct NewResultBody {
    #[serde(flatten)]
    result: ResultInput,
    #[serde(default)]
    section_id: Option<String>,
}

fn with_new_result(
    def: &practices::PracticeDefinition,
    input: &ResultInput,
    section_id: Option<&str>,
) -> practices::PracticeDefinition {
    let mut after = def.clone();
    let symbol = input.symbol.trim().to_string();
    after.results.push(db::PracticeResult {
        id: String::new(),
        practice_id: def.practice_id.clone(),
        symbol: symbol.clone(),
        name: input.name.trim().to_string(),
        unit: input.unit.trim().to_string(),
        formula: input.formula.trim().to_string(),
        position: i64::MAX,
        tolerance: input.tolerance.flatten(),
        is_final: input.is_final,
        has_uncertainty: input.has_uncertainty,
    });
    if let Some(section) = after
        .sections
        .iter_mut()
        .find(|s| Some(s.id.as_str()) == section_id)
    {
        section.results.push(symbol);
    }
    after
}

fn with_updated_result(
    def: &practices::PracticeDefinition,
    result_id: &str,
    input: &ResultInput,
) -> Option<practices::PracticeDefinition> {
    let mut after = def.clone();
    let item = after.results.iter_mut().find(|x| x.id == result_id)?;
    let old_symbol = std::mem::replace(&mut item.symbol, input.symbol.trim().to_string());
    item.formula = input.formula.trim().to_string();
    let new_symbol = item.symbol.clone();
    for symbol in after.sections.iter_mut().flat_map(|s| s.results.iter_mut()) {
        if *symbol == old_symbol {
            symbol.clone_from(&new_symbol);
        }
    }
    Some(after)
}

pub(super) async fn create_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<NewResultBody>,
) -> Result<Json<db::PracticeResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let NewResultBody {
        result: input,
        section_id,
    } = body;
    let section_id = section_id.filter(|s| !s.is_empty());
    let def = load_definition(&state, &id).await?;
    validate_result(&def, &input)?;
    if let Some(section) = &section_id {
        if !def.sections.iter().any(|s| &s.id == section) {
            return Err(AppError::bad_request("La seccion no es de esta practica."));
        }
    }
    guard_symbol(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    guard_change(
        &def,
        &with_new_result(&def, &input, section_id.as_deref()),
        "guardar el mensurando",
    )?;
    Ok(Json(
        practices::create_result(&state.pool, &id, input, section_id.as_deref()).await?,
    ))
}

pub(super) async fn update_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((practice_id, rid)): Path<(String, String)>,
    Json(input): Json<ResultInput>,
) -> Result<Json<db::PracticeResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &practice_id).await?;
    validate_result(&def, &input)?;
    guard_symbol(
        &state.pool,
        &practice_id,
        &input.symbol,
        None,
        Some(&rid),
        None,
        None,
        None,
    )
    .await?;
    let after = with_updated_result(&def, &rid, &input)
        .ok_or_else(|| AppError::not_found("mensurando no encontrado en la practica"))?;
    guard_change(&def, &after, "guardar el mensurando")?;
    let updated = practices::update_result(&state.pool, &rid, input)
        .await?
        .ok_or_else(|| AppError::not_found("mensurando no encontrado"))?;
    Ok(Json(updated))
}

pub(super) async fn delete_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, rid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.results.retain(|x| x.id != rid);
    if after.results.len() == before.results.len() {
        return Err(AppError::not_found(
            "mensurando no encontrado en la practica",
        ));
    }
    guard_change(&before, &after, "eliminar el mensurando")?;
    if !practices::delete_result(&state.pool, &rid).await? {
        return Err(AppError::not_found("mensurando no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn set_result_tolerance(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((practice_id, rid)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let tolerance = match body.get("tolerance") {
        Some(serde_json::Value::Null) | None => None,
        Some(serde_json::Value::Number(n)) => {
            let v = n
                .as_f64()
                .ok_or_else(|| AppError::bad_request("tolerancia debe ser un numero"))?;
            if v < 0.0 {
                return Err(AppError::bad_request("tolerancia no puede ser negativa"));
            }
            Some(v)
        }
        _ => {
            return Err(AppError::bad_request(
                "tolerancia debe ser un numero o null",
            ))
        }
    };
    if !practices::set_result_tolerance(&state.pool, &rid, &practice_id, tolerance).await? {
        return Err(AppError::not_found("mensurando no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

fn validate_symbol_format(symbol: &str) -> Result<(), AppError> {
    let s = symbol.trim();
    let valid = !s.is_empty()
        && s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(AppError::bad_request(format!(
            "El simbolo \"{}\" no es valido. Usa solo letras, digitos y guion bajo, \
             comenzando con una letra o guion bajo.",
            s
        )));
    }
    Ok(())
}

const ENDPOINT_SUFFIXES: [&str; 4] = ["_first", "_first2", "_last", "_last2"];

fn validate_symbol_not_reserved(symbol: &str) -> Result<(), AppError> {
    let s = symbol.trim();
    if matches!(s, "pi" | "e" | "slope" | "intercept") {
        return Err(AppError::bad_request(format!(
            "El simbolo \"{}\" es una constante o variable reservada del motor. Elegi otro simbolo.",
            s
        )));
    }
    if let Some(suffix) = ENDPOINT_SUFFIXES.iter().find(|suf| s.ends_with(**suf)) {
        return Err(AppError::bad_request(format!(
            "El simbolo \"{s}\" termina en \"{suffix}\", un sufijo reservado para los valores de \
             extremo por punto (p. ej. \"h_first\"). Elegi otro simbolo.",
        )));
    }
    Ok(())
}

fn duplicate_symbol_error(symbol: &str) -> AppError {
    AppError::bad_request(format!(
        "Ya existe una magnitud o mensurando con el simbolo \"{}\" en esta practica. Elegi otro simbolo.",
        symbol.trim()
    ))
}

#[allow(clippy::too_many_arguments)]
async fn guard_symbol(
    pool: &sqlx::SqlitePool,
    practice_id: &str,
    symbol: &str,
    exclude_quantity_id: Option<&str>,
    exclude_result_id: Option<&str>,
    exclude_intermediate_id: Option<&str>,
    exclude_point_result_id: Option<&str>,
    exclude_aggregate_id: Option<&str>,
) -> Result<(), AppError> {
    validate_symbol_format(symbol)?;
    validate_symbol_not_reserved(symbol)?;
    if practices::symbol_taken_in_practice(
        pool,
        practice_id,
        symbol,
        exclude_quantity_id,
        exclude_result_id,
        exclude_intermediate_id,
        exclude_point_result_id,
        exclude_aggregate_id,
    )
    .await?
    {
        return Err(duplicate_symbol_error(symbol));
    }
    Ok(())
}

const MAX_DISPLAY_SYMBOL_CHARS: usize = 40;

fn validate_display_symbol(display: &Option<String>) -> Result<(), AppError> {
    if display
        .as_deref()
        .is_some_and(|d| d.trim().chars().count() > MAX_DISPLAY_SYMBOL_CHARS)
    {
        return Err(AppError::bad_request(format!(
            "El símbolo a mostrar puede tener hasta {MAX_DISPLAY_SYMBOL_CHARS} caracteres."
        )));
    }
    Ok(())
}

fn validate_quantity(input: &QuantityInput) -> Result<(), AppError> {
    validate_display_symbol(&input.display_symbol)?;
    if input.symbol.trim().is_empty() || input.name.trim().is_empty() {
        return Err(AppError::bad_request(
            "La magnitud necesita un simbolo y un nombre. La unidad puede quedar vacia (adimensional).",
        ));
    }
    if input.slot_count.is_some_and(|n| n < 1) {
        return Err(AppError::bad_request(
            "La cantidad de ranuras tiene que ser un entero de 1 en adelante (vacio = sin restriccion).",
        ));
    }
    if input.slot_count.is_some() && !input.is_given {
        return Err(AppError::bad_request(
            "Las ranuras solo aplican a un dato dado (marca \"Dato dado\" o deja las ranuras vacias).",
        ));
    }
    Ok(())
}

fn validate_result(
    def: &practices::PracticeDefinition,
    input: &ResultInput,
) -> Result<(), AppError> {
    validate_display_symbol(&input.display_symbol)?;
    if input.symbol.trim().is_empty()
        || input.name.trim().is_empty()
        || input.formula.trim().is_empty()
    {
        return Err(AppError::bad_request(
            "El mensurando necesita un simbolo, un nombre y una formula. La unidad puede quedar vacia (adimensional).",
        ));
    }
    if let Some(Some(t)) = input.tolerance {
        if t < 0.0 {
            return Err(AppError::bad_request("tolerancia no puede ser negativa"));
        }
    }
    computation::check_formula(input.formula.trim(), &result_scope(def))
        .map_err(|e| AppError::bad_request(e.to_string()))
}

fn require_title(title: &str) -> Result<(), AppError> {
    if title.trim().is_empty() {
        return Err(AppError::bad_request("Falta el titulo."));
    }
    Ok(())
}

pub(super) async fn create_part(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<practices::PartInput>,
) -> Result<Json<practices::PracticePart>, AppError> {
    require_teacher(&state, &headers).await?;
    require_title(&input.title)?;
    let before = load_definition(&state, &id).await?;
    guard_change(&before, &with_new_part(&before), "agregar la parte")?;
    let mut created = practices::create_part(&state.pool, &id, &input).await?;
    let chosen = before.parts.iter().any(|p| p.analysis_kind.is_some());
    if chosen {
        created.analysis_kind = Some("estadistico".into());
    }
    let mut after = before;
    after.parts.push(created.clone());
    if chosen {
        practices::save_part_kinds(&state.pool, &after).await?;
    }
    created.effective_kind = after.part_kind(&created).to_string();
    Ok(Json(created))
}

fn with_new_part(def: &practices::PracticeDefinition) -> practices::PracticeDefinition {
    let mut after = def.clone();
    after.parts.push(practices::PracticePart {
        id: String::new(),
        practice_id: def.practice_id.clone(),
        title: String::new(),
        position: i64::MAX,
        analysis_kind: None,
        effective_kind: String::new(),
    });
    after
}

#[derive(Debug, Deserialize)]
pub(super) struct PartKindBody {
    analysis_kind: String,
}

fn with_part_kind(
    def: &practices::PracticeDefinition,
    part_id: &str,
    kind: &str,
) -> Option<practices::PracticeDefinition> {
    def.parts.iter().find(|p| p.id == part_id)?;
    let point_based = practices::is_point_based(Some(kind));
    let current = def.part_kinds();
    let mut after = def.clone();
    for (part, current) in after.parts.iter_mut().zip(current) {
        part.analysis_kind = Some(if part.id == part_id {
            kind.to_string()
        } else if point_based {
            "estadistico".into()
        } else {
            current
        });
    }
    Some(after.with_mirrored_kind())
}

fn parse_analysis_kind(raw: &str) -> Result<&str, AppError> {
    let kind = raw.trim();
    if ANALYSIS_KINDS.contains(&kind) {
        Ok(kind)
    } else {
        Err(AppError::bad_request(
            "analysis_kind debe ser estadistico, regresion_lineal o curva",
        ))
    }
}

pub(super) async fn set_part_kind(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    Json(body): Json<PartKindBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let kind = parse_analysis_kind(&body.analysis_kind)?;
    let before = load_definition(&state, &id).await?;
    let after = with_part_kind(&before, &pid, kind)
        .ok_or_else(|| AppError::not_found("parte no encontrada"))?;
    guard_change(&before, &after, "cambiar el tipo de la parte")?;
    practices::save_part_kinds(&state.pool, &after).await?;
    Ok(Json(Health { status: "ok" }))
}

fn with_part_deleted(
    def: &practices::PracticeDefinition,
    part_id: &str,
) -> practices::PracticeDefinition {
    let mut after = def.clone();
    after.parts.retain(|p| p.id != part_id);
    for section in after
        .sections
        .iter_mut()
        .filter(|s| s.part_id.as_deref() == Some(part_id))
    {
        section.part_id = None;
    }
    after.with_mirrored_kind()
}

fn with_part_moved(
    def: &practices::PracticeDefinition,
    part_id: &str,
    up: bool,
) -> practices::PracticeDefinition {
    let mut after = def.clone();
    if let Some(index) = after.parts.iter().position(|p| p.id == part_id) {
        let other = if up {
            index.checked_sub(1)
        } else {
            Some(index + 1)
        };
        if let Some(other) = other.filter(|o| *o < after.parts.len()) {
            after.parts.swap(index, other);
        }
    }
    after
}

pub(super) async fn update_part(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    Json(input): Json<practices::PartInput>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    require_title(&input.title)?;
    if !practices::update_part(&state.pool, &id, &pid, &input).await? {
        return Err(AppError::not_found("parte no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn delete_part(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let after = with_part_deleted(&before, &pid);
    guard_change(&before, &after, "eliminar la parte")?;
    if !practices::delete_part_saving_kinds(&state.pool, &pid, &after).await? {
        return Err(AppError::not_found("parte no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn move_part(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    guard_change(
        &before,
        &with_part_moved(&before, &pid, body.up),
        "mover la parte",
    )?;
    move_in_list(
        &state,
        &headers,
        practices::Orderable::Parts,
        &id,
        &pid,
        body.up,
    )
    .await
}

pub(super) async fn create_section(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<practices::SectionInput>,
) -> Result<Json<practices::PracticeSection>, AppError> {
    require_teacher(&state, &headers).await?;
    require_title(&input.title)?;
    ensure_practice(&state, &id).await?;
    practices::create_section(&state.pool, &id, &input)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::bad_request("La parte no es de esta practica."))
}

pub(super) async fn update_section(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, sid)): Path<(String, String)>,
    Json(input): Json<practices::SectionInput>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    require_title(&input.title)?;
    let before = load_definition(&state, &id).await?;
    let part_id = input.part_id.clone().filter(|p| !p.is_empty());
    if let Some(part) = &part_id {
        if !before.parts.iter().any(|p| &p.id == part) {
            return Err(AppError::bad_request("La parte no es de esta practica."));
        }
    }
    let mut after = before.clone();
    if let Some(section) = after.sections.iter_mut().find(|s| s.id == sid) {
        section.part_id = part_id;
    }
    guard_change(&before, &after, "cambiar la parte de la sección")?;
    match practices::update_section(&state.pool, &id, &sid, &input).await? {
        None => Err(AppError::bad_request("La parte no es de esta practica.")),
        Some(false) => Err(AppError::not_found("seccion no encontrada")),
        Some(true) => Ok(Json(Health { status: "ok" })),
    }
}

pub(super) async fn delete_section(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, sid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.sections.retain(|s| s.id != sid);
    guard_change(&before, &after, "eliminar la sección")?;
    if !practices::delete_section(&state.pool, &id, &sid).await? {
        return Err(AppError::not_found("seccion no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn move_section(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, sid)): Path<(String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Health>, AppError> {
    move_in_list(
        &state,
        &headers,
        practices::Orderable::Sections,
        &id,
        &sid,
        body.up,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub(super) struct SetSectionBody {
    #[serde(default)]
    section_id: Option<String>,
}

async fn set_section(
    state: &SharedState,
    headers: &HeaderMap,
    member: practices::SectionMember,
    practice_id: &str,
    member_id: &str,
    body: SetSectionBody,
) -> Result<Json<Health>, AppError> {
    require_teacher(state, headers).await?;
    let section_id = body.section_id.as_deref().filter(|s| !s.is_empty());
    let before = load_definition(state, practice_id).await?;
    let exists = match member {
        practices::SectionMember::Quantity => before.quantities.iter().any(|q| q.id == member_id),
        practices::SectionMember::Result => before.results.iter().any(|r| r.id == member_id),
    };
    if !exists {
        return Err(AppError::not_found("elemento no encontrado en la practica"));
    }
    let mut after = before.clone();
    let symbol = match member {
        practices::SectionMember::Quantity => before
            .quantities
            .iter()
            .find(|q| q.id == member_id)
            .map(|q| &q.symbol),
        practices::SectionMember::Result => before
            .results
            .iter()
            .find(|r| r.id == member_id)
            .map(|r| &r.symbol),
    };
    if let Some(symbol) = symbol {
        for section in &mut after.sections {
            let members = match member {
                practices::SectionMember::Quantity => &mut section.quantities,
                practices::SectionMember::Result => &mut section.results,
            };
            members.retain(|m| m != symbol);
            if Some(section.id.as_str()) == section_id {
                members.push(symbol.clone());
            }
        }
    }
    guard_change(&before, &after, "asignar la sección")?;
    if !practices::set_member_section(&state.pool, practice_id, member, member_id, section_id)
        .await?
    {
        return Err(AppError::bad_request("La seccion no es de esta practica."));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn set_quantity_section(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, qid)): Path<(String, String)>,
    Json(body): Json<SetSectionBody>,
) -> Result<Json<Health>, AppError> {
    set_section(
        &state,
        &headers,
        practices::SectionMember::Quantity,
        &id,
        &qid,
        body,
    )
    .await
}

pub(super) async fn set_result_section(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, rid)): Path<(String, String)>,
    Json(body): Json<SetSectionBody>,
) -> Result<Json<Health>, AppError> {
    set_section(
        &state,
        &headers,
        practices::SectionMember::Result,
        &id,
        &rid,
        body,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn into_shared_section(def: &mut practices::PracticeDefinition, result: &str) {
        for section in &mut def.sections {
            section.results.retain(|r| r != result);
        }
        def.sections
            .iter_mut()
            .find(|s| s.part_id.is_none())
            .expect("la practica tiene una seccion compartida")
            .results
            .push(result.to_string());
    }

    #[test]
    fn validate_intermediate_checks_symbol_and_formula() {
        let qty = |symbol: &str| db::PracticeQuantity {
            id: format!("q-{symbol}"),
            practice_id: "p".into(),
            symbol: symbol.into(),
            name: symbol.into(),
            unit: "u".into(),
            repeated: true,
            quantity: None,
            position: 0,
            is_given: false,
            replicas_per_point: None,
            per_point: true,
            has_uncertainty: true,
            optional: false,
            default_value: None,
            slot_count: None,
        };
        let def = practices::PracticeDefinition {
            practice_id: "p".into(),
            status: "borrador".into(),
            analysis_kind: Some("regresion_lineal".into()),
            x_formula: None,
            y_formula: None,
            quantities: vec![qty("V"), qty("t")],
            results: vec![],
            curves: vec![],
            operator_count: None,
            intermediates: vec![practices::PracticeIntermediate {
                id: "i1".into(),
                practice_id: "p".into(),
                position: 0,
                symbol: "Q".into(),
                name: "Q".into(),
                unit: "u".into(),
                formula: "V/t".into(),
            }],
            point_results: vec![],
            aggregates: vec![],
            parts: vec![],
            sections: vec![],
            display_symbols: Default::default(),
            current_version: None,
        };
        let input = |symbol: &str, formula: &str| IntermediateInput {
            display_symbol: None,
            symbol: symbol.into(),
            name: "x".into(),
            unit: "u".into(),
            formula: formula.into(),
        };

        assert!(validate_intermediate(&def, &input("pi", "V*2"), None).is_err());
        assert!(validate_intermediate(&def, &input("Re", "V*zzz"), None).is_err());
        assert!(validate_intermediate(&def, &input("Re", "Q*V"), None).is_ok());
        assert!(validate_intermediate(&def, &input("Q", "Q*2"), Some("i1")).is_err());
    }

    #[test]
    fn validate_aggregate_checks_symbols_endpoints_and_order() {
        let h = db::PracticeQuantity {
            id: "q-h".into(),
            practice_id: "p".into(),
            symbol: "h".into(),
            name: "h".into(),
            unit: "u".into(),
            repeated: true,
            quantity: None,
            position: 0,
            is_given: false,
            replicas_per_point: None,
            per_point: true,
            has_uncertainty: true,
            optional: false,
            default_value: None,
            slot_count: None,
        };
        let mut c = h.clone();
        c.id = "q-c".into();
        c.symbol = "c".into();
        c.per_point = false;
        let agg = |id: &str, symbol: &str, position: i64| practices::PracticeAggregate {
            id: id.into(),
            practice_id: "p".into(),
            position,
            symbol: symbol.into(),
            name: symbol.into(),
            unit: "".into(),
            formula: "slope".into(),
            is_final: false,
        };
        let def = practices::PracticeDefinition {
            practice_id: "p".into(),
            status: "borrador".into(),
            analysis_kind: Some("regresion_lineal".into()),
            x_formula: None,
            y_formula: None,
            quantities: vec![h.clone(), c],
            results: vec![db::PracticeResult {
                id: "r-m".into(),
                practice_id: "p".into(),
                position: 0,
                symbol: "m".into(),
                name: "m".into(),
                unit: "u".into(),
                formula: "slope".into(),
                tolerance: None,
                is_final: false,
                has_uncertainty: true,
            }],
            curves: vec![],
            operator_count: None,
            intermediates: vec![practices::PracticeIntermediate {
                id: "i1".into(),
                practice_id: "p".into(),
                position: 0,
                symbol: "Q".into(),
                name: "Q".into(),
                unit: "u".into(),
                formula: "h".into(),
            }],
            point_results: vec![],
            aggregates: vec![agg("a0", "Re_max", 0), agg("a1", "Re_min", 1)],
            parts: vec![],
            sections: vec![],
            display_symbols: Default::default(),
            current_version: None,
        };
        let input = |symbol: &str, formula: &str| AggregateInput {
            display_symbol: None,
            symbol: symbol.into(),
            name: "x".into(),
            unit: "".into(),
            formula: formula.into(),
            is_final: false,
        };

        assert!(validate_aggregate(
            &def,
            &input("Re_medio", "c + m + slope + h_first - h_last + Q_first2"),
            None
        )
        .is_ok());
        assert!(validate_aggregate(&def, &input("pi", "slope"), None).is_err());
        assert!(validate_aggregate(&def, &input("Re_medio", "zzz"), None).is_err());
        assert!(validate_aggregate(&def, &input("Re_medio", "h"), None).is_err());
        assert!(validate_aggregate(&def, &input("Re_medio", "(Re_max + Re_min)/2"), None).is_ok());
        assert!(validate_aggregate(&def, &input("Re_max", "Re_max + 1"), Some("a0")).is_err());
        assert!(validate_aggregate(&def, &input("Re_max", "Re_min + 1"), Some("a0")).is_err());
        assert!(validate_aggregate(&def, &input("Re_min", "Re_max + 1"), Some("a1")).is_ok());
    }

    #[test]
    fn validate_symbol_format_accepts_valid_and_rejects_invalid() {
        assert!(validate_symbol_format("T").is_ok());
        assert!(validate_symbol_format("tau").is_ok());
        assert!(validate_symbol_format("V_g").is_ok());
        assert!(validate_symbol_format("_priv").is_ok());
        assert!(validate_symbol_format("R1").is_ok());
        assert!(validate_symbol_format("").is_err());
        assert!(validate_symbol_format("  ").is_err());
        assert!(validate_symbol_format("2R").is_err());
        assert!(validate_symbol_format("a b").is_err());
        assert!(validate_symbol_format("a+b").is_err());
        assert!(validate_symbol_format("a.b").is_err());
    }

    #[test]
    fn validate_display_symbol_limits_the_length_after_trimming() {
        assert!(validate_display_symbol(&None).is_ok());
        assert!(validate_display_symbol(&Some(format!(" {} ", "x".repeat(40)))).is_ok());
        assert!(validate_display_symbol(&Some("x".repeat(41))).is_err());
    }

    #[test]
    fn validate_quantity_allows_dimensionless_unit() {
        let q = |unit: &str| QuantityInput {
            display_symbol: None,
            symbol: "kp".into(),
            name: "Factor geometrico".into(),
            unit: unit.into(),
            repeated: false,
            quantity: Some("adimensional".into()),
            is_given: false,
            replicas_per_point: None,
            per_point: false,
            has_uncertainty: true,
            optional: false,
            default_value: None,
            slot_count: None,
        };
        assert!(validate_quantity(&q("")).is_ok());
        assert!(validate_quantity(&q("   ")).is_ok());
        assert!(validate_quantity(&q("m")).is_ok());
        assert!(validate_quantity(&QuantityInput {
            symbol: "".into(),
            ..q("")
        })
        .is_err());
        assert!(validate_quantity(&QuantityInput {
            name: "  ".into(),
            ..q("")
        })
        .is_err());
        for (n, ok) in [(0, false), (-3, false), (1, true), (9, true)] {
            let input = QuantityInput {
                slot_count: Some(n),
                is_given: true,
                ..q("cm")
            };
            assert_eq!(validate_quantity(&input).is_ok(), ok, "slot_count = {n}");
        }
        assert!(validate_quantity(&QuantityInput {
            slot_count: Some(9),
            ..q("cm")
        })
        .is_err());
    }

    #[test]
    fn validate_result_allows_dimensionless_unit() {
        let def = practices::PracticeDefinition {
            practice_id: "p".into(),
            status: "borrador".into(),
            analysis_kind: Some("regresion_lineal".into()),
            x_formula: None,
            y_formula: None,
            quantities: vec![],
            results: vec![],
            curves: vec![],
            operator_count: None,
            intermediates: vec![],
            point_results: vec![],
            aggregates: vec![],
            parts: vec![],
            sections: vec![],
            display_symbols: Default::default(),
            current_version: None,
        };
        let r = |unit: &str| ResultInput {
            display_symbol: None,
            symbol: "M_medio".into(),
            name: "Coeficiente medio".into(),
            unit: unit.into(),
            formula: "slope".into(),
            tolerance: None,
            is_final: false,
            has_uncertainty: true,
        };
        assert!(validate_result(&def, &r("")).is_ok());
        assert!(validate_result(&def, &r("   ")).is_ok());
        assert!(validate_result(&def, &r("Pa.s")).is_ok());
        assert!(validate_result(
            &def,
            &ResultInput {
                formula: "".into(),
                ..r("")
            }
        )
        .is_err());
    }

    #[tokio::test]
    async fn seeded_formulas_pass_validation() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        for practice in db::practices(&pool).await.unwrap() {
            let def = practices::definition(&pool, &practice.id)
                .await
                .unwrap()
                .unwrap();
            let problems = formula_problems(&def);
            assert!(problems.is_empty(), "{}: {problems:?}", def.practice_id);
        }
    }

    #[tokio::test]
    async fn the_series_stays_in_a_single_part() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cc.series_parts(), vec!["p2-cc-potencia".to_string()]);
        let mut split = cc.clone();
        for section in &mut split.sections {
            section.quantities.retain(|q| q != "R");
            if section.part_id.as_deref() == Some("p2-cc-serie") {
                section.quantities.push("R".into());
            }
        }
        let err = guard_series_part(&cc, &split, "x").unwrap_err();
        assert!(err.message().contains("misma parte"), "{}", err.message());
        let mut shared = cc.clone();
        for section in &mut shared.sections {
            section.part_id = None;
        }
        assert!(guard_series_part(&cc, &shared, "x").is_ok());
        assert!(guard_series_part(&split, &split, "x").is_ok());
        let mut elsewhere = cc.clone();
        for section in &mut elsewhere.sections {
            section.quantities.retain(|q| q != "R");
            if section.part_id.as_deref() == Some("p2-cc-paralelo") {
                section.quantities.push("R".into());
            }
        }
        assert!(guard_series_part(&split, &elsewhere, "x").is_err());
    }

    #[tokio::test]
    async fn a_published_practice_cannot_be_left_incomplete() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let published = practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(published.status, "publicada");
        let mut emptied = published.clone();
        emptied.results.clear();
        let err = guard_change(&published, &emptied, "x").unwrap_err();
        assert!(
            err.message().contains("no tiene mensurandos"),
            "{}",
            err.message()
        );
        let mut draft = published.clone();
        draft.status = "borrador".into();
        let mut draft_emptied = draft.clone();
        draft_emptied.results.clear();
        assert!(guard_change(&draft, &draft_emptied, "x").is_ok());
        let reg = practices::definition(&pool, "fluidos-2")
            .await
            .unwrap()
            .unwrap();
        let mut no_axes = reg.clone();
        no_axes.y_formula = None;
        assert!(guard_change(&reg, &no_axes, "x").is_err());
    }

    #[tokio::test]
    async fn changes_that_break_saved_formulas_are_detected() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let def = practices::definition(&pool, "fluidos-2")
            .await
            .unwrap()
            .unwrap();
        let rejected = |after: &practices::PracticeDefinition| {
            guard_change(&def, after, "x")
                .map(|_| String::new())
                .unwrap_or_else(|e| e.message().to_string())
        };
        let mut kind = def.clone();
        kind.analysis_kind = Some("estadistico".into());
        assert!(rejected(&kind).contains("slope"));
        let mut renamed = def.clone();
        renamed
            .quantities
            .iter_mut()
            .find(|q| q.symbol == "R_cap")
            .unwrap()
            .symbol = "Rc".into();
        assert!(rejected(&renamed).contains("R_cap"));
        let mut per_point = def.clone();
        per_point
            .quantities
            .iter_mut()
            .find(|q| q.symbol == "R_cap")
            .unwrap()
            .per_point = true;
        assert!(!rejected(&per_point).is_empty());
        let mut agg = def.clone();
        agg.aggregates
            .iter_mut()
            .find(|a| a.symbol == "Re_max")
            .unwrap()
            .symbol = "Remax".into();
        assert!(rejected(&agg).contains("Re_max"));
        let mut unit = def.clone();
        unit.quantities[0].unit = "cm".into();
        assert!(rejected(&unit).is_empty());

        let mut broken = def.clone();
        broken.results[0].formula = "slope * no_existe".into();
        let mut also_unit = broken.clone();
        also_unit.quantities[0].unit = "cm".into();
        assert!(guard_change(&broken, &also_unit, "x").is_ok());
        broken.results[0].formula = "slope * no_existe * Rc".into();
        let mut half_fixed = broken.clone();
        half_fixed.results[0].formula = "slope * no_existe * R_cap".into();
        assert!(guard_change(&broken, &half_fixed, "x").is_ok());
    }

    #[tokio::test]
    async fn publish_requires_a_complete_definition() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let draft = practices::create_practice(
            &pool,
            &practices::NewPracticeInput {
                name: "Vacia".into(),
                description: String::new(),
                analysis_kind: "regresion_lineal".into(),
            },
            "u1",
        )
        .await
        .unwrap();
        let def = practices::definition(&pool, &draft.id)
            .await
            .unwrap()
            .unwrap();
        let problems = publish_problems(&def).join(" | ");
        assert!(problems.contains("magnitudes"), "{problems}");
        assert!(problems.contains("ejes"), "{problems}");
        for practice in db::practices(&pool).await.unwrap() {
            if practice.id == draft.id {
                continue;
            }
            let def = practices::definition(&pool, &practice.id)
                .await
                .unwrap()
                .unwrap();
            assert!(publish_problems(&def).is_empty(), "{}", practice.id);
        }
    }

    #[tokio::test]
    async fn shared_results_only_see_shared_data_and_one_part_has_the_series() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        assert!(formula_problems(&cc).is_empty());
        assert!(structure_problems(&cc).is_empty());

        let mut inferred = cc.clone();
        for section in &mut inferred.sections {
            section.results.retain(|r| r != "Req");
        }
        assert_eq!(inferred.result_part("Req"), Some("p2-cc-paralelo"));
        assert!(formula_problems(&inferred).is_empty());

        let mut shared_req = cc.clone();
        into_shared_section(&mut shared_req, "Req");
        let problems = formula_problems(&shared_req);
        assert!(
            problems
                .iter()
                .any(|(element, message)| message.contains("compartidos")
                    && element.contains("Req")
                    && message.contains("RA_p")),
            "{problems:?}"
        );
    }

    #[tokio::test]
    async fn changing_the_practice_kind_revalidates_the_parts_that_inherit_it() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let hidro = practices::definition(&pool, "hidrostatica")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hidro.status, "publicada");
        let mut after = hidro.clone();
        after.analysis_kind = Some("regresion_lineal".into());
        let err = guard_change(&hidro, &after, "x").unwrap_err();
        assert!(format!("{err:?}").contains("ejes"), "{err:?}");
    }

    #[tokio::test]
    async fn section_moves_cannot_take_a_quantity_out_of_reach_of_its_results() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let mut before = cc.clone();
        let mut shared_result = cc.results[0].clone();
        shared_result.id = "rsh".into();
        shared_result.symbol = "Rsh".into();
        shared_result.formula = "R1 * 2".into();
        before.results.push(shared_result);
        into_shared_section(&mut before, "Rsh");
        assert!(formula_problems(&before).is_empty());

        let mut after = before.clone();
        for section in &mut after.sections {
            section.quantities.retain(|q| q != "R1");
            if section.part_id.as_deref() == Some("p2-cc-serie")
                && section.quantities.contains(&"Vg_s".to_string())
            {
                section.quantities.push("R1".into());
            }
        }
        assert!(guard_change(&before, &after, "asignar la sección").is_err());
        assert!(formula_problems(&after)
            .iter()
            .any(|(element, _)| element.contains("Rsh")));

        let mut broken = cc.clone();
        into_shared_section(&mut broken, "Req");
        assert!(!formula_problems(&broken).is_empty());
        assert!(guard_change(&broken, &cc, "asignar la sección").is_ok());
        assert!(guard_change(&cc, &broken, "eliminar la sección").is_err());
    }

    #[tokio::test]
    async fn a_new_result_is_validated_where_it_will_live() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let body: NewResultBody = serde_json::from_str(
            r#"{"symbol":"X","name":"X","unit":"","formula":"Vg_s * 2","tolerance":null}"#,
        )
        .unwrap();
        assert_eq!(body.result.tolerance, Some(None));
        assert_eq!(body.section_id, None);
        assert!(validate_result(&cc, &body.result).is_ok());
        let inferred = with_new_result(&cc, &body.result, None);
        assert_eq!(inferred.result_part("X"), Some("p2-cc-serie"));
        assert!(guard_change(&cc, &inferred, "guardar el mensurando").is_ok());

        let crossing: ResultInput =
            serde_json::from_str(r#"{"symbol":"X","name":"X","unit":"","formula":"Vg_s * Vg_p"}"#)
                .unwrap();
        let unsectioned = with_new_result(&cc, &crossing, None);
        assert_eq!(unsectioned.result_part("X"), None);
        assert!(guard_change(&cc, &unsectioned, "guardar el mensurando").is_err());
        let shared_section = cc.sections.iter().find(|s| s.part_id.is_none()).unwrap();
        let in_shared = with_new_result(&cc, &body.result, Some(&shared_section.id));
        assert!(guard_change(&cc, &in_shared, "guardar el mensurando").is_err());

        let serie = cc
            .sections
            .iter()
            .find(|s| s.quantities.contains(&"Vg_s".to_string()))
            .unwrap();
        let placed = with_new_result(&cc, &body.result, Some(&serie.id));
        assert_eq!(placed.result_part("X"), Some("p2-cc-serie"));
        assert!(guard_change(&cc, &placed, "guardar el mensurando").is_ok());
    }

    #[tokio::test]
    async fn renaming_a_result_keeps_it_in_its_part() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let i_s = cc.results.iter().find(|r| r.symbol == "I_s").unwrap();
        let input = ResultInput {
            display_symbol: None,
            symbol: "I_serie".into(),
            name: i_s.name.clone(),
            unit: i_s.unit.clone(),
            formula: i_s.formula.clone(),
            tolerance: None,
            is_final: i_s.is_final,
            has_uncertainty: i_s.has_uncertainty,
        };
        let after = with_updated_result(&cc, &i_s.id, &input).unwrap();
        assert_eq!(after.result_part("I_serie"), Some("p2-cc-serie"));
        assert!(guard_change(&cc, &after, "guardar el mensurando").is_ok());
        assert!(with_updated_result(&cc, "no-existe", &input).is_none());
    }

    #[tokio::test]
    async fn a_practice_with_parts_still_needs_some_result_to_publish() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let hidro = practices::definition(&pool, "hidrostatica")
            .await
            .unwrap()
            .unwrap();
        let mut only_shared = hidro.clone();
        only_shared.results.clear();
        for section in &mut only_shared.sections {
            section.part_id = None;
            section.results.clear();
        }
        assert!(only_shared
            .parts
            .iter()
            .all(|p| !only_shared.part_has_content(p)));
        assert!(structure_problems(&only_shared)
            .iter()
            .any(|p| p == "no tiene mensurandos"));
        assert!(publish_problems(&only_shared)
            .iter()
            .any(|p| p.contains("no tiene mensurandos")));
    }

    #[tokio::test]
    async fn series_artifacts_only_see_the_series_part() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let curve = |x: &str, y: &str| CurveInput {
            x_formula: x.into(),
            y_formula: y.into(),
            x_log: false,
        };
        assert!(validate_curve(&cc, &curve("R", "I*R1")).is_ok());
        assert!(validate_curve(&cc, &curve("R", "Vg_s")).is_err());
        let intermediate = |formula: &str| IntermediateInput {
            display_symbol: None,
            symbol: "Pn".into(),
            name: "Pn".into(),
            unit: "".into(),
            formula: formula.into(),
        };
        assert!(validate_intermediate(&cc, &intermediate("I*I*R"), None).is_ok());
        assert!(validate_intermediate(&cc, &intermediate("I*Vg_p"), None).is_err());
    }

    #[tokio::test]
    async fn a_regression_with_a_layout_part_keeps_its_series() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        practices::create_part(
            &pool,
            "fluidos-1",
            &practices::PartInput {
                title: "Unica".into(),
            },
        )
        .await
        .unwrap();
        let def = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        let part = &def.parts[0];
        assert_eq!(def.part_kind(part), "regresion_lineal");
        assert!(def.part_has_content(part));
        assert!(
            structure_problems(&def).is_empty(),
            "{:?}",
            structure_problems(&def)
        );

        let mut no_axes = def.clone();
        no_axes.x_formula = None;
        assert!(structure_problems(&no_axes)
            .iter()
            .any(|p| p.contains("ejes")));

        let mut only_point_results = def.clone();
        only_point_results.results.clear();
        assert!(!only_point_results.point_results.is_empty());
        assert!(!structure_problems(&only_point_results)
            .iter()
            .any(|p| p == "no tiene mensurandos"));
    }

    #[tokio::test]
    async fn the_series_part_is_chosen_by_position() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let mut cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let serie = cc
            .sections
            .iter()
            .position(|s| s.part_id.as_deref() == Some("p2-cc-serie"))
            .unwrap();
        cc.sections[serie].quantities.push("I".into());
        for section in cc
            .sections
            .iter_mut()
            .filter(|s| s.part_id.as_deref() != Some("p2-cc-serie"))
        {
            section.quantities.retain(|q| q != "I");
        }
        assert_eq!(cc.series_parts(), ["p2-cc-serie", "p2-cc-potencia"]);
    }

    #[tokio::test]
    async fn adding_or_moving_parts_cannot_strand_a_result() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let fluidos = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        assert!(guard_change(&fluidos, &with_new_part(&fluidos), "agregar la parte").is_ok());

        let mut shared_section = fluidos.clone();
        shared_section.sections.push(practices::PracticeSection {
            id: "s".into(),
            practice_id: fluidos.practice_id.clone(),
            part_id: None,
            title: "Resultados".into(),
            position: 0,
            quantities: vec![],
            results: vec!["mu".into()],
        });
        assert!(formula_problems(&shared_section).is_empty());
        assert!(guard_change(
            &shared_section,
            &with_new_part(&shared_section),
            "agregar la parte"
        )
        .is_err());

        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let mut unsectioned_series = cc.clone();
        for section in &mut unsectioned_series.sections {
            section.quantities.retain(|q| q != "R" && q != "I");
            section
                .results
                .retain(|r| r != "P_max_e" && r != "RP_max_e");
            if section.part_id.as_deref() == Some("p2-cc-serie")
                && section.quantities.contains(&"Vg_s".to_string())
            {
                section.results.push("P_max_e".into());
                section.results.push("RP_max_e".into());
            }
        }
        assert_eq!(
            unsectioned_series.series_part().map(|p| p.id.as_str()),
            Some("p2-cc-serie")
        );
        assert!(
            formula_problems(&unsectioned_series).is_empty(),
            "{:?}",
            formula_problems(&unsectioned_series)
        );
        let first = unsectioned_series.parts[0].id.clone();
        let last = unsectioned_series.parts.last().unwrap().id.clone();
        let moved = with_part_moved(&unsectioned_series, &last, true);
        assert_eq!(moved.parts[0].id, first);
        let to_front = with_part_moved(
            &unsectioned_series,
            &unsectioned_series.parts[1].id.clone(),
            true,
        );
        assert!(guard_change(&unsectioned_series, &to_front, "mover la parte").is_err());
    }

    #[tokio::test]
    async fn a_statistical_part_can_only_record_data() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let hidro = practices::definition(&pool, "hidrostatica")
            .await
            .unwrap()
            .unwrap();
        let mut data_only = hidro.clone();
        let ts = data_only
            .parts
            .iter()
            .find(|p| p.title.contains("Tension") || p.id.ends_with("-ts"))
            .unwrap()
            .id
            .clone();
        let ts_results: Vec<String> = data_only
            .results
            .iter()
            .filter(|r| data_only.result_part(&r.symbol) == Some(ts.as_str()))
            .map(|r| r.symbol.clone())
            .collect();
        data_only
            .results
            .retain(|r| !ts_results.contains(&r.symbol));
        for section in &mut data_only.sections {
            section.results.retain(|r| !ts_results.contains(r));
        }
        assert!(
            structure_problems(&data_only).is_empty(),
            "{:?}",
            structure_problems(&data_only)
        );
    }

    #[tokio::test]
    async fn each_part_chooses_its_kind() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        for title in ["Datos", "Barrido"] {
            practices::create_part(
                &pool,
                "fluidos-1",
                &practices::PartInput {
                    title: title.into(),
                },
            )
            .await
            .unwrap();
        }
        let fluidos = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        let (datos, barrido) = (fluidos.parts[0].id.clone(), fluidos.parts[1].id.clone());
        assert_eq!(fluidos.series_part().unwrap().id, datos);
        assert_eq!(fluidos.parts[0].effective_kind, "regresion_lineal");

        let after = with_part_kind(&fluidos, &barrido, "regresion_lineal").unwrap();
        assert_eq!(after.series_part().unwrap().id, barrido);
        assert!(guard_change(&fluidos, &after, "x").is_ok());
        practices::save_part_kinds(&pool, &after).await.unwrap();
        let saved = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        let kinds: Vec<&str> = saved
            .parts
            .iter()
            .map(|p| p.effective_kind.as_str())
            .collect();
        assert_eq!(kinds, ["estadistico", "regresion_lineal"]);
        assert!(with_part_kind(&saved, "no-existe", "curva").is_none());

        let hidro = practices::definition(&pool, "hidrostatica")
            .await
            .unwrap()
            .unwrap();
        let to_regression = with_part_kind(&hidro, &hidro.parts[0].id, "regresion_lineal").unwrap();
        assert_eq!(
            to_regression.analysis_kind.as_deref(),
            Some("regresion_lineal")
        );
        assert!(guard_change(&hidro, &to_regression, "x").is_err());

        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let serie_as_curve = with_part_kind(&cc, "p2-cc-serie", "curva").unwrap();
        assert!(structure_problems(&serie_as_curve)
            .iter()
            .any(|p| p.contains("fuera de la parte")));
        let potencia_as_stat = with_part_kind(&cc, "p2-cc-potencia", "estadistico").unwrap();
        assert_eq!(
            potencia_as_stat.analysis_kind.as_deref(),
            Some("estadistico")
        );
        assert!(guard_change(&cc, &potencia_as_stat, "x").is_err());
    }

    #[tokio::test]
    async fn the_chosen_series_part_and_the_series_stay_together() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        for title in ["A", "B"] {
            practices::create_part(
                &pool,
                "fluidos-1",
                &practices::PartInput {
                    title: title.into(),
                },
            )
            .await
            .unwrap();
        }
        let mut def = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        let (a, b) = (def.parts[0].id.clone(), def.parts[1].id.clone());
        let series: Vec<String> = def
            .quantities
            .iter()
            .filter(|q| q.per_point && !q.is_given)
            .map(|q| q.symbol.clone())
            .collect();
        def.sections.push(practices::PracticeSection {
            id: "sa".into(),
            practice_id: def.practice_id.clone(),
            part_id: Some(a.clone()),
            title: "Serie".into(),
            position: 0,
            quantities: series.clone(),
            results: vec![],
        });
        assert_eq!(def.series_part().unwrap().id, a);

        let b_series = with_part_kind(&def, &b, "regresion_lineal").unwrap();
        let err = guard_change(&def, &b_series, "cambiar el tipo de la parte").unwrap_err();
        assert!(format!("{err:?}").contains("parte de la"), "{err:?}");

        let a_series = with_part_kind(&def, &a, "regresion_lineal").unwrap();
        assert_eq!(
            a_series.parts[1].analysis_kind.as_deref(),
            Some("estadistico")
        );
        let mut moved = a_series.clone();
        moved.sections[0].part_id = Some(b.clone());
        assert!(guard_change(&a_series, &moved, "cambiar la parte de la sección").is_err());

        let mut without_a = a_series.clone();
        without_a.parts.retain(|p| p.id != a);
        without_a.sections[0].part_id = None;
        assert!(without_a.series_part().is_none());
    }

    #[tokio::test]
    async fn explicit_kinds_do_not_move_the_series_by_accident() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let serie_stat = with_part_kind(&cc, "p2-cc-serie", "estadistico").unwrap();
        assert_eq!(serie_stat.series_part().unwrap().id, "p2-cc-potencia");
        assert!(guard_change(&cc, &serie_stat, "x").is_ok());

        let with_new = with_new_part(&serie_stat);
        let new_part = with_new.parts.last().unwrap();
        assert_eq!(with_new.part_kind(new_part), "estadistico");
        assert_eq!(with_new.series_part().unwrap().id, "p2-cc-potencia");

        let without_series = with_part_deleted(&serie_stat, "p2-cc-potencia");
        assert_eq!(without_series.analysis_kind.as_deref(), Some("estadistico"));
        assert!(without_series.series_part().is_none());
        let without_other = with_part_deleted(&serie_stat, "p2-cc-serie");
        assert_eq!(without_other.analysis_kind.as_deref(), Some("curva"));
    }

    #[tokio::test]
    async fn deleting_an_inferred_series_part_moves_the_series() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        practices::create_part(
            &pool,
            "fluidos-1",
            &practices::PartInput { title: "A".into() },
        )
        .await
        .unwrap();
        let one = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        let only = with_part_deleted(&one, &one.parts[0].id);
        assert_eq!(only.analysis_kind.as_deref(), Some("regresion_lineal"));
        assert!(guard_change(&one, &only, "eliminar la parte").is_ok());

        practices::create_part(
            &pool,
            "fluidos-1",
            &practices::PartInput { title: "B".into() },
        )
        .await
        .unwrap();
        let two = practices::definition(&pool, "fluidos-1")
            .await
            .unwrap()
            .unwrap();
        let (a, b) = (two.parts[0].id.clone(), two.parts[1].id.clone());
        assert_eq!(two.series_part().unwrap().id, a);
        let without_a = with_part_deleted(&two, &a);
        assert_eq!(without_a.analysis_kind.as_deref(), Some("regresion_lineal"));
        assert_eq!(without_a.series_part().unwrap().id, b);
        assert!(guard_change(&two, &without_a, "eliminar la parte").is_ok());
    }

    #[tokio::test]
    async fn once_kinds_are_chosen_new_parts_are_statistical() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let cc = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        let chosen = with_part_kind(&cc, "p2-cc-serie", "estadistico").unwrap();
        practices::save_part_kinds(&pool, &chosen).await.unwrap();
        let part =
            practices::create_part(&pool, "p2-cc", &practices::PartInput { title: "C".into() })
                .await
                .unwrap();
        let mut after = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        after
            .parts
            .iter_mut()
            .find(|p| p.id == part.id)
            .unwrap()
            .analysis_kind = Some("estadistico".into());
        practices::save_part_kinds(&pool, &after).await.unwrap();
        let reloaded = practices::definition(&pool, "p2-cc")
            .await
            .unwrap()
            .unwrap();
        assert!(reloaded.parts.iter().all(|p| p.analysis_kind.is_some()));
        let without_potencia = with_part_deleted(&reloaded, "p2-cc-potencia");
        let without_serie = with_part_deleted(&without_potencia, "p2-cc-serie");
        assert_eq!(without_serie.analysis_kind.as_deref(), Some("estadistico"));
        assert!(without_serie.series_part().is_none());
    }

    #[tokio::test]
    async fn result_formulas_respect_the_engine_scope() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let def = |id: &'static str| {
            let pool = pool.clone();
            async move { practices::definition(&pool, id).await.unwrap().unwrap() }
        };
        let check = |def: &practices::PracticeDefinition, formula: &str| {
            computation::check_formula(formula, &result_scope(def)).is_ok()
        };
        let p1 = def("p1-estadistica").await;
        assert!(check(&p1, "4*pi^2*L/T1^2"));
        assert!(!check(&p1, "slope"));
        assert!(!check(&p1, "L*zzz"));
        let f2 = def("fluidos-2").await;
        assert!(check(&f2, "slope*R_cap"));
        assert!(!check(&f2, "h*2"));
        let cc = def("p2-cc").await;
        assert!(check(&cc, "P_max + R_at_P_max + I_at_P_max"));
        assert!(!check(&cc, "P*2"));
        let err = check_axes(&f2, "h", "t*zzz").unwrap_err();
        assert!(format!("{err:?}").contains("Eje y"), "{err:?}");
    }

    #[tokio::test]
    async fn results_can_use_other_results() {
        let (pool, _dir) = crate::computation::tests::setup().await;
        let input = |symbol: &str, formula: &str| ResultInput {
            display_symbol: None,
            symbol: symbol.into(),
            name: symbol.into(),
            unit: "m".into(),
            formula: formula.into(),
            tolerance: None,
            is_final: false,
            has_uncertainty: true,
        };
        let p1 = practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        let with_l2 = with_new_result(&p1, &input("L2", "L*2"), None);
        assert!(guard_change(&p1, &with_l2, "x").is_ok());
        assert_eq!(with_l2.result_part("L2"), None);
        assert!(validate_result(&with_l2, &input("G1", "L2 + T1")).is_ok());
        let with_g1 = with_new_result(&with_l2, &input("G1", "L2 + T1"), None);
        assert!(guard_change(&with_l2, &with_g1, "x").is_ok());
        let op1 = with_g1.quantity_part("T1").unwrap();
        assert_eq!(with_g1.result_part("G1"), Some(op1));
        let with_h = with_new_result(&with_g1, &input("H", "G1*3"), None);
        assert_eq!(with_h.result_part("H"), Some(op1));
        let crossing = with_new_result(&with_g1, &input("K", "G1 + T2"), None);
        assert!(guard_change(&with_g1, &crossing, "x").is_err());

        let l2 = practices::create_result(&pool, "p1-estadistica", input("L2", "L*2"), None)
            .await
            .unwrap();
        practices::create_result(&pool, "p1-estadistica", input("G1", "L2 + T1"), None)
            .await
            .unwrap();
        let def = practices::definition(&pool, "p1-estadistica")
            .await
            .unwrap()
            .unwrap();
        let cyclic = with_updated_result(&def, &l2.id, &input("L2", "G1*2")).unwrap();
        let err = guard_change(&def, &cyclic, "x").unwrap_err();
        assert!(format!("{err:?}").contains("ciclo"), "{err:?}");
        let crossing = with_new_result(&def, &input("K", "L2 + T1 + T2"), None);
        let crossing = with_new_result(&crossing, &input("S", "K*2"), None);
        assert!(crossing.parts.iter().all(|p| crossing
            .part_view(p)
            .results
            .iter()
            .all(|r| r.symbol != "K" && r.symbol != "S")));
        assert!(crossing.parts.iter().all(|p| crossing
            .part_view(p)
            .results
            .iter()
            .any(|r| r.symbol == "L2")));
        let problems: Vec<String> = formula_problems(&crossing)
            .into_iter()
            .map(|(element, _)| element)
            .filter(|element| element.ends_with(" K") || element.ends_with(" S"))
            .collect();
        assert_eq!(problems, ["el mensurando K"]);
        let mut without_l2 = def.clone();
        without_l2.results.retain(|r| r.symbol != "L2");
        assert!(guard_change(&def, &without_l2, "x").is_err());

        let id = |symbol: &str| {
            def.quantities
                .iter()
                .find(|q| q.symbol == symbol)
                .unwrap()
                .id
                .clone()
        };
        let measurement =
            |symbol: &str, values: Vec<f64>, given_u: Option<f64>| computation::MeasurementInput {
                quantity_id: id(symbol),
                instrument_id: None,
                scale_id: None,
                values,
                given_u,
                point_replicas: None,
                operator_replicas: None,
            };
        let analysis = computation::analyze(
            &pool,
            "p1-estadistica",
            &[
                measurement("L", vec![1.0], Some(0.002)),
                measurement("t_med", vec![95.3], None),
                measurement("T1", vec![2.0, 2.0, 2.0], None),
            ],
        )
        .await
        .unwrap();
        let mut all: Vec<&computation::DerivedComputation> = analysis.derived.iter().collect();
        for part in &analysis.parts {
            all.extend(&part.analysis.derived);
            all.extend(part.analysis.operators.iter().flat_map(|o| &o.derived));
        }
        assert_eq!(all.iter().filter(|d| d.symbol == "L2").count(), 1);
        assert!(analysis.derived.iter().any(|d| d.symbol == "L2"));
        let g1 = all.iter().find(|d| d.symbol == "G1").unwrap();
        assert!((g1.value - 4.0).abs() < 1e-9, "{}", g1.value);
        let l2 = all.iter().find(|d| d.symbol == "L2").unwrap();
        assert!(l2.u > 0.0);
        assert!((g1.u - l2.u).abs() < 1e-12, "{} {}", g1.u, l2.u);
    }

    #[test]
    fn validate_symbol_not_reserved_rejects_reserved_symbols() {
        assert!(validate_symbol_not_reserved("pi").is_err());
        assert!(validate_symbol_not_reserved("e").is_err());
        assert!(validate_symbol_not_reserved("slope").is_err());
        assert!(validate_symbol_not_reserved("intercept").is_err());
        assert!(validate_symbol_not_reserved("h_first").is_err());
        assert!(validate_symbol_not_reserved("v_first2").is_err());
        assert!(validate_symbol_not_reserved("Q_last").is_err());
        assert!(validate_symbol_not_reserved("x_last2").is_err());
        assert!(validate_symbol_not_reserved("T").is_ok());
        assert!(validate_symbol_not_reserved("tau").is_ok());
        assert!(validate_symbol_not_reserved("V_g").is_ok());
        assert!(validate_symbol_not_reserved("first_h").is_ok());
        assert!(validate_symbol_not_reserved("h_max").is_ok());
    }
}
