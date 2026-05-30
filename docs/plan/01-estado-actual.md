# 01 — Estado actual del repo

Snapshot del MVP tal como está en `main` al 29-05-2026 (post Fases 0–2.5).

## Stack

- **Backend**: Rust + Axum 0.8, SQLite vía `sqlx` 0.8, Tokio.
- **Frontend**: HTML/CSS/JS estático (`static/`) servido por el backend con `ServeDir`.
  Lógica pura/selectores extraída a `static/lib.js` (ES module) para poder testearla.
- **Auth**: sesiones por cookie `quantify_session` (12 h), hash de contraseña SHA-256 con salt.
- **Tests**: backend con `cargo test`; frontend con `node --test` (`node:test`, sin
  dependencias) sobre `static/lib.js`. CI en `.github/workflows/ci.yml` corre ambas suites.
- **Deploy**: binario Rust o Docker Compose (`Dockerfile`, `docker-compose.yml`, `deploy/ubuntu.md`).

## Estructura de código (`src/`)

| Archivo | Responsabilidad |
|---------|-----------------|
| `main.rs` | Bootstrap: env vars, pool SQLite, `migrate`, `seed_*`, router, `ServeDir`. |
| `db.rs` | **Persistencia y lógica de dominio** (≈2100 líneas): tipos, migraciones, seeds, queries. |
| `routes.rs` | Handlers HTTP de la API (`/api/*`), auth helpers, validaciones. |
| `instruments.rs` | Catálogo de instrumentos por curso: CRUD de instrumentos/escalas, export/import, seed. |
| `uncertainty.rs` | Motor de incertidumbres: `type_a`, `type_b` (resolución/apreciación/fabricante), `combine`, `expand`, `propagate`. |
| `analysis.rs` | Análisis CSV: stats por columna + regresión lineal (usa `uncertainty`). |
| `error.rs` | Tipo `AppError` → respuestas HTTP. |

> Nota: `db.rs` todavía concentra mucha responsabilidad. Ya se extrajeron `instruments`
> y `uncertainty`; `practices` queda pendiente (Fase 3).

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
- Prácticas: `GET /practices` (P1/P2/P3 reales, ver doc 02).
- Instrumentos (docente/admin): `GET/POST /instruments`, `POST/DELETE /instruments/{id}`,
  escalas `/instruments/{id}/scales[/{scale_id}]`, y `GET /instruments/export` +
  `POST /instruments/import`.
- Entregas: `GET/POST /submissions`, `GET /submissions/{id}`, `POST /submissions/{id}/review`.

## Análisis CSV (`analysis.rs`) — lo que hace hoy

Dado un CSV con encabezados, por cada columna numérica calcula `count`, `mean`,
`std_dev` (poblacional, divide por `n`), `min`, `max`; arma una **regresión lineal**
entre las dos primeras columnas numéricas (pendiente, intercepto, R²) y junta
**advertencias** por celdas vacías o no numéricas.

### Limitaciones del flujo CSV frente a Física 103

> Los bloques de cálculo ya **existen** (`uncertainty.rs` cubre tipo A/B/combinada/expandida y
> propagación numérica; el catálogo de instrumentos está en `instruments.rs` + UI), pero el flujo
> de entrega por CSV **todavía no los usa**: integrarlos al ingreso de datos es la Fase 4
> (entrega por formulario). Estado de cada punto:

1. ~~No calcula incertidumbres~~ → **motor disponible** (`uncertainty.rs`); falta conectarlo a la entrega (Fase 4).
2. `std_dev` de `analysis.rs` sigue siendo **poblacional** (`/n`); el motor de tipo A usa la muestral. ⚠️ CONFIRMAR convención del curso.
3. ~~No conoce instrumentos~~ → **catálogo implementado** (Fase 2); falta que el estudiante elija instrumento por magnitud al cargar (Fase 4).
4. **Determinaciones indirectas** (`Q = f(a,b,c)`) y **propagación**: el motor las soporta; falta la definición de prácticas (Fase 3) y el formulario (Fase 4).
5. **Relajación exponencial** (P3) y escalas/resistencia interna de amperímetro/voltímetro (P2): pendiente de completar (Fase 6).
6. ~~Prácticas ficticias~~ → **P1/P2/P3 reales sembradas** (Fase 0).

El cierre de estas brechas es el objetivo de las Fases 3–6.
