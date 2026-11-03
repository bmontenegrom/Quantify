use super::{
    current_user, is_valid_email, not_blank, require_teacher, validate_password, Health,
    SharedState,
};
use crate::{db, error::AppError};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};

pub(super) async fn users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::AuthUser>>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(db::users(&state.pool).await?))
}

pub(super) async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::CreateUser>,
) -> Result<Json<db::AuthUser>, AppError> {
    require_teacher(&state, &headers).await?;
    if !is_valid_email(&input.email)
        || input.display_name.trim().is_empty()
        || !matches!(input.role.as_str(), "estudiante" | "docente" | "admin")
    {
        return Err(AppError::bad_request("datos de usuario invalidos"));
    }
    validate_password(&input.password)?;
    Ok(Json(db::create_user(&state.pool, input).await?))
}

pub(super) async fn reset_password(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<db::ResetPassword>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    validate_password(&input.password)?;
    if !db::reset_password(&state.pool, &id, input).await? {
        return Err(AppError::not_found("usuario no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn update_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<db::UpdateUser>,
) -> Result<Json<db::AuthUser>, AppError> {
    require_teacher(&state, &headers).await?;
    if !is_valid_email(&input.email)
        || input.display_name.trim().is_empty()
        || !matches!(input.role.as_str(), "estudiante" | "docente" | "admin")
    {
        return Err(AppError::bad_request("datos de usuario invalidos"));
    }
    let updated = db::update_user(&state.pool, &id, input)
        .await?
        .ok_or_else(|| AppError::not_found("usuario no encontrado"))?;
    Ok(Json(updated))
}

pub(super) async fn grades(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::CourseGradebook>>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(db::gradebook_for_user(&state.pool, &user).await?))
}

pub(super) async fn create_grade_component(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::CreateGradeComponent>,
) -> Result<Json<db::GradeComponent>, AppError> {
    require_teacher(&state, &headers).await?;
    if input.name.trim().is_empty()
        || !matches!(input.kind.as_str(), "pregunta" | "informe" | "parcial")
        || input.max_points <= 0.0
        || input.weight_points <= 0.0
    {
        return Err(AppError::bad_request("datos de componente invalidos"));
    }
    Ok(Json(db::create_grade_component(&state.pool, input).await?))
}

pub(super) async fn upsert_grade_score(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::UpsertGradeScore>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if input.raw_points < 0.0 {
        return Err(AppError::bad_request("la nota no puede ser negativa"));
    }
    db::upsert_grade_score(&state.pool, input)
        .await
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn academic_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<db::AcademicContext>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(db::academic_context(&state.pool, &user).await?))
}

pub(super) async fn create_course(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::CreateCourse>,
) -> Result<Json<db::Course>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre es requerido")?;
    not_blank(&input.term, "periodo es requerido")?;
    Ok(Json(db::create_course(&state.pool, input).await?))
}

pub(super) async fn update_course(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::UpdateCourse>,
) -> Result<Json<db::Course>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre es requerido")?;
    not_blank(&input.term, "periodo es requerido")?;
    let updated = db::update_course(&state.pool, &course_id, input)
        .await?
        .ok_or_else(|| AppError::not_found("curso no encontrado"))?;
    Ok(Json(updated))
}

pub(super) async fn create_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::CreateGroup>,
) -> Result<Json<db::LabGroup>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre de grupo es requerido")?;
    if !valid_group_table_count(input.table_count.unwrap_or(4)) {
        return Err(AppError::bad_request("cantidad de mesas invalida"));
    }
    Ok(Json(
        db::create_group(&state.pool, &course_id, input).await?,
    ))
}

pub(super) async fn update_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<db::UpdateGroup>,
) -> Result<Json<db::LabGroup>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre de grupo es requerido")?;
    if !valid_group_table_count(input.table_count) {
        return Err(AppError::bad_request("cantidad de mesas invalida"));
    }
    let updated = db::update_group(&state.pool, &group_id, input)
        .await?
        .ok_or_else(|| AppError::not_found("grupo no encontrado"))?;
    Ok(Json(updated))
}

fn valid_group_table_count(value: i64) -> bool {
    (1..=24).contains(&value)
}

pub(super) async fn create_subgroup(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::CreateSubgroup>,
) -> Result<Json<db::PracticeSubgroup>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre de subgrupo es requerido")?;
    not_blank(&input.practice_id, "practica es requerida")?;
    not_blank(&input.group_id, "grupo es requerido")?;
    db::create_subgroup(&state.pool, &course_id, input)
        .await?
        .map(Json)
        .ok_or_else(|| {
            AppError::bad_request(
                "Solo se crean subgrupos de una practica publicada y habilitada en el curso.",
            )
        })
}

pub(super) async fn add_group_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<db::AddGroupMember>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    db::add_group_member(&state.pool, &group_id, input)
        .await?
        .ok_or_else(|| AppError::not_found("estudiante no encontrado"))?;
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn add_course_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::EnrollCourseMember>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    db::add_course_member(&state.pool, &course_id, input)
        .await?
        .ok_or_else(|| AppError::not_found("estudiante no encontrado"))?;
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn remove_group_member(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<db::AddGroupMember>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let removed = db::remove_group_member(&state.pool, &group_id, &input.user_id).await?;
    if !removed {
        return Err(AppError::not_found("miembro de grupo no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn remove_group_member_path(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    let removed = db::remove_group_member(&state.pool, &group_id, &user_id).await?;
    if !removed {
        return Err(AppError::not_found("miembro de grupo no encontrado"));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn set_practice_table(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<db::SetPracticeTable>,
) -> Result<Json<db::PracticeTableAssignment>, AppError> {
    let user = current_user(&state, &headers).await?;
    if input.practice_id.trim().is_empty() || !valid_group_table_count(input.table_number) {
        return Err(AppError::bad_request("datos de mesa invalidos"));
    }
    let assignment = db::set_practice_table_assignment(&state.pool, &group_id, &user.id, input)
        .await?
        .ok_or_else(|| AppError::bad_request("mesa no disponible para este grupo"))?;
    Ok(Json(assignment))
}

pub(super) async fn enable_course_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::SetCoursePractice>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !db::enable_course_practice(&state.pool, &course_id, input).await? {
        return Err(AppError::bad_request(
            "Solo se puede habilitar una practica publicada (no un borrador ni una archivada).",
        ));
    }
    Ok(Json(Health { status: "ok" }))
}

pub(super) async fn disable_course_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::SetCoursePractice>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    if !db::disable_course_practice(&state.pool, &course_id, &input.practice_id).await? {
        return Err(AppError::not_found("practica no habilitada en el curso"));
    }
    Ok(Json(Health { status: "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_group_table_count_range() {
        assert!(valid_group_table_count(1));
        assert!(valid_group_table_count(24));
        assert!(!valid_group_table_count(0));
        assert!(!valid_group_table_count(25));
    }
}
