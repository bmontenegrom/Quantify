# 04 — Roadmap por fases

Secuencia incremental. Cada fase deja la app compilando, con tests verdes y desplegable.
Las fases 1–3 son backend puro (bajo riesgo, muy testeable); 4–6 agregan API y UI.

> **Convención transversal de tests.** "Tests verdes" abarca **backend y frontend**:
> el backend con `cargo test` y el frontend con `node --test` (módulo puro
> `static/lib.js`, ver Fase 2.5). De aquí en más, toda fase que toque `static/`
> agrega tests JS de su lógica (extrayéndola a `lib.js` cuando haga falta), y el CI
> (`.github/workflows/ci.yml`) corre ambas suites en cada push/PR.

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

## Mejoras y próximos pasos (al 30-05-2026)

Detectadas con el código ya construido. Ordenadas por valor/riesgo; ninguna bloquea lo hecho.

### Correctitud / robustez
1. **Unicidad de símbolos cruzada por práctica** (deuda de Fase 3/4, ahora más relevante con
   regresión y comparación). Hoy `practice_quantities` y `practice_results` validan
   `UNIQUE(practice_id, symbol)` por separado: una magnitud `l` y un mensurando `l` pueden
   coexistir y la fórmula/`compareResults` quedan ambiguos. **Acción**: validar unicidad global
   de símbolos por práctica (al crear/editar magnitud o mensurando) + que el símbolo sea un
   identificador válido (sin espacios/operadores). Reservar `slope`/`intercept` en regresión.
2. **`relajacion_exponencial` es un `analysis_kind` sin motor** (solo etiqueta). O bien se
   implementa (linealizar `V(t)=V0·e^{-t/τ}` → `ln V` vs `t`, que ya cubre `regresion_lineal`
   con `y_formula = math::ln(V)`), o se **elimina el kind** y se modela esa parte como
   `regresion_lineal`. **Resuelto (2026-06-12, con el docente): se elimina** — en el curso τ se
   obtiene solo por medida directa (P3-parte1, estadístico) y por desfasaje (P3-parte2,
   regresión); no hay ajuste exponencial directo. Si algún día hace falta, se modela como
   `regresion_lineal` con `y_formula = math::ln(V)`.
3. **Mensajes de error internos en inglés** ("submission not found", "submission not found")
   en `routes.rs`: normalizar a español por la convención de errores amigables (son casos
   "no debería pasar", de bajo impacto).

### Calidad / infra
4. **E2E de navegador en CI (Playwright)**. Ya validamos el flujo de comparación/regresión con un
   script Playwright a mano; conviene versionarlo y correrlo en CI (job aparte, con el server
   levantado) para fijar el comportamiento de la UI. Hoy el CI solo corre `cargo test` + `node --test`.
5. **Ergonomía de dev DB**. Las pruebas manuales dejan entregas y ediciones en `data/quantify.db`
   (p. ej. el símbolo de P1 quedó como `Area`). Documentar/automatizar un reset (`rm data/quantify.db`
   + re-seed) o un script `dev-reset`.

### Funcionalidad (con el docente)
6. **P2-parte2 como `regresion_lineal`**: la parte de ajuste `P(R)`/recta de P2 puede sembrarse
   como práctica de regresión (el motor ya existe), igual que P3-parte2.
7. **Comparación — veredicto opcional**: hoy solo se muestran las diferencias. Evaluar un umbral
   de tolerancia configurable por el docente (✓/✗) si lo piden; se dejó fuera a propósito.
8. **Entrega/corrección por mesa de trabajo** (postergado): las hojas se califican por mesa;
   modelar la entrega por mesa cuando se encare la iteración de evaluación.

### Fase 6 (pulido, sin cambios)
- Completar P2/P3 con definiciones reales confirmadas; documentar formato y fórmulas en el README;
  revisar seeds y eventual migración de datos del curso.

---

# Plan de expansión — Fases 13–16 (2026-06-12)

Insumos: hojas de resultados reales del curso (9 prácticas, convertidas con markitdown desde
`OneDrive/Escritorio/tecnicas 103/informes`) y decisiones del docente:
**no habrá gráfico de exponencial alguna**, y la app **va a operar desde PCs distintas**
(deja de ser localhost single-machine).

## Hallazgos de los informes reales

Prácticas ya modeladas — confirmadas por las hojas:

