# Graph Report - C:/repos/Quantify  (2026-07-10)

## Corpus Check
- 72 files · ~106,930 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1256 nodes · 4569 edges · 27 communities (25 shown, 2 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 158 edges (avg confidence: 0.76)
- Token cost: 110,645 input · 0 output

## Community Hubs (Navigation)
- Practice Results & CRUD
- App Errors
- Computation Engine
- DB State & App Bootstrap
- Auth Routes & Middleware
- Frontend Account & App Shell
- Frontend Constants & Forms
- Instrument Catalog Backend
- Submissions & Grading Backend
- Courses & Groups Backend
- Domain Concepts (Uncertainty, Motors, Deploy)
- Frontend Gradebook & Groups UI
- Frontend API Client & Admin UI
- Frontend Analysis Rendering
- Frontend Academic/Courses UI
- Frontend Submissions & Invitations UI
- Uncertainty Engine (Rust)
- Frontend Instruments UI
- Regression/Analysis Engine (Rust)
- E2E Test Runner
- Frontend Chronometer
- E2E Visual Forms Test
- E2E Fluidos2 Smoke Test
- Package Config
- Dev Reset Script (sh)

## God Nodes (most connected - your core abstractions)
1. `AppError` - 107 edges
2. `escapeHtml()` - 103 edges
3. `require_teacher()` - 59 edges
4. `postJson()` - 50 edges
5. `fetchJson()` - 37 edges
6. `renderPracticesPage()` - 34 edges
7. `AuthUser` - 33 edges
8. `setup()` - 31 edges
9. `definition()` - 29 edges
10. `current_user()` - 29 edges

## Surprising Connections (you probably didn't know these)
- `student_grade_summary()` --calls--> `clean_zero()`  [INFERRED]
  src/submissions.rs → src/db.rs
- `04 — Roadmap por fases` --references--> `P3 — Relajación Exponencial, parte 1 (medida directa de τ)`  [EXTRACTED]
  docs/plan/04-roadmap-fases.md → README.md
- `Quantify README` --references--> `Deploy en Ubuntu local`  [EXTRACTED]
  README.md → deploy/ubuntu.md
- `Quantify README` --conceptually_related_to--> `static/index.html — SPA shell`  [INFERRED]
  README.md → static/index.html
- `Deploy de Quantify (LAN/producción)` --references--> `Hash de contraseña Argon2id (re-hash transparente de SHA-256 legacy)`  [EXTRACTED]
  docs/deploy.md → README.md

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Modelo de incertidumbre tipo B compartido entre prácticas y motor** — concept_type_b_uncertainty, concept_instrument_catalog, concept_uncertainty_engine, practice_fluidos_1, practice_viscosidad, practice_fluidos_2 [INFERRED 0.85]
- **Motores A-F extienden el modelo de práctica para sembrar las 6 prácticas nuevas** — concept_replicas_per_point_motor_a, concept_curve_list_motor_b, concept_intermediate_quantity_motor_c, concept_operators_motor_d, concept_shared_scalars_motor_e, concept_aggregates_motor_f, practice_fluidos_1, practice_fluidos_2, practice_viscosidad, practice_filtros, practice_p2_cc [EXTRACTED 1.00]
- **Capas de seguridad de despliegue multi-PC (Fase 13)** — concept_csrf_protection, concept_app_secret_key, concept_login_rate_limiting, concept_argon2_password_hash, docs_deploy [EXTRACTED 1.00]

## Communities (27 total, 2 thin omitted)

### Community 0 - "Practice Results & CRUD"
Cohesion: 0.06
Nodes (121): D, FromRow, Send, SqliteRow, result(), PracticeResult, aggregate_crud_roundtrip(), AggregateInput (+113 more)

### Community 1 - "App Errors"
Cohesion: 0.09
Nodes (94): From, Into, Self, AppError, ErrorBody, Error, IntoResponse, Response (+86 more)

### Community 2 - "Computation Engine"
Cohesion: 0.06
Nodes (96): HashSet, Item, Iterator, Node, PointContext, PointSeries, AggregateComputation, analyze() (+88 more)

### Community 3 - "DB State & App Bootstrap"
Cohesion: 0.06
Nodes (89): Arc, Mutex, PathBuf, add_column_if_missing(), AppState, AttemptInfo, clean_zero(), digest_password() (+81 more)

### Community 4 - "Auth Routes & Middleware"
Cohesion: 0.08
Nodes (74): Field, HeaderValue, Multipart, Next, Request, Router, change_password(), login() (+66 more)

### Community 5 - "Frontend Account & App Shell"
Cohesion: 0.05
Nodes (67): setCsrfToken(), init(), startApp(), accountDefaultGroup(), accountDefaultTable(), renderAccount(), renderDefaultGroupSelect(), renderDefaultTableSelect() (+59 more)

### Community 6 - "Frontend Constants & Forms"
Cohesion: 0.07
Nodes (71): PRACTICE_GROUPS, PRACTICE_PARTS, PRACTICE_SECTIONS, PRACTICES_WITHOUT_CHRONO_HELPER, SERIES_LIVE_COLUMNS, SYMBOL_FIRST_QUANTITIES, applyDraftPrefill(), applyFinalResultsPrefillFrom() (+63 more)

### Community 7 - "Instrument Catalog Backend"
Cohesion: 0.11
Nodes (63): Instrument, CatalogExport, create_and_list_instruments(), create_instrument(), create_scale(), CreateInstrument, delete_instrument(), delete_instrument_removes_its_scales() (+55 more)

### Community 8 - "Submissions & Grading Backend"
Cohesion: 0.11
Nodes (64): accept_report_invitation(), AcceptOutcome, add_report_member(), CourseGradebook, create_grade_component(), create_submission(), CreateGradeComponent, delete_submission() (+56 more)

### Community 9 - "Courses & Groups Backend"
Cohesion: 0.12
Nodes (62): academic_context(), academic_context_differs_by_role(), AcademicContext, add_and_remove_group_member(), add_course_member(), add_course_member_requires_student(), add_group_member(), AddGroupMember (+54 more)

### Community 10 - "Domain Concepts (Uncertainty, Motors, Deploy)"
Cohesion: 0.06
Nodes (62): Motor F — mensurandos agregados escalares (practice_aggregates), APP_SECRET_KEY — clave estable para derivar tokens CSRF, Hash de contraseña Argon2id (re-hash transparente de SHA-256 legacy), Backend Rust + Axum + SQLite (sqlx) + Tokio, Balance de energía y masa en descarga de recipiente (Fluidos II), Incertidumbre combinada u_c y expandida U=2·u_c, Fase de comparación — cálculo alumno vs automático, Protección CSRF (token HMAC-SHA256 por sesión, header X-CSRF-Token) (+54 more)

### Community 11 - "Frontend Gradebook & Groups UI"
Cohesion: 0.10
Nodes (47): groupDirectory, histogramSvg(), renderSeriesDebug(), renderGradeField(), renderKindTotals(), renderStudentGradeCard(), closeGroupWorkspace(), openGroupWorkspace() (+39 more)

### Community 12 - "Frontend API Client & Admin UI"
Cohesion: 0.12
Nodes (47): deleteJson(), errorText(), fetchJson(), postJson(), analysisKindLabel(), closePracticeWorkspace(), curvePayloadFromForm(), deletePracticeCurve() (+39 more)

### Community 13 - "Frontend Analysis Rendering"
Cohesion: 0.20
Nodes (34): aggregatesMarkup(), comparisonMarkup(), derivedBlockMarkup(), formAnalysisMarkup(), measuredVsTheoreticalMarkup(), measurementMetaMarkup(), membersEditorMarkup(), plotSvg() (+26 more)

### Community 14 - "Frontend Academic/Courses UI"
Cohesion: 0.13
Nodes (30): loadAcademic(), refreshAcademic(), renderAdmin(), withAdminError(), closeCourseWorkspace(), openCourseWorkspace(), renderCourseDirectory(), renderCourseGroupForm() (+22 more)

### Community 15 - "Frontend Submissions & Invitations UI"
Cohesion: 0.14
Nodes (27): invitationBanner, submissionList, submissionsListTitle, submissionsSubtitle, submissionsTitle, submissionWorkspace, checkExistingReport(), acceptInvitation() (+19 more)

### Community 16 - "Uncertainty Engine (Rust)"
Cohesion: 0.14
Nodes (17): F, BModel, combine(), expand(), measured_given(), measured_quantity(), measured_quantity_combines_a_and_b(), propagate() (+9 more)

### Community 17 - "Frontend Instruments UI"
Cohesion: 0.23
Nodes (22): closeInstrumentWorkspace(), deleteInstrument(), deleteScale(), exportInstruments(), importInstruments(), loadInstruments(), openInstrumentWorkspace(), refreshInstruments() (+14 more)

### Community 18 - "Regression/Analysis Engine (Rust)"
Cohesion: 0.29
Nodes (16): AnalysisResult, analyze_csv(), ColumnStats, computes_basic_stats_and_regression(), first_regression(), linear_regression(), LinearRegression, parse_record() (+8 more)

### Community 19 - "E2E Test Runner"
Cohesion: 0.28
Nodes (15): ARTIFACTS, assert(), buildServer(), loginAs(), main(), ROOT, startServer(), step() (+7 more)

### Community 21 - "E2E Visual Forms Test"
Cohesion: 0.29
Nodes (11): ARTIFACTS, assert(), assertInputContrast(), buildServer(), login(), main(), openPractice(), ROOT (+3 more)

### Community 22 - "E2E Fluidos2 Smoke Test"
Cohesion: 0.31
Nodes (10): ARTIFACTS, assert(), buildServer(), ROOT, run(), startServer(), step(), STUDENT (+2 more)

### Community 23 - "Package Config"
Cohesion: 0.20
Nodes (9): devDependencies, playwright, name, private, scripts, test, test:e2e, type (+1 more)

## Knowledge Gaps
- **30 isolated node(s):** `name`, `private`, `type`, `test`, `test:e2e` (+25 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **2 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AppError` connect `App Errors` to `Auth Routes & Middleware`, `Instrument Catalog Backend`?**
  _High betweenness centrality (0.079) - this node is a cross-community bridge._
- **Why does `AuthUser` connect `Courses & Groups Backend` to `App Errors`, `Computation Engine`, `DB State & App Bootstrap`, `Auth Routes & Middleware`, `Submissions & Grading Backend`?**
  _High betweenness centrality (0.056) - this node is a cross-community bridge._
- **Why does `require_teacher()` connect `App Errors` to `Courses & Groups Backend`, `Auth Routes & Middleware`, `Instrument Catalog Backend`?**
  _High betweenness centrality (0.036) - this node is a cross-community bridge._
- **Are the 52 inferred relationships involving `require_teacher()` (e.g. with `add_course_member()` and `add_group_member()`) actually correct?**
  _`require_teacher()` has 52 INFERRED edges - model-reasoned connections that need verification._
- **What connects `name`, `private`, `type` to the rest of the system?**
  _33 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Practice Results & CRUD` be split into smaller, more focused modules?**
  _Cohesion score 0.06451612903225806 - nodes in this community are weakly interconnected._
- **Should `App Errors` be split into smaller, more focused modules?**
  _Cohesion score 0.09427284427284427 - nodes in this community are weakly interconnected._