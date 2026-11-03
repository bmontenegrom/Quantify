use super::{current_user, require_teacher, Health, SharedState};
use crate::{
    computation, db,
    error::AppError,
    practices::{
        self, AggregateInput, CurveInput, IntermediateInput, PointResultInput, QuantityInput,
        ResultInput,
    },
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct SetAnalysisKindBody {
    analysis_kind: String,
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
    require_teacher(&state, &headers).await?;
    let status = body.status.trim();
    if status == "publicada" {
        let def = load_definition(&state, &id).await?;
        let problems = publish_problems(&def);
        if !problems.is_empty() {
            return Err(AppError::bad_request(format!(
                "Todavia no se puede publicar: {}.",
                problems.join("; ")
            )));
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

pub(super) async fn practice_definition(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<practices::PracticeDefinition>, AppError> {
    let user = current_user(&state, &headers).await?;
    ensure_student_can_see(&state, &user, &id, &["publicada", "archivada"]).await?;
    let def = load_definition(&state, &id).await?;
    Ok(Json(def))
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
}

pub(super) async fn analyze_preview(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AnalyzePreviewBody>,
) -> Result<Json<computation::FormAnalysis>, AppError> {
    let user = current_user(&state, &headers).await?;
    ensure_student_can_see(&state, &user, &id, &["publicada"]).await?;
    let analysis = computation::analyze(&state.pool, &id, &body.measurements)
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
    let kind = body.analysis_kind.trim();
    if !ANALYSIS_KINDS.contains(&kind) {
        return Err(AppError::bad_request(
            "analysis_kind debe ser estadistico, regresion_lineal o curva",
        ));
    }
    let before = load_definition(&state, &id).await?;
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
    q.symbol = input.symbol.trim().to_string();
    q.per_point = input.per_point;
    q.is_given = input.is_given;
    guard_change(&before, &after, "guardar la magnitud")?;
    let updated = practices::update_quantity(&state.pool, &qid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud no encontrada"))?;
    Ok(Json(updated))
}

pub(super) async fn delete_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, qid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let before = load_definition(&state, &id).await?;
    let mut after = before.clone();
    after.quantities.retain(|q| q.id != qid);
    if after.quantities.len() == before.quantities.len() {
        return Err(AppError::not_found("magnitud no encontrada en la practica"));
    }
    guard_change(&before, &after, "eliminar la magnitud")?;
    if !practices::delete_quantity(&state.pool, &qid).await? {
        return Err(AppError::not_found("magnitud no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
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

fn formula_problems(def: &practices::PracticeDefinition) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    let mut note = |what: &str, symbol: &str, result: Result<(), AppError>| {
        if let Err(e) = result {
            problems.push((format!("{what} {symbol}"), e.message().to_string()));
        }
    };
    for r in &def.results {
        let input = ResultInput {
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

fn guard_change(
    before: &practices::PracticeDefinition,
    after: &practices::PracticeDefinition,
    what: &str,
) -> Result<(), AppError> {
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
    let shared = def
        .quantities
        .iter()
        .filter(|q| !q.per_point || q.is_given)
        .map(|q| q.symbol.clone());
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
        _ => def.quantities.iter().map(|q| q.symbol.clone()).collect(),
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

pub(super) async fn create_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ResultInput>,
) -> Result<Json<db::PracticeResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = load_definition(&state, &id).await?;
    validate_result(&def, &input)?;
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
        practices::create_result(&state.pool, &id, input).await?,
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
    let mut after = def.clone();
    let item = after
        .results
        .iter_mut()
        .find(|x| x.id == rid)
        .ok_or_else(|| AppError::not_found("mensurando no encontrado en la practica"))?;
    item.symbol = input.symbol.trim().to_string();
    item.formula = input.formula.trim().to_string();
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

fn validate_quantity(input: &QuantityInput) -> Result<(), AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        let input = |symbol: &str, formula: &str| IntermediateInput {
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
        };
        let input = |symbol: &str, formula: &str| AggregateInput {
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
    fn validate_quantity_allows_dimensionless_unit() {
        let q = |unit: &str| QuantityInput {
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
        };
        let r = |unit: &str| ResultInput {
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
