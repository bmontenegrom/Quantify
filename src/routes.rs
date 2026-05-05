use crate::{
    analysis,
    db::{self, AppState, NewSubmission, ReviewSubmission},
    error::AppError,
};
use axum::{
    extract::{Multipart, Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

type SharedState = Arc<AppState>;

pub fn api_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
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

async fn practices(State(state): State<SharedState>) -> Result<Json<Vec<db::Practice>>, AppError> {
    Ok(Json(db::practices(&state.pool).await?))
}

async fn submissions(
    State(state): State<SharedState>,
) -> Result<Json<Vec<db::SubmissionListItem>>, AppError> {
    Ok(Json(db::submission_list(&state.pool).await?))
}

async fn submission_detail(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let submission = db::submission_detail(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::not_found("submission not found"))?;
    Ok(Json(submission))
}

async fn create_submission(
    State(state): State<SharedState>,
    mut multipart: Multipart,
) -> Result<Json<db::SubmissionDetail>, AppError> {
    let mut student_name = None;
    let mut group_name = None;
    let mut course = None;
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
            "student_name" => student_name = Some(read_text(field).await?),
            "group_name" => group_name = Some(read_text(field).await?),
            "course" => course = Some(read_text(field).await?),
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

    let submission = NewSubmission {
        student_name: required(student_name, "student_name")?,
        group_name: required(group_name, "group_name")?,
        course: required(course, "course")?,
        practice_id: required(practice_id, "practice_id")?,
        file_name: file_name.unwrap_or_else(|| "medidas.csv".into()),
        csv_content,
        analysis,
    };

    let created = db::create_submission(&state.pool, &state.upload_dir, submission).await?;
    Ok(Json(created))
}

async fn review_submission(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(review): Json<ReviewSubmission>,
) -> Result<Json<db::SubmissionDetail>, AppError> {
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
