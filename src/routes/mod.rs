use crate::{
    db::{self, AppState},
    error::AppError,
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::sync::Arc;
use subtle::ConstantTimeEq;

type SharedState = Arc<AppState>;

mod auth;
mod courses;
mod instruments;
mod practice_admin;
mod submissions;
use auth::*;
use courses::*;
use instruments::*;
use practice_admin::*;
use submissions::*;

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

/// `GET /api/practices`: catálogo completo de prácticas (requiere sesión válida).
async fn list_practices(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<db::Practice>>, AppError> {
    current_user(&state, &headers).await?;
    Ok(Json(db::practices(&state.pool).await?))
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
