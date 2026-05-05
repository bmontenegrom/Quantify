use crate::{
    analysis,
    db::{self, AppState, NewSubmission, ReviewSubmission},
    error::AppError,
};
use axum::{
    extract::{Multipart, Path, State},
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

type SharedState = Arc<AppState>;

pub fn api_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
        .route("/users", get(users).post(create_user))
        .route("/users/{id}/password", post(reset_password))
        .route("/academic/context", get(academic_context))
        .route("/academic/courses", post(create_course))
        .route("/academic/courses/{id}/groups", post(create_group))
        .route(
            "/academic/courses/{id}/practices",
            post(enable_course_practice),
        )
        .route("/academic/groups/{id}/members", post(add_group_member))
        .route("/practices", get(practices))
        .route("/submissions", get(submissions).post(create_submission))
        .route("/submissions/{id}", get(submission_detail))
        .route("/submissions/{id}/review", post(review_submission))
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn login(
    State(state): State<SharedState>,
    Json(request): Json<db::LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let Some((token, user)) = db::login(&state.pool, request).await? else {
        return Err(AppError::unauthorized("email o contrasena invalidos"));
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, session_cookie(&token, 12 * 60 * 60));
    Ok((headers, Json(db::LoginResponse { user })))
}

async fn logout(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = session_token(&headers) {
        db::logout(&state.pool, &token).await?;
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::SET_COOKIE, session_cookie("", 0));
    Ok((response_headers, Json(Health { status: "ok" })))
}

async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<db::LoginResponse>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(db::LoginResponse { user }))
}

async fn change_password(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::ChangePassword>,
) -> Result<Json<Health>, AppError> {
    let user = current_user(&state, &headers).await?;
    validate_password(&input.new_password)?;
    let changed = db::change_password(&state.pool, &user.id, input).await?;
    if !changed {
        return Err(AppError::unauthorized("contrasena actual incorrecta"));
    }
    Ok(Json(Health { status: "ok" }))
}

async fn users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::AuthUser>>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(db::users(&state.pool).await?))
}

async fn create_user(
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

async fn reset_password(
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

async fn practices(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::Practice>>, AppError> {
    current_user(&state, &headers).await?;
    Ok(Json(db::practices(&state.pool).await?))
}

async fn academic_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<db::AcademicContext>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(db::academic_context(&state.pool, &user).await?))
}

async fn create_course(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::CreateCourse>,
) -> Result<Json<db::Course>, AppError> {
    require_teacher(&state, &headers).await?;
    if input.name.trim().is_empty() || input.term.trim().is_empty() {
        return Err(AppError::bad_request("name and term are required"));
    }
    Ok(Json(db::create_course(&state.pool, input).await?))
}

async fn create_group(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::CreateGroup>,
) -> Result<Json<db::LabGroup>, AppError> {
    require_teacher(&state, &headers).await?;
    if input.name.trim().is_empty() {
        return Err(AppError::bad_request("name is required"));
    }
    Ok(Json(
        db::create_group(&state.pool, &course_id, input).await?,
    ))
}

async fn add_group_member(
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

async fn enable_course_practice(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::SetCoursePractice>,
) -> Result<Json<Health>, AppError> {
    require_teacher(&state, &headers).await?;
    db::enable_course_practice(&state.pool, &course_id, input).await?;
    Ok(Json(Health { status: "ok" }))
}

async fn submissions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::SubmissionListItem>>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(db::submission_list(&state.pool).await?))
}

async fn submission_detail(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    require_teacher(&state, &headers).await?;
    let submission = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("submission not found"))?;
    Ok(Json(submission))
}

async fn create_submission(
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
        .map_err(|_| AppError::bad_request("invalid multipart payload"))?
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
        submitted_by_user_id: user.id,
        course_id,
        group_id,
        practice_id,
        file_name: file_name.unwrap_or_else(|| "medidas.csv".into()),
        csv_content,
        analysis,
    };

    let created = db::create_submission(&state.pool, &state.upload_dir, submission).await?;
    Ok(Json(created))
}

async fn review_submission(
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
            "status must be pendiente, observada or aprobada",
        ));
    }

    let updated = db::update_review(&state.pool, &id, review)
        .await?
        .ok_or_else(|| AppError::not_found("submission not found"))?;
    Ok(Json(updated))
}

async fn read_text(field: axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    field
        .text()
        .await
        .map_err(|_| AppError::bad_request("invalid text field"))
}

fn required(value: Option<String>, name: &str) -> Result<String, AppError> {
    let value = value.ok_or_else(|| AppError::bad_request(format!("{name} is required")))?;
    if value.trim().is_empty() {
        return Err(AppError::bad_request(format!("{name} is required")));
    }
    Ok(value)
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::bad_request(
            "la contrasena debe tener al menos 8 caracteres",
        ));
    }
    Ok(())
}

fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

async fn current_user(state: &SharedState, headers: &HeaderMap) -> Result<db::AuthUser, AppError> {
    let token = session_token(headers).ok_or_else(|| AppError::unauthorized("login requerido"))?;
    db::user_by_session(&state.pool, &token)
        .await?
        .ok_or_else(|| AppError::unauthorized("sesion invalida o vencida"))
}

async fn require_teacher(
    state: &SharedState,
    headers: &HeaderMap,
) -> Result<db::AuthUser, AppError> {
    let user = current_user(state, headers).await?;
    if matches!(user.role.as_str(), "docente" | "admin") {
        Ok(user)
    } else {
        Err(AppError::forbidden("se requiere rol docente"))
    }
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(name, value)| (name == "quantify_session").then(|| value.to_string()))
}

fn session_cookie(token: &str, max_age_seconds: i64) -> HeaderValue {
    let value = format!(
        "quantify_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}"
    );
    HeaderValue::from_str(&value).expect("valid cookie header")
}
