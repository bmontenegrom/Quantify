use super::*;
use axum::body::Body;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

pub(crate) fn test_state(pool: sqlx::SqlitePool, dir: &TempDir) -> SharedState {
    Arc::new(AppState {
        pool,
        upload_dir: dir.path().to_path_buf(),
        secret_key: "secreto".into(),
        secure_cookies: false,
        login_attempts: Default::default(),
    })
}

const TEACHER: (&str, &str) = ("docente@quantify.local", "docente123");
const STUDENT: (&str, &str) = ("estudiante@quantify.local", "estudiante123");
const ADMIN: (&str, &str) = ("admin@quantify.local", "admin123");

struct Session {
    cookie: String,
    csrf: String,
}

struct TestApp {
    app: Router,
    _dir: TempDir,
}

impl TestApp {
    async fn new() -> Self {
        let (pool, dir) = crate::computation::tests::setup().await;
        crate::computation::tests::p1_partes(&pool).await;
        let app = api_router(test_state(pool, &dir));
        TestApp { app, _dir: dir }
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        session: Option<&Session>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = axum::http::Request::builder().method(method).uri(uri);
        if let Some(session) = session {
            builder = builder
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf);
        }
        let body = match body {
            Some(value) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let response = self
            .app
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes)
                    .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
            },
        )
    }

    async fn get(&self, uri: &str, session: Option<&Session>) -> (StatusCode, Value) {
        self.send(Method::GET, uri, session, None).await
    }

    async fn post(&self, uri: &str, session: &Session, body: Value) -> (StatusCode, Value) {
        self.send(Method::POST, uri, Some(session), Some(body))
            .await
    }

    async fn delete(&self, uri: &str, session: &Session) -> (StatusCode, Value) {
        self.send(Method::DELETE, uri, Some(session), None).await
    }

    async fn try_login(
        &self,
        (email, password): (&str, &str),
    ) -> (StatusCode, Option<String>, Value) {
        let request = axum::http::Request::post("/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "email": email, "password": password }).to_string(),
            ))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(str::to_string);
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            cookie,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn login(&self, credentials: (&str, &str)) -> Session {
        let (status, cookie, body) = self.try_login(credentials).await;
        assert_eq!(status, StatusCode::OK, "login de {}", credentials.0);
        Session {
            cookie: cookie.unwrap(),
            csrf: body["csrf_token"].as_str().unwrap().to_string(),
        }
    }

    async fn user_id(&self, teacher: &Session, email: &str) -> String {
        let (_, users) = self.get("/users", Some(teacher)).await;
        users
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["email"] == email)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn new_student(&self, teacher: &Session, email: &str) -> String {
        let (status, user) = self
            .post(
                "/users",
                teacher,
                json!({
                    "email": email,
                    "display_name": email,
                    "role": "estudiante",
                    "password": "clave-segura",
                }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{user}");
        user["id"].as_str().unwrap().to_string()
    }

    async fn enroll(&self, teacher: &Session, course_id: &str, group_id: &str, user_id: &str) {
        let body = json!({ "user_id": user_id });
        let (status, _) = self
            .post(
                &format!("/academic/courses/{course_id}/members"),
                teacher,
                body.clone(),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = self
            .post(
                &format!("/academic/groups/{group_id}/members"),
                teacher,
                body,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn course_with_student(&self, teacher: &Session) -> (String, String) {
        let (status, course) = self
            .post(
                "/academic/courses",
                teacher,
                json!({ "name": "Física", "term": "2026" }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{course}");
        let course_id = course["id"].as_str().unwrap().to_string();
        let (status, group) = self
            .post(
                &format!("/academic/courses/{course_id}/groups"),
                teacher,
                json!({ "name": "Grupo 1", "table_count": 4 }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{group}");
        let group_id = group["id"].as_str().unwrap().to_string();
        let student = self.user_id(teacher, STUDENT.0).await;
        self.enroll(teacher, &course_id, &group_id, &student).await;
        let (status, _) = self
            .post(
                &format!("/academic/courses/{course_id}/practices"),
                teacher,
                json!({ "practice_id": "p1-partes" }),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        (course_id, group_id)
    }

    async fn form_submission(
        &self,
        student: &Session,
        course_id: &str,
        group_id: &str,
        table_number: i64,
    ) -> (StatusCode, Value) {
        let (status, table) = self
            .post(
                &format!("/academic/groups/{group_id}/practice-table"),
                student,
                json!({ "practice_id": "p1-partes", "table_number": table_number }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{table}");
        self.submit_form(student, course_id, group_id, table_number)
            .await
    }

    async fn submit_form(
        &self,
        student: &Session,
        course_id: &str,
        group_id: &str,
        table_number: i64,
    ) -> (StatusCode, Value) {
        let (_, definition) = self
            .get("/practices/p1-partes/definition", Some(student))
            .await;
        let t1 = definition["quantities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["symbol"] == "T1")
            .unwrap()["id"]
            .clone();
        self.post(
            "/submissions/form",
            student,
            json!({
                "course_id": course_id,
                "group_id": group_id,
                "practice_id": "p1-partes",
                "measurements": [{ "quantity_id": t1, "values": [5.0, 5.2, 4.9] }],
                "table_number": table_number,
                "student_results": [],
            }),
        )
        .await
    }
}

#[tokio::test]
async fn login_sets_a_session_and_me_returns_the_csrf_token() {
    let app = TestApp::new().await;
    let session = app.login(TEACHER).await;
    let (status, me) = app.get("/auth/me", Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["email"], TEACHER.0);
    assert_eq!(me["csrf_token"].as_str().unwrap().len(), 64);
    let (status, _) = app.get("/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn repeated_wrong_passwords_block_the_email() {
    let app = TestApp::new().await;
    for _ in 0..db::MAX_LOGIN_ATTEMPTS {
        let (status, cookie, _) = app.try_login((STUDENT.0, "incorrecta")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(cookie.is_none());
    }
    let (status, _, _) = app.try_login(STUDENT).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = app.try_login(TEACHER).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn logout_ends_the_session() {
    let app = TestApp::new().await;
    let session = app.login(STUDENT).await;
    let (status, _) = app
        .send(Method::POST, "/auth/logout", Some(&session), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.get("/auth/me", Some(&session)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_user_changes_the_profile_and_the_password() {
    let app = TestApp::new().await;
    let session = app.login(STUDENT).await;
    let (status, _) = app
        .post(
            "/auth/profile",
            &session,
            json!({ "display_name": "Ana", "email": "no-es-email" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, me) = app
        .post(
            "/auth/profile",
            &session,
            json!({ "display_name": "Ana", "email": STUDENT.0 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["display_name"], "Ana");

    let change =
        |current: &str, new: &str| json!({ "current_password": current, "new_password": new });
    let (status, _) = app
        .post(
            "/auth/password",
            &session,
            change("incorrecta", "nueva-clave"),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .post("/auth/password", &session, change(STUDENT.1, "corta"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post("/auth/password", &session, change(STUDENT.1, "nueva-clave"))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = app.try_login(STUDENT).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.login((STUDENT.0, "nueva-clave")).await;
}

#[tokio::test]
async fn writes_without_a_valid_csrf_token_are_rejected() {
    let app = TestApp::new().await;
    let mut session = app.login(TEACHER).await;
    let body = json!({ "name": "X", "term": "2026" });
    let without_header = axum::http::Request::post("/academic/courses")
        .header(header::COOKIE, &session.cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.app.clone().oneshot(without_header).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (status, _) = app
        .send(Method::POST, "/academic/courses", None, Some(body.clone()))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    session.csrf = "falso".into();
    let (status, _) = app.post("/academic/courses", &session, body).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn students_and_anonymous_users_cannot_manage_courses_or_users() {
    let app = TestApp::new().await;
    let student = app.login(STUDENT).await;
    let (status, _) = app
        .post(
            "/academic/courses",
            &student,
            json!({ "name": "X", "term": "2026" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.get("/users", Some(&student)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.get("/users", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app.get("/academic/context", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_teacher_manages_courses_groups_and_members() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (status, _) = app
        .post(
            "/academic/courses",
            &teacher,
            json!({ "name": " ", "term": "2026" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (course_id, group_id) = app.course_with_student(&teacher).await;

    let (status, course) = app
        .post(
            &format!("/academic/courses/{course_id}"),
            &teacher,
            json!({ "name": "Física I", "term": "2026", "submission_edit_hours": 48 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(course["name"], "Física I");
    let (status, _) = app
        .post(
            "/academic/courses/no-existe",
            &teacher,
            json!({ "name": "X", "term": "2026" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = app
        .post(
            &format!("/academic/courses/{course_id}/groups"),
            &teacher,
            json!({ "name": "Grupo 2", "table_count": 30 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, group) = app
        .post(
            &format!("/academic/groups/{group_id}"),
            &teacher,
            json!({ "name": "Grupo A", "table_count": 6, "group_type": "practico" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{group}");
    assert_eq!(group["name"], "Grupo A");
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}"),
            &teacher,
            json!({ "name": "Grupo A", "table_count": 0, "group_type": "practico" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, context) = app.get("/academic/context", Some(&teacher)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(context.to_string().contains(&course_id));
    let student = app.login(STUDENT).await;
    let (status, context) = app.get("/academic/context", Some(&student)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(context.to_string().contains(&course_id));

    let (status, table) = app
        .post(
            &format!("/academic/groups/{group_id}/practice-table"),
            &student,
            json!({ "practice_id": "p1-partes", "table_number": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{table}");
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}/practice-table"),
            &student,
            json!({ "practice_id": "p1-partes", "table_number": 99 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, subgroup) = app
        .post(
            &format!("/academic/courses/{course_id}/subgroups"),
            &teacher,
            json!({ "practice_id": "p1-partes", "group_id": group_id, "name": "Mesa 1" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{subgroup}");
    let (status, _) = app
        .post(
            &format!("/academic/courses/{course_id}/subgroups"),
            &teacher,
            json!({ "practice_id": "no-existe", "group_id": group_id, "name": "Mesa 1" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let student_id = app.user_id(&teacher, STUDENT.0).await;
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}/members/remove"),
            &teacher,
            json!({ "user_id": student_id }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}/members/remove"),
            &teacher,
            json!({ "user_id": student_id }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.enroll(&teacher, &course_id, &group_id, &student_id)
        .await;
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}/members/{student_id}/remove"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}/members/{student_id}/remove"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .post(
            &format!("/academic/groups/{group_id}/members"),
            &teacher,
            json!({ "user_id": "no-existe" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let practice = |id: &str| json!({ "practice_id": id });
    let (status, _) = app
        .post(
            &format!("/academic/courses/{course_id}/practices/remove"),
            &teacher,
            practice("p1-partes"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .post(
            &format!("/academic/courses/{course_id}/practices/remove"),
            &teacher,
            practice("p1-partes"),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .post(
            &format!("/academic/courses/{course_id}/practices"),
            &teacher,
            practice("no-existe"),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_teacher_manages_users() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (status, _) = app
        .post(
            "/users",
            &teacher,
            json!({ "email": "mal", "display_name": "X", "role": "estudiante", "password": "clave-segura" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post(
            "/users",
            &teacher,
            json!({ "email": "x@quantify.local", "display_name": "X", "role": "estudiante", "password": "corta" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let id = app.new_student(&teacher, "nuevo@quantify.local").await;
    app.login(("nuevo@quantify.local", "clave-segura")).await;

    let (status, updated) = app
        .post(
            &format!("/users/{id}"),
            &teacher,
            json!({ "email": "nuevo@quantify.local", "display_name": "Nuevo", "role": "docente" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["role"], "docente");
    let (status, _) = app
        .post(
            &format!("/users/{id}"),
            &teacher,
            json!({ "email": "nuevo@quantify.local", "display_name": "Nuevo", "role": "rey" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post(
            "/users/no-existe",
            &teacher,
            json!({ "email": "z@quantify.local", "display_name": "Z", "role": "estudiante" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let admin = app.user_id(&teacher, ADMIN.0).await;
    let (status, _) = app
        .post(
            &format!("/users/{admin}"),
            &teacher,
            json!({ "email": ADMIN.0, "display_name": "Admin", "role": "docente" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = app
        .post(
            &format!("/users/{id}/password"),
            &teacher,
            json!({ "password": "otra-clave-segura" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    app.login(("nuevo@quantify.local", "otra-clave-segura"))
        .await;
    let (status, _) = app
        .post(
            "/users/no-existe/password",
            &teacher,
            json!({ "password": "otra-clave-segura" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_teacher_grades_and_the_gradebook_shows_it() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, _) = app.course_with_student(&teacher).await;
    let component = |kind: &str, max: f64| json!({ "course_id": course_id, "kind": kind, "name": "Informe 1", "max_points": max, "weight_points": 10.0 });
    let (status, _) = app
        .post("/grades/components", &teacher, component("otro", 10.0))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, created) = app
        .post("/grades/components", &teacher, component("informe", 10.0))
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let component_id = created["id"].as_str().unwrap();
    let student_id = app.user_id(&teacher, STUDENT.0).await;
    let score = |points: f64| json!({ "component_id": component_id, "student_id": student_id, "raw_points": points });
    let (status, _) = app.post("/grades/scores", &teacher, score(-1.0)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app.post("/grades/scores", &teacher, score(7.5)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .post(
            "/grades/scores",
            &teacher,
            json!({ "component_id": "no-existe", "student_id": student_id, "raw_points": 1.0 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let student = app.login(STUDENT).await;
    let (status, grades) = app.get("/grades", Some(&student)).await;
    assert_eq!(status, StatusCode::OK);
    let score = &grades[0]["students"][0]["scores"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["component_id"] == component_id)
        .unwrap()["raw_points"];
    assert_eq!(score.as_f64(), Some(7.5), "{grades}");
}

#[tokio::test]
async fn a_teacher_manages_instruments_and_scales() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, _) = app.course_with_student(&teacher).await;
    let (_, other) = app
        .post(
            "/academic/courses",
            &teacher,
            json!({ "name": "Otro", "term": "2026" }),
        )
        .await;
    let other_id = other["id"].as_str().unwrap();

    let instrument = |kind: &str| json!({ "course_id": course_id, "name": "Regla", "kind": kind, "quantity": "longitud", "unit": "mm" });
    let (status, _) = app.post("/instruments", &teacher, instrument("otro")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, created) = app
        .post("/instruments", &teacher, instrument("analogico"))
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    let (status, _) = app
        .post(
            &format!("/instruments/{id}"),
            &teacher,
            json!({ "name": "Regla 30 cm", "kind": "analogico", "quantity": "longitud", "unit": "mm" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let scale =
        |step: f64| json!({ "label": "1 mm", "step": step, "b_model": "resolucion", "unit": "mm" });
    let (status, _) = app
        .post(&format!("/instruments/{id}/scales"), &teacher, scale(0.0))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, created_scale) = app
        .post(&format!("/instruments/{id}/scales"), &teacher, scale(1.0))
        .await;
    assert_eq!(status, StatusCode::OK, "{created_scale}");
    let scale_id = created_scale["id"].as_str().unwrap().to_string();
    let (status, _) = app
        .post(
            &format!("/instruments/{id}/scales/{scale_id}"),
            &teacher,
            scale(0.5),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let student = app.login(STUDENT).await;
    let (status, listed) = app
        .get(
            &format!("/instruments?course_id={course_id}"),
            Some(&student),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    let (status, _) = app
        .get(
            &format!("/instruments?course_id={other_id}"),
            Some(&student),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post("/instruments", &student, instrument("analogico"))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, exported) = app
        .get(
            &format!("/instruments/export?course_id={course_id}"),
            Some(&teacher),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .post(
            "/instruments/import",
            &teacher,
            json!({ "course_id": other_id, "instruments": exported["instruments"] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, imported) = app
        .get(
            &format!("/instruments?course_id={other_id}"),
            Some(&teacher),
        )
        .await;
    assert_eq!(imported.as_array().unwrap().len(), 1);
    let (status, _) = app
        .post(
            "/instruments/import",
            &teacher,
            json!({ "course_id": " ", "instruments": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, magnitudes) = app.get("/magnitudes", Some(&teacher)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!magnitudes.as_array().unwrap().is_empty());

    let scale_uri = format!("/instruments/{id}/scales/{scale_id}");
    assert_eq!(app.delete(&scale_uri, &teacher).await.0, StatusCode::OK);
    assert_eq!(
        app.delete(&scale_uri, &teacher).await.0,
        StatusCode::NOT_FOUND
    );
    let instrument_uri = format!("/instruments/{id}");
    assert_eq!(
        app.delete(&instrument_uri, &teacher).await.0,
        StatusCode::OK
    );
    assert_eq!(
        app.delete(&instrument_uri, &teacher).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_student_submits_a_form_and_only_members_see_it() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, group_id) = app.course_with_student(&teacher).await;
    let student = app.login(STUDENT).await;
    let (status, created) = app
        .form_submission(&student, &course_id, &group_id, 1)
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["id"].as_str().unwrap().to_string();

    for session in [&student, &teacher] {
        let (status, list) = app.get("/submissions", Some(session)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(list.to_string().contains(&id));
    }
    let (status, detail) = app.get(&format!("/submissions/{id}"), Some(&student)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(detail["analysis"].is_null());
    let (status, detail) = app.get(&format!("/submissions/{id}"), Some(&teacher)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!detail["analysis"].is_null());
    let (status, _) = app
        .get(&format!("/submissions/{id}/definition"), Some(&student))
        .await;
    assert_eq!(status, StatusCode::OK);

    app.new_student(&teacher, "otro@quantify.local").await;
    let other = app.login(("otro@quantify.local", "clave-segura")).await;
    let (status, _) = app.get(&format!("/submissions/{id}"), Some(&other)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.submit_form(&other, &course_id, &group_id, 1).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.get("/submissions/no-existe", Some(&teacher)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_student_edits_and_cancels_a_submission() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, group_id) = app.course_with_student(&teacher).await;
    let student = app.login(STUDENT).await;
    let (_, created) = app
        .form_submission(&student, &course_id, &group_id, 1)
        .await;
    let id = created["id"].as_str().unwrap().to_string();
    let measurements = created["measurements"].clone();
    let (status, edited) = app
        .post(
            &format!("/submissions/{id}/edit"),
            &student,
            json!({ "measurements": measurements_input(&measurements), "student_comment": "  revisado  " }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["student_comment"], "revisado");

    let uri = format!("/submissions/{id}");
    assert_eq!(app.delete(&uri, &student).await.0, StatusCode::OK);
    assert_eq!(app.get(&uri, Some(&teacher)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.delete(&uri, &student).await.0, StatusCode::NOT_FOUND);
}

fn measurements_input(measurements: &Value) -> Value {
    let ids: std::collections::BTreeSet<String> = measurements
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["quantity_id"].as_str().unwrap().to_string())
        .collect();
    ids.into_iter()
        .map(|id| json!({ "quantity_id": id, "values": [5.1, 5.0, 4.8] }))
        .collect()
}

#[tokio::test]
async fn a_reviewed_submission_is_locked_for_the_student() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, group_id) = app.course_with_student(&teacher).await;
    let student = app.login(STUDENT).await;
    let (_, created) = app
        .form_submission(&student, &course_id, &group_id, 1)
        .await;
    let id = created["id"].as_str().unwrap().to_string();

    let (status, _) = app
        .post(
            &format!("/submissions/{id}/student-results"),
            &student,
            json!({ "results": [{ "symbol": "no-existe", "value": 1.0 }] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/student-results"),
            &student,
            json!({ "results": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let review = |status: &str| json!({ "status": status, "score": 9.0, "teacher_comment": "bien", "results_visible": true });
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/review"),
            &teacher,
            review("rara"),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/review"),
            &student,
            review("aprobada"),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, reviewed) = app
        .post(
            &format!("/submissions/{id}/review"),
            &teacher,
            review("aprobada"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reviewed}");
    assert_eq!(reviewed["status"], "aprobada");
    let (status, _) = app
        .post(
            "/submissions/no-existe/review",
            &teacher,
            review("aprobada"),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, detail) = app.get(&format!("/submissions/{id}"), Some(&student)).await;
    assert!(!detail["analysis"].is_null());
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/student-results"),
            &student,
            json!({ "results": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/student-results"),
            &teacher,
            json!({ "results": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/edit"),
            &student,
            json!({ "measurements": measurements_input(&created["measurements"]) }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        app.delete(&format!("/submissions/{id}"), &student).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn report_members_are_invited_accepted_declined_and_removed() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, group_id) = app.course_with_student(&teacher).await;
    let partner_id = app.new_student(&teacher, "companero@quantify.local").await;
    let third_id = app.new_student(&teacher, "tercero@quantify.local").await;
    app.enroll(&teacher, &course_id, &group_id, &partner_id)
        .await;
    let student = app.login(STUDENT).await;
    let partner = app
        .login(("companero@quantify.local", "clave-segura"))
        .await;
    let third = app.login(("tercero@quantify.local", "clave-segura")).await;
    let (_, created) = app
        .form_submission(&student, &course_id, &group_id, 2)
        .await;
    let id = created["id"].as_str().unwrap().to_string();
    let members_uri = format!("/submissions/{id}/members");

    let invite = |user_id: &str| json!({ "user_id": user_id });
    let (status, members) = app.post(&members_uri, &teacher, invite(&partner_id)).await;
    assert_eq!(status, StatusCode::OK, "{members}");
    let (status, _) = app.post(&members_uri, &teacher, invite("no-existe")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.post(&members_uri, &student, invite(&third_id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, invitations) = app.get("/submissions/invitations", Some(&partner)).await;
    assert!(invitations.to_string().contains(&id));
    let existing =
        format!("/submissions/existing?practice_id=p1-partes&group_id={group_id}&table_number=2");
    let (status, report) = app.get(&existing, Some(&partner)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["submission_id"], id.as_str());
    assert_eq!(report["can_accept"], true);
    let (_, report) = app.get(&existing, Some(&student)).await;
    assert_eq!(report["is_owner"], true);
    let (status, none) = app
        .get(
            &format!(
                "/submissions/existing?practice_id=p1-partes&group_id={group_id}&table_number=3"
            ),
            Some(&student),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(none.is_null());

    let (status, _) = app
        .post(&format!("/submissions/{id}/accept"), &third, json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post(&format!("/submissions/{id}/accept"), &partner, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.get(&format!("/submissions/{id}"), Some(&partner)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app.post(&members_uri, &teacher, invite(&third_id)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, invitations) = app.get("/submissions/invitations", Some(&third)).await;
    assert!(invitations.to_string().contains(&id));
    let (status, _) = app
        .post(&format!("/submissions/{id}/decline"), &third, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, invitations) = app.get("/submissions/invitations", Some(&third)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!invitations.to_string().contains(&id));

    let (status, members) = app.get(&members_uri, Some(&teacher)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(members.to_string().contains(&partner_id));
    let remove_uri = format!("/submissions/{id}/members/remove");
    let (status, members) = app.post(&remove_uri, &teacher, invite(&partner_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!members.to_string().contains(&partner_id));
    let (status, _) = app.post(&remove_uri, &teacher, invite(&partner_id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_teacher_moves_a_report_to_another_table() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, group_id) = app.course_with_student(&teacher).await;
    let student = app.login(STUDENT).await;
    let (_, created) = app
        .form_submission(&student, &course_id, &group_id, 1)
        .await;
    let id = created["id"].as_str().unwrap().to_string();
    let (status, updated) = app
        .post(
            &format!("/submissions/{id}/report"),
            &teacher,
            json!({ "table_number": 3 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["table_number"], 3);
    let (status, _) = app
        .post(
            &format!("/submissions/{id}/report"),
            &student,
            json!({ "table_number": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let partner_id = app.new_student(&teacher, "companero@quantify.local").await;
    app.enroll(&teacher, &course_id, &group_id, &partner_id)
        .await;
    let partner = app
        .login(("companero@quantify.local", "clave-segura"))
        .await;
    let (status, other) = app
        .form_submission(&partner, &course_id, &group_id, 2)
        .await;
    assert_eq!(status, StatusCode::OK, "{other}");
    let (status, error) = app
        .post(
            &format!("/submissions/{id}/report"),
            &teacher,
            json!({ "table_number": 2 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        error.to_string().contains("Ya existe un informe"),
        "{error}"
    );
    let (status, _) = app
        .post(
            "/submissions/no-existe/report",
            &teacher,
            json!({ "table_number": 3 }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_student_uploads_a_csv_submission() {
    let app = TestApp::new().await;
    let teacher = app.login(TEACHER).await;
    let (course_id, group_id) = app.course_with_student(&teacher).await;
    let student = app.login(STUDENT).await;
    let multipart = |csv: &str| {
        let boundary = "quantify-boundary";
        let fields = [
            ("course_id", course_id.as_str()),
            ("group_id", group_id.as_str()),
            ("practice_id", "p1-partes"),
            ("csv_text", csv),
        ];
        let body: String = fields
            .iter()
            .map(|(name, value)| {
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
            })
            .chain(std::iter::once(format!("--{boundary}--\r\n")))
            .collect();
        axum::http::Request::post("/submissions")
            .header(header::COOKIE, &student.cookie)
            .header("x-csrf-token", &student.csrf)
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap()
    };
    let response = app
        .app
        .clone()
        .oneshot(multipart("T1\n5.0\n5.2\n4.9"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app.app.clone().oneshot(multipart("")).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
    assert_eq!(token.len(), 64);
}
