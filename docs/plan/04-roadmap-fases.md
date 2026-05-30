# 04 — Roadmap por fases

Secuencia incremental. Cada fase deja la app compilando, con tests verdes y desplegable.
Las fases 1–3 son backend puro (bajo riesgo, muy testeable); 4–6 agregan API y UI.

> **Convención transversal de tests.** "Tests verdes" abarca **backend y frontend**:
> el backend con `cargo test` y el frontend con `node --test` (módulo puro
> `static/lib.js`, ver Fase 2.5). De aquí en más, toda fase que toque `static/`
> agrega tests JS de su lógica (extrayéndola a `lib.js` cuando haga falta), y el CI
> (`.github/workflows/ci.yml`) corre ambas suites en cada push/PR.

**Estado al 30-05-2026:** Fases 0–4 **hechas**: prácticas reales; `uncertainty.rs`; catálogo de
instrumentos (API + UI); base de tests JS + CI; definición de prácticas (editor teacher-only) con
**P1/P2/P3 sembradas**; entrega por formulario con cálculo automático; y visibilidad del cálculo
controlada por el docente. Próximo: P3 parte 2 / `regresion_lineal`, o la fase de comparación.

---

## Fase 0 — Prácticas reales y limpieza de seeds

**Objetivo**: reemplazar los placeholders por P1/P2/P3.

- En `db.rs::seed_practices`, sustituir `pendulo/hooke/caida-libre` por:
  - `p1-estadistica` — "Tratamiento Estadístico de Datos" — `analysis_kind = estadistico`
  - `p2-corriente-continua` — "Circuitos de Corriente Continua" — `regresion_lineal`
  - `p3-relajacion` — "Relajación Exponencial" — `relajacion_exponencial`
- Ajustar `seed_academic` (prácticas habilitadas del curso de prueba).
- Migración: `ALTER TABLE practices ADD COLUMN analysis_kind` vía `add_column_if_missing`.
- **Datos existentes**: `data/quantify.db` está en `.gitignore` (es local, no versionado),
  así que en desarrollo el reset es trivial (borrar el archivo y re-sembrar). Las migraciones
  de `db.rs` son idempotentes; igualmente, si una base local ya tiene `submissions` con
  `practice_id` antiguos, conviene resetearla para evitar referencias colgadas.

**Aceptación**: `GET /practices` devuelve P1/P2/P3; la app arranca sobre una base limpia.

---

## Fase 1 — Motor de incertidumbres (`src/uncertainty.rs`)

**Objetivo**: lógica de cálculo pura, sin tocar la base ni la API todavía.

- Crear módulo `uncertainty.rs` con: `type_a`, `type_b`, `combine`, `expand`, `propagate`.
- Agregar dependencia de evaluador de expresiones (`evalexpr` o `meval`) en `Cargo.toml`.
- Implementar propagación numérica por diferencias finitas centradas.
- Ajustar `analysis.rs`: `std_dev` muestral + `u_slope`/`u_intercept` en la regresión.
- **Tests** (criterio de aceptación):
  - tipo A con dataset de `s` conocido ⇒ `u_A = s/√n`.
  - tipo B en los 3 modelos: `resolucion` (`R/(2√3)`), `apreciacion` (`A/√6`) y `fabricante`
    (`(pct·|valor| + coef·step + fijo)/2`); validar con el caso osciloscopio `3% + 0.1·(V/div) + 1mV`.
  - combinada y expandida (`U = 2·u_c`).
  - propagación del ejemplo `Q = l·a + l·b` contra valor analítico.
  - relajación: datos sintéticos con `τ` conocido ⇒ `τ` recuperado dentro de tolerancia.

**Aceptación**: `cargo test` verde; el motor no depende de SQLite.
⚠️ Bloqueante suave: confirmar divisores de tipo B y convención de `s` antes de fijar los asserts.

---

## Fase 2 — Catálogo de instrumentos (datos + API + UI)

**Objetivo**: el docente administra instrumentos y escalas.

- Migraciones: tablas `instruments` (con `course_id` obligatorio), `instrument_scales`
  (idempotentes, patrón de `db.rs`).
- Funciones en `db.rs` (o nuevo `src/instruments.rs`): CRUD instrumentos y escalas, **por curso**.
- API: `GET/POST /instruments`, `POST /instruments/{id}`, escalas, y **`GET /instruments/export`
  + `POST /instruments/import`** (ver doc 03 §4), todo bajo `require_teacher`.
- Seed inicial de instrumentos típicos del curso de prueba:
  - regla/calibre → `apreciacion`;
  - cronómetro/balanza digital → `resolucion`;
  - **testers** `A830L` (corriente, con `internal_res`/`internal_res_u` por escala) y
    `EXTECH MN35` (voltaje/resistencia) → `fabricante`;
  - **osciloscopio** `GW Instek GDS-1052-U` → `fabricante` con escalas VOLTS/DIV
    (`pct=3`, `coef=0.1`, `fijo=1 mV`).
  ⚠️ Valores de apreciación de instrumentos analógicos a confirmar; specs de testers y
  osciloscopio (eje Y) ya cargadas; eje X/tiempo del osciloscopio para P3 pendiente.
