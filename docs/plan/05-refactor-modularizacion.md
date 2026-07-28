# Refactor: modularizar + dedup (frontend primero)

## Context

Varios archivos crecieron mucho y cuesta navegarlos. La auditoría (ponytail-audit + 3 exploraciones) mostró que **el tamaño viene sobre todo de volumen, no de sobre-abstracción**: los archivos Rust son 30–58% tests y ~2400 líneas de datos de seed; el frontend tiene bloques cohesivos gigantes (tabla de series en `forms.js`) y duplicación real de markup de tablas. Casi no hay código muerto ni abstracciones especulativas para borrar.

Objetivo: bajar el tamaño por archivo partiendo por costuras de cohesión ya existentes (los tests/bloques se mueven con su módulo) y encoger la duplicación byte-idéntica. **Sin cambios de comportamiento.** Alcance elegido: **modularizar + dedup** (no se gatean seeds en esta tanda). Orden: **frontend primero**, un PR por área para que sean revisables.

Carga JS confirmada: entry único `static/app.js` (`type=module`) + imports ES. Dividir archivos = crear módulos nuevos e importarlos donde se usan; **no se toca `index.html`**.

---

## PR 1 — Frontend

### A. Partir `static/forms.js` (1809 → ~700)

Extraer a lo largo de bloques ya cohesivos:

1. **`static/forms-series.js`** (~400 líneas) — todo el bloque de tabla de series/regresión (Motor E), `forms.js:1130–1525`: `renderSeriesTable`, `updateRegressionPreview`, `seriesRowHtml`, `seriesPointResultCols`, `seriesRowComplete`, `collectSeriesPointResults`, `sharedRowHtml`, `seriesCellValue`, `replicaValueAt`, `rowReplicaMean`, `sharedSingleValue`, `updateSeriesLive`, `updateSeriesMeans`, `wireSeriesRemove`, y las live-columns `fluidosCaudalLiveValue`/`viscosidadVelocityLiveValue`.
2. **`static/forms-chrono.js`** (~330) — `forms.js:492–545, 623–627, 1527–1740`: `chronoWidgetInnerHtml`, `chronoHelperSectionHtml`, `wireChronoHelpers`, `needsChronoHelper`, `chronoKeyFor`, `formatElapsed`, `wireChronometerWidget`, `renderSeriesDebug`, `histogramSvg`.
3. **`static/forms-draft.js`** (~65) — `forms.js:1742–1809`: `draftKey`, `saveDraft`, `loadDraft`, `clearDraft`, `scheduleDraftSave`.

**Evitar import circular:** los helpers de markup chicos compartidos entre `forms.js` y `forms-series.js`/`forms-chrono.js` — `prefixSelectHtml`, `renderReplicaInput`, `replicaInputHtml`, `cellReplicaValues`, `populateScaleOptions`, `groupBySections` — se mueven a un **`static/forms-shared.js`** nuevo, del que importan ambos lados. Así `forms.js` importa `renderSeriesTable` de `forms-series.js` sin que este re-importe de `forms.js`.

### B. Partir + dedup `static/analysis.js` (750 → ~430)

1. **`static/analysis-plots.js`** — `regressionMarkup`, `plotSvg`, `regressionSvg`, `scatterMarkup`, `scatterSvg` (`analysis.js:352–441`). Superficie pública ya usada por `forms.js` (preview en vivo) → imports.
2. **`static/analysis-compare.js`** — `measuredVsTheoreticalMarkup`, `comparisonMarkup`, `pointResultsComparisonMarkup` (`443–524, 622–653`).
3. **`static/analysis-members.js`** — `membersEditorMarkup`, `wireMembersEditor` (`655–729`).

**Dedup (el ahorro real, ~120 líneas):**
- Un helper **`compareTableMarkup({headers, rows})`** en `analysis-compare.js` que arma el `.data-table-wrap`+`.compare-table`+thead/tbody que hoy repiten las 3 funciones de comparación.
- Hoistear los formatters **`num`/`pct`** (3 copias idénticas en `analysis.js:447/484/629`) a `static/lib.js` como exports (es el módulo puro compartido y ya lo importan `analysis.js` y `forms.js`).
- Inline `corridaCount` (`585–588`, 2 líneas, un solo uso) dentro de `pointResultsEntryMarkup`.

### C. Dedup `static/practices-admin.js` (989 → ~800)

