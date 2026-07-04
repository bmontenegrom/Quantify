use crate::{
    analysis,
    computation::{self, FormSubmissionInput},
    db::{self, AppState, NewSubmission, ReviewSubmission},
    error::AppError,
    instruments::{self, CatalogExport, CreateInstrument, ScaleInput, UpdateInstrument},
    practices::{
        self, AggregateInput, CurveInput, IntermediateInput, PointResultInput, QuantityInput,
        ResultInput,
    },
};
use axum::{
    extract::{Multipart, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;
use subtle::ConstantTimeEq;

type SharedState = Arc<AppState>;

// ── CSRF ──────────────────────────────────────────────────────────────────────

/// Deriva el token CSRF de un token de sesión usando HMAC-SHA256 con `secret_key`.
/// Stateless: no requiere consulta a la base de datos. HMAC evita ataques de
/// extensión de longitud que afectarían a un `SHA-256(secret || token)` plano.
///
/// # Ejemplos
///
/// ```
/// let token = quantify::routes::compute_csrf("mi-sesion", "clave-secreta");
/// // HMAC-SHA256 produce siempre 64 caracteres hexadecimales.
/// assert_eq!(token.len(), 64);
/// assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
/// // Mismos inputs → mismo token (determinista).
/// assert_eq!(token, quantify::routes::compute_csrf("mi-sesion", "clave-secreta"));
/// // Distinto secret → distinto token.
/// assert_ne!(token, quantify::routes::compute_csrf("mi-sesion", "otra-clave"));
/// ```
pub fn compute_csrf(session_token: &str, secret_key: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC acepta claves de cualquier longitud");
    mac.update(session_token.as_bytes());
    format!("{:x}", mac.finalize().into_bytes())
}

/// Middleware que valida el token CSRF en todas las solicitudes mutantes (POST/PUT/PATCH/DELETE)
/// excepto `POST /auth/login` (que no tiene sesión aún) y `POST /auth/logout`
/// (que no necesita protección: el daño de un logout forzado es mínimo y reversible).
///
/// Nota: el middleware vive dentro del router montado en `/api`, por lo que Axum
/// le entrega la ruta ya sin ese prefijo (`/auth/login`, no `/api/auth/login`).
pub async fn csrf_middleware(
    State(state): State<SharedState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method();
    let path = request.uri().path();

    let needs_csrf = matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) && path != "/auth/login"
        && path != "/auth/logout";

    if !needs_csrf {
        return next.run(request).await;
    }

    let session_tok = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .filter_map(|c| c.trim().split_once('='))
                .find_map(|(k, v)| (k == "quantify_session").then(|| v.to_string()))
        });

    let csrf_header = request
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let valid = match (session_tok, csrf_header) {
        (Some(tok), Some(csrf)) => {
            let expected = compute_csrf(&tok, &state.secret_key);
            // Comparación en tiempo constante para no filtrar el token por timing.
            expected.as_bytes().ct_eq(csrf.as_bytes()).into()
        }
        _ => false,
    };

    if valid {
        next.run(request).await
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "token CSRF inválido o ausente" })),
        )
            .into_response()
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Construye el router de la API bajo `/api`, registrando todas las rutas y el estado compartido.
pub fn api_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/profile", post(update_profile))
        .route("/auth/password", post(change_password))
        .route("/users", get(users).post(create_user))
        .route("/users/{id}", post(update_user))
        .route("/users/{id}/password", post(reset_password))
        .route("/grades", get(grades))
        .route("/grades/components", post(create_grade_component))
        .route("/grades/scores", post(upsert_grade_score))
        .route("/academic/context", get(academic_context))
        .route("/academic/courses", post(create_course))
        .route("/academic/courses/{id}", post(update_course))
        .route("/academic/groups/{id}", post(update_group))
        .route("/academic/courses/{id}/groups", post(create_group))
        .route("/academic/courses/{id}/subgroups", post(create_subgroup))
        .route(
            "/academic/courses/{id}/practices",
            post(enable_course_practice),
        )
        .route("/academic/courses/{id}/members", post(add_course_member))
        .route("/academic/groups/{id}/members", post(add_group_member))
        .route(
            "/academic/groups/{id}/members/remove",
            post(remove_group_member),
        )
        .route(
            "/academic/groups/{id}/practice-table",
            post(set_practice_table),
        )
        .route(
            "/academic/groups/{group_id}/members/{user_id}/remove",
            post(remove_group_member_path),
        )
        .route("/practices", get(list_practices))
        .route("/practices/{id}/definition", get(practice_definition))
        .route("/practices/{id}/analyze-preview", post(analyze_preview))
        .route(
            "/practices/{id}/analysis-kind",
            post(set_practice_analysis_kind),
        )
        .route(
            "/practices/{id}/regression-formulas",
            post(set_practice_regression_formulas),
        )
        .route(
            "/practices/{id}/operator-count",
            post(set_practice_operator_count),
        )
        .route("/practices/{id}/quantities", post(create_quantity))
        .route(
            "/practices/{id}/quantities/{qid}",
            post(update_quantity).delete(delete_quantity),
        )
        .route("/practices/{id}/results", post(create_result))
        .route(
            "/practices/{id}/results/{rid}",
            post(update_result).delete(delete_result),
        )
        .route("/practices/{id}/curves", post(create_curve))
        .route(
            "/practices/{id}/curves/{cid}",
            post(update_curve).delete(delete_curve),
        )
        .route("/practices/{id}/curves/{cid}/move", post(move_curve))
        .route("/practices/{id}/intermediates", post(create_intermediate))
        .route(
            "/practices/{id}/intermediates/{iid}",
            post(update_intermediate).delete(delete_intermediate),
        )
        .route("/practices/{id}/point-results", post(create_point_result))
        .route(
            "/practices/{id}/point-results/{pid}",
            post(update_point_result).delete(delete_point_result),
        )
        .route("/practices/{id}/aggregates", post(create_aggregate))
        .route(
            "/practices/{id}/aggregates/{aid}",
            post(update_aggregate).delete(delete_aggregate),
        )
        .route(
            "/practices/{id}/results/{rid}/tolerance",
            post(set_result_tolerance),
        )
        .route(
            "/instruments",
            get(list_instruments).post(create_instrument),
        )
        .route("/instruments/export", get(export_instruments))
        .route("/instruments/import", post(import_instruments))
        .route(
            "/instruments/{id}",
            post(update_instrument).delete(delete_instrument),
        )
        .route("/instruments/{id}/scales", post(create_scale))
        .route(
            "/instruments/{id}/scales/{scale_id}",
            post(update_scale).delete(delete_scale),
        )
        .route("/submissions", get(submissions).post(create_submission))
        .route("/submissions/form", post(create_form_submission))
        .route("/submissions/invitations", get(submission_invitations))
        .route("/submissions/existing", get(existing_report))
        .route("/submissions/{id}/edit", post(edit_form_submission))
        .route(
            "/submissions/{id}",
            get(submission_detail).delete(cancel_submission),
        )
        .route("/submissions/{id}/review", post(review_submission))
        .route(
            "/submissions/{id}/student-results",
            post(set_student_results),
        )
        .route("/submissions/{id}/accept", post(accept_invitation))
        .route("/submissions/{id}/decline", post(decline_invitation))
        .route(
            "/submissions/{id}/members",
            get(submission_members).post(add_submission_member),
        )
        .route(
            "/submissions/{id}/members/remove",
            post(remove_submission_member),
        )
        .route("/submissions/{id}/report", post(update_report_meta))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            csrf_middleware,
        ))
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