- Frontend: pestaña **Instrumentos** (`teacher-only`) con selector de curso, alta/edición de
  escalas, y botones **Exportar**/**Importar** (descarga/carga de JSON).

**Aceptación**: un docente crea un instrumento digital y uno analógico con escalas en un curso,
se listan, persisten tras reinicio, y puede exportar el catálogo de un curso e importarlo en otro.

---

## Fase 2.5 — Infraestructura de tests de frontend + CI (transversal) ✅

**Objetivo**: que el frontend sea testeable y que "tests verdes" incluya al JS, antes de
las fases con UI pesada (4–5, con previsualización en vivo de incertidumbres).

- Runner **`node:test`** (built-in de Node, sin dependencias ni `node_modules`); `package.json`
  mínimo con `"type": "module"` y `scripts.test = "node --test"`.
- **`static/lib.js`**: módulo ES puro (sin DOM, sin efectos al cargar) con la lógica testeable
  extraída de `app.js`:
  - helpers puros: `escapeHtml`, `cssEscape`, `format`, `formatDate`, `groupBy`,
    `renderGroupType`, `scalePayload`;
  - selectores de datos (reciben `state`/`academic`/`gradebooks` por parámetro): `canReview`,
    `studentCourses`, `studentGroups`, `studentGradebooks`, `studentTotals`,
    `availableCoursesForStudent`, `availableGroupsForStudent`, `allStudents`, `allGroups`.
  `app.js` importa de `lib.js`; quedan en `app.js` los selectores acoplados al DOM
  (`selectedCourse`, `selectedTableAssignment`) y todo lo de render/red.
- **`tests/lib.test.js`**: un test por función exportada (helpers + lógica de negocio de los
  selectores), fuera de `static/` para no servirlo.
- **CI** (`.github/workflows/ci.yml`): jobs `rust` (`cargo test`) y `js` (`node --test`).

**Aceptación**: `node --test` y `cargo test` verdes; el CI corre ambas suites en cada push/PR.
Tests render/DOM (jsdom) y E2E (Playwright) quedan como follow-up.

---

## Fase 3 — Definición de prácticas (magnitudes y mensurandos) ✅

**Objetivo**: cada práctica declara qué se mide y qué se deriva.

- Migraciones: `practice_quantities`, `practice_results`.
- Funciones de lectura/escritura + API `GET /practices/{id}/definition` y altas.
- Seed de la definición de **P1** (magnitudes `l`, `a`, `b`; resultado `Q = l*a + l*b` como
  ejemplo guía) y esqueleto de P2/P3. ⚠️ CONFIRMAR magnitudes y fórmulas reales con el docente.
- Frontend: editor de definición de práctica (sub-vista en Cursos o pestaña propia).

**Aceptación**: `GET /practices/p1-estadistica/definition` devuelve magnitudes + fórmula;
editable desde la UI.

**Implementado:** `practice_quantities`/`practice_results` + CRUD en `src/practices.rs`;
API `/practices/{id}/definition` + endpoints de alta/edición/borrado; pestaña "Prácticas"
teacher-only con editor inline. **Seeds de las 3 prácticas** (de las técnicas de Física 103):
- **P1** — `l/a/b` → `Q = l*a + l*b` (área del cordón).
- **P2 (CC)** — `Vg, R1, R2, R3, RA` → `Req = R1 + RA + 1/(1/R2 + 1/R3)`, `I = Vg/Req`
  (R1 y RA en serie con el paralelo de R2/R3).
- **P3 (relajación, parte 1)** — `R, Rint, C, tmedio` → `tau_teorico = (R+Rint)*C`,
  `tau_exp = tmedio/ln2`. La **parte 2** (desfasaje, recta tg φ vs ω) espera `regresion_lineal`.

Las tres calzan con el motor estadístico actual (tipo A/B + propagación). El docente puede
ajustar cualquier definición desde el editor.

---

## Fase 4 — Entrega por formulario + cálculo

**Objetivo**: el estudiante carga datos guiado y recibe incertidumbres.

**Visión confirmada:** el estudiante sube **solo las lecturas crudas** (valores medidos +
instrumento/escala elegido por magnitud) → la app **genera automáticamente** las
incertidumbres (u_A, u_B, u_c, U y mensurandos derivados con su U). El comparativo con el
cálculo manual del estudiante se modela en la **Fase de comparación** (ver más abajo).

- Migraciones: `submission_measurements` + `submissions.entry_mode`.
- `db::create_submission` (variante `form`): persiste mediciones y llama al motor para
  producir `analysis_json` con `quantity_results[]` + `derived_results[]`.
- API: `GET /submissions/new?practice_id=...` (formulario), `POST /submissions` con
  `measurements[]`; mantener validación de permisos (`user_can_submit`) y mesa asignada.
  Requiere agregar `evalexpr` (o similar) para parsear/evaluar la fórmula del mensurando.
- ⚠️ **Unicidad de símbolos cruzada (heredado de Fase 3):** hoy `practice_quantities` y
  `practice_results` tienen su `UNIQUE(practice_id, symbol)` por separado, así que una magnitud
  `l` y un mensurando `l` pueden coexistir. Para el evaluador de fórmulas hay que garantizar que
  **todos** los símbolos de una práctica (magnitudes + mensurandos) sean únicos entre sí, o la
  fórmula queda ambigua. Validar el símbolo como identificador (sin espacios/operadores) y que
  la fórmula solo referencie símbolos declarados.
- Frontend: formulario dinámico por práctica con selector de instrumento/escala por magnitud,
  réplicas, y **previsualización en vivo** de `n, media, s, u_A, u_B, u_c, U`.

**Aceptación**: cargar P1 con réplicas e instrumentos produce una entrega cuyo detalle
muestra incertidumbres correctas (validadas contra cálculo manual).

---

## Fase 5 — Visualización y revisión docente

**Objetivo**: detalle rico y corrección.

- Detalle de entrega: tabla de magnitudes con incertidumbres y mensurandos derivados con `U`;
  gráfico para `regresion_lineal`/`relajacion_exponencial` (con la recta/linealización).
- Revisión docente sobre la entrega (ya existe `POST /submissions/{id}/review`); evaluar
  el enfoque **por mesa de trabajo** (las hojas de resultados se califican por mesa).
  ⚠️ Decisión de modelado de "entrega por mesa" — puede empujarse a la iteración de evaluación.

**Aceptación**: el docente abre una entrega, ve incertidumbres y gráfico, y registra revisión.

---

## Visibilidad del cálculo (hecho) — el docente decide cuándo mostrarlo al alumno ✅

El curso entrega el informe **en papel**: el alumno calcula a mano. Por eso el cálculo
automático queda **oculto al estudiante por defecto** y el **docente lo habilita por entrega**
al revisar (checkbox "Mostrar el cálculo automático al estudiante"). El alumno carga solo las
lecturas ("a ciegas") y, una vez habilitado, ve la tabla de incertidumbres para contrastar.

- `submissions.results_visible_to_student` (default 0); `ReviewSubmission.results_visible`.
- Gating **en el servidor**: a un estudiante se le devuelve `analysis: null` mientras no esté
  habilitado (no solo se oculta en la UI). Se eliminó `POST /submissions/preview` (exponía el cálculo).

## Fase de comparación (futuro) — Cálculo del estudiante vs. automático

**Objetivo**: el estudiante ingresa en la app sus propios cálculos de incertidumbre → se
contrastan lado a lado con los automáticos, marcando divergencias. Mejora opcional sobre lo
ya hecho (la visibilidad ya está); esbozo:

- Tabla `submission_student_calculations[]` (misma forma que `quantity_results`, cargada por el alumno).
- API `POST /submissions/{id}/student-calc`; vista comparativa con un helper de divergencia en `lib.js`.
- El docente ve ambas columnas y puede usarlas como criterio de corrección.

---

## Fase 6 — Pulido y P2/P3 completas

- Completar P2 (escalas adecuadas, resistencia interna del instrumento) y P3 (ajuste exponencial)
  con sus definiciones reales confirmadas por el docente.
- Documentar el formato de carga y las fórmulas en el README.
- Revisión de seeds y, si corresponde, migración de datos reales del curso.

**Aceptación**: las tres prácticas del primer bloque funcionan de punta a punta.

---

## Riesgos y dependencias

| Riesgo | Mitigación |
|--------|------------|
| Fórmulas de incertidumbre como imágenes en el `.docx` | Confirmar divisores con el docente (co-autor) antes de fijar tests (Fase 1). |
| Propagación simbólica vs numérica | Empezar con numérica (diferencias finitas); cubre el curso. |
| `db.rs` monolítico crece | Extraer `instruments.rs`/`uncertainty.rs`/`practices.rs` de forma incremental. |
| Base local `data/quantify.db` con datos viejos (no versionada) | Reset trivial en dev: borrar archivo y re-sembrar (Fase 0). |
| Modelo "por mesa" para hojas de resultados | Postergar a la iteración de evaluación; no bloquea el motor. |

## Orden sugerido de ejecución

`Fase 0 → 1 → 2 → 2.5 → 3 → 4 → 5 → Comparación → 6`. La **Fase 2.5** (tests + CI) es
transversal; se hizo tras la 2. El primer hito demostrable end-to-end es el final de la
**Fase 4** (el estudiante sube lecturas y ve incertidumbres calculadas automáticamente).
