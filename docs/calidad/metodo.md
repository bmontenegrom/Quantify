# Método de medición de la calidad del código

**Versión del método: v1** (2026-11-28). Plan y motivación en
[`docs/plan/07-calidad-codigo.md`](../plan/07-calidad-codigo.md).

Cada medición se hace sobre un commit exacto, con las herramientas y versiones de este documento, y
deja un informe en `docs/calidad/`. Dos mediciones son comparables solo si usan la misma versión del
método.

## Índice

1. [Regla de versionado](#1-regla-de-versionado)
2. [Indicadores](#2-indicadores)
3. [Herramientas y versiones](#3-herramientas-y-versiones)
4. [Alcance y exclusiones](#4-alcance-y-exclusiones)
5. [Decisiones declaradas](#5-decisiones-declaradas)
6. [Cómo medir](#6-cómo-medir)
7. [Salida](#7-salida)
8. [SonarQube](#8-sonarqube)

## 1. Regla de versionado

- Cambiar una herramienta, su versión, un flag, un umbral o una exclusión **sube la versión** del
  método (v1 → v2) y obliga a volver a medir el commit de la medición anterior con el método nuevo,
  para tener una base comparable.
- Las versiones esperadas están al principio de `scripts/calidad/medir.sh`; el script verifica las
  herramientas instaladas antes de medir y se detiene si alguna no coincide. Las que se usan con
  `npx -y` o Docker llevan la versión en el comando.
- Cada informe dice con qué versión del método se midió (primera línea de `commit.txt`).

## 2. Indicadores

Características de ISO/IEC 25010 que aplican al código.

| Característica | Indicador | Fuente | Referencia |
|---|---|---|---|
| Tamaño | Líneas de código por lenguaje en `src/` y `static/` | `tokei` | descriptivo |
| | Tests: unitarios Rust, doctests, `node --test`, E2E | salida de cada corredor | descriptivo |
| Mantenibilidad | Funciones con complejidad ciclomática > 10 | `rust-code-analysis` | 0 (tendencia) |
| | Funciones con complejidad cognitiva > 15 | `rust-code-analysis` | 0 (tendencia) |
| | Duplicación (% de líneas) | Sonar | < 3 % |
| | Avisos de clippy por lint, por defecto y `pedantic` | `cargo clippy` | tendencia decreciente |
| | Nota de mantenibilidad, ratio de deuda técnica, *code smells* | Sonar | A, ≤ 5 % |
| | Dependencias sin uso (Rust y JS) | `cargo-machete`, `knip` | 0 |
| | Ciclos entre módulos JS | `madge` | 0 |
| | Archivos con comentarios fuera de la política | `scripts/comments.py --check` | 0 |
| | Funciones `pub` del motor sin doctest | `consolidar.py` | 0 |
| Fiabilidad | Cobertura de líneas y regiones, Rust | `cargo-llvm-cov` | ≥ 80 % / ≥ 70 % |
| | Cobertura de líneas y ramas, JS, archivos cargados por los tests | `node --test` | ≥ 80 % / ≥ 70 % |
| | Cobertura global (Rust + JS, archivos sin tests cuentan 0 %) | Sonar | ≥ 80 % |
| | *Mutation score* del motor | `cargo-mutants` | ≥ 60 % |
| | Nota de fiabilidad y bugs | Sonar | A, 0 |
| Seguridad | Vulnerabilidades en dependencias | `cargo-audit`, `npm audit` | 0 altas / críticas |
| | Hallazgos SAST | Semgrep, CodeQL | 0 sin revisar |
| | Nota de seguridad y *security hotspots* | Sonar | A, 0 sin revisar |
| Eficiencia | Lighthouse performance (mediana de 3) | Lighthouse | ≥ 90 |
| Usabilidad | Lighthouse accesibilidad (mediana de 3) | Lighthouse | ≥ 90 |
| | Violaciones de axe (WCAG 2.1 AA) por impacto | `@axe-core/playwright` | 0 críticas / serias |
| Proceso | PRs mergeadas, mediana de horas hasta el merge, % de corridas de CI en verde | `gh` | descriptivo |

## 3. Herramientas y versiones

| Herramienta | Versión | Instalación |
|---|---|---|
| tokei | 15.0.0 | `cargo install --locked tokei@15.0.0` |
| rust-code-analysis-cli | 0.0.25 | `cargo install --locked rust-code-analysis-cli@0.0.25` |
| cargo-llvm-cov | 0.9.2 | `cargo install --locked cargo-llvm-cov@0.9.2` + `rustup component add llvm-tools-preview` |
| cargo-mutants | 27.1.0 | `cargo install --locked cargo-mutants@27.1.0` |
| cargo-machete | 0.10.1 | `cargo install --locked cargo-machete@0.10.1` |
| cargo-audit | 0.23.0 | `cargo install --locked cargo-audit@0.23.0` |
| knip | 6.49.0 | `npx -y knip@6.49.0` |
| madge | 8.0.0 | `npx -y madge@8.0.0` |
| Lighthouse | 13.6.0 | `npm i --no-save` en el worktree |
| @axe-core/playwright | 4.14.1 | `npm i --no-save` en el worktree |
| Semgrep | 1.181.1 | imagen `semgrep/semgrep:1.181.1` |
| CodeQL CLI | 2.27.3 | bundle descomprimido; ruta en `CODEQL` |
| SonarQube Community Build | 26.10.0.133078 | imagen `sonarqube:26.10.0.133078-community` |
| SonarScanner npm | 5.1.0 | `npx -y @sonar/scan@5.1.0` |

SonarQube queda en 26.10 a propósito: la 26.11 Community Build ya no trae el analizador de Rust
(`sonar-rust-plugin`), que la 26.10 incluye. Subir de versión exige comprobar que lo siga trayendo.

Del proyecto: el toolchain Rust estable y Node de la máquina (se anotan en el informe), `cargo test`,
`cargo clippy`, `node --test`, `npm run test:e2e` y Playwright de `package.json`.

## 4. Alcance y exclusiones

- **Código medido:** `src/` (Rust) y `static/` (JS, CSS, HTML).
- **Excluido en todas las herramientas:** `target/`, `node_modules/`, `tests/golden/` (datos),
  `tests/fixtures/`, `templates/` (plantillas JSON), `tests/e2e/artifacts/`, `graphify-out/`, `data/`.
- **Tests:** `tests/`, `src/**/tests.rs` y `src/**/golden_tests.rs` son código de test para Sonar y
  para la complejidad.

## 5. Decisiones declaradas

- **Tests Rust en línea.** Los módulos `#[cfg(test)] mod tests { … }` dentro de un archivo de `src/`
  no se pueden separar por ruta: cuentan como código fuente en `tokei`, Sonar y
  `rust-code-analysis`. Solo los archivos `tests.rs` y `golden_tests.rs` se tratan como tests.
- **Cobertura JS.** `node --test` solo informa los archivos que los tests importan; el resto de
  `static/` no aparece. Por eso se informan dos números: la cobertura de `node --test` (archivos
  cargados) y la global de Sonar, donde un archivo sin tests cuenta 0 %. Los E2E no aportan cobertura.
- **Cobertura Rust.** `cargo llvm-cov` corre los tests unitarios y de integración, no los doctests.
- **Mutación.** Solo el motor (`src/uncertainty.rs` y `src/computation/`), corriendo solo los tests
  de la librería (`--lib`, donde están los del motor), 4 mutantes en paralelo, *timeout* de 3 veces
  la línea base por mutante y un máximo de 2 h en total. Los mutantes se evalúan en orden aleatorio
  (`--shuffle`), así que si el máximo corta la corrida los evaluados son una muestra al azar y el
  *score* es una estimación; el total sale de `cargo mutants --list`. El *score* es
  `caught / (caught + missed)`; los mutantes con *timeout*, los no viables y los que no llegaron a
  evaluarse se informan aparte. Al cumplirse el máximo, `medir.sh` mata todo el árbol de procesos de
  `cargo mutants` (en Windows, matar solo el proceso padre deja vivos los tests en curso).
- **Funciones `pub` sin doctest.** Se cuentan las `pub fn` de `src/uncertainty.rs` y
  `src/computation/` (fuera de módulos de test) que no tienen un `///` con un bloque de código
  inmediatamente antes. Usa el mismo lector de comentarios que `scripts/comments.py`.
- **Complejidad.** `rust-code-analysis` calcula ciclomática y cognitiva por función con el mismo
  criterio para Rust y JS. Cada función cuenta solo su propio cuerpo: se le resta lo de las
  clausuras y funciones anidadas, que se cuentan aparte. Se informan la cantidad de funciones por
  encima de 10 y 15, el máximo y la lista de todas las que superan algún umbral.
- **Duplicación.** La da Sonar, que la calcula para Rust y JS. No se usa `jscpd`.
- **Clippy.** No bloquea: se cuentan avisos por lint con la configuración del repo (`clippy.toml`) y,
  aparte, sumando `-W clippy::pedantic`. Sonar importa el reporte por defecto, no corre clippy por
  su cuenta.
- **Dependencias JS.** El proyecto no tiene dependencias de producción (`npm audit --omit=dev` da
  0 por construcción); `knip` se configura con `static/app.js` y los tests como entradas.
- **Lighthouse y axe.** App en `--release`, base temporal con `SEED_DEMO_DATA=true`, preset de
  escritorio. La app es una sola página sin rutas: se audita la pantalla de login, la vista inicial
  del estudiante y la del docente (Lighthouse, mediana de 3 corridas), y axe además recorre cada
  vista del docente. Siempre en la misma máquina de referencia (anotada en el informe).
- **SAST.** Semgrep con `p/rust`, `p/javascript`, `p/secrets` y `p/owasp-top-ten`; CodeQL con las
  suites `security-and-quality` de Rust y JavaScript, sin compilar (`--build-mode=none`).
- **Proceso.** PRs mergeadas a `development` y corridas completas del workflow `ci.yml` (sin
  canceladas ni salteadas) entre la fecha del commit medido anterior y la del actual; en la primera
  medición, desde el inicio del repo.

## 6. Cómo medir

Requisitos: Git Bash, Docker, Python 3 con matplotlib, Node 24, `gh` autenticado, las herramientas
de §3 y una instancia de SonarQube.

```bash
docker run -d --name quantify-sonar -p 9000:9000 sonarqube:26.10.0.133078-community
```

En `http://localhost:9000` (admin/admin la primera vez) se crea un token de usuario, que analiza y
lee la API, y se pasa como variable de entorno; nunca se guarda en el repo.

```bash
SONAR_TOKEN=<token> bash scripts/calidad/medir.sh [commit]
python scripts/calidad/proceso.py <commit> [commit-anterior]
python scripts/calidad/consolidar.py docs/calidad/datos/<fecha>-<commit>
python scripts/calidad/graficos.py
```

- `medir.sh` mide `origin/development` si no se le pasa un commit. Crea un *git worktree* temporal
  fuera del repo (nunca mide la copia de trabajo) y lo borra al terminar.
- `PASOS=a,b` repite solo algunos pasos: `tamano`, `tests`, `e2e`, `clippy`, `complejidad`,
  `cobertura`, `mutantes`, `deps`, `politica`, `audit`, `sast`, `ux`, `sonar`. Sin `SONAR_TOKEN` se
  saltea Sonar.
- `CODEQL` (ruta al ejecutable), `SONAR_URL` y `PORT` (de la app para Lighthouse) tienen valores por
  defecto en el script.
- Un paso que falla no corta los demás; su log queda en `crudo/`.
- Si `cargo llvm-cov` o `cargo mutants` fallan en Windows se corren en WSL y se anota en el informe.

## 7. Salida

```
docs/calidad/
  historial.csv                   una fila por medición (consolidar.py)
  <fecha>-<commit>.md             informe de la medición
  figuras/                        gráficos desde historial.csv (graficos.py)
  datos/<fecha>-<commit>/
    commit.txt                    versión del método, commit, fecha, asunto, sistema
    resumen.json                  todos los indicadores (consolidar.py)
    proceso.json                  métricas de GitHub (proceso.py)
    crudo/                        salida cruda de cada herramienta (no se versiona)
```

`<fecha>` es la fecha del commit medido. La salida cruda queda local (`.gitignore`) porque pesa
mucho; `resumen.json` tiene todo lo que usan el informe y los gráficos.

## 8. SonarQube

- **Proyecto** `quantify`. Toda la configuración (fuentes, tests, exclusiones, reportes de clippy y
  cobertura, fecha del commit en `sonar.projectDate`) va como `-D` en `medir.sh`: el scanner de npm
  no lee `sonar-project.properties` y toma la clave de `package.json`. Sonar no corre clippy por su
  cuenta (`sonar.rust.clippy.enabled=false`), importa el reporte del paso `clippy`.
- **Rutas de cobertura.** `medir.sh` deja relativas las rutas de los `lcov` para que coincidan con
  el worktree de cualquier corrida.
- **Orden cronológico.** Sonar solo acepta análisis con fecha posterior al último. Rehacer una
  medición vieja exige borrar el proyecto y volver a analizar todo en orden.
- **Falsos positivos.** Se marcan en la instancia (falso positivo o aceptado, con comentario que lo
  justifica) sin tocar el código, y se listan en el informe porque solo existen en esa instancia.
- **Historial.** El historial que vale es `historial.csv`; el de Sonar es auxiliar.
