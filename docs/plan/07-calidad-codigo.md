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

## 1. Objetivo y decisiones

- **Primera medición:** solo el estado actual de `development` (no se reconstruye la historia).
- **Mediciones periódicas:** una por cada promoción `development` → `staging`, sobre el commit que
  se promueve. Cada medición agrega una fila al historial y un informe.
- **Todo en el repo:** el método (`docs/calidad/metodo.md`), los scripts (`scripts/calidad/`) y los
  resultados (`docs/calidad/`), versionados y entrando por PR.
- **Informes en Markdown:** un `.md` por medición, con tablas y gráficos PNG generados por script;
  se leen en GitHub y el diff entre mediciones se ve en la PR.
- **Con SonarQube** (Community Build, local): notas A–E, deuda técnica, *code smells* e historial.
  Hay que verificar primero qué analiza de Rust (ver §7).
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
| Duplicación | `jscpd` | Rust y JS juntos; mínimo de tokens fijo. |
| Lint | `cargo clippy --message-format=json` | Por defecto y con `-W clippy::pedantic`; se cuentan avisos por lint, no bloquea. |
| Deuda, smells, bugs, hotspots | SonarQube Community Build (Docker) | Importa la cobertura (`lcov`) de Rust y JS. |
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
  datos/<fecha>-<commit>/ salida cruda de cada herramienta
  figuras/               gráficos generados desde historial.csv
scripts/calidad/
  medir.sh               worktree + todas las métricas locales + Sonar
  ux.mjs                 Lighthouse y axe con sesión
  consolidar.py          datos crudos → fila de historial.csv
  graficos.py            historial.csv → figuras/
  proceso.py             métricas de GitHub entre dos mediciones
sonar-project.properties claves y exclusiones de Sonar
```

Los scripts siguen la política de comentarios del repo (sin comentarios); lo que explica cada paso va
en `metodo.md`.

## 6. PRs

1. **`docs(calidad): método de medición y scripts` (1/3).** `metodo.md` (v1), `scripts/calidad/`,
   `sonar-project.properties` y una regla en `CONTRIBUTING.md`: cada promoción a `staging` lleva su
   medición. Incluye la verificación de Sonar con Rust (§7) y la elección final de herramientas.
2. **`docs(calidad): primera medición de development` (2/3).** Corre el método v1 sobre
   `development`, agrega `historial.csv`, las figuras y el informe con el estado actual: resumen por
   característica, tabla contra referencias, hallazgos principales y recomendaciones.
3. **`docs(calidad): plan de mejoras` (3/3, opcional).** Si la medición muestra deudas concretas
   (cobertura baja en un módulo, funciones demasiado complejas, duplicación), se suman como ítems a
   este plan, cada uno con su PR.

Después: cada promoción a `staging` corre `scripts/calidad/medir.sh` y suma su informe.

## 7. Riesgos y decisiones abiertas

- **Sonar y Rust:** confirmar si SonarQube Community Build analiza Rust (reglas propias o importando
  el JSON de clippy). Si no, Sonar mide solo el JS y la mantenibilidad de Rust sale de clippy,
  `rust-code-analysis` y `jscpd`; se declara en el método.
- **Sonar y el orden cronológico:** solo acepta análisis posteriores al último; las mediciones
  periódicas van en orden, pero rehacer una vieja exige recrear el proyecto.
- **`cargo-llvm-cov` en Windows:** necesita `llvm-tools-preview`; si da problemas se mide en
  Linux/WSL y se anota.
- **`cargo-mutants`:** puede tardar mucho; se limita al motor y a un tiempo máximo fijo, y el informe
  dice cuántos mutantes quedaron sin evaluar.
- **Lighthouse:** varía entre corridas; mediana de 3 en la misma máquina.
