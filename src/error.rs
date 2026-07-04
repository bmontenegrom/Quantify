use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Error de la aplicación: combina un código HTTP con un mensaje legible para el cliente.
#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
}

/// Cuerpo JSON de error que se serializa en la respuesta (`{"error": "..."}`).
#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

impl AppError {
    /// Crea un error 400 Bad Request con el mensaje dado.
    ///
    /// # Ejemplos
    ///
    /// ```
    /// use axum::response::IntoResponse;
    /// let resp = quantify::error::AppError::bad_request("dato invalido").into_response();
    /// assert_eq!(resp.status().as_u16(), 400);
    /// ```
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    /// Crea un error 404 Not Found con el mensaje dado.
    ///
    /// # Ejemplos
    ///
    /// ```
    /// use axum::response::IntoResponse;
    /// let resp = quantify::error::AppError::not_found("no existe").into_response();
    /// assert_eq!(resp.status().as_u16(), 404);
    /// ```
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    /// Crea un error 401 Unauthorized con el mensaje dado.
    ///
    /// # Ejemplos
    ///
    /// ```
    /// use axum::response::IntoResponse;
    /// let resp = quantify::error::AppError::unauthorized("inicia sesion").into_response();
    /// assert_eq!(resp.status().as_u16(), 401);
    /// ```
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    /// Crea un error 403 Forbidden con el mensaje dado.
    ///
    /// # Ejemplos
    ///
    /// ```
    /// use axum::response::IntoResponse;
    /// let resp = quantify::error::AppError::forbidden("sin permiso").into_response();
    /// assert_eq!(resp.status().as_u16(), 403);
    /// ```
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    /// Crea un error 429 Too Many Requests con el mensaje dado (rate-limiting).
    ///
    /// # Ejemplos
    ///
    /// ```
    /// use axum::response::IntoResponse;
    /// let resp = quantify::error::AppError::too_many_requests("frená un poco").into_response();
    /// assert_eq!(resp.status().as_u16(), 429);
    /// ```
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: message.into(),
        }
    }

    /// Mapea un error de dominio (`anyhow`) a la respuesta HTTP adecuada: los de base de datos van
    /// a un 500 genérico (sin filtrar detalle); el resto son errores de dominio (práctica/escala
    /// inexistente, fórmula inválida, etc.) y llevan su mensaje amigable como 400.
    pub fn from_domain_or_db(err: anyhow::Error) -> Self {
        if err.downcast_ref::<sqlx::Error>().is_some() {
            AppError::from(err)
        } else {
            AppError::bad_request(err.to_string())
        }
    }
}

impl IntoResponse for AppError {
    /// Convierte el error en una respuesta HTTP con su código y cuerpo JSON.
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: &self.message,
            }),
        )
            .into_response()
    }
}

/// Mensaje genérico para errores internos: claro para el usuario, sin filtrar detalle técnico
/// (el detalle real queda en el log del servidor).
const INTERNAL_MESSAGE: &str = "Ocurrio un error interno. Volve a intentar en un momento.";

impl From<sqlx::Error> for AppError {
    /// Mapea errores de base de datos a un 500 genérico, registrando el detalle real en el log.
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = ?err, "database error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: INTERNAL_MESSAGE.into(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    /// Mapea errores de aplicación (`anyhow`) a un 500 genérico, registrando el detalle en el log.
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = ?err, "application error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: INTERNAL_MESSAGE.into(),
        }
    }
}