/// `GET /api/health`: chequeo de vida del servicio; siempre responde `{"status":"ok"}`.
async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

/// `POST /api/auth/login`: valida credenciales y, si son correctas, setea la cookie de sesión.
/// Aplica rate-limiting: bloquea 15 minutos tras 5 intentos fallidos consecutivos por email.
async fn login(
    State(state): State<SharedState>,
    Json(request): Json<db::LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email_key = request
        .email
        .as_deref()
        .or(request.username.as_deref())
        .unwrap_or("")
        .trim()
        .to_lowercase();

    let now = Utc::now();

    // Verificar rate-limit antes de consultar la BD.
    {
        let mut attempts = state
            .login_attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(info) = attempts.get_mut(&email_key) {
            if info.is_blocked(now) {
                return Err(AppError::too_many_requests(format!(
                    "demasiados intentos fallidos, esperá {} minutos",
                    db::LOGIN_BLOCK_MINUTES
                )));
            }
        }
    }

    let result = db::login(&state.pool, request).await?;

    // Actualizar contadores según el resultado.
    {
        let mut attempts = state
            .login_attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match &result {
            Some(_) => {
                attempts.remove(&email_key);
            }
            None => {
                attempts
                    .entry(email_key.clone())
                    .or_default()
                    .register_failure(now);
            }
        }
        // Acota el crecimiento del mapa ante enumeración de emails.
        db::purge_expired_attempts(&mut attempts, now);
    }

    let Some((token, auth_user)) = result else {
        return Err(AppError::unauthorized("email o contrasena invalidos"));
    };
    let user = db::me_user(&state.pool, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::not_found("usuario no encontrado"))?;

    let csrf_token = compute_csrf(&token, &state.secret_key);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        session_cookie(&token, 12 * 60 * 60, state.secure_cookies),
    );
    Ok((headers, Json(db::LoginResponse { user, csrf_token })))
}

