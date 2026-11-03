use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

impl AppError {
    /// Mensaje que ve el usuario.
    ///
    /// ```
    /// let err = quantify::error::AppError::bad_request("dato invalido");
    /// assert_eq!(err.message(), "dato invalido");
    /// ```
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Crea un error 400 Bad Request con el mensaje dado.
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

    /// Un error de dominio pasa como 400 con su mensaje; uno de base de datos, como 500 genérico.
    ///
    /// ```
    /// use axum::response::IntoResponse;
    /// use quantify::error::AppError;
    /// let domain = AppError::from_domain_or_db(anyhow::anyhow!("la practica no existe"));
    /// assert_eq!(domain.message(), "la practica no existe");
    /// let db = AppError::from_domain_or_db(sqlx::Error::RowNotFound.into());
    /// assert_eq!(db.into_response().status().as_u16(), 500);
    /// ```
    pub fn from_domain_or_db(err: anyhow::Error) -> Self {
        if err.downcast_ref::<sqlx::Error>().is_some() {
            AppError::from(err)
        } else {
            AppError::bad_request(err.to_string())
        }
    }
}

impl IntoResponse for AppError {
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

const INTERNAL_MESSAGE: &str = "Ocurrio un error interno. Volve a intentar en un momento.";

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = ?err, "database error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: INTERNAL_MESSAGE.into(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = ?err, "application error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: INTERNAL_MESSAGE.into(),
        }
    }
}
