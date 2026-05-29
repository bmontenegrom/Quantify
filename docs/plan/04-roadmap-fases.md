# 04 — Roadmap por fases

Secuencia incremental. Cada fase deja la app compilando, con tests verdes y desplegable.
Las fases 1–3 son backend puro (bajo riesgo, muy testeable); 4–6 agregan API y UI.

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

## Fase 3 — Definición de prácticas (magnitudes y mensurandos)

**Objetivo**: cada práctica declara qué se mide y qué se deriva.

- Migraciones: `practice_quantities`, `practice_results`.
- Funciones de lectura/escritura + API `GET /practices/{id}/definition` y altas.
- Seed de la definición de **P1** (magnitudes `l`, `a`, `b`; resultado `Q = l*a + l*b` como
  ejemplo guía) y esqueleto de P2/P3. ⚠️ CONFIRMAR magnitudes y fórmulas reales con el docente.
- Frontend: editor de definición de práctica (sub-vista en Cursos o pestaña propia).

**Aceptación**: `GET /practices/p1-estadistica/definition` devuelve magnitudes + fórmula;
editable desde la UI.

---

## Fase 4 — Entrega por formulario + cálculo

**Objetivo**: el estudiante carga datos guiado y recibe incertidumbres.

- Migraciones: `submission_measurements` + `submissions.entry_mode`.
- `db::create_submission` (variante `form`): persiste mediciones y llama al motor para
  producir `analysis_json` con `quantity_results[]` + `derived_results[]`.
- API: `GET /submissions/new?practice_id=...` (formulario), `POST /submissions` con
  `measurements[]`; mantener validación de permisos (`user_can_submit`) y mesa asignada.
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

`Fase 0 → 1 → 2 → 3 → 4 → 5 → 6`. Las fases 1–3 pueden adelantarse en paralelo al diseño de
UI, ya que son backend testeable. El primer hito demostrable end-to-end es el final de la **Fase 4**.