/// `POST /api/auth/logout`: elimina la sesión actual y limpia la cookie.
async fn logout(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = session_token(&headers) {
        db::logout(&state.pool, &token).await?;
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        session_cookie("", 0, state.secure_cookies),
    );
    Ok((response_headers, Json(Health { status: "ok" })))
}

/// `GET /api/auth/me`: devuelve el usuario autenticado con sus defaults de perfil.
/// Incluye `csrf_token` para que el frontend pueda reconstituirlo tras un reload sin re-login.
async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<db::LoginResponse>, AppError> {
    let auth_user = current_user(&state, &headers).await?;
    // `current_user` ya validó que la cookie de sesión existe, así que el token siempre está
    // presente; el `unwrap_or_default` es solo defensivo y nunca debería usarse en la práctica.
    let token = session_token(&headers).unwrap_or_default();
    let user = db::me_user(&state.pool, &auth_user.id)
        .await?
        .ok_or_else(|| AppError::not_found("usuario no encontrado"))?;
    let csrf_token = compute_csrf(&token, &state.secret_key);
    Ok(Json(db::LoginResponse { user, csrf_token }))
}

/// `POST /api/auth/password`: cambia la contraseña del usuario actual validando la actual.
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

/// `POST /api/auth/profile`: actualiza nombre, email y opcionalmente grupo/mesa por defecto.
async fn update_profile(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::UpdateProfileInput>,
) -> Result<Json<db::MeUser>, AppError> {
    let user = current_user(&state, &headers).await?;
    if !is_valid_email(&input.email) || input.display_name.trim().is_empty() {
        return Err(AppError::bad_request("datos de usuario invalidos"));
    }
    db::update_user(
        &state.pool,
        &user.id,
        db::UpdateUser {
            email: input.email,
            display_name: input.display_name,
            role: user.role,
        },
    )
    .await?
    .ok_or_else(|| AppError::not_found("usuario no encontrado"))?;

    if let Some(group_id) = input.default_group_id.as_deref().filter(|s| !s.is_empty()) {
        db::set_user_default_group(&state.pool, &user.id, group_id).await?;
        if let Some(table_number) = input.default_table_number {
            db::set_user_default_table(&state.pool, &user.id, group_id, table_number).await?;
        }
    }

    let me = db::me_user(&state.pool, &user.id)
        .await?
        .ok_or_else(|| AppError::not_found("usuario no encontrado"))?;
    Ok(Json(me))
}

/// `GET /api/users`: lista de usuarios (solo docente/admin).
async fn users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::AuthUser>>, AppError> {
    require_teacher(&state, &headers).await?;
    Ok(Json(db::users(&state.pool).await?))
}

/// `POST /api/users`: crea un usuario (solo docente/admin) validando email, rol y contraseña.
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

/// `POST /api/users/{id}/password`: restablece la contraseña de un usuario (solo docente/admin).
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

/// `POST /api/users/{id}`: actualiza email, nombre y rol de un usuario (solo docente/admin).
async fn update_user(
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

/// `GET /api/grades`: libreta de calificaciones según el rol del usuario autenticado.
async fn grades(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::CourseGradebook>>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(db::gradebook_for_user(&state.pool, &user).await?))
}