- **Estadística (P1)**: péndulo; serie de T por operador (n≈100), L dado, deriva g=4π²L/T²
  por operador, y t½/τ/Q del decaimiento. La hoja real trabaja con **3 operadores** (3 series
  de la misma magnitud) — hoy la app modela 1 serie por magnitud. Decidir con el docente si
  cada operador es una entrega o si se agrega la dimensión "operador".
- **Continua (P2)**: serie y paralelo confirman las definiciones sembradas (I, VR1–VR3; Req, I).
  La hoja tiene una **Segunda Parte: curva de potencia** — tabla I(µA), R(kΩ), P(W) y búsqueda
  del máximo (R_Pmax teórico, P_max). No es regresión: **solo se grafican los puntos**.
- **RC (P3)**: partes 1 (τ directo) y 2 (desfasaje) confirman lo sembrado.

Prácticas nuevas a modelar (6):

| práctica | tipo | magnitudes clave | derivados / salida |
|----------|------|------------------|--------------------|
| **CA (RLC)** | estadístico | R, C, L, Vg, f_trabajo; por canal: a, b (Lissajous) | f_res teórica = 1/(2π√(LC)) vs experimental; módulos I, VR, VC, VL teóricos vs experimentales; desfasajes φ = asin(b/a) teóricos vs medidos |
| **Filtros** | curva (sin ajuste) | barrido en f: VRpp, Vgpp, a, b por punto | f_pasaje y f_bloqueo teóricas vs experimentales; curvas VR/Vg vs log(ω) y φ vs log(ω) — **eje x logarítmico** |
| **Fluidos I** | regresión | radio y largo del capilar, ρ, g; por altura: volumen, tiempo (réplicas) | caudal por altura; ajuste Q/h² vs 1/Q → viscosidad del agua de la pendiente; Reynolds por corrida |
| **Fluidos II** | regresión | radios capilar/recipiente, g; serie H_i, t_i | ajuste √H₁−√H_i vs Δt → pendiente, ordenada, M_medio vs M_teórico |
| **Hidrostática y TS** | estadístico | masa goma, ρ fluido, g; 3 medidas de pesas/posiciones; marco: m, d, d₁ | E (empuje), ρ_goma por medida y promedio; γ por medida y promedio |
| **Viscosidad (Stokes)** | regresión | ρ acero, ρ fluido, g, distancia; por esfera: radio, tiempos (réplicas) | velocidad límite por esfera, Reynolds; ajuste v_lím vs R² → viscosidad de la glicerina |

## Fase 13 (P0) — Seguridad multi-PC y despliegue

Prerequisito del uso real desde varias PCs; antes de exponer el server a la red.

1. **CSRF**: token por sesión (devuelto en `/api/session`), header `X-CSRF-Token` exigido en
   todo POST/DELETE; `fetchJson`/`postJson` lo agregan centralizadamente en `api.js`.
2. **Cookies**: `Secure` (detrás de TLS) y `HttpOnly` (verificar), `SameSite=Lax` se mantiene.
3. **Despliegue LAN**: documentar topología (server en una máquina, clientes por IP), bind
   configurable (`0.0.0.0`), reverse proxy con TLS (caddy local o autofirmado); script de
   arranque. Backups: copia programada de `data/quantify.db`.
4. **Hardening login**: rate-limit básico de intentos por IP/usuario (en memoria).

## Fase 14 (P1) — `analysis_kind = "curva"` (puntos sin ajuste)

Nuevo tipo de análisis: como `regresion_lineal` (tabla de puntos, fórmulas de eje x/y) pero
**sin ajuste** — solo scatter SVG + tabla. Opción `x_log: bool` para Filtros (log ω).
Habilita P2-parte2 (curva de potencia) y las dos curvas de Filtros (evaluar si una práctica
admite **dos curvas** o se modelan como dos partes). Sin esto no se pueden sembrar Filtros
ni la parte 2 de Continua.

## Fase 15 (P1) — Sembrar las 6 prácticas nuevas + P2-parte2

Con el docente, validando fórmulas contra las hojas reales (sección Hallazgos):
1. P2-parte2 curva de potencia (depende de Fase 14).
2. Hidrostática y TS; Viscosidad; Fluidos I; Fluidos II (el motor actual alcanza).
3. CA (RLC) — verificar que `evalexpr` cubra `asin` para los desfasajes.
4. Filtros (depende de Fase 14, eje log).
Decisión pendiente con el docente: operadores múltiples en Estadística.

### Estado al 2026-06-13

**Motores completados** (todos en `main`):

