use super::{current_user, read_text, require_teacher, required, Health, SharedState};
use crate::{
    analysis,
    computation::{self, FormSubmissionInput},
    db::{self, NewSubmission, ReviewSubmission},
    error::AppError,
    practices,
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;

pub(super) async fn submissions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::SubmissionListItem>>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(
        db::submission_list_for_user(&state.pool, &user).await?,
    ))
}

pub(super) async fn submission_detail(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let user = current_user(&state, &headers).await?;
    if !matches!(user.role.as_str(), "docente" | "admin") {
        let is_member = db::is_accepted_member(&state.pool, &id, &user.id).await?;
        if !is_member {
            return Err(AppError::forbidden("no tenes acceso a esta entrega"));
        }
    }
    let submission = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(gate_analysis(submission, &user)))
}

fn analysis_hidden_for(role: &str, results_visible: bool) -> bool {
    !matches!(role, "docente" | "admin") && !results_visible
}

fn gate_analysis(
    mut submission: db::SubmissionDetail,
    user: &db::AuthUser,
) -> db::SubmissionDetail {
    if analysis_hidden_for(&user.role, submission.results_visible_to_student) {
        submission.analysis = serde_json::Value::Null;
        submission.result_tolerances.clear();
    }
    submission
}

pub(super) async fn create_submission(
    State(state): State<SharedState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let user = current_user(&state, &headers).await?;
    let mut course_id = None;
    let mut group_id = None;
    let mut practice_id = None;
    let mut file_name = None;
    let mut csv_content = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("el formulario enviado no es valido"))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "course_id" => course_id = Some(read_text(field).await?),
            "group_id" => group_id = Some(read_text(field).await?),
            "practice_id" => practice_id = Some(read_text(field).await?),
            "csv_file" => {
                file_name = field
                    .file_name()
                    .map(str::to_string)
                    .or(Some("medidas.csv".into()));
                csv_content = Some(read_text(field).await?);
            }
            "csv_text" => csv_content = Some(read_text(field).await?),
            _ => {}
        }
    }

    let csv_content = required(csv_content, "csv_file or csv_text")?;
    let analysis = analysis::analyze_csv(&csv_content)
        .map_err(|err| AppError::bad_request(format!("CSV invalido: {err}")))?;

    let course_id = required(course_id, "course_id")?;
    let group_id = required(group_id, "group_id")?;
    let practice_id = required(practice_id, "practice_id")?;

    if !db::user_can_submit(&state.pool, &user, &course_id, &group_id, &practice_id).await? {
        return Err(AppError::forbidden(
            "no tenes acceso a ese curso, grupo o practica",
        ));
    }

    let submission = NewSubmission {
        submitted_by_user_id: user.id.clone(),
        course_id,
        group_id,
        practice_id,
        file_name: file_name.unwrap_or_else(|| "medidas.csv".into()),
        csv_content,
        analysis,
    };

    let created = db::create_submission(&state.pool, &state.upload_dir, submission).await?;
    Ok(Json(gate_analysis(created, &user)))
}

pub(super) async fn create_form_submission(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<FormSubmissionInput>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let user = current_user(&state, &headers).await?;
    if !db::user_can_submit(
        &state.pool,
        &user,
        &input.course_id,
        &input.group_id,
        &input.practice_id,
    )
    .await?
    {
        return Err(AppError::forbidden(
            "no tenes acceso a ese curso, grupo o practica",
        ));
    }
    let detail = computation::create_form_submission(&state.pool, &user, input)
        .await
        .map_err(AppError::from_domain_or_db)?;
    Ok(Json(gate_analysis(detail, &user)))
}

#[derive(serde::Deserialize)]
pub(super) struct EditFormBody {
    measurements: Vec<computation::MeasurementInput>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
    #[serde(default)]
    student_results: Option<Vec<db::StudentResultInput>>,
    #[serde(default)]
    student_comment: Option<String>,
}

