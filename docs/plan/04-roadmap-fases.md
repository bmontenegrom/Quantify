# 04 — Roadmap por fases

Secuencia incremental. Cada fase deja la app compilando, con tests verdes y desplegable.
Las fases 1–3 son backend puro (bajo riesgo, muy testeable); 4–6 agregan API y UI.

> **Convención transversal de tests.** "Tests verdes" abarca **backend y frontend**:
> el backend con `cargo test` y el frontend con `node --test` (módulo puro
> `static/lib.js`, ver Fase 2.5). De aquí en más, toda fase que toque `static/`
> agrega tests JS de su lógica (extrayéndola a `lib.js` cuando haga falta), y el CI
> (`.github/workflows/ci.yml`) corre ambas suites en cada push/PR.

**Estado al 11-06-2026:** Fases 0–5 **hechas** + motor `regresion_lineal` + comparación
alumno-vs-automático. Además, ya en `main`: las 3 prácticas de Física 103 completas (P1 péndulo,
P2 serie/paralelo con tabs, P3 directa/desfasaje con tabs), modularización del frontend en 18
módulos ES, navegación lateral (sidebar), ficha de entrega full-page con ventana de edición
configurable, e informes compartidos por mesa (PR #21). El plan de continuación está en las
**Fases 7–12** al final de este documento.

**Estado al 30-05-2026:** Fases 0–5 **hechas** + motor `regresion_lineal` + comparación
alumno-vs-automático. Resumen: prácticas reales; `uncertainty.rs`; catálogo de instrumentos
(API + UI); base de tests JS + CI; definición de prácticas (editor teacher-only) con **P1/P2/P3
sembradas**; entrega por formulario con cálculo automático; visibilidad del cálculo controlada por
el docente; **motor de ajuste lineal** (`regresion_lineal`) con editor de fórmulas de eje, tabla de
serie y gráfico SVG; **P3-parte2 (desfasaje) sembrada**; y la **comparación** (el alumno carga sus
mensurandos y se contrastan con los automáticos). Convención de docs reforzada: toda `fn` con `///`
+ test, y **doctest con `assert`** en funciones públicas puras (`cargo test --doc` en verde).
Próximo: ver "Mejoras y próximos pasos" al final.

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

## Fase 4 — Entrega por formulario + cálculo ✅

**Objetivo**: el estudiante carga datos guiado y recibe incertidumbres.

**Implementado:** `submission_measurements` + `submissions.entry_mode`; `computation::create_form_submission`
calcula y persiste `analysis_json` (`quantities[]` + `derived[]` + `warnings`); `POST /submissions/form`
con validación de permisos; formulario dinámico por práctica con instrumento/escala y réplicas. El
cálculo se persiste (no se recalcula). El ⚠️ de unicidad de símbolos cruzada **sigue abierto** (ver
"Mejoras y próximos pasos").

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

## Fase 5 — Visualización y revisión docente ✅

**Objetivo**: detalle rico y corrección.

- Detalle de entrega: tabla de magnitudes con incertidumbres y mensurandos derivados con `U`;
  **gráfico SVG** para `regresion_lineal` (scatter + recta + ejes rotulados con las fórmulas).
- Revisión docente sobre la entrega (`POST /submissions/{id}/review`: estado, comentario, nota,
  visibilidad).

**Implementado** (parte vía el motor de regresión, abajo). Pendiente menor: el gráfico de
`relajacion_exponencial` como kind propio (hoy P3-parte1 es estadístico y la linealización se
cubre con `regresion_lineal`). El enfoque **por mesa de trabajo** sigue postergado a la iteración
de evaluación (⚠️ no bloquea).

**Aceptación**: el docente abre una entrega, ve incertidumbres y gráfico, y registra revisión.

---

## Motor `regresion_lineal` (ajuste lineal con incertidumbre) ✅

**Objetivo**: prácticas cuyo resultado sale de la pendiente/intercepto de un ajuste lineal.

- **Backend** (`computation::compute_regresion`): el alumno carga una serie de puntos con
  magnitudes crudas; dos **fórmulas de eje** por práctica (`practices.x_formula`/`y_formula`)
  derivan `(x, y)` por punto; `analysis::linear_regression` da pendiente, intercepto, sus
  incertidumbres y R²; los mensurandos se propagan desde `slope`/`intercept`. `evalexpr` con
  constantes `pi`/`e` y funciones `math::*`. API `POST /practices/{id}/regression-formulas`.
- **Frontend**: editor de fórmulas de eje; **tabla de serie** (un punto por fila) en la entrega;
  render del ajuste (`valor ± U`, R²) + **gráfico SVG** (helper puro `regressionPlot` en `lib.js`).
- **P3-parte2 sembrada** (`p3-relajacion-desfasaje`): `x = 2*pi*f`, `y = b/math::sqrt(a*a - b*b)`
  (= tg φ por figura de Lissajous), mensurando `tau = slope` (= RC).

---

## Fase de comparación — Cálculo del estudiante vs. automático ✅

**Objetivo**: el alumno ingresa sus mensurandos finales calculados a mano y se contrastan con los
automáticos.

- **Backend**: tabla `submission_student_results` (`UNIQUE(submission_id, symbol)`,
  `ON DELETE CASCADE`); `POST /submissions/{id}/student-results` (solo el alumno dueño; bloqueado
  una vez que el docente habilita la visibilidad, para no copiar el automático; valida que los
  símbolos sean mensurandos de la práctica). `SubmissionDetail.student_results` (sin gatear).
- **Frontend**: formulario "Mis cálculos" (editable hasta habilitar, luego solo lectura) + **tabla
  de comparación** auto-vs-alumno con diferencia absoluta y relativa (%) de valor y de U (sin
  veredicto). Helper puro `compareResults` en `lib.js`.
- Verificado de punta a punta con un **test de navegador (Playwright)** además de los unit tests.

---

## Visibilidad del cálculo (hecho) — el docente decide cuándo mostrarlo al alumno ✅

El curso entrega el informe **en papel**: el alumno calcula a mano. Por eso el cálculo
automático queda **oculto al estudiante por defecto** y el **docente lo habilita por entrega**
al revisar (checkbox "Mostrar el cálculo automático al estudiante"). El alumno carga solo las
lecturas ("a ciegas") y, una vez habilitado, ve la tabla de incertidumbres para contrastar.

- `submissions.results_visible_to_student` (default 0); `ReviewSubmission.results_visible`.
- Gating **en el servidor**: a un estudiante se le devuelve `analysis: null` mientras no esté
  habilitado (no solo se oculta en la UI). Se eliminó `POST /submissions/preview` (exponía el cálculo).

> **La fase de comparación ya está implementada** — ver la sección "Fase de comparación" arriba.

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

`Fase 0 → 1 → 2 → 2.5 → 3 → 4 → 5 → regresion_lineal → Comparación → 6`. La **Fase 2.5**
(tests + CI) es transversal; se hizo tras la 2. El primer hito demostrable end-to-end fue el final
de la **Fase 4**. Hecho hasta **Comparación** inclusive; queda la **Fase 6** y las mejoras de abajo.

---

## Plan de continuación — Fases 7–12 (al 11-06-2026)

Surge de una auditoría del código en `main` (seguridad, deuda técnica, cobertura de tests,
higiene del repo) más las mejoras que venían registradas del 30-05. Orden recomendado:
**7 → 8 → 9 → 10 → 11 → 6 → 12**. Las fases 7–8 son chicas y de mayor impacto; la 9 va
antes que la 10 para que el refactor grande quede protegido por E2E.

### Fase 7 — Seguridad e higiene (P0, ~medio día)

- **Migrar hash de contraseñas a Argon2.** Hoy es SHA-256 + salt UUID
  (`db.rs::hash_password`/`digest_password`, ~línea 3420): inadecuado para contraseñas
  (fuerza bruta barata). Usar el crate `argon2` con **re-hash transparente en el login**:
  `verify_password` detecta el formato viejo (`salt:hex`), valida, y re-hashea con Argon2
  al pasar. Sin migración de datos ni corte de servicio.
- **Cookie de sesión con `SameSite=Lax`** (hoy no se setea; mitiga CSRF básico). CSRF
  tokens completos quedan para la Fase 12 (solo si la app se expone públicamente).
- **`.gitignore` + limpieza del working tree**: agregar `screenshot-*.png`, `ss-*.png`,
  `*.bak*`, `/data/data/`, `/deploy/data/`; borrar los ~20 archivos sueltos (screenshots
  de sesiones de trabajo y backups de DB).

**Aceptación**: login funciona con usuarios pre-existentes (re-hash verificado con test);
`git status` limpio tras una sesión de smoke test con screenshots.

### Fase 8 — Correctitud: unicidad cruzada de símbolos (P1, ~medio día)

Deuda de Fase 3/4, hoy más relevante con regresión y comparación. `practice_quantities` y
`practice_results` validan `UNIQUE(practice_id, symbol)` por separado: una magnitud `l` y un
mensurando `l` pueden coexistir y la fórmula/`compareResults` quedan ambiguos.

- Validar **unicidad global** de símbolos por práctica al crear/editar magnitud o mensurando.
- Validar que el símbolo sea **identificador válido** (sin espacios/operadores).
- **Reservar `slope`/`intercept`** como símbolos en prácticas `regresion_lineal`.
- ⚠️ Antes de escribir la validación en `routes.rs`, revisar cómo consume los símbolos el
  motor (`computation.rs`/`uncertainty.rs`) — no validar por analogía.

**Aceptación**: crear un mensurando con el símbolo de una magnitud existente (o viceversa)
devuelve 400 con mensaje en español; tests de ambos sentidos + símbolos reservados.

### Fase 9 — E2E Playwright versionado en CI (P1, ~1 día)

El smoke test pre-PR ya se corre a mano en cada PR; fijarlo como script versionado.

- Script Playwright en `tests/e2e/` (fuera de `static/`): login (alumno y docente),
  formulario de medición (péndulo con cronómetro), entrega, revisión docente con
  visibilidad, ficha de detalle, comparación.
- Job de CI aparte que compila, levanta el server sobre una DB temporal sembrada y corre
  el script (server en puerto dedicado; `DATABASE_URL` y `APP_BIND_ADDR` por env).
- Falla del job = falla del PR, igual que `rust` y `js`.

**Aceptación**: el job corre en verde en CI sobre un push de prueba; un cambio que rompa
el login lo pone en rojo.

### Fase 10 — Refactor `db.rs` (P1, ~1–2 días, después de la 9)

`db.rs` tiene ~4200 líneas y centraliza esquema, seeds, sesiones y CRUD de todos los
dominios. Refactor **mecánico, sin cambio de comportamiento**, en commits chicos:

- Extraer módulos por dominio: `sessions.rs`, `users.rs`, `courses.rs`, `submissions.rs`
  (siguiendo el patrón ya usado con `instruments.rs`/`practices.rs`).
- `db.rs` queda con: pool, migraciones (`add_column_if_missing` y compañía) y seeds.
- Cada commit compila con tests verdes; el E2E de la Fase 9 respalda el conjunto.

**Aceptación**: `cargo test` + `node --test` + E2E verdes; ningún diff de comportamiento.

### Fase 11 — Pulido menor (P2, agrupable en un PR)

- **Errores en inglés** que quedaron en `routes.rs` ("submission not found", "is required"
  en multipart): normalizar a español por la convención de errores amigables.
- **Script `dev-reset`**: borrar `data/quantify.db` + re-seed en un comando (hoy es manual
  cada vez que cambia un seed; convención dev documentada).
- **Decidir `relajacion_exponencial`** (con el docente): es un `analysis_kind` sin motor.
  O se implementa linealizando (`ln V` vs `t`, que ya cubre `regresion_lineal` con
  `y_formula = math::ln(V)`), o se elimina el kind y esa parte se modela como
  `regresion_lineal`.

### Fase 6 — Pulido y P2/P3 completas (con el docente, sin cambios)

- Completar P2/P3 con definiciones reales confirmadas; documentar formato de carga y
  fórmulas en el README; revisar seeds y eventual migración de datos reales del curso.
- **P2-parte2 como `regresion_lineal`**: la parte de ajuste `P(R)`/recta de P2 puede
  sembrarse como práctica de regresión (el motor ya existe), igual que P3-parte2.
- **Comparación — veredicto opcional**: hoy solo se muestran diferencias. Evaluar umbral
  de tolerancia configurable por el docente (✓/✗) si lo piden; se dejó fuera a propósito.

### Fase 12 — Otras oportunidades (registro; tomar de forma oportunista)

Detectadas en la auditoría del 11-06. Ninguna es urgente; se registran para no perderlas
y tomarlas cuando se toque el área correspondiente.

1. **`forms.js` (~900 líneas, sin tests)**: el módulo JS más grande, con 4 ramas de
   renderizado por tipo de medición (`is_given`/`is_chrono`/`repeated`/simple) de ~50+
   líneas cada una. Extraer la lógica pura de validación/armado de payload a `lib.js`
   (testeable con `node --test`) cuando se lo toque por otra razón.
2. **Cobertura JS**: 18 de 21 módulos en `static/` no tienen tests unitarios. Los puros
   (`lib`, `chronometer`, `stats`) sí. Criterio: el E2E (Fase 9) cubre los módulos DOM
   mejor que unit tests de DOM; solo extraer-y-testear lógica que sea pura.
3. **CSRF tokens / validación de origen**: hoy no hay; riesgo bajo en app interna.
   Encarar solo si la app se expone públicamente (la cookie `SameSite=Lax` de Fase 7
   cubre el caso básico).
4. **Validación de email/rol duplicada en `routes.rs`** (~5 endpoints repiten
   `is_valid_email` + `matches!(role, ...)`): unificar en un helper si se vuelve a tocar.
5. **Entrega/corrección por mesa de trabajo** (postergado de Fase 5): las hojas se
   califican por mesa; modelar la entrega por mesa cuando se encare la iteración de
   evaluación. Los informes compartidos por mesa (PR #21) son el primer paso.
6. **Gráfico de `relajacion_exponencial` como kind propio** (pendiente menor de Fase 5;
   depende de la decisión de Fase 11).