| Motor | PR | Descripción |
|---|---|---|
| A | #31 | Réplicas por punto en regresión/curva (`replicas_per_point`, grilla) |
| B | #32 | Lista de curvas por práctica `curva` (`practice_curves`, x/y/x_log) |
| C | #34 | Magnitud intermedia por punto (`practice_intermediates`): fórmula por réplica, promediada |
| D | #33 | Operadores en estadística (`operator_count`): serie por operador, mensurandos sin promediar |
| E | #35 | Regresión completa: escalares compartidos (`per_point=false`), derivadas por punto post-ajuste |
| F | #38 | Mensurandos agregados escalares en regresión (`practice_aggregates`): referencian extremos de serie |

**Prácticas sembradas** (todas en `main`):

| Práctica | PR | Motores | Estado |
|---|---|---|---|
| Viscosidad (Stokes) | #37 | A + E | ✓ Sembrada, probada |
| Fluidos I (Hagen-Poiseuille) | #36 | A + C + E | ✓ Sembrada, probada |
| Fluidos II (descarga capilar) | #39 | E + F | ✓ Sembrada, ecuaciones confirmadas por docente |
| Filtros (barrido RLC) | #40 | B (2 curvas, x_log) | ✓ Sembrada |
| P2-parte2 (curva de potencia) | #40 | B + E (curva + escalares) | ✓ Sembrada |

**Pendientes de sembrar:**

| Práctica | Motor | Notas |
|---|---|---|
| Hidrostática y TS | D (estadístico) | Decisión de modelado pendiente con docente |
| CA (RLC) | estadístico | Teórico vs experimental; `asin` verificado en `evalexpr` |

## Fase 16 (P1) — Refactor de módulos grandes

Tres módulos superan las 2 400 líneas (estado al 2026-06-13). No es bloqueante hoy, pero
agregar código nuevo ya se siente incómodo. Orden sugerido por retorno/riesgo:

1. **`practices.rs` (≈2 900 líneas)** — el módulo `#[cfg(test)]` representa ≈1 500 líneas.
   Extraer los tests a `tests/practices_integration.rs` (o `src/practices/tests.rs`).
   Bajo riesgo: no toca lógica productiva.

2. **`routes.rs` (≈2 400 líneas)** — router monolítico con handlers de todos los recursos.
   Split natural por dominio: `routes/practices.rs`, `routes/submissions.rs`,
   `routes/courses.rs`, `routes/instruments.rs`; un `routes/mod.rs` reexporta el router.

3. **`computation.rs` (≈3 200 líneas)** — posponer hasta que se agregue un motor nuevo
   (regresión no-lineal, exportación). En ese momento extraer
   `computation/curva.rs` y `computation/statistics.rs`.

**Aceptación**: `cargo test` verde, `cargo clippy` limpio, ningún comportamiento cambia.

## Fase 16 (P2) — Rediseño visual (moderno, vistoso, eficiente)

Mantener vanilla JS/CSS (sin framework). Candidatos, a validar con screenshots Playwright:
1. **Design tokens**: consolidar paleta/espaciado/radios en `:root` (hoy hay vars sueltas),
   modo oscuro con `prefers-color-scheme` + toggle.
2. **Tipografía e identidad**: stack tipográfico mejorado (Inter/variable), jerarquía de
   títulos, marca simple para login y sidebar.
3. **Componentes**: tarjetas con sombras suaves y hover, tablas con sticky header y zebra,
   estados vacíos, toasts para feedback (hoy: texto inline), skeletons de carga.
4. **Navegación**: sidebar colapsable con iconos, breadcrumbs en vistas de detalle,
   transiciones de vista discretas.
5. **Accesibilidad/eficiencia**: focus visible consistente, contraste AA, `prefers-reduced-motion`.

## Descartado

- ~~Gráfico de la curva exponencial en P3-parte1~~ — el docente confirmó que **no** debe haber
  gráfico de ninguna exponencial.

## Decisiones tomadas — Fase 15 (2026-06-13)

Tras leer las 6 hojas de resultados reales (markitdown), se confirmó que solo **P2-parte2**
(curva de potencia) y **Fluidos II** (regresión) encajan en el motor tal cual; las demás
exigían decisiones de modelado o extensiones. Decisiones:

- **Réplicas por punto (motor A):** en regresión/curva, cada punto puede tener varias réplicas
  de una magnitud (p.ej. tiempo medio por altura en Fluidos I, por esfera en Viscosidad), con su
  incertidumbre tipo A. **Se extiende el motor** (no se aproxima con un valor único por punto).