pub(super) async fn edit_form_submission(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<EditFormBody>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let user = current_user(&state, &headers).await?;
    let detail = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;

    if !matches!(user.role.as_str(), "docente" | "admin") {
        let is_member = db::is_accepted_member(&state.pool, &id, &user.id).await?;
        if !is_member {
            return Err(AppError::forbidden(
                "Solo podés editar entregas de las que sos miembro.",
            ));
        }
    }
    if !detail.can_edit {
        let expired = detail
            .editable_until
            .map(|until| chrono::Utc::now() >= until)
            .unwrap_or(true);
        let message = if expired {
            "El plazo de edición venció: ya no podés modificar esta entrega."
        } else {
            "No podés editar una entrega que ya fue corregida."
        };
        return Err(AppError::bad_request(message));
    }

    let student_comment = input
        .student_comment
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let updated = computation::update_form_submission(
        &state.pool,
        &id,
        &detail.practice_id,
        &input.measurements,
        input.meta.as_ref(),
        input.student_results.as_deref(),
        student_comment,
    )
    .await
    .map_err(AppError::from_domain_or_db)?;
    Ok(Json(gate_analysis(updated, &user)))
}

pub(super) async fn cancel_submission(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Health>, AppError> {
    let user = current_user(&state, &headers).await?;
    let detail = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;

    if !matches!(user.role.as_str(), "docente" | "admin") {
        let is_member = db::is_accepted_member(&state.pool, &id, &user.id).await?;
        if !is_member {
            return Err(AppError::forbidden(
                "Solo podés cancelar entregas de las que sos miembro.",
            ));
        }
    }
    if !detail.can_edit {
        let expired = detail
            .editable_until
            .map(|until| chrono::Utc::now() >= until)
            .unwrap_or(true);
        let message = if expired {
            "El plazo de edición venció: ya no podés cancelar esta entrega."
        } else {
            "No podés cancelar una entrega que ya fue corregida."
        };
        return Err(AppError::bad_request(message));
    }

    db::delete_submission(&state.pool, &id).await?;
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn review_submission(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(review): Json<ReviewSubmission>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    require_teacher(&state, &headers).await?;
    if !matches!(
        review.status.as_str(),
        "pendiente" | "observada" | "aprobada"
    ) {
        return Err(AppError::bad_request(
            "el estado debe ser pendiente, observada o aprobada",
        ));
    }

    let updated = db::update_review(&state.pool, &id, review)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub(super) struct SaveStudentResults {
    results: Vec<db::StudentResultInput>,
}

pub(super) async fn set_student_results(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<SaveStudentResults>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let user = current_user(&state, &headers).await?;
    if !matches!(user.role.as_str(), "docente" | "admin") {
        let is_member = db::is_accepted_member(&state.pool, &id, &user.id).await?;
        if !is_member {
            return Err(AppError::forbidden("no tenes acceso a esta entrega"));
        }
    }
    let submission = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    let is_teacher = matches!(user.role.as_str(), "docente" | "admin");
    if submission.results_visible_to_student && !is_teacher {
        return Err(AppError::bad_request(
            "no podes modificar tus calculos una vez que el docente habilito los resultados",
        ));
    }
    let definition = practices::definition(&state.pool, &submission.practice_id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    computation::check_student_result_symbols(&definition, &body.results)
        .map_err(AppError::bad_request)?;
    db::save_student_results(&state.pool, &id, &body.results).await?;
    let updated = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(gate_analysis(updated, &user)))
}

pub(super) async fn submission_invitations(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::PendingInvitation>>, AppError> {
    let user = current_user(&state, &headers).await?;
    let invitations = db::pending_invitations_for(&state.pool, &user.id).await?;
    Ok(Json(invitations))
}

#[derive(Debug, Deserialize)]
pub(super) struct ExistingReportQuery {
    practice_id: String,
    group_id: String,
    table_number: i64,
}

pub(super) async fn existing_report(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ExistingReportQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = current_user(&state, &headers).await?;
    let submission_id =
        db::find_existing_report(&state.pool, &q.practice_id, &q.group_id, q.table_number).await?;
    match submission_id {
        None => Ok(Json(serde_json::Value::Null)),
        Some(sid) => {
            let is_member = db::is_accepted_member(&state.pool, &sid, &user.id).await?;
            let members = db::report_members_for(&state.pool, &sid).await?;
            let is_owner = members
                .iter()
                .any(|m| m.user_id == user.id && m.role == "owner");
            let can_accept = !is_member
                && members
                    .iter()
                    .any(|m| m.user_id == user.id && m.status == "pending");
            Ok(Json(serde_json::json!({
                "submission_id": sid,
                "is_member": is_member,
                "is_owner": is_owner,
                "can_accept": can_accept,
            })))
        }
    }
}

pub(super) async fn accept_invitation(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let user = current_user(&state, &headers).await?;
    let outcome = db::accept_report_invitation(&state.pool, &id, &user.id).await?;
    match outcome {
        db::AcceptOutcome::Accepted => {}
        db::AcceptOutcome::AlreadyAccepted => {}
        db::AcceptOutcome::NotInvited => {
            return Err(AppError::forbidden(
                "No tenés una invitación para este informe.",
            ));
        }
        db::AcceptOutcome::Expired => {
            return Err(AppError::bad_request(
                "La ventana de aceptación venció. Pedile al docente que te agregue manualmente.",
            ));
        }
    }
    let submission = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(gate_analysis(submission, &user)))
}

pub(super) async fn decline_invitation(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Health>, AppError> {
    let user = current_user(&state, &headers).await?;
    sqlx::query(
        "DELETE FROM report_members WHERE submission_id = ?1 AND user_id = ?2 AND status = 'pending'",
    )
    .bind(&id)
    .bind(&user.id)
    .execute(&state.pool)
    .await?;
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn submission_members(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<db::ReportMember>>, AppError> {
    require_teacher(&state, &headers).await?;
    let members = db::report_members_for(&state.pool, &id).await?;
    Ok(Json(members))
}

#[derive(Debug, Deserialize)]
pub(super) struct AddMemberBody {
    user_id: String,
    #[serde(default)]
    force_accept: bool,
}

pub(super) async fn add_submission_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<AddMemberBody>,
) -> Result<Json<Vec<db::ReportMember>>, AppError> {
    require_teacher(&state, &headers).await?;
    let added = db::add_report_member(&state.pool, &id, &body.user_id, body.force_accept).await?;
    if !added {
        return Err(AppError::not_found(
            "usuario no encontrado o no es estudiante",
        ));
    }
    Ok(Json(db::report_members_for(&state.pool, &id).await?))
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoveMemberBody {
    user_id: String,
}

pub(super) async fn remove_submission_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RemoveMemberBody>,
) -> Result<Json<Vec<db::ReportMember>>, AppError> {
    require_teacher(&state, &headers).await?;
    let removed = db::remove_report_member(&state.pool, &id, &body.user_id).await?;
    if !removed {
        return Err(AppError::not_found("miembro no encontrado en este informe"));
    }
    Ok(Json(db::report_members_for(&state.pool, &id).await?))
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateReportMeta {
    group_id: Option<String>,
    table_number: Option<i64>,
}

pub(super) async fn update_report_meta(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateReportMeta>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    require_teacher(&state, &headers).await?;
    let updated = db::update_report_meta(
        &state.pool,
        &id,
        body.group_id.as_deref(),
        body.table_number,
    )
    .await
    .map_err(|e| AppError::bad_request(e.to_string()))?
    .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_hidden_only_for_students_without_visibility() {
        assert!(!analysis_hidden_for("docente", false));
        assert!(!analysis_hidden_for("admin", false));
        assert!(!analysis_hidden_for("docente", true));
        assert!(analysis_hidden_for("estudiante", false));
        assert!(!analysis_hidden_for("estudiante", true));
    }
}