/// `POST /api/grades/components`: crea un componente evaluable (solo docente/admin), validando tipo y puntajes.
async fn create_grade_component(
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

/// `POST /api/grades/scores`: carga o actualiza el puntaje de un estudiante (solo docente/admin).
async fn upsert_grade_score(
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

/// `GET /api/practices`: catálogo de prácticas (requiere sesión válida).
/// `GET /api/practices`: catálogo completo de prácticas (requiere sesión válida).
async fn list_practices(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::Practice>>, AppError> {
    current_user(&state, &headers).await?;
    Ok(Json(db::practices(&state.pool).await?))
}

/// `GET /api/academic/context`: contexto académico (cursos/grupos/prácticas) según el rol.
async fn academic_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<db::AcademicContext>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(db::academic_context(&state.pool, &user).await?))
}

/// `POST /api/academic/courses`: crea un curso (solo docente/admin).
async fn create_course(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(input): Json<db::CreateCourse>,
) -> Result<Json<db::Course>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre es requerido")?;
    not_blank(&input.term, "periodo es requerido")?;
    Ok(Json(db::create_course(&state.pool, input).await?))
}

/// `POST /api/academic/courses/{id}`: actualiza un curso (solo docente/admin).
async fn update_course(
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

/// `POST /api/academic/courses/{id}/groups`: crea un grupo en el curso (solo docente/admin).
async fn create_group(
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

/// `POST /api/academic/groups/{id}`: actualiza un grupo (solo docente/admin).
async fn update_group(
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

/// Valida que la cantidad de mesas esté en el rango permitido (1..=24).
fn valid_group_table_count(value: i64) -> bool {
    (1..=24).contains(&value)
}

/// `POST /api/academic/courses/{id}/subgroups`: crea un subgrupo de práctica (solo docente/admin).
async fn create_subgroup(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(course_id): Path<String>,
    Json(input): Json<db::CreateSubgroup>,
) -> Result<Json<db::PracticeSubgroup>, AppError> {
    require_teacher(&state, &headers).await?;
    not_blank(&input.name, "nombre de subgrupo es requerido")?;
    not_blank(&input.practice_id, "practica es requerida")?;
    not_blank(&input.group_id, "grupo es requerido")?;
    Ok(Json(
        db::create_subgroup(&state.pool, &course_id, input).await?,
    ))
}

/// `POST /api/academic/groups/{id}/members`: agrega un estudiante a un grupo (solo docente/admin).
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

/// `POST /api/academic/courses/{id}/members`: inscribe un estudiante en un curso (solo docente/admin).
async fn add_course_member(
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

/// `POST /api/academic/groups/{id}/members/remove`: quita un estudiante de un grupo (body con `user_id`).
async fn remove_group_member(
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

/// `POST /api/academic/groups/{group_id}/members/{user_id}/remove`: variante que toma ambos ids en la ruta.
async fn remove_group_member_path(
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

/// `POST /api/academic/groups/{id}/practice-table`: el usuario elige su mesa para una práctica del grupo.
async fn set_practice_table(
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

/// `POST /api/academic/courses/{id}/practices`: habilita una práctica en el curso (solo docente/admin).
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

/// `GET /api/submissions`: lista de entregas visibles para el usuario actual.
async fn submissions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::SubmissionListItem>>, AppError> {
    let user = current_user(&state, &headers).await?;
    Ok(Json(
        db::submission_list_for_user(&state.pool, &user).await?,
    ))
}

/// `GET /api/submissions/{id}`: detalle de una entrega; un estudiante solo puede ver
/// informes donde es miembro aceptado (o el owner original).
async fn submission_detail(
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

/// `true` si hay que ocultarle el cálculo a este viewer: solo los estudiantes, y solo
/// mientras el docente no haya habilitado la visibilidad de la entrega. Docente/admin nunca.
fn analysis_hidden_for(role: &str, results_visible: bool) -> bool {
    !matches!(role, "docente" | "admin") && !results_visible
}

/// Oculta el cálculo automático (`analysis = null`) cuando corresponde según [`analysis_hidden_for`].
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

/// `POST /api/submissions`: recibe un multipart (curso/grupo/práctica + CSV), analiza el CSV,
/// valida permisos y crea la entrega.
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

/// `POST /api/submissions/form`: crea una entrega por formulario (lecturas crudas) calculando
/// las incertidumbres automáticamente. Valida acceso al curso/grupo/práctica.
async fn create_form_submission(
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

/// Cuerpo para editar una entrega por formulario (lecturas + meta de depuración).
#[derive(serde::Deserialize)]
struct EditFormBody {
    measurements: Vec<computation::MeasurementInput>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
    /// `None` = no tocar los cálculos del alumno ya guardados; `Some(vec)` los reemplaza.
    #[serde(default)]
    student_results: Option<Vec<db::StudentResultInput>>,
    /// Observaciones/comentarios libres del alumno; se reemplaza por completo en cada edición.
    #[serde(default)]
    student_comment: Option<String>,
}

/// `POST /api/submissions/{id}/edit`: el alumno dueño reemplaza sus lecturas dentro de la
/// ventana de edición (submitted_at + horas del curso). Recalcula el análisis sin tocar
/// `submitted_at`. Rechaza si no es el dueño, venció el plazo, o ya fue corregida/visible.
async fn edit_form_submission(
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

/// `DELETE /api/submissions/{id}`: el alumno dueño (o miembro aceptado) cancela su entrega dentro
/// de la ventana de edición, borrándola por completo (mediciones, resultados propios e integrantes
/// se van con ella). Mismas reglas de propiedad/ventana que `edit_form_submission`.
async fn cancel_submission(
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

/// `POST /api/submissions/{id}/review`: registra la revisión docente (estado/comentario/nota).
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
            "el estado debe ser pendiente, observada o aprobada",
        ));
    }

    let updated = db::update_review(&state.pool, &id, review)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(updated))
}

/// Cuerpo para guardar los mensurandos calculados por el estudiante.
#[derive(Debug, Deserialize)]
struct SaveStudentResults {
    results: Vec<db::StudentResultInput>,
}

/// `POST /api/submissions/{id}/student-results`: el estudiante dueño guarda sus mensurandos
/// finales (valor ± U) para compararlos con el cálculo automático. Solo se permite mientras el
/// docente no haya habilitado la visibilidad del cálculo (para no copiar el resultado).
async fn set_student_results(
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
    // Los símbolos deben corresponder a mensurandos de la práctica.
    let definition = practices::definition(&state.pool, &submission.practice_id)
        .await?
        .ok_or_else(|| AppError::not_found("practica no encontrada"))?;
    let valid: std::collections::HashSet<&str> = definition
        .results
        .iter()
        .map(|r| r.symbol.as_str())
        .collect();
    for result in &body.results {
        if !valid.contains(result.symbol.trim()) {
            return Err(AppError::bad_request(format!(
                "el simbolo \"{}\" no es un mensurando de esta practica",
                result.symbol.trim()
            )));
        }
    }
    db::save_student_results(&state.pool, &id, &body.results).await?;
    let updated = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("entrega no encontrada"))?;
    Ok(Json(gate_analysis(updated, &user)))
}

/// `GET /api/submissions/invitations`: invitaciones vigentes del alumno autenticado.
async fn submission_invitations(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::PendingInvitation>>, AppError> {
    let user = current_user(&state, &headers).await?;
    let invitations = db::pending_invitations_for(&state.pool, &user.id).await?;
    Ok(Json(invitations))
}

/// Query params para `GET /api/submissions/existing`.
#[derive(Debug, Deserialize)]
struct ExistingReportQuery {
    practice_id: String,
    group_id: String,
    table_number: i64,
}

/// `GET /api/submissions/existing`: busca si ya existe un informe para (práctica, grupo, mesa).
/// Devuelve `null` o `{ submission_id, is_member, is_owner }`.
async fn existing_report(
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

/// `POST /api/submissions/{id}/accept`: el alumno acepta una invitación al informe compartido.
async fn accept_invitation(
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

/// `POST /api/submissions/{id}/decline`: el alumno declina la invitación al informe.
/// Elimina la fila pending del alumno en `report_members`. Idempotente: 200 si ya no existía.
async fn decline_invitation(
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

/// `GET /api/submissions/{id}/members`: lista los miembros del informe (docente/admin).
async fn submission_members(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<db::ReportMember>>, AppError> {
    require_teacher(&state, &headers).await?;
    let members = db::report_members_for(&state.pool, &id).await?;
    Ok(Json(members))
}

/// Cuerpo para agregar un miembro a un informe (docente).
#[derive(Debug, Deserialize)]
struct AddMemberBody {
    user_id: String,
    #[serde(default)]
    force_accept: bool,
}

/// `POST /api/submissions/{id}/members`: el docente agrega un miembro (accepted directamente si force_accept).
async fn add_submission_member(
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

/// Cuerpo para quitar un miembro de un informe (docente).
#[derive(Debug, Deserialize)]
struct RemoveMemberBody {
    user_id: String,
}

/// `POST /api/submissions/{id}/members/remove`: el docente quita un miembro del informe.
async fn remove_submission_member(
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

/// Cuerpo para actualizar grupo y/o mesa de un informe (docente).
#[derive(Debug, Deserialize)]
struct UpdateReportMeta {
    group_id: Option<String>,
    table_number: Option<i64>,
}

/// `POST /api/submissions/{id}/report`: el docente actualiza grupo y/o mesa del informe.
async fn update_report_meta(
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

/// Parámetro de query `?course_id=...` para las operaciones de catálogo por curso.
#[derive(Debug, Deserialize)]
struct CourseQuery {
    course_id: String,
}

/// Cuerpo para importar un catálogo a un curso destino.
#[derive(Debug, Deserialize)]
struct ImportRequest {
    course_id: String,
    instruments: Vec<instruments::InstrumentExport>,
}

/// `GET /api/instruments?course_id=...`: lista los instrumentos de un curso con sus escalas.
/// Solo lectura del catálogo (material del curso): accesible a cualquier usuario autenticado,
/// para que el estudiante pueda elegir instrumento/escala al cargar una entrega por formulario.
/// La gestión (alta/edición/baja) sigue siendo solo docente/admin.
async fn list_instruments(
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
async fn create_instrument(
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
async fn update_instrument(
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
async fn delete_instrument(
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
async fn create_scale(
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
async fn update_scale(
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
async fn delete_scale(
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
async fn export_instruments(
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
async fn import_instruments(
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

// ── Definición de prácticas ───────────────────────────────────────────────────

/// Cuerpo para actualizar el tipo de análisis de una práctica.
#[derive(Debug, Deserialize)]
struct SetAnalysisKindBody {
    analysis_kind: String,
}

/// `GET /api/practices/{id}/definition`: magnitudes + mensurandos de una práctica (requiere sesión).
async fn practice_definition(
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
struct AnalyzePreviewBody {
    measurements: Vec<computation::MeasurementInput>,
}

/// `POST /api/practices/{id}/analyze-preview`: calcula el análisis (incl. regresión) sin
/// persistir, para previsualizar el gráfico/parámetros mientras el alumno carga datos.
async fn analyze_preview(
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
async fn set_practice_analysis_kind(
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
struct RegressionFormulasBody {
    x_formula: String,
    y_formula: String,
}

/// Cuerpo para definir la cantidad de operadores de una práctica estadística (Motor D).
#[derive(Debug, Deserialize)]
struct OperatorCountBody {
    /// Cantidad de operadores; `<= 1` desactiva los operadores (comportamiento por defecto).
    count: i64,
}

/// `POST /api/practices/{id}/operator-count`: fija la cantidad de operadores de una práctica
/// estadística (docente/admin). Se acota a un máximo razonable para no explotar el formulario.
async fn set_practice_operator_count(
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
async fn set_practice_regression_formulas(
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
async fn create_quantity(
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
async fn update_quantity(
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
async fn delete_quantity(
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
async fn create_curve(
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
async fn update_curve(
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
struct MoveCurveBody {
    /// `true` mueve la curva una posición hacia arriba; `false` (o ausente), hacia abajo.
    #[serde(default)]
    up: bool,
}

/// `POST /api/practices/{id}/curves/{cid}/move`: reordena una curva intercambiándola con la vecina
/// (docente/admin). Si ya está en el extremo, no cambia nada.
async fn move_curve(
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
async fn create_intermediate(
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
async fn update_intermediate(
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
async fn delete_intermediate(
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
async fn create_point_result(
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
async fn update_point_result(
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
async fn delete_point_result(
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
async fn create_aggregate(
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
async fn update_aggregate(
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
async fn delete_aggregate(
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
async fn delete_curve(
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
async fn create_result(
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
async fn update_result(
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
async fn delete_result(
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
async fn set_result_tolerance(
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

/// Valida los campos de un instrumento: tipo en {analogico, digital} y textos no vacíos.
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

/// Lee un campo de texto de un formulario multipart, devolviendo error si no es texto válido.
async fn read_text(field: axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    field
        .text()
        .await
        .map_err(|_| AppError::bad_request("campo de texto invalido"))
}

/// Exige que un campo opcional esté presente y no vacío; si falta, devuelve 400 con su nombre.
fn required(value: Option<String>, name: &str) -> Result<String, AppError> {
    let value = value.ok_or_else(|| AppError::bad_request(format!("falta el campo {name}")))?;
    if value.trim().is_empty() {
        return Err(AppError::bad_request(format!("falta el campo {name}")));
    }
    Ok(value)
}

/// Exige que un campo `String` no sea vacío o solo espacios.
fn not_blank(value: &str, msg: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        Err(AppError::bad_request(msg))
    } else {
        Ok(())
    }
}

/// Valida la longitud mínima de una contraseña (8 caracteres); devuelve 400 si no cumple.
fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::bad_request(
            "la contrasena debe tener al menos 8 caracteres",
        ));
    }
    Ok(())
}

/// Validación mínima de email: tiene `@`, parte local no vacía y dominio con punto interno.
fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// Resuelve el usuario autenticado a partir de la cookie de sesión; error 401 si no hay sesión válida.
async fn current_user(state: &SharedState, headers: &HeaderMap) -> Result<db::AuthUser, AppError> {
    let token = session_token(headers).ok_or_else(|| AppError::unauthorized("login requerido"))?;
    db::user_by_session(&state.pool, &token)
        .await?
        .ok_or_else(|| AppError::unauthorized("sesion invalida o vencida"))
}

/// Igual que `current_user` pero exige rol docente o admin; error 403 en caso contrario.
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

/// Extrae el token de la cookie `quantify_session` de los headers, si está presente.
fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(name, value)| (name == "quantify_session").then(|| value.to_string()))
}

/// Construye el header `Set-Cookie` de sesión (HttpOnly, SameSite=Lax) con el token y su vigencia.
/// El flag `Secure` se agrega solo cuando `secure` es `true` (deploy con TLS).
fn session_cookie(token: &str, max_age_seconds: i64, secure: bool) -> HeaderValue {
    let secure_flag = if secure { "; Secure" } else { "" };
    let value = format!(
        "quantify_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}{secure_flag}"
    );
    HeaderValue::from_str(&value).expect("valid cookie header")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_hidden_only_for_students_without_visibility() {
        // Docente/admin: nunca se oculta, esté o no habilitado.
        assert!(!analysis_hidden_for("docente", false));
        assert!(!analysis_hidden_for("admin", false));
        assert!(!analysis_hidden_for("docente", true));
        // Estudiante: oculto hasta que el docente habilite.
        assert!(analysis_hidden_for("estudiante", false));
        assert!(!analysis_hidden_for("estudiante", true));
    }

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

    #[test]
    fn valid_group_table_count_range() {
        assert!(valid_group_table_count(1));
        assert!(valid_group_table_count(24));
        assert!(!valid_group_table_count(0));
        assert!(!valid_group_table_count(25));
    }

    #[test]
    fn is_valid_email_basic_cases() {
        assert!(is_valid_email("a@b.com"));
        assert!(!is_valid_email("ab.com"));
        assert!(!is_valid_email("a@bcom"));
        assert!(!is_valid_email("@b.com"));
    }

    #[test]
    fn validate_password_length_rule() {
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("corta").is_err());
    }

    #[test]
    fn required_checks_presence_and_blank() {
        assert!(required(Some("x".into()), "f").is_ok());
        assert!(required(None, "f").is_err());
        assert!(required(Some("   ".into()), "f").is_err());
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

    #[test]
    fn compute_csrf_is_deterministic() {
        let t1 = compute_csrf("token-abc", "secreto");
        let t2 = compute_csrf("token-abc", "secreto");
        assert_eq!(t1, t2);
    }

    #[test]
    fn compute_csrf_changes_with_different_secret() {
        let t1 = compute_csrf("token-abc", "secreto-a");
        let t2 = compute_csrf("token-abc", "secreto-b");
        assert_ne!(t1, t2);
    }

    #[test]
    fn compute_csrf_changes_with_different_session_token() {
        let t1 = compute_csrf("token-abc", "secreto");
        let t2 = compute_csrf("token-xyz", "secreto");
        assert_ne!(t1, t2);
    }

    #[test]
    fn compute_csrf_output_is_valid_hex() {
        let token = compute_csrf("cualquier-sesion", "clave-secreta");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        // SHA-256 produce 32 bytes = 64 caracteres hexadecimales.
        assert_eq!(token.len(), 64);
    }
}
