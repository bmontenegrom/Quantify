//! Handlers de `/api/practices/{id}/*`: definición de una práctica (magnitudes, mensurandos,
//! curvas, intermedias, derivadas por punto, agregados) y su edición por el docente/admin.

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

/// Cuerpo para actualizar el tipo de análisis de una práctica.
#[derive(Debug, Deserialize)]
pub(super) struct SetAnalysisKindBody {
    analysis_kind: String,
}

/// `GET /api/practices/{id}/definition`: magnitudes + mensurandos de una práctica (requiere sesión).
pub(super) async fn practice_definition(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<practices::PracticeDefinition>, AppError> {
    current_user(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    Ok(Json(def))
}

/// Cuerpo del preview de análisis: sólo las lecturas crudas (sin curso/grupo).
#[derive(serde::Deserialize)]
pub(super) struct AnalyzePreviewBody {
    measurements: Vec<computation::MeasurementInput>,
}

/// `POST /api/practices/{id}/analyze-preview`: calcula el análisis (incl. regresión) sin
/// persistir, para previsualizar el gráfico/parámetros mientras el alumno carga datos.
pub(super) async fn analyze_preview(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AnalyzePreviewBody>,
) -> Result<Json<computation::FormAnalysis>, AppError> {
    current_user(&state, &headers).await?;
    let analysis = computation::analyze(&state.pool, &id, &body.measurements)
        .await
        .map_err(AppError::from_domain_or_db)?;
    Ok(Json(analysis))
}

/// `POST /api/practices/{id}/analysis-kind`: actualiza el tipo de análisis (docente/admin).
pub(super) async fn set_practice_analysis_kind(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<SetAnalysisKindBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !matches!(
        body.analysis_kind.trim(),
        "estadistico" | "regresion_lineal" | "curva"
    ) {
        return Err(AppError::bad_request(
            "analysis_kind debe ser estadistico, regresion_lineal o curva",
        ));
    }
    if !practices::set_analysis_kind(&state.pool, &id, body.analysis_kind.trim()).await? {
        return Err(AppError::not_found("practica no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// Cuerpo para definir las fórmulas de eje del ajuste lineal de una práctica `regresion_lineal`.
#[derive(Debug, Deserialize)]
pub(super) struct RegressionFormulasBody {
    x_formula: String,
    y_formula: String,
}

/// Cuerpo para definir la cantidad de operadores de una práctica estadística (Motor D).
#[derive(Debug, Deserialize)]
pub(super) struct OperatorCountBody {
    /// Cantidad de operadores; `<= 1` desactiva los operadores (comportamiento por defecto).
    count: i64,
}

/// `POST /api/practices/{id}/operator-count`: fija la cantidad de operadores de una práctica
/// estadística (docente/admin). Se acota a un máximo razonable para no explotar el formulario.
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

/// `POST /api/practices/{id}/regression-formulas`: define las fórmulas de eje `x`/`y` del ajuste
/// lineal de una práctica de regresión (docente/admin).
pub(super) async fn set_practice_regression_formulas(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RegressionFormulasBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::set_regression_formulas(&state.pool, &id, &body.x_formula, &body.y_formula)
        .await?
    {
        return Err(AppError::not_found("practica no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `POST /api/practices/{id}/quantities`: agrega una magnitud a la práctica (docente/admin).
pub(super) async fn create_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<QuantityInput>,
) -> Result<Json<db::PracticeQuantity>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_quantity(&input)?;
    validate_symbol_format(&input.symbol)?;
    validate_symbol_not_reserved(&input.symbol)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    Ok(Json(
        practices::create_quantity(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/practices/{id}/quantities/{qid}`: actualiza una magnitud (docente/admin).
pub(super) async fn update_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((practice_id, qid)): Path<(String, String)>,
    Json(input): Json<QuantityInput>,
) -> Result<Json<db::PracticeQuantity>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_quantity(&input)?;
    validate_symbol_format(&input.symbol)?;
    validate_symbol_not_reserved(&input.symbol)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &practice_id,
        &input.symbol,
        Some(&qid),
        None,
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    let updated = practices::update_quantity(&state.pool, &qid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud no encontrada"))?;
    Ok(Json(updated))
}

/// `DELETE /api/practices/{id}/quantities/{qid}`: elimina una magnitud (docente/admin).
pub(super) async fn delete_quantity(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((_id, qid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::delete_quantity(&state.pool, &qid).await? {
        return Err(AppError::not_found("magnitud no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `POST /api/practices/{id}/curves`: agrega una curva a una práctica `curva` (docente/admin).
pub(super) async fn create_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<CurveInput>,
) -> Result<Json<practices::PracticeCurve>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_curve(&input)?;
    Ok(Json(
        practices::create_curve(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/practices/{id}/curves/{cid}`: actualiza una curva (docente/admin).
pub(super) async fn update_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, cid)): Path<(String, String)>,
    Json(input): Json<CurveInput>,
) -> Result<Json<practices::PracticeCurve>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_curve(&input)?;
    let updated = practices::update_curve(&state.pool, &id, &cid, input)
        .await?
        .ok_or_else(|| AppError::not_found("curva no encontrada"))?;
    Ok(Json(updated))
}

/// Cuerpo para reordenar una curva: dirección del movimiento.
#[derive(Debug, Deserialize)]
pub(super) struct MoveCurveBody {
    /// `true` mueve la curva una posición hacia arriba; `false` (o ausente), hacia abajo.
    #[serde(default)]
    up: bool,
}

/// `POST /api/practices/{id}/curves/{cid}/move`: reordena una curva intercambiándola con la vecina
/// (docente/admin). Si ya está en el extremo, no cambia nada.
pub(super) async fn move_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, cid)): Path<(String, String)>,
    Json(body): Json<MoveCurveBody>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::move_curve(&state.pool, &id, &cid, body.up).await? {
        return Err(AppError::not_found("curva no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `POST /api/practices/{id}/intermediates`: agrega una magnitud intermedia por punto (docente).
pub(super) async fn create_intermediate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<IntermediateInput>,
) -> Result<Json<practices::PracticeIntermediate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    validate_intermediate(&def, &input, None)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    Ok(Json(
        practices::create_intermediate(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/practices/{id}/intermediates/{iid}`: actualiza una magnitud intermedia (docente).
pub(super) async fn update_intermediate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, iid)): Path<(String, String)>,
    Json(input): Json<IntermediateInput>,
) -> Result<Json<practices::PracticeIntermediate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    validate_intermediate(&def, &input, Some(&iid))?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        Some(&iid),
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    let updated = practices::update_intermediate(&state.pool, &id, &iid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud intermedia no encontrada"))?;
    Ok(Json(updated))
}

/// `DELETE /api/practices/{id}/intermediates/{iid}`: elimina una magnitud intermedia (docente).
pub(super) async fn delete_intermediate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, iid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::delete_intermediate(&state.pool, &id, &iid).await? {
        return Err(AppError::not_found("magnitud intermedia no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// Valida una magnitud intermedia contra la definición de la práctica (docente): símbolo con
/// formato válido, no reservado y único (vs magnitudes, mensurandos y otras intermedias), y fórmula
/// que compila usando las magnitudes + las intermedias **anteriores** (por posición). `exclude_id`
/// ignora la propia fila al editar. Todos los errores son 400 amigables.
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
    // (La unicidad del símbolo se verifica en el handler con `symbol_taken_in_practice`, que cubre
    // los tres espacios de símbolos: magnitudes, mensurandos e intermedias.)
    // Símbolos permitidos en la fórmula: magnitudes + intermedias anteriores (al crear, todas las
    // existentes; al editar, solo las de menor posición que la editada).
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

/// `POST /api/practices/{id}/point-results`: agrega una magnitud derivada por punto (docente).
pub(super) async fn create_point_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PointResultInput>,
) -> Result<Json<practices::PracticePointResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    validate_point_result(&def, &input)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    Ok(Json(
        practices::create_point_result(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/practices/{id}/point-results/{pid}`: actualiza una magnitud derivada por punto.
pub(super) async fn update_point_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    Json(input): Json<PointResultInput>,
) -> Result<Json<practices::PracticePointResult>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    validate_point_result(&def, &input)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        Some(&pid),
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    let updated = practices::update_point_result(&state.pool, &id, &pid, input)
        .await?
        .ok_or_else(|| AppError::not_found("magnitud derivada por punto no encontrada"))?;
    Ok(Json(updated))
}

/// `DELETE /api/practices/{id}/point-results/{pid}`: elimina una magnitud derivada por punto.
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

/// Valida símbolo (formato, no reservado) y fórmula de una magnitud derivada por punto. La fórmula
/// compila usando magnitudes + intermedias + mensurandos + `slope`/`intercept` (símbolos
/// disponibles tras el ajuste). La unicidad del símbolo la verifica el handler con
/// `symbol_taken_in_practice`.
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
    // Símbolos disponibles tras el ajuste: magnitudes + intermedias + mensurandos + slope/intercept.
    let mut allowed: Vec<String> = def.quantities.iter().map(|q| q.symbol.clone()).collect();
    allowed.extend(def.intermediates.iter().map(|it| it.symbol.clone()));
    allowed.extend(def.results.iter().map(|r| r.symbol.clone()));
    allowed.push("slope".into());
    allowed.push("intercept".into());
    computation::check_formula(formula, &allowed)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(())
}

/// `POST /api/practices/{id}/aggregates`: agrega un mensurando agregado (Motor F, docente).
pub(super) async fn create_aggregate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AggregateInput>,
) -> Result<Json<practices::PracticeAggregate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    validate_aggregate(&def, &input, None)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    Ok(Json(
        practices::create_aggregate(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/practices/{id}/aggregates/{aid}`: actualiza un mensurando agregado.
pub(super) async fn update_aggregate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, aid)): Path<(String, String)>,
    Json(input): Json<AggregateInput>,
) -> Result<Json<practices::PracticeAggregate>, AppError> {
    require_teacher(&state, &headers).await?;
    let def = practices::definition(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    validate_aggregate(&def, &input, Some(&aid))?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        Some(&aid),
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    let updated = practices::update_aggregate(&state.pool, &id, &aid, input)
        .await?
        .ok_or_else(|| AppError::not_found("mensurando agregado no encontrado"))?;
    Ok(Json(updated))
}

/// `DELETE /api/practices/{id}/aggregates/{aid}`: elimina un mensurando agregado.
pub(super) async fn delete_aggregate(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, aid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::delete_aggregate(&state.pool, &id, &aid).await? {
        return Err(AppError::not_found("mensurando agregado no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// Valida símbolo (formato, no reservado) y fórmula de un mensurando agregado. La fórmula compila
/// usando los escalares compartidos + mensurandos + `slope`/`intercept` + los agregados **anteriores**
/// (por posición) + los extremos de cada magnitud/intermedia por punto
/// (`{sym}_first`/`_first2`/`_last`/`_last2`). `exclude_id` ignora la propia fila al editar. La
/// unicidad del símbolo la verifica el handler con `symbol_taken_in_practice`.
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
    // Escalares compartidos (per_point=false o is_given) + mensurandos + slope/intercept + agregados
    // anteriores + extremos de cada magnitud por punto e intermedia.
    let mut allowed: Vec<String> = def
        .quantities
        .iter()
        .filter(|q| !q.per_point || q.is_given)
        .map(|q| q.symbol.clone())
        .collect();
    allowed.extend(def.results.iter().map(|r| r.symbol.clone()));
    allowed.push("slope".into());
    allowed.push("intercept".into());
    // Solo los agregados **anteriores** (al editar, los de menor posición que el editado; al crear,
    // todos los existentes): `compute_regresion` solo liga los agregados previos, así que admitir uno
    // posterior o el propio dejaría pasar una fórmula que luego falla al computar la entrega.
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

/// Una curva necesita ambas fórmulas de eje (sin ellas no se puede graficar). Error 400 amigable.
fn validate_curve(input: &CurveInput) -> Result<(), AppError> {
    if input.x_formula.trim().is_empty() || input.y_formula.trim().is_empty() {
        return Err(AppError::bad_request(
            "La curva necesita las formulas de ambos ejes (x e y).",
        ));
    }
    Ok(())
}

/// `DELETE /api/practices/{id}/curves/{cid}`: elimina una curva (docente/admin).
pub(super) async fn delete_curve(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((id, cid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::delete_curve(&state.pool, &id, &cid).await? {
        return Err(AppError::not_found("curva no encontrada"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `POST /api/practices/{id}/results`: agrega un mensurando derivado (docente/admin).
pub(super) async fn create_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ResultInput>,
) -> Result<Json<db::PracticeResult>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_result(&input)?;
    validate_symbol_format(&input.symbol)?;
    validate_symbol_not_reserved(&input.symbol)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &id,
        &input.symbol,
        None,
        None,
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    Ok(Json(
        practices::create_result(&state.pool, &id, input).await?,
    ))
}

/// `POST /api/practices/{id}/results/{rid}`: actualiza un mensurando derivado (docente/admin).
pub(super) async fn update_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((practice_id, rid)): Path<(String, String)>,
    Json(input): Json<ResultInput>,
) -> Result<Json<db::PracticeResult>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_result(&input)?;
    validate_symbol_format(&input.symbol)?;
    validate_symbol_not_reserved(&input.symbol)?;
    if practices::symbol_taken_in_practice(
        &state.pool,
        &practice_id,
        &input.symbol,
        None,
        Some(&rid),
        None,
        None,
        None,
    )
    .await?
    {
        return Err(duplicate_symbol_error(&input.symbol));
    }
    let updated = practices::update_result(&state.pool, &rid, input)
        .await?
        .ok_or_else(|| AppError::not_found("mensurando no encontrado"))?;
    Ok(Json(updated))
}

/// `DELETE /api/practices/{id}/results/{rid}`: elimina un mensurando derivado (docente/admin).
pub(super) async fn delete_result(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((_id, rid)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !practices::delete_result(&state.pool, &rid).await? {
        return Err(AppError::not_found("mensurando no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

/// `POST /api/practices/{id}/results/{rid}/tolerance`: fija la tolerancia % del veredicto.
/// Body: `{ "tolerance": 5.0 }` para activar, `{ "tolerance": null }` para desactivar.
/// Se mantiene como endpoint independiente para actualizar solo la tolerancia sin reenviar
/// símbolo, nombre, unidad ni fórmula del mensurando.
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

/// Verifica que el símbolo sea un identificador válido: `[a-zA-Z_][a-zA-Z0-9_]*`.
/// Solo ASCII por compatibilidad con el parser de evalexpr.
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

/// Sufijos reservados para los **alias de extremo** que el Motor F genera por cada magnitud por
/// punto e intermedia (`{base}_first`, `{base}_first2`, `{base}_last`, `{base}_last2`). Reservarlos
/// globalmente evita que un símbolo real (escalar compartido, mensurando, agregado) colisione con un
/// alias generado y termine ligándose al valor equivocado en las fórmulas de agregados.
const ENDPOINT_SUFFIXES: [&str; 4] = ["_first", "_first2", "_last", "_last2"];

/// Verifica que el símbolo no sea una constante o variable reservada del motor de fórmulas.
///
/// `pi` y `e` son constantes matemáticas siempre presentes en evalexpr. `slope` e `intercept`
/// son variables inyectadas por el motor en prácticas de regresión. Los cuatro están reservados
/// globalmente para evitar colisiones independientemente del tipo de análisis de la práctica.
///
/// Además, ningún símbolo puede terminar en un sufijo de extremo del Motor F
/// ([`ENDPOINT_SUFFIXES`]): esos nombres se reservan para los alias generados (`h_first`, etc.).
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

/// Error 400 amigable para un símbolo ya usado dentro de la misma práctica.
fn duplicate_symbol_error(symbol: &str) -> AppError {
    AppError::bad_request(format!(
        "Ya existe una magnitud o mensurando con el simbolo \"{}\" en esta practica. Elegi otro simbolo.",
        symbol.trim()
    ))
}

/// Valida los campos de una magnitud: símbolo y nombre no vacíos. La unidad **puede** ir vacía:
/// representa una magnitud adimensional (p. ej. un factor o coeficiente como `kp` en Fluidos II).
fn validate_quantity(input: &QuantityInput) -> Result<(), AppError> {
    if input.symbol.trim().is_empty() || input.name.trim().is_empty() {
        return Err(AppError::bad_request(
            "La magnitud necesita un simbolo y un nombre. La unidad puede quedar vacia (adimensional).",
        ));
    }
    Ok(())
}

/// Valida los campos de un mensurando derivado: símbolo, nombre y fórmula no vacíos, y tolerancia
/// no negativa si se proporciona. La unidad **puede** ir vacía: mensurando adimensional (p. ej. un
/// coeficiente como `M_medio` en Fluidos II).
fn validate_result(input: &ResultInput) -> Result<(), AppError> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_intermediate_checks_symbol_and_formula() {
        // Práctica con magnitudes V, t y una intermedia previa Q = V/t.
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
        };
        let def = practices::PracticeDefinition {
            practice_id: "p".into(),
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

        // Símbolo reservado (constante del motor) y fórmula con símbolo inexistente → 400.
        // (La unicidad del símbolo se valida aparte, vía `symbol_taken_in_practice`.)
        assert!(validate_intermediate(&def, &input("pi", "V*2"), None).is_err());
        assert!(validate_intermediate(&def, &input("Re", "V*zzz"), None).is_err());
        // Nueva intermedia válida que referencia a Q (anterior) y magnitudes.
        assert!(validate_intermediate(&def, &input("Re", "Q*V"), None).is_ok());
        // Al editar Q (posición 0), no puede referenciarse a sí misma ni a posteriores.
        assert!(validate_intermediate(&def, &input("Q", "Q*2"), Some("i1")).is_err());
    }

    #[test]
    fn validate_aggregate_checks_symbols_endpoints_and_order() {
        // Práctica regresión: h por punto, c escalar compartido, mensurando m, intermedia Q, y dos
        // agregados (Re_max pos 0, Re_min pos 1).
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
        };
        let mut c = h.clone();
        c.id = "q-c".into();
        c.symbol = "c".into();
        c.per_point = false; // escalar compartido
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

        // Válido: usa escalar compartido c, mensurando m, slope, y extremos de h (per punto) y Q.
        assert!(validate_aggregate(
            &def,
            &input("Re_medio", "c + m + slope + h_first - h_last + Q_first2"),
            None
        )
        .is_ok());
        // Símbolo reservado y fórmula con símbolo inexistente → 400.
        assert!(validate_aggregate(&def, &input("pi", "slope"), None).is_err());
        assert!(validate_aggregate(&def, &input("Re_medio", "zzz"), None).is_err());
        // Una magnitud **por punto** sin sufijo de extremo no es un escalar válido aquí.
        assert!(validate_aggregate(&def, &input("Re_medio", "h"), None).is_err());
        // Al crear, puede referenciar agregados existentes (Re_max, Re_min).
        assert!(validate_aggregate(&def, &input("Re_medio", "(Re_max + Re_min)/2"), None).is_ok());
        // Al editar Re_max (posición 0), no puede referenciarse a sí mismo ni a Re_min (posterior).
        assert!(validate_aggregate(&def, &input("Re_max", "Re_max + 1"), Some("a0")).is_err());
        assert!(validate_aggregate(&def, &input("Re_max", "Re_min + 1"), Some("a0")).is_err());
        // Pero al editar Re_min (posición 1) sí puede usar Re_max (anterior).
        assert!(validate_aggregate(&def, &input("Re_min", "Re_max + 1"), Some("a1")).is_ok());
    }

    #[test]
    fn validate_symbol_format_accepts_valid_and_rejects_invalid() {
        // Identificadores válidos
        assert!(validate_symbol_format("T").is_ok());
        assert!(validate_symbol_format("tau").is_ok());
        assert!(validate_symbol_format("V_g").is_ok());
        assert!(validate_symbol_format("_priv").is_ok());
        assert!(validate_symbol_format("R1").is_ok());
        // Inválidos: vacío, espacios, operadores, empieza con dígito
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
        };
        // Unidad vacía (o solo espacios) → magnitud adimensional, válida.
        assert!(validate_quantity(&q("")).is_ok());
        assert!(validate_quantity(&q("   ")).is_ok());
        assert!(validate_quantity(&q("m")).is_ok());
        // Símbolo o nombre vacíos siguen siendo inválidos.
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
    }

    #[test]
    fn validate_result_allows_dimensionless_unit() {
        let r = |unit: &str| ResultInput {
            symbol: "M_medio".into(),
            name: "Coeficiente medio".into(),
            unit: unit.into(),
            formula: "slope".into(),
            tolerance: None,
            is_final: false,
            has_uncertainty: true,
        };
        // Unidad vacía → mensurando adimensional, válido.
        assert!(validate_result(&r("")).is_ok());
        assert!(validate_result(&r("   ")).is_ok());
        assert!(validate_result(&r("Pa.s")).is_ok());
        // Fórmula vacía sigue siendo inválida.
        assert!(validate_result(&ResultInput {
            formula: "".into(),
            ..r("")
        })
        .is_err());
    }

    #[test]
    fn validate_symbol_not_reserved_rejects_reserved_symbols() {
        // Constantes matematicas siempre presentes en evalexpr.
        assert!(validate_symbol_not_reserved("pi").is_err());
        assert!(validate_symbol_not_reserved("e").is_err());
        // Variables inyectadas por el motor de regresion; reservadas globalmente.
        assert!(validate_symbol_not_reserved("slope").is_err());
        assert!(validate_symbol_not_reserved("intercept").is_err());
        // Sufijos de extremo del Motor F: reservados para los alias generados (`h_first`, etc.).
        assert!(validate_symbol_not_reserved("h_first").is_err());
        assert!(validate_symbol_not_reserved("v_first2").is_err());
        assert!(validate_symbol_not_reserved("Q_last").is_err());
        assert!(validate_symbol_not_reserved("x_last2").is_err());
        // Identificadores comunes validos (incluido uno que contiene "first" sin ser sufijo).
        assert!(validate_symbol_not_reserved("T").is_ok());
        assert!(validate_symbol_not_reserved("tau").is_ok());
        assert!(validate_symbol_not_reserved("V_g").is_ok());
        assert!(validate_symbol_not_reserved("first_h").is_ok());
        assert!(validate_symbol_not_reserved("h_max").is_ok());
    }
}