- **Filtros — dos curvas (motor B):** una práctica `curva` admite una **lista de curvas**, cada
  una con su `x_formula`/`y_formula`/`x_log`. Filtros define dos (VR/Vg vs logω y φ vs logω)
  sobre el mismo barrido. (No se modela como "partes".)
- **Operadores en estadística (motor D):** una práctica estadística puede declarar **N operadores**
  (P1 = 3). Cada operador carga su **propia serie de las magnitudes repetidas** (cada uno mide T);
  las **dadas por cátedra y de medida única se comparten** (L se carga una vez). Salida: los
  mensurandos derivados **por operador** (g por operador), **sin agregado/promedio automático** —
  el alumno/docente compara las determinaciones. Default sin operadores = comportamiento actual.
- **Teórico vs experimental (RLC, Continua) — solo visual, sin motor nuevo:** el valor
  **experimental** es una **magnitud medida** (su incertidumbre sale del instrumento, tipo B);
  el **teórico** es un **mensurando derivado** por fórmula sobre otras magnitudes medidas (su
  incertidumbre sale de propagación de varianzas). La app muestra ambos con sus incertidumbres
  lado a lado; el alumno los compara **visualmente, pueden diferir, sin veredicto**. La
  comparación con tolerancia/veredicto existente sigue siendo solo **alumno-vs-automático**.
- **`evalexpr` soporta** `math::asin/sin/cos/atan/sqrt/ln` (verificado) → `φ = asin(b/a)` de RLC
  es viable.

Pendiente de confirmar con el docente al sembrar: Hidrostática deriva E/ρ_goma/γ **por medida y
luego promedia** (3 determinaciones independientes, no réplicas) — evaluar si el promedio de
derivados por medida es aceptable vs derivar del promedio de entradas.

Orden de ejecución de Fase 15: motor A (réplicas/punto) → motor B (lista de curvas) → motor D
(operadores en estadística) → siembra de las 6 prácticas + P2-parte2.

## Fase 17 (P2) — Cobertura E2E (Playwright) ampliada

Retoma y reemplaza la mejora #4 ("E2E de navegador en CI") con un plan acotado.
Insumo: hoy `npm run test:e2e` corre **solo** `tests/e2e/run.mjs` (flujo P1); el
`tests/e2e/smoke-fluidos2.mjs` (entrega regresión + análisis con M_medio/agregados +
alta de magnitud adimensional desde el admin) existe pero **quedó huérfano** (CI no lo corre).

**Criterio**: no automatizar "toda la app" (sería redundante con los unit tests y caro de
mantener). Apuntar a las **costuras** que ningún unit test cubre: el gating servidor, el render
del formulario por tipo de análisis, auth/CSRF/ruteo por rol, y que el pipeline de cálculo llegue
a la pantalla. El grueso de la cobertura sigue en los tests rápidos (Rust + `node --test`).

Pasos por orden de valor:
1. **Harness compartido + wire a CI**: extraer `tests/e2e/lib.mjs` con bootstrap/login/server
   (hoy `run.mjs` y `smoke-fluidos2.mjs` lo duplican ~70 líneas c/u) y hacer que `test:e2e`
   corra **todos** los `tests/e2e/*.mjs` (rescata fluidos-2 en CI). Baja el costo de cada flujo
   nuevo a ~20 líneas.
2. **Un flujo por tipo de análisis**: estadístico (ya: P1), regresión (ya: fluidos-2),
   **curva (falta)**.
3. **Auth + ruteo por rol**: login falla/bloqueo, logout, el alumno no ve vistas de docente.
4. **CRUD admin round-trip**: crear magnitud/mensurando/agregado y verlo — 1 flujo, no uno por
   entidad.

**No automatizar en E2E** (más mantenimiento que valor): matemática de fórmulas/incertidumbre
(unit Rust), funciones JS puras (`node --test`), cada permutación de CRUD. Costos a vigilar:
flakiness (usar `waitFor*`, nunca sleep fijo), selectores frágiles (evaluar `data-testid`),
mantenimiento continuo.

**Aceptación**: `test:e2e` corre todos los smokes en CI; los 4 journeys críticos quedan cubiertos.

## Orden propuesto

13 (seguridad, P0: van a operar multi-PC) → 14 (kind curva, bloquea prácticas) →
15 (prácticas nuevas, con el docente) → 16 (rediseño UI) → 17 (cobertura E2E).
