# 01 — Estado actual del repo

Snapshot del MVP tal como está en `main` al 29-05-2026.

## Stack

- **Backend**: Rust + Axum 0.8, SQLite vía `sqlx` 0.8, Tokio.
- **Frontend**: HTML/CSS/JS estático (`static/`) servido por el backend con `ServeDir`.
- **Auth**: sesiones por cookie `quantify_session` (12 h), hash de contraseña SHA-256 con salt.
- **Deploy**: binario Rust o Docker Compose (`Dockerfile`, `docker-compose.yml`, `deploy/ubuntu.md`).

## Estructura de código (`src/`)

| Archivo | Responsabilidad |
|---------|-----------------|
| `main.rs` | Bootstrap: env vars, pool SQLite, `migrate`, `seed_*`, router, `ServeDir`. |
| `db.rs` | **Toda la persistencia y lógica de dominio** (≈2100 líneas): tipos, migraciones, seeds, queries. |
| `routes.rs` | Handlers HTTP de la API (`/api/*`), auth helpers, validaciones. |
| `analysis.rs` | Análisis CSV genérico: stats por columna + regresión lineal. |
| `error.rs` | Tipo `AppError` → respuestas HTTP. |

> Nota: `db.rs` concentra demasiada responsabilidad. El plan propone empezar a
> separar módulos (`instruments`, `uncertainty`, `practices`) sin reescribir lo existente.

## Modelo de datos actual (tablas)

- `users` (role: estudiante/docente/admin, login por email único), `sessions`.
- `courses`, `lab_groups` (`table_count` = nº de mesas, `group_type` regular/recuperación).
- `course_members`, `group_members`, `course_practices`.
- `practices` (catálogo), `practice_subgroups`, `practice_subgroup_members`.
- `practice_table_assignments` (mesa asignada a un estudiante por práctica/grupo).
- `submissions` (CSV + `analysis_json` serializado + estado/comentario/nota docente).
- `grade_components` (pregunta/informe/parcial, `max_points`/`weight_points`), `grade_scores`.

## API actual (`/api`)

- Auth: `POST /auth/login`, `/auth/logout`, `GET /auth/me`, `POST /auth/profile`, `/auth/password`.
- Usuarios: `GET/POST /users`, `POST /users/{id}`, `/users/{id}/password`.
- Académico: `GET /academic/context`, CRUD de cursos/grupos/subgrupos/miembros/prácticas/mesas.
- Notas: `GET /grades`, `POST /grades/components`, `/grades/scores`.
- Prácticas: `GET /practices`.
- Entregas: `GET/POST /submissions`, `GET /submissions/{id}`, `POST /submissions/{id}/review`.

## Análisis CSV (`analysis.rs`) — lo que hace hoy

Dado un CSV con encabezados, por cada columna numérica calcula `count`, `mean`,
`std_dev` (poblacional, divide por `n`), `min`, `max`; arma una **regresión lineal**
entre las dos primeras columnas numéricas (pendiente, intercepto, R²) y junta
**advertencias** por celdas vacías o no numéricas.

### Limitaciones frente a Física 103

1. **No calcula incertidumbres** (ni tipo A `sₘ = s/√n`, ni tipo B desde instrumento, ni combinada/expandida).
2. `std_dev` es **poblacional** (`/n`); para tipo A se necesita la **muestral** (`/(n-1)`) y luego `sₘ = s/√n`. ⚠️ CONFIRMAR convención del curso.
3. No conoce **instrumentos** ni **tipos de instrumento** → no puede aportar la componente tipo B.
4. No modela **determinaciones indirectas** (`Q = f(a,b,c)`) ni **propagación de varianzas**.
5. No soporta el ajuste de **relajación exponencial** (P3) ni el manejo de escalas/resistencia interna de amperímetro/voltímetro (P2).
6. Las **prácticas sembradas son ficticias** (péndulo, Hooke, caída libre), no las de Física 103.

Estas limitaciones son exactamente el objetivo de la próxima iteración.
