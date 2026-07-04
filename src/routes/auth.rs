//! Handlers de `/api/auth/*`: login, logout, sesión actual y perfil.

use super::{
    compute_csrf, current_user, is_valid_email, session_cookie, session_token, validate_password,
    Health, SharedState,
};
use crate::{db, error::AppError};
use axum::{
    extract::State,
    http::{header, HeaderMap},
    response::IntoResponse,
    Json,
};
use chrono::Utc;

/// `POST /api/auth/login`: valida credenciales y, si son correctas, setea la cookie de sesión.
/// Aplica rate-limiting: bloquea 15 minutos tras 5 intentos fallidos consecutivos por email.
pub(super) async fn login(
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
pub(super) async fn logout(
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
pub(super) async fn me(
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
pub(super) async fn change_password(
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
pub(super) async fn update_profile(
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
