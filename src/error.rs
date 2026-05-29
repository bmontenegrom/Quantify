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
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    /// Crea un error 404 Not Found con el mensaje dado.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    /// Crea un error 401 Unauthorized con el mensaje dado.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    /// Crea un error 403 Forbidden con el mensaje dado.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
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

impl From<sqlx::Error> for AppError {
    /// Mapea errores de base de datos a un 500 genérico, registrando el detalle real en el log.
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = ?err, "database error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "database error".into(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    /// Mapea errores de aplicación (`anyhow`) a un 500 genérico, registrando el detalle en el log.
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = ?err, "application error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "application error".into(),
        }
    }
}
