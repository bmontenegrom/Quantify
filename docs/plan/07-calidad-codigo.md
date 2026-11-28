# 07 — Medición de la calidad del código

Plan para medir el estado del código de `development` y repetir la medición de forma periódica,
siempre con el mismo método, para que las mediciones sean comparables entre sí. Es el análogo para
Quantify de la evaluación de calidad hecha en PIS 2026 (Gurises Unidos), adaptado al stack
Rust + JS sin framework + SQLite.

Escrito el 2026-11-28, con `development` en `142ef86` (después de #172).

## Índice

1. [Objetivo y decisiones](#1-objetivo-y-decisiones)
2. [Modelo de calidad e indicadores](#2-modelo-de-calidad-e-indicadores)
3. [Herramientas](#3-herramientas)
4. [Cómo se mide (regla de coherencia)](#4-cómo-se-mide-regla-de-coherencia)
5. [Estructura en el repo](#5-estructura-en-el-repo)
6. [PRs](#6-prs)
7. [Riesgos y decisiones abiertas](#7-riesgos-y-decisiones-abiertas)
8. [Plan de mejoras](#8-plan-de-mejoras)

## 1. Objetivo y decisiones

- **Primera medición:** solo el estado actual de `development` (no se reconstruye la historia).
- **Mediciones periódicas:** cada tanto (por ejemplo, alrededor de una promoción a `staging`), sin
  una regla fija. Cada medición agrega una fila al historial y un informe.
- **Todo en el repo:** el método (`docs/calidad/metodo.md`), los scripts (`scripts/calidad/`) y los
  resultados (`docs/calidad/`), versionados y entrando por PR.
- **Informes en Markdown:** un `.md` por medición, con tablas y gráficos PNG generados por script;
  se leen en GitHub y el diff entre mediciones se ve en la PR.
- **Con SonarQube** (Community Build 26.10, local): notas A–E, deuda técnica, *code smells* e
  historial, para Rust y JS (ver §7).
- **Sin cambios de comportamiento:** medir no toca el código de la app. Las herramientas que no son
  dependencias del proyecto se instalan aparte (`cargo install`, `npx -y`, Docker), con versión fija.

## 2. Modelo de calidad e indicadores

Características de **ISO/IEC 25010** que aplican al código, con valores de referencia habituales
(y los *quality gates* por defecto de Sonar donde corresponda).

| Característica | Indicador | Referencia |
|---|---|---|
| Tamaño | Líneas de código por lenguaje (Rust, JS, CSS, HTML), sin tests ni generados | descriptivo |
| | Cantidad de tests: unitarios Rust, doctests, golden, `node --test`, E2E | descriptivo |
| Mantenibilidad | Complejidad ciclomática por función (Rust y JS) | ≤ 10 |
| | Complejidad cognitiva por función | ≤ 15 |
| | Duplicación | < 3 % |
| | Avisos de clippy por lint (por defecto y `pedantic`, contados, no bloqueantes) | tendencia decreciente |
| | Nota de mantenibilidad y ratio de deuda técnica (Sonar) | A, ≤ 5 % |
| | Dependencias sin uso (Rust y npm) | 0 |
| | Ciclos entre módulos JS | 0 |
| | Funciones `pub` puras sin doctest y comentarios fuera de la política | 0 |
| Fiabilidad | Cobertura de líneas y de ramas/regiones, Rust (`cargo-llvm-cov`) | ≥ 80 % / ≥ 70 % |
| | Cobertura de líneas y ramas, JS (`node --test --experimental-test-coverage`) | ≥ 80 % / ≥ 70 % |
| | *Mutation score* del motor (`src/uncertainty.rs`, `src/computation/`) | ≥ 60 % |
| | Bugs de análisis estático (Sonar) | 0 |
| Seguridad | Vulnerabilidades en dependencias (RustSec y npm) | 0 altas / críticas |
| | Hallazgos SAST (Semgrep, CodeQL) y *security hotspots* (Sonar) | 0 sin revisar |
| Eficiencia | Lighthouse performance (login, formulario de entrega, análisis) | ≥ 90 |
| Usabilidad | Lighthouse accesibilidad y violaciones de axe (WCAG 2.1 AA) | ≥ 90 / 0 críticas |
| Proceso | PRs mergeadas, tiempo hasta el merge, tasa de CI en verde | descriptivo |

## 3. Herramientas

Cada una con su comando fijo (en `scripts/calidad/`) y su versión anotada en `docs/calidad/metodo.md`.

| Indicador | Herramienta | Notas |
|---|---|---|
| Tamaño | `tokei` | Excluye `tests/`, `target/`, `node_modules/`, `tests/golden/`, `templates/`. |
| Tests | `cargo test` / `node --test` / `npm run test:e2e` | Se cuentan desde la salida (no con grep). |
| Complejidad | `rust-code-analysis-cli` (Mozilla) | Ciclomática y cognitiva por función para Rust y JS con el mismo criterio. |
| Duplicación | SonarQube | Rust y JS con el mismo criterio; no se usa `jscpd`. |
| Lint | `cargo clippy --message-format=json` | Por defecto y con `-W clippy::pedantic`; se cuentan avisos por lint, no bloquea. |
| Deuda, smells, bugs, hotspots | SonarQube Community Build 26.10 (Docker) | Analiza Rust y JS; importa clippy y la cobertura (`lcov`) de ambos. |
| Dependencias sin uso | `cargo-machete`, `knip` | knip con entrada `static/app.js` e `index.html`. |
| Ciclos JS | `madge --circular static/` | |
| Política del repo | `scripts/comments.py --check`, chequeo de doctests | Reglas de `CONTRIBUTING.md`. |
| Cobertura Rust | `cargo-llvm-cov` | `lcov` + resumen JSON (líneas, regiones, funciones). |
| Cobertura JS | `node --test --experimental-test-coverage` | Reportero `lcov`; los E2E no aportan cobertura. |
| Mutación | `cargo-mutants` | Solo el motor, con tiempo máximo fijo; es lento. |
| Dependencias vulnerables | `cargo-audit`, `npm audit --omit=dev` | `cargo-deny` opcional para licencias. |
| SAST | Semgrep (Docker), CodeQL CLI | Rust y JavaScript. |
| Performance y accesibilidad | Lighthouse ×3 (mediana), `@axe-core/playwright` | App en `--release`, base con `SEED_DEMO_DATA=true`, sesión de docente y de estudiante. |
| Proceso | `gh pr list`, `gh run list` | Desde la medición anterior hasta esta. |

## 4. Cómo se mide (regla de coherencia)

Para que dos mediciones sean comparables:

- **Mismo método:** `docs/calidad/metodo.md` tiene una **versión** (`método v1`). Cada informe dice
  con qué versión se midió. Cambiar una herramienta, un umbral, una exclusión o un flag sube la
  versión y obliga a volver a medir el punto anterior con el método nuevo (para tener la base).
- **Versiones fijas:** cada herramienta se instala con versión exacta (`cargo install --locked
  <tool>@x.y.z`, `npx -y <pkg>@x.y.z`, imagen Docker con tag). El script verifica la versión antes
  de medir y falla si no coincide.
- **Mismas exclusiones** en todas las herramientas: `target/`, `node_modules/`, `tests/golden/`
  (datos), `templates/` (plantillas JSON), `tests/e2e/artifacts/`.
- **Mismo entorno:** se mide en un *git worktree* temporal del commit exacto, nunca sobre la copia de
  trabajo; misma máquina de referencia (anotada en el informe) para Lighthouse y tiempos.
- **Salida cruda guardada** (`docs/calidad/datos/<fecha>-<commit>/`) y un CSV acumulado
  (`docs/calidad/historial.csv`) con una fila por medición; los gráficos se regeneran desde el CSV.
- **Se declara** lo excluido y por qué, y se justifican los indicadores por debajo de la referencia en
  vez de omitirlos.

## 5. Estructura en el repo

```
docs/calidad/
  metodo.md              qué se mide, cómo, versión del método, versiones de herramientas
  historial.csv          una fila por medición
  2026-11-29-<commit>.md informe de cada medición
  datos/<fecha>-<commit>/ resumen.json versionado; crudo/ con la salida de cada herramienta, local
  figuras/               gráficos generados desde historial.csv
scripts/calidad/
  medir.sh               worktree + todas las métricas locales + Sonar
  ux.mjs                 Lighthouse y axe con sesión
  consolidar.py          datos crudos → fila de historial.csv
  graficos.py            historial.csv → figuras/
  proceso.py             métricas de GitHub entre dos mediciones
```

Los scripts siguen la política de comentarios del repo (sin comentarios); lo que explica cada paso va
en `metodo.md`.

## 6. PRs

1. **`docs(calidad): método de medición y scripts` (1/3).** `metodo.md` (v1) y `scripts/calidad/`
   (la configuración de Sonar va en `medir.sh`). Incluye la verificación de Sonar con Rust (§7) y
   la elección final de herramientas.
2. **`docs(calidad): primera medición de development` (2/3).** Corre el método v1 sobre
   `development`, agrega `historial.csv`, las figuras y el informe con el estado actual: resumen por
   característica, tabla contra referencias, hallazgos principales y recomendaciones.
3. **`docs(calidad): plan de mejoras` (3/3).** Las recomendaciones de la línea base
   ([informe](../calidad/2026-11-28-02e6ce8.md#5-recomendaciones)) pasan a ítems de este plan, cada
   uno con su PR: ver [§8](#8-plan-de-mejoras).

Después: cuando se quiera una medición nueva, se corre `scripts/calidad/medir.sh` y se suma su
informe.

## 7. Riesgos y decisiones abiertas

- **Sonar y Rust (resuelto):** la Community Build 26.10 trae `sonar-rust-plugin` (reglas propias,
  importa el JSON de clippy y `lcov`); la 26.11 ya no lo trae, así que la versión queda fija en 26.10
  y subirla exige comprobarlo de nuevo.
- **Sonar y el orden cronológico:** solo acepta análisis posteriores al último; las mediciones
  periódicas van en orden, pero rehacer una vieja exige recrear el proyecto.
- **`cargo-llvm-cov` en Windows:** necesita `llvm-tools-preview`; si da problemas se mide en
  Linux/WSL y se anota.
- **`cargo-mutants`:** puede tardar mucho; se limita al motor y a un tiempo máximo fijo, y el informe
  dice cuántos mutantes quedaron sin evaluar.
- **Lighthouse:** varía entre corridas; mediana de 3 en la misma máquina.

## 8. Plan de mejoras

Sale de la línea base del 2026-11-28 sobre `02e6ce8`
([informe](../calidad/2026-11-28-02e6ce8.md)). Cada ítem es una PR squash a `development`, con su
criterio de hecho medido con el método v1: se corre
`PASOS=<pasos> bash scripts/calidad/medir.sh <commit de la rama>` y se consolida, además de los
tests de siempre. Las funciones de complejidad son las 69 de
[Funciones sobre los umbrales](../calidad/2026-11-28-02e6ce8.md#funciones-sobre-los-umbrales).

| # | PR | Alcance | Hecho cuando |
|---|---|---|---|
| M1 | `fix(deps): actualizar dependencias con avisos de RustSec` | `cargo update` y sqlx 0.8 → 0.9, que permite `libsqlite3-sys` ≥ 0.39 (cierra `libsqlite3-sys`, `url`, `webpki-roots` y saca `rsa` del lockfile). Las consultas con SQL armado en el código pasan por `AssertSqlSafe`. | `cargo audit` sin vulnerabilidades. |
| M2 | `chore: política de comentarios y doctest faltante` | `comments.py --fix` en `src/practices/mod.rs`, `src/practices/parts.rs` y `src/routes/practice_admin.rs`; doctest para `validate_student_results`. | `comments.py --check` sin hallazgos y ninguna `pub fn` del motor sin doctest. |
| M3 | `fix(frontend): manejar promesas sin capturar y etiquetas de index.html` | Las 26 promesas sin manejar (S9383) de `navigation.js`, `forms.js`, `practices-admin.js` y otros, con un `.catch` que avisa con `toast.js`; las 2 etiquetas de `index.html` (`:90` y `:291`). | Sonar con 0 bugs y fiabilidad A. |
| M4 | `test(rutas): tests de integración de auth, entregas, cursos e instrumentos` | Router en memoria con `tower::ServiceExt::oneshot`, el mismo patrón que los tests de `src/routes/practice_admin.rs`. | Cobertura de líneas ≥ 70 % en `routes/auth.rs`, `routes/submissions.rs`, `routes/courses.rs` y `routes/instruments.rs`. |
| M5 | `refactor(motor): bajar la complejidad de engines.rs` | Las 8 funciones de prioridad 1: `compute_regresion`, `analyze_definition`, `run_kind_raw`, `build_points`, `compute_determinations`, `per_replica_points`, `compute_quantities` y `derive_results`. Antes del refactor, tests que maten los mutantes no detectados de `engines.rs`. | Ninguna función de `engines.rs` sobre los umbrales y `cargo mutants --file src/computation/engines.rs` sin mutantes no detectados nuevos. |
| M6 | `refactor(motor): complejidad de submission.rs y dual.rs` | `create_form_submission`, `check_student_result_symbols`, `update_form_submission`, `eval_dual` y `function`, con tests para los mutantes no detectados de esos archivos. | Esos archivos sin funciones sobre los umbrales. |
| M7 | `refactor(practicas): complejidad de routes/practice_admin.rs` | Las 14 funciones (3 de prioridad 1 y 11 de prioridad 2). Si el diff crece mucho, se parte en dos PRs por prioridad. | El archivo sin funciones sobre los umbrales. |
| M8 | `refactor(frontend): complejidad de formularios y análisis` | `forms.js` (5), `lib.js` (4), `analysis.js`, `forms-draft.js` y `forms-chrono.js`. | Esos archivos sin funciones sobre los umbrales. |
| M9 | `refactor(frontend): romper ciclos de navigation.js y borrar exports sin uso` | Las vistas se registran en `navigation.js` en vez de que él las importe; la clausura de las pestañas (`navigation.js:95`) y las 6 funciones de `practices-admin.js`; los 35 *exports* sin uso que marca knip. | `madge` sin ciclos, knip sin *exports* sin uso y esos archivos sin funciones sobre los umbrales. |
| M10 | `refactor(practicas): complejidad de src/practices/` | `template.rs` (3), `versions.rs` (2), `changes.rs` (2), `lifecycle.rs` y `crud.rs`. | Esos archivos sin funciones sobre los umbrales. |
| M11 | `refactor: complejidad restante de Rust` | `dimensions.rs` (3), `units.rs` (2), `db/mod.rs` (2), `routes/submissions.rs` (2), `instruments/seed.rs`, `routes/auth.rs`, `submissions/mod.rs`, `main.rs` y `db/schema.rs` (`migrate`, con un paso por versión). | Ninguna función de Rust sobre los umbrales. |
| M12 | `refactor(deps): reemplazar chrono por jiff` | `chrono` no tiene mantenimiento ([RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543), [chrono#1768](https://github.com/chronotope/chrono/issues/1768)); su mantenedor y RustSec recomiendan `jiff`. Se usa `jiff` con serde y `jiff-sqlx` 1.0 (SQLite, sqlx 0.9) para `Utc::now()`, las sumas de duraciones y los 21 campos `DateTime<Utc>`. Hay que probar: que lea las fechas que guardó chrono en una copia de una base vieja, que el JSON siga en RFC 3339 y cómo se integran los tipos de `jiff-sqlx` con `FromRow`. | `chrono` fuera de `Cargo.lock` y `cargo audit` sin avisos; la app arranca sobre una base vieja y los E2E pasan sin cambios en el frontend. |

**Orden.** Primero lo barato y lo que corrige riesgos (M1–M3). Después M4, porque los tests de rutas
cubren los refactors de M7 y M11. M12 va después de M4, por la misma razón. Los refactors del motor (M5 y M6) agregan sus tests de mutantes
antes de tocar el código.

**Reglas de los refactors:**

- No cambian comportamiento: tests, E2E y golden sin cambios.
- No suman comentarios (política del repo) ni jobs de CI.
- Si una función no baja de los umbrales sin empeorar la lectura (por ejemplo, un `match` de
  despacho plano), se deja y se justifica en la PR y en la próxima medición: la cognitiva es mejor
  guía que la ciclomática.

**Cierre.** Cuando terminen los ítems se hace una medición nueva con el método v1 y su informe
compara contra la línea base en `historial.csv`.