- **`runPracticeAction(fn, successMsg)`** — wrapper único para el patrón `preventDefault → postJson → refetch definition → set status → renderPracticesPage` con try/catch, hoy copiado ~10× (`practices-admin.js:146–191` del motor genérico + 6 handlers `732–985`). Colapsa la mayor parte de la repetición.
- Mantener `SYMBOL_FORMULA_KINDS` (buen dedup, no tocar). No abstraer los forms de quantities/curves/results a mano en esta tanda (queda parcial pero es aceptable; no es objetivo ahora).

### D. Micro-limpiezas en `forms.js`

- Borrar `setSubmissionBusy` (`805–807`, delega una línea) → inline `submitButton.disabled = busy`.
- Inline `formatSeriesStat` (`51–53`) en su único uso (`renderSeriesDebug`, se va con `forms-chrono.js`).
- Quitar el fallback de `partForResult` (`547–550, 557–558`): los callers ya pasan `sectionId` explícito.

### Utilidades a reusar (no crear nuevas)
- `static/lib.js` es el destino de las utilidades compartidas (`format`, `prefixFactor`, `SI_PREFIXES`, `pointPower`, `flowRate`, `seriesStats`, `histogram`, `normalCurve`, `compareResults`). Agregar ahí `num`/`pct`. No partir `lib.js` (módulo puro testeado; partirlo no aporta).

### Verificación PR 1
- `npm test` → los 78 tests deben seguir verdes (cubren `lib.js`: `validateMeasurements`, `seriesStats`, `histogram`, `normalCurve`, `compareResults`).
- Smoke visual con Playwright (server local, login estudiante): (1) abrir formulario de Viscosidad, ver tabla de series + columna Re + velocidad en vivo; (2) cargar puntos y entregar; (3) abrir la entrega y ver "Mis cálculos" + comparación por corrida. Comportamiento idéntico a hoy.
- `git grep` de cada símbolo movido para confirmar que todos los consumidores importan del módulo nuevo.

---

## PR 2 — Rust engine (siguiente tanda, esbozo)

- `src/practices.rs` (3707) → **`practices/seed.rs`** (el bloque de builders `qty*`/`res*` + los 8 `seed_*` + `seed_definitions`, `L969–2037`, ~1050 líneas, un solo punto de entrada — **el mayor win individual**); `practices/crud.rs` (CRUD por entidad + `SymbolFormulaRow`); `practices/mod.rs` (DTOs + `definition`).
- `src/computation.rs` (4071) → `computation/formula.rs` (`compile_formula`/`check_formula`/`eval_compiled`/`CONSTANTS`, `L212–303`), `computation/engines.rs` (los motores, `L304–1103`), `computation/submission.rs` (IO/CRUD de entregas, `L1289–1706`), `mod.rs` (DTOs + `analyze`). Tests se mueven con cada módulo.
- Cada archivo Rust es 42–58% tests; moverlos con su módulo es la mayor baja mecánica.

Nota (2026-07-28): los tests de `practices.rs` ya se extrajeron a `src/practices/tests.rs` (PR #51) antes de que este plan se recuperara. El split de `seed.rs`/`crud.rs`/`mod.rs` sigue pendiente.

## PR 3 — Rust data/routes (siguiente tanda, esbozo)

- `src/db.rs` → `db/schema.rs` (migraciones/DDL ~700), `db/password.rs` (hash/verify), deja DTOs+queries.
- `src/submissions.rs` → **`gradebook.rs`** (corte limpio en `L503`, ~250) + opcional `report_members.rs`.
- `src/courses.rs` → `courses/{course,group,subgroup}.rs` (split por entidad, el más limpio).
- `src/instruments.rs` → `instruments/seed.rs` (~300) + `instruments/import_export.rs`.
- Dedup: helper `guard_symbol` en `routes/practice_admin.rs` (trío format+reserved+duplicate repetido en 7 handlers); mover validadores puros a `practice_admin/validation.rs`.

Nota (2026-07-28): `routes.rs` ya estaba dividido por dominio desde antes de este plan (commit `5896801`, `src/routes/{auth,courses,instruments,practice_admin,submissions,mod}.rs`). Falta el resto: `db.rs`, `submissions.rs` → `gradebook.rs`, `courses.rs`, `instruments.rs`, y el dedup de `guard_symbol`.

## Fuera de alcance (por ahora)
- **Gatear seeds de dev** (`seed_users`/`seed_academic`/`seed_submissions`) con `#[cfg(debug_assertions)]`: requiere criterio prod-vs-dev y `seed_practices`/`seed_definitions`/`seed_instruments` son catálogo real. Ítem separado si se decide después.
- Aplanar `SymbolFormulaRow` a fns con `&'static str table`: ~15 líneas, bajo valor.
