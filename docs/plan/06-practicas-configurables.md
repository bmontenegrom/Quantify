# 06 — Prácticas configurables desde la UI

Roadmap para que el **docente cree y edite prácticas completas desde la interfaz**, con el mismo
poder expresivo que las 10 prácticas sembradas hoy en código: partes (pestañas), datos compartidos
entre partes, series de datos, réplicas, datos de cátedra, intermedias, curvas, agregados, etc.
El diseño contempla además usos futuros: otros cursos de física, otras materias, compartir
prácticas entre cursos/instalaciones y nuevos tipos de análisis.

Escrito el 2026-11-02, con `main` en `16f48c6` (después de #70).

## Índice

1. [Objetivo y decisiones](#1-objetivo-y-decisiones)
2. [Estado actual](#2-estado-actual)
3. [Modelo objetivo](#3-modelo-objetivo)
4. [Cobertura de las 10 prácticas existentes](#4-cobertura-de-las-10-prácticas-existentes)
5. [Fases](#5-fases)
6. [Orden, dependencias y riesgos](#6-orden-dependencias-y-riesgos)
7. [Decisiones del docente](#7-decisiones-del-docente)

---

## 1. Objetivo y decisiones

**Objetivo.** Un docente, sin tocar código ni reiniciar el servidor, puede:

- crear una práctica nueva (o duplicar una existente como punto de partida);
- dividirla en **partes** (pestañas de una misma entrega), cada una con su propio tipo de análisis;
- declarar qué datos son **compartidos por todas las partes** y cuáles son propios de una parte;
- declarar, por magnitud, si es un **dato de cátedra**, una **medida única**, una medida con
  **réplicas**, una **serie de datos** (un valor por punto) o una serie con réplicas por punto;
- definir mensurandos derivados, intermedias, resultados por punto, agregados y curvas con
  fórmulas validadas al guardar;
- agrupar visualmente el formulario del alumno en **secciones**;
- ver cómo queda el formulario del alumno y probar el cálculo antes de publicar;
- publicar la práctica y habilitarla en un curso.

**Decisiones tomadas (2026-11-02).**

| Tema | Decisión |
|------|----------|
| **Partes** | Una práctica es **una sola entrega con pestañas**. **Cada parte elige su tipo de análisis** (`estadistico`, `regresion_lineal`, `curva`, y los que vengan). Los datos compartidos se declaran a nivel práctica y todas las partes los ven. P3 (hoy dos prácticas agrupadas en el menú) pasa a ser **una práctica de dos partes**. |
| **Ediciones con entregas** | **Versionado.** Cada entrega queda atada a la versión de la práctica con la que se hizo; un cambio estructural crea una versión nueva y las entregas viejas se siguen viendo y recalculando como eran. |
| **Usos futuros a contemplar** | Otros cursos de física; otras materias (química, biología: unidades, constantes y funciones distintas); compartir prácticas (exportar/importar, plantillas); nuevos tipos de análisis (ajustes no lineales, ponderados). |
| **Datos existentes** | **No hay datos de producción que conservar.** Las migraciones pueden borrar entregas y definiciones cuando simplifica el cambio (en particular, las entregas de P3). Igual se mantienen los golden de cálculo (F0): lo que importa preservar es la física de cada práctica, no las filas. |
| **Quién crea y publica** | **Docente y admin** (los dos roles que hoy pasan `require_teacher`). |
| **Alcance de una práctica** | **Catálogo global**: toda práctica la ven todos los cursos, para poder reutilizarla. Cada curso decide cuáles habilita. |
| **P1 (operadores)** | **Se mantiene una pestaña por operador.** |
| **Versiones y entregas** | Las entregas **se quedan en su versión**; no hay migración de entregas entre versiones (queda para más adelante si aparece la necesidad). |
| **Otra materia probable** | **Química.** El modelo tiene que soportar concentraciones y unidades compuestas (mol/L, g/mol, ppm, % m/m…), logaritmos decimales (pH) y curvas de calibración. Ver [F10](#f10--futuro). |

**Restricción transversal.** Las 10 prácticas existentes tienen que seguir calculando
**exactamente lo mismo** en todas las fases. Para eso la Fase 0 congela su comportamiento con
tests "golden" (ver [F0](#f0--base-de-seguridad)).

---

## 2. Estado actual

### 2.1 Modelo de datos (`src/db/schema.rs`)

| Tabla | Qué guarda | Notas |
|-------|------------|-------|
| `practices` | `id`, `name`, `description`, `analysis_kind`, `x_formula`, `y_formula`, `operator_count` | Solo las crea `seed_practices` (`src/db/mod.rs:350`). No hay estado, autor ni curso. |
| `practice_quantities` | Magnitudes de entrada | Flags: `repeated`, `is_given`, `per_point`, `has_uncertainty`, `optional`, `replicas_per_point`, `default_value`, `slot_count`, `quantity` (magnitud física, sugiere instrumentos). `UNIQUE(practice_id, symbol)`. |
| `practice_results` | Mensurandos derivados | `formula`, `tolerance`, `is_final`, `has_uncertainty`. |
| `practice_intermediates` | Motor C: fórmula por réplica, promediada por punto | Solo regresión/curva. |
| `practice_point_results` | Motor E: derivada por punto tras el ajuste | Solo regresión. |
| `practice_aggregates` | Motor F: escalar tras el ajuste; ve `X_first`, `X_last`, … | Solo regresión. |
| `practice_curves` | Motor B: curvas `x_formula`/`y_formula`/`x_log` | Solo `curva`. |
| `course_practices` | Práctica habilitada en un curso (`enabled`, #70) | Las prácticas son un catálogo global. |
| `submissions`, `submission_measurements` | Entregas y valores (`point_index`, `operator_index`, `replicate_index`) | `submissions.practice_id` y `submission_measurements.quantity_id` **no** tienen `ON DELETE CASCADE`. |

Todos los símbolos (magnitudes, resultados, intermedias, por punto, agregados) son únicos por
práctica (`guard_symbol` en `src/routes/practice_admin.rs`), ASCII `[A-Za-z_][A-Za-z0-9_]*`, con
reservados `pi`, `e`, `slope`, `intercept` y los sufijos `_first/_first2/_last/_last2`.

### 2.2 Motores (`src/computation/engines.rs`, `analyze` ≈ línea 871)

- **`estadistico`** (y `NULL`): cada magnitud → media y `u_c` (tipo A + tipo B); cada mensurando se
  propaga **solo contra magnitudes** (los resultados no se encadenan). Con `operator_count ≥ 2`
  (motor D) las magnitudes repetidas se calculan por operador y los mensurandos se derivan una vez
  por operador.
- **`regresion_lineal`**: puntos a partir de las series (`per_point`), escalares compartidos
  difundidos a cada punto, intermedias (C), ajuste con `u` de pendiente y ordenada, derivadas por
  punto (E) y agregados (F).
- **`curva`**: igual que regresión pero sin ajuste; lista de curvas (B) con `x_log`; los resultados
  ven alias `S_max` y `T_at_S_max`.
- Fórmulas con `evalexpr` v11 (`src/computation/formula.rs`): constantes `pi`, `e`; funciones
  `math::*` (`ln`, `sqrt`, `asin`, `atan`, …); propagación numérica por diferencias centradas.

### 2.3 Partes y secciones: solo existen en JS

No hay ningún concepto de parte ni de sección en la base ni en Rust. Todo vive en
`static/constants.js`, indexado por id de práctica y por símbolo:

- `PRACTICE_GROUPS` (l. 3): P3 y P3-desfasaje son **dos prácticas** que el menú muestra como dos
  pestañas.
- `PRACTICE_PARTS` (l. 10): pestañas de `p1-estadistica` (op1/op2/op3), `p2-cc`
  (serie/paralelo/potencia) e `hidrostatica` (arq/ts).
- `PRACTICE_SECTIONS` (l. 35): por sección `{id?, title, symbols[], results[], series?}`. **Una
  sección sin `id` es siempre visible: es la única forma de expresar "dato compartido entre
  partes"** (p. ej. R1/R2/R3 en P2-CC).
- Los datos de cada parte se separan **por convención de nombre**: `_s/_p/_c` (y `_t` para
  teóricos) en P2-CC; `m{k}_{i}`, `E{k}`, `rho{k}` en Hidrostática; `T1/T2/T3`, `g1/g2/g3` en P1.
- La vista de análisis del docente (`static/analysis.js`) ignora partes y secciones.
- Rareza: en Hidrostática `g_ac` está en una sección solo de la parte 1, pero la parte 2 la usa.

### 2.4 Cómo está modelada hoy cada práctica

| Práctica | `analysis_kind` | Particularidades |
|----------|-----------------|------------------|
| `p1-estadistica` | estadístico (`operator_count` forzado a NULL) | L dato; T1, T2, T3 (opcionales 2 y 3) con réplicas; g1–g3, γ, Q finales. 3 pestañas por operador vía JS. |
| `p3-relajacion` | estadístico | 5 medidas únicas; τ teórico vs experimental. Agrupada en el menú con la siguiente. |
| `p3-relajacion-desfasaje` | regresión, x=`2*pi*f`, y=`b / math::sqrt(a*a - b*b)` | τ = `slope`. |
| `p2-cc` | curva | 17 magnitudes (series `R`, `I`; escalares por parte con sufijo); intermedia `P=I*I*R`; curva R→P; alias `P_max`, `R_at_P_max`. 3 pestañas vía JS. |
| `fluidos-1` | regresión | V, t con 2 réplicas por punto; intermedia `Q=V/t`; derivada por punto Re. |
| `viscosidad` | regresión | t con 5 réplicas por punto; derivada por punto Re. |
| `fluidos-2` | regresión | 4 agregados encadenados con `_first`/`_last`. |
| `filtros` | curva | 2 curvas con `x_log`; 3 intermedias. |
| `ca-rlc` | estadístico | 15 medidas únicas; 17 resultados con **fórmulas expandidas** (no hay encadenamiento). |
| `hidrostatica` | estadístico | 48 magnitudes; ranuras con `slot_count`; 3 determinaciones como símbolos separados + promedio; 2 pestañas vía JS. |

### 2.5 Casos especiales que bloquean la creación genérica

Todo lo que sigue está atado a un id de práctica o a un nombre de símbolo. Una práctica creada desde
la UI no se beneficiaría de nada de esto; cada ítem tiene que convertirse en dato de la definición
(o desaparecer).

**Rust**

| # | Dónde | Qué |
|---|-------|-----|
| R1 | `src/db/mod.rs::seed_practices` | Filas de `practices` (kind, ejes) solo existen en la siembra. |
| R2 | `src/db/mod.rs::seed_academic` | El curso de ejemplo habilita una lista fija de 10 ids. |
| R3 | `src/db/schema.rs` (≈ l. 635-697) | Backfills de `is_final` y `has_uncertainty` por id + símbolo en cada migración. |
| R4 | `src/practices/seed.rs` | Cada `seed_*` corre "autorreparaciones" por id + símbolo en cada arranque (p1 fuerza `operator_count = NULL`; `fix_ca_rlc_labels`; `reset_legacy_hidrostatica`; viscosidad, fluidos-2, filtros). Pueden pisar ediciones del docente sobre esos símbolos. |
| R5 | `src/db/mod.rs::seed_practices` | Baja de los ids legacy `p2-serie`, `p2-corriente-continua`, `p2-potencia`. |

**JS**

| # | Dónde | Qué |
|---|-------|-----|
| J1 | `static/constants.js` `PRACTICE_GROUPS`/`PARTS`/`SECTIONS` | Partes, secciones y datos compartidos. |
| J2 | `static/constants.js` `SERIES_LIVE_COLUMNS` + `updateSeriesLive` (`static/forms-series.js`) | Columnas calculadas en vivo por id: Q de fluidos-1, v de viscosidad, potencia de p2-cc. |
| J3 | `static/constants.js` `PRACTICES_WITHOUT_CHRONO_HELPER` | P3 sin ayudante de cronómetro. |
| J4 | `static/constants.js` `SYMBOL_FIRST_QUANTITIES` (+ `T_oc` en `forms.js`) | Magnitudes que se rotulan por símbolo en vez de por nombre. |
| J5 | `static/lib.js` `symbolHtml` + `CC_PART_SUFFIX` | Representaciones especiales (`tmedio`, `Vg`, `VRpp`, `phiX_teo`…) y recorte de sufijos de P2-CC. |
| J6 | `static/lib.js` `compareMeasuredVsTheoretical` | Empareja magnitud `X` con resultado `X_t` por convención de nombre. |
| J7 | `static/forms.js` (≈ l. 355) | Widget de cronómetro si `repeated && quantity === "tiempo"`; instrumento por defecto por regex `/cron[oó]metro/`. |

### 2.6 Huecos de la API de administración

Existe (`src/routes/mod.rs`, `practice_admin.rs`, `practices/crud.rs`): leer definición,
`analyze-preview`, cambiar tipo de análisis, fórmulas de ejes y cantidad de operadores; alta,
edición y baja de magnitudes, resultados, curvas (con `move`), intermedias, por punto y agregados;
tolerancias. Todo con `require_teacher`.

Falta:

- **crear, renombrar, duplicar, archivar o borrar una práctica**;
- partes y secciones;
- reordenar magnitudes y resultados (solo las curvas tienen `move`);
- editar `default_value`;
- **validar las fórmulas** de resultados, ejes y curvas al guardar (`validate_result`,
  `set_regression_formulas` y `validate_curve` no llaman a `check_formula`; las intermedias, por
  punto y agregados sí);
- borrar una magnitud con entregas falla por la FK sin cascade de `submission_measurements`.

### 2.7 Bugs encontrados durante el relevamiento

Verificados en `main` (`16f48c6`):

1. **La calibración de las escalas no entra al cálculo.** `load_scales`
   (`src/computation/engines.rs`) no selecciona `u_cal_pct` ni `u_cal_fixed`; con
   `#[sqlx(default)]` quedan en 0. La balanza (3 %), el densímetro y los brazos de Mohr de
   Hidrostática se calculan sin su término de calibración.
2. **Restaurar o editar una entrega de serie rompe el formulario.** `static/forms.js` usa
   `seriesRowHtml`, `wireSeriesRemove`, `updateSeriesMeans` y `seriesRowComplete`, que
   `forms-series.js` exporta pero `forms.js` no importa (solo importa `renderSeriesTable` y
   `collectSeriesPointResults`). Editar o restaurar el borrador de una entrega de regresión o curva
   tira `ReferenceError`.
3. Comentario desactualizado en `seed.rs` sobre `P_max_e`/`RP_max_e` "no editables": en realidad
   `validate_result` no valida fórmulas, así que sí son editables (y se pueden romper).

---

## 3. Modelo objetivo

### 3.1 Vista general

```
Práctica (id, nombre, descripción, estado, autor)
 ├── Versión publicada N (instantánea JSON inmutable) ← las entregas apuntan acá
 ├── Borrador (tablas relacionales editables)
 │    ├── Datos compartidos (part_id = NULL): magnitudes, resultados compartidos
 │    ├── Parte 1 (tipo de análisis + config: ejes, curvas, operadores/determinaciones)
 │    │    ├── Secciones (agrupación visual)
 │    │    └── magnitudes, intermedias, resultados, por punto, agregados, curvas
 │    └── Parte 2 …
 └── Habilitaciones en cursos (course_practices)
```

Una práctica de **una sola parte sin datos compartidos** es exactamente una práctica de hoy. Eso
permite migrar todo sin cambiar resultados.

### 3.2 Práctica y ciclo de vida

`practices` suma:

- `status`: `borrador` | `publicada` | `archivada`. Solo las publicadas aparecen en "Habilitar
  práctica" de Cursos; las archivadas siguen mostrando sus entregas pero no admiten nuevas.
- `created_by` (usuario), `created_at`, `updated_at`.
- `current_version_id` (FK a `practice_versions`).
- Las columnas de análisis (`analysis_kind`, `x_formula`, `y_formula`, `operator_count`) **se mudan
  a la parte** (ver 3.4). Durante la transición se mantienen y se leen de la primera parte.

El `id` lo genera el servidor a partir del nombre (slug único), editable solo en borrador.

### 3.3 Versionado

- `practice_versions(id, practice_id, number, definition_json, published_at, published_by,
  notes)`. `definition_json` es la **definición completa** (la misma estructura que devuelve hoy
  `GET /practices/{id}/definition`, ampliada con partes y secciones). Inmutable.
- Las tablas relacionales (`practice_quantities`, `practice_results`, …) pasan a ser **el borrador
  editable**. "Publicar" valida el borrador, lo serializa y crea la versión `N+1`.
- `submissions.practice_version_id`: la entrega queda atada a la versión vigente al crearla.
  El análisis (`analyze`, recálculo al editar, vista del docente) carga **la definición de esa
  versión**, no el borrador.
- Los ids de magnitudes y resultados son **estables entre versiones**: una magnitud que sigue
  existiendo conserva su id, así las mediciones viejas se siguen leyendo. Quitar una magnitud del
  borrador no borra la fila si alguna versión publicada la referencia (se marca `removed`).
- Cambios "cosméticos" (nombre, descripción, textos de ayuda, tolerancias, y el orden de magnitudes,
  mensurandos, partes y secciones) se pueden aplicar a la versión vigente sin crear otra, y
  queda registrado quién y cuándo (`practice_version_updates`). Las tolerancias alcanzan enseguida a
  las entregas de esa versión; los nombres y el orden se ven en el formulario, y una entrega ya
  guardada los toma recién cuando se recalcula, porque su análisis queda guardado con ella. Los estructurales (símbolos, fórmulas,
  unidades, roles, partes, tipo de análisis, pertenencia a secciones, orden de las intermedias y
  agregados, que se evalúan en secuencia, y de las curvas, porque los resultados de los extremos
  salen de los puntos de la primera) siempre crean versión. La UI lo explica antes de publicar ("esto crea la versión 4; las
  12 entregas existentes siguen con la versión 3").
- Las entregas **se quedan en su versión** (decisión 2026-11-02). Migrar entregas de la versión N
  a la N+1 queda fuera de alcance; si aparece la necesidad, va a F10.

### 3.4 Partes

`practice_parts(id, practice_id, key, title, position, analysis_kind, x_formula, y_formula,
operator_count, config_json)`.

- `analysis_kind` por parte: `estadistico` | `regresion_lineal` | `curva` (+ futuros, ver 3.10).
- `config_json` para opciones del motor que no merecen columna (p. ej. determinaciones, F9).
- Una práctica sin partes declaradas se trata como una parte implícita (compatibilidad).
- El formulario del alumno muestra una pestaña por parte; la entrega es una sola.
- La vista del docente y la comparación alumno-vs-automático se agrupan por parte.
- Validación: cada parte debe tener al menos un resultado o una curva; una parte de regresión
  necesita ejes; una de curva, al menos una curva.

### 3.5 Datos compartidos y alcance de los símbolos

Toda definición (magnitud, intermedia, resultado, por punto, agregado, curva) lleva `part_id`:

- `part_id = NULL` → **compartido**: se ingresa una sola vez (en una sección "Datos comunes"
  visible en todas las pestañas) y se calcula una sola vez.
- `part_id = X` → propio de la parte X.

Reglas de visibilidad en fórmulas:

- Una parte ve: lo compartido + lo propio.
- Lo compartido ve solo lo compartido.
- Una parte **no** ve lo de otra parte. Si hace falta, el dato se declara compartido. (Más adelante
  se puede permitir referenciar resultados de otra parte con un orden de partes explícito; no hace
  falta para ninguna práctica actual.)
- Los símbolos siguen siendo **únicos por práctica** aunque estén en partes distintas: las
  fórmulas nunca son ambiguas y las mediciones se identifican sin parte. Para que el alumno vea "R"
  en dos partes distintas se usa `display_symbol` (3.8); eso reemplaza los sufijos `_s/_p/_c`.

Motor: `analyze` calcula primero lo compartido (magnitudes compartidas → media y `u_c`;
resultados compartidos) y luego cada parte con su motor, recibiendo lo compartido como **escalares
con incertidumbre** (igual que hoy los escalares compartidos de regresión/curva).

### 3.6 Rol de cada magnitud

Un único selector "¿Cómo se ingresa?" reemplaza la combinación de flags, que se sigue guardando
igual (no cambia el motor):

| Rol en la UI | `is_given` | `has_uncertainty` | `repeated` | `per_point` | `replicas_per_point` | Disponible en |
|--------------|-----------|-------------------|-----------|-------------|----------------------|---------------|
| Dato de cátedra (valor ± U) | 1 | 1 | – | 0 | – | todas |
| Dato de cátedra sin incertidumbre / constante | 1 | 0 | – | 0 | – | todas |
| Medida única con instrumento | 0 | 1 | 0 | 0 | – | todas |
| Medida con réplicas (tipo A) | 0 | 1 | 1 | 0 | – | todas |
| Serie de datos (un valor por punto) | 0 | 1 | 0 | 1 | – | regresión, curva |
| Serie con réplicas por punto | 0 | 1 | 1 | 1 | n | regresión, curva |

Atributos adicionales por magnitud: `optional`, `default_value` (editable), `slot_count`
(renombrado en la UI como "ranura entera 1..n"), magnitud física (para sugerir instrumentos),
widget de entrada (3.8).

En una parte estadística con operadores/determinaciones (F9) se agrega "¿Se repite por
operador/determinación?".

### 3.7 Encadenamiento de resultados

Hoy un mensurando estadístico solo ve magnitudes; CA (RLC) e Hidrostática repiten fórmulas
expandidas enormes. Objetivo:

- Un resultado puede referenciar otros resultados (compartidos o de su parte).
- El servidor arma el grafo de dependencias, **rechaza ciclos** al guardar y evalúa en orden
  topológico. La propagación usa las incertidumbres de los resultados intermedios **con sus
  correlaciones**: para no subestimar `u`, la propagación se hace siempre **contra las magnitudes
  de entrada** (se compone la fórmula hacia abajo), no contra el valor ± u del resultado
  intermedio. Esto es una decisión técnica importante: mantiene la física actual (los números de
  las prácticas no cambian) y elimina la redundancia de las fórmulas.
- Lo mismo para intermedias, por punto y agregados, que ya encadenan parcialmente.

### 3.8 Propiedades de presentación (reemplazan los casos especiales de JS)

| Propiedad | Dónde | Reemplaza |
|-----------|-------|-----------|
| `display_symbol` (texto con `_` para subíndice, como hoy `symbolHtml`, o vacío = derivado del símbolo) | toda definición con símbolo | J5 (`symbolHtml` especiales, `CC_PART_SUFFIX`) |
| `label_mode`: `nombre` \| `simbolo` \| `nombre_simbolo` | magnitud | J4 (`SYMBOL_FIRST_QUANTITIES`, `T_oc`) |
| `input_widget`: `teclado` \| `cronometro` | magnitud | J3, J7 (`PRACTICES_WITHOUT_CHRONO_HELPER`, detección por `quantity === "tiempo"`) |
| `compare_with` (símbolo de la magnitud medida) | resultado | J6 (convención `X` ↔ `X_t`) |
| `show_live` | intermedia | J2 (`SERIES_LIVE_COLUMNS` por id); el cálculo en vivo lo hace el backend por fila |
| `help_text` (pasa a F7) | toda definición, parte y sección | textos de ayuda hoy fijos en la técnica |
| `section_id` (ya existe) | magnitud, resultado | J1 (`PRACTICE_SECTIONS`) |

Descartadas en F6: `input_widget: ranura` (ya lo cubre `slot_count`) y `default_instrument_id` (los
instrumentos son de cada curso y las prácticas son globales: el cronómetro preselecciona el primer
instrumento de tiempo del curso).

Secciones: `practice_sections(id, practice_id, part_id NULL, title, position)`. Una sección con
`part_id = NULL` se muestra en todas las pestañas (reemplaza "sección sin `id`").

### 3.9 Validación de la definición

Un único validador del lado servidor (reutiliza `check_formula`, `guard_symbol`) que corre al
guardar cada elemento y, completo, al publicar. Devuelve una lista de problemas con ubicación
(parte, elemento, campo) que la UI muestra en un panel:

- símbolos desconocidos o fuera de alcance (de otra parte);
- ciclos entre resultados;
- roles incompatibles con el tipo de análisis de la parte (serie en una parte estadística);
- regresión sin ejes, curva sin curvas, parte sin resultados;
- ejes que no referencian ninguna serie;
- `slot_count` sin rol de dato de cátedra, réplicas por punto sin serie, etc.;
- (futuro) chequeo dimensional de unidades (3.10).

### 3.10 Pensado para el futuro

- **Nuevos tipos de análisis.** Cada motor implementa una interfaz común en Rust
  (`trait PartEngine { fn validate(..); fn analyze(..) -> PartAnalysis }`) y declara qué roles de
  magnitud y qué elementos acepta (ejes, curvas, agregados…). La UI se arma a partir de esa
  declaración, no de `if kind == …`. Candidatos: ajuste exponencial/potencial no lineal, polinómico,
  regresión ponderada por `u_y`, histograma/estadística descriptiva.
- **Otras materias.** Catálogo de constantes por práctica o globales (con incertidumbre), catálogo
  de funciones documentado en la UI (las de `evalexpr` más `log10`, `exp`, etc.), unidades
  estructuradas con prefijos SI y **chequeo dimensional opcional** de fórmulas.
- **Otros tipos de entrada.** Texto libre, opción múltiple, adjunto/imagen (respuesta del alumno
  sin cálculo), para preguntas de la hoja de resultados.
- **Compartir.** Formato de intercambio JSON versionado (`schema_version`), autocontenido (sin ids
  internos), igual que el exportar/importar de instrumentos.
- **Alcance.** Las prácticas siguen en un **catálogo global** (decidido): se registra el autor,
  pero no hay visibilidad por curso ni por docente.

---

## 4. Cobertura de las 10 prácticas existentes

Cómo queda cada práctica en el modelo nuevo. Ninguna cambia sus números.

| Práctica | Partes | Compartido | Notas de migración |
|----------|--------|------------|--------------------|
| `p1-estadistica` | 1 estadística con 3 determinaciones (F9) mostradas **como pestañas** — hasta F9, 3 partes estadísticas Op1/Op2/Op3 | L, `t_med` | **Se mantiene una pestaña por operador** (decidido). Las pestañas dejan de ser JS. Con F9, T se declara "repetida por operador" y g se deriva por operador. |
| P3 (nueva, une `p3-relajacion` + `p3-relajacion-desfasaje`) | Parte 1 estadística (τ directo) + parte 2 regresión (desfasaje) | `Rint` y lo que usen ambas | Desaparece `PRACTICE_GROUPS`. Las dos prácticas viejas **se borran con sus entregas** (decidido: no hay datos que conservar). |
| `p2-cc` | Serie (estadística), Paralelo (estadística), Potencia (curva) | R1, R2, R3 | Los sufijos `_s/_p/_c` se reemplazan por `display_symbol`; los símbolos internos pueden quedar iguales (no hace falta renombrar). |
| `fluidos-1` | 1 regresión | – | `Q` en vivo por `show_live` en la intermedia (J2). |
| `viscosidad` | 1 regresión | – | `v` en vivo: nueva intermedia `v = dx/t` con `show_live` (hoy calculada en JS). |
| `fluidos-2` | 1 regresión | – | Sin cambios. |
| `filtros` | 1 curva (2 curvas) | – | Sin cambios. |
| `ca-rlc` | 1 estadística, una sola pestaña (decidido) | – | Fórmulas encadenadas desde F4 (`I_teo = Vg/Z`…); teórico vs experimental vía `compare_with`. |
| `hidrostatica` | Arquímedes (estadística) + Tensión superficial (estadística) | `g_ac` (hoy mal ubicado en la parte 1) | Con F9 las 3 determinaciones dejan de ser símbolos separados (`m{k}_{i}` → magnitudes repetidas por determinación). |

---

## 5. Fases

Cada fase es una o más PRs chicas sobre `main`, con título convencional, y deja todo verde
(`cargo test`, `cargo clippy`, `npm test`, `npm run test:e2e`).

### F0 — Base de seguridad ✅ HECHA

Antes de tocar el modelo, congelar el comportamiento y sacar los bugs.

**Estado (2026-11-02)**: los puntos 2 y 3 se resolvieron en #73 (más el `Re#k` que se perdía al
restaurar y el recálculo de columnas en vivo). Los puntos 1, 4 y 5 en la PR de F0:
- Golden: `tests/golden/<practica>.input.json` (mediciones por símbolo, con instrumento y escala
  del catálogo por nombre) y `<practica>.expected.json` (el `FormAnalysis` completo sin ids),
  comparados por `golden_practices_match` (`src/computation/golden_tests.rs`) con tolerancia
  relativa 1e-9. Se regeneran con `QUANTIFY_UPDATE_GOLDEN=1 cargo test golden`. Un archivo por
  práctica (no una carpeta): alcanza con un caso realista por práctica que cubra su camino
  (réplicas por punto, opcionales vacíos, ranuras, escalas calibradas).
- Validación: `result_scope` y `axis_scope` (`src/routes/practice_admin.rs`) replican los símbolos
  que liga el motor; un test verifica que las fórmulas de las 10 prácticas validan.
- Reordenar: `practices::move_item` (curvas, magnitudes y mensurandos) renumera la lista con el
  mismo desempate que la definición; botones ▲▼ en el admin. `default_value` editable ("Valor
  inicial").

1. **Tests golden por práctica.** Para cada una de las 10 prácticas, una o más entregas de ejemplo
   (las de `seed_submissions` más casos con operadores, réplicas por punto, opcionales vacíos) y el
   JSON de `analyze` esperado, guardados en `tests/golden/<practica>/*.json`. Un test de Rust
   recalcula y compara con tolerancia numérica. Toda fase posterior debe dejarlos idénticos (o
   explicar el cambio en la PR).
2. **Bug de calibración**: `load_scales` selecciona `u_cal_pct` y `u_cal_fixed`. Test que falla sin
   el arreglo. Ojo: esto **cambia** los números de Hidrostática (se corrige un error); los golden de
   Hidrostática se generan después del arreglo.
3. **Bug de restauración de series**: importar en `forms.js` lo que usa de `forms-series.js`; E2E
   que edita una entrega de `fluidos-2`.
4. **Validar todas las fórmulas al guardar**: `validate_result`, `set_regression_formulas` y
   `validate_curve` llaman a `check_formula` con el conjunto de símbolos permitido de cada motor
   (incluye alias `S_max`/`T_at_S_max` para curva). Corregir el comentario de `seed.rs`.
5. **Reordenar** magnitudes y resultados (endpoint `move`, como el de curvas) y editar
   `default_value` desde el admin.

**Archivos**: `src/computation/engines.rs`, `src/routes/practice_admin.rs`, `src/practices/crud.rs`,
`static/forms.js`, `static/practices-admin.js`, tests nuevos.
**Aceptación**: golden de las 10 prácticas en verde; los dos bugs con test.

### F1 — Práctica como entidad editable ✅ HECHA

**Estado (2026-11-02)**: hecho como se describe abajo, con estos cambios:
- **Sin `updated_at`**: solo se agregaron `status`, `created_by` y `created_at`; nada lo usaba
  todavía.
- **El id no se edita**: se genera del nombre (slug sin tildes, `-2` si se repite) y queda fijo,
  porque es la clave primaria y las tablas de la definición la referencian sin `ON UPDATE`.
- **Un solo endpoint de estado** `POST /practices/{id}/status` (`publicada` / `archivada`) en vez
  de `/archive`: publicar exige magnitudes, mensurandos (o curvas) y fórmulas que compilen.
- Una archivada sale de "Prácticas habilitadas" y del menú del alumno aunque siga habilitada;
  sus entregas se conservan. El estudiante no ve borradores ni archivadas en el catálogo.
- Limitación hasta F2/F6: duplicar una práctica no copia lo que está escrito a mano para su id
  en `static/constants.js` (pestañas de P1/P2-CC/Hidrostática, columnas en vivo).

1. Schema: `practices.status`, `created_by`, `created_at`, `updated_at`. Las 10 existentes quedan
   `publicada`.
2. API (docente y admin, `require_teacher`; catálogo global): `POST /practices` (nombre,
   descripción, tipo de análisis → crea borrador con id slug),
   `POST /practices/{id}` (nombre, descripción, id mientras sea borrador),
   `POST /practices/{id}/duplicate`, `POST /practices/{id}/archive`, `DELETE /practices/{id}`
   (solo borrador sin entregas).
3. UI (Prácticas): botón "+ Nueva práctica", "Duplicar", "Archivar"; insignia de estado en el
   listado. La ficha actual (`static/practices-admin.js`) sirve tal cual para cargar el contenido.
4. Cursos: el selector "Habilitar práctica" lista solo las `publicada`; una archivada no admite
   entregas nuevas (`user_can_submit`).
5. Quitar R1 de a poco: `seed_practices` solo inserta si la práctica no existe (ya no actualiza
   nombre ni descripción, que ahora son del docente).

**Aceptación**: el docente crea una práctica estadística simple desde cero (2 magnitudes, 1
resultado), la publica, la habilita en un curso y un alumno la entrega. Smoke E2E nuevo.
**Nota**: mientras no exista F5, una práctica con entregas se edita como hoy (sin versión). Para
el uso real conviene no editar estructuralmente prácticas con entregas hasta F5.

### F2 — Partes y secciones en la base ✅ HECHA

**Estado (2026-11-03)**: hecho, con estas diferencias:
- **Sin `part_id` por tabla**: magnitudes y mensurandos tienen solo `section_id`, y la parte de un
  elemento es la de su sección (una sección sin parte es compartida). Evita dos datos que pueden
  contradecirse; F3 deriva la parte desde la sección.
- **La tabla de la serie** va en la parte de la sección de sus magnitudes por punto (reemplaza el
  `{ id, series: true }` de JS).
- **Sin `help_text`**: ninguna sección lo usaba; se agrega cuando haga falta.
- La sección de una magnitud o mensurando se asigna con `POST …/quantities/{qid}/section` y
  `…/results/{rid}/section` (un selector en cada fila del admin), no en su formulario.
- Los ids de las partes migradas son estables (`p2-cc-serie`, `hidrostatica-arq`…). `PRACTICE_GROUPS`
  (P3) sigue en JS hasta F3.
- **Cambio visible a propósito** (punto 5): en Hidrostática `g_ac` pasa a una sección compartida
  "Datos comunes" y se ve en las dos pestañas; antes estaba solo en Arquímedes aunque la usa la
  parte 2.
- Solo son pestañas las partes que tienen alguna sección con contenido; en regresión y curva todas
  las magnitudes por punto quedan en una misma parte (el motor arma una sola serie hasta F3).

1. Schema: `practice_parts`, `practice_sections`, columnas `part_id` y `section_id` en todas las
   tablas de definición; `help_text`.
2. Migración de una sola vez (`applied_migrations`): crea las partes y secciones de
   `p1-estadistica`, `p2-cc` e `hidrostatica` a partir de `PRACTICE_PARTS`/`PRACTICE_SECTIONS`, y
   una parte implícita para el resto. En este paso **todas las partes de una práctica comparten su
   `analysis_kind`** (el motor no cambia todavía).
3. `GET /practices/{id}/definition` devuelve `parts` y `sections`; CRUD de partes y secciones;
   selector de parte/sección en cada alta del admin.
4. Frontend: `renderPartTabs`, `applyPartVisibility` y `groupBySections` leen la definición; se
   borran `PRACTICE_PARTS` y `PRACTICE_SECTIONS` (J1 salvo `PRACTICE_GROUPS`).
5. Corregir `g_ac` de Hidrostática a sección compartida.

**Aceptación**: golden idénticos; los formularios de P1, P2-CC e Hidrostática se ven igual que
antes (capturas de `visual-forms.mjs`); una práctica nueva puede tener pestañas.

### F3 — Análisis por parte ✅ HECHA

**Estado (2026-11-03)**: hecho en #91, #92 y la PR del admin por parte. Estas son las diferencias con
lo planeado:
- **Una sola parte con serie.** Cada parte elige su tipo (`practice_parts.analysis_kind`), pero
  solo una puede ser de regresión o curva. Es la que lleva las magnitudes por punto, las
  intermedias, las curvas, el ajuste, las derivadas por punto y los agregados. Las demás son
  estadísticas. Las 10 prácticas entran en este modelo. Cuando una práctica necesite dos series,
  hay que agregar `part_id` a las intermedias, curvas, derivadas por punto y agregados.
- **Ejes y operadores siguen en la práctica.** Con una sola parte con serie, los ejes de
  `practices` son los de esa parte. Los operadores solo aplican si la práctica es estadística. El
  tipo de `practices` es un espejo: es el de la parte con serie, o `estadistico` si no hay
  ninguna. Elegir el tipo de una parte lo actualiza.
- **Sin parte elegida.** Si ninguna parte tiene tipo guardado (prácticas sembradas o anteriores),
  la parte con serie es la que tiene las magnitudes por punto, o la primera si la serie no tiene
  sección. Cuando el docente elige el tipo de una parte, se guarda el de todas, y desde ahí solo
  cuenta lo elegido: una parte nueva nace estadística, y borrar la parte con serie deja la
  práctica estadística. Las magnitudes por punto tienen que estar en la parte con serie (se valida
  en cada cambio).
- **JSON sin adaptador.** `FormAnalysis` suma `parts[]`, que no se serializa si está vacío. Con
  partes, el nivel superior es lo compartido. Las prácticas sin partes no cambian.
- **Alcance de las fórmulas.**
  - Una parte ve lo compartido y lo suyo; lo compartido, solo lo compartido.
  - Un mensurando **sin sección** va a la parte de la que dependen sus símbolos (magnitudes,
    `slope`, alias de curva, intermedias, o el hecho de que lo usen las derivadas por punto). Si
    no depende de ninguna, es compartido.
  - Uno puesto a propósito en una sección compartida sigue siendo compartido.
  - Una migración de una sola vez pasó a su parte los mensurandos compartidos que usaban datos de
    una sola parte: `Q` de P1 pasó a Op1.
- **Motor tolerante.** Si un mensurando, una derivada por punto o un agregado no se puede evaluar,
  o si una parte entera falla, queda un aviso y no se pierde la entrega. El admin sigue
  rechazando los cambios que rompen fórmulas: crear, renombrar o mover elementos, secciones y
  partes pasa por `guard_change`.
- **P3 conserva su id** (`p3-relajacion`). La migración conserva la habilitación en cursos, los
  subgrupos y las mesas, y borra las entregas de las dos prácticas viejas.

1. ~~Mover `analysis_kind`, `x_formula`, `y_formula`, `operator_count` a `practice_parts`; las
   columnas de `practices` quedan como espejo de la primera parte hasta retirarlas.~~ Hecho
   distinto: solo `analysis_kind` pasó a la parte; las columnas de `practices` son el espejo de
   **la parte con serie** (ver Estado arriba).
2. Motor: `analyze` calcula lo compartido y después cada parte con su motor
   (`compute`/`compute_regresion`/`compute_curva`), recibiendo lo compartido como escalares. El
   resultado pasa a ser `FormAnalysis { shared, parts: [PartAnalysis] }`; se mantiene un
   adaptador para el JSON viejo mientras la UI migra.
3. P2-CC: Serie y Paralelo pasan a partes estadísticas y Potencia a curva (hoy todo es una curva
   con escalares). Golden idénticos.
4. **P3 unificada**: práctica nueva de dos partes (estadística + regresión) con lo compartido; las
   dos prácticas viejas se borran con sus entregas (migración de una sola vez). Se borra
   `PRACTICE_GROUPS` y su lógica en `forms.js`/`navigation.js`.
5. Vista del docente (`static/analysis.js`) y comparación alumno-vs-automático agrupadas por parte.
6. UI admin: el tipo de análisis, ejes y operadores se configuran por parte.

**Aceptación**: golden idénticos (los golden de P3 se reescriben para la práctica nueva y deben
dar lo mismo que las dos viejas sobre los mismos datos); E2E de una práctica nueva de dos partes
con distinto análisis.

### F4 — Encadenamiento y resultados compartidos ✅ HECHA

**Estado (2026-11-03)**: hecho en dos PRs. Los detalles:
- **Composición sin reescribir texto.** Cada mensurando se propaga con una sola llamada a
  `propagate` sobre sus hojas (magnitudes, `slope`/`intercept`, escalares de curva). El closure
  evalúa las dependencias en orden topológico. La `u` es la de la fórmula expandida.
- **Sin `part_id` en los resultados**, igual que en F2. Un mensurando sin sección que usa uno de
  una parte va a esa parte. Los compartidos se recalculan en cada parte y se sacan de su salida.
- **Motor tolerante.** Un ciclo, o depender de un mensurando que no compila, deja un aviso y
  saltea solo los afectados. El admin rechaza los ciclos con `guard_change`.
- **Solo mensurandos.** Las intermedias, las derivadas por punto y los agregados siguen
  encadenando por posición, como antes.
- **Fórmulas simplificadas** de `ca-rlc` (`Z`, `I_teo`, `V*_teo`, `phi*_teo`) y de `hidrostatica`
  (los tres promedios). Una migración de una sola vez cambia solo las filas que tienen la
  fórmula sembrada vieja. En los golden cambia el campo `formula`; `value` y `u` dan iguales
  hasta 1e-9 relativo (la `u` de `phiC_teo` difiere en ~1e-11 y la de `phiL_teo` en ~1e-10:
  ruido de la derivada numérica al cambiar cómo está escrita la fórmula).

1. Resultados que referencian resultados (compartidos o de su parte), con grafo de dependencias,
   rechazo de ciclos y orden topológico; propagación compuesta contra las magnitudes de entrada
   (3.7).
2. Resultados compartidos (`part_id = NULL`) visibles en todas las partes.
3. Simplificar las fórmulas de `ca-rlc` (Z, I_teo, VR_teo…) e `hidrostatica` usando
   encadenamiento, con migración de una sola vez que solo toque filas en su estado sembrado.

**Aceptación**: golden idénticos tras simplificar las fórmulas; test de ciclo rechazado; test de
correlación (un resultado que usa dos resultados que dependen de la misma magnitud propaga igual
que la fórmula expandida).

**Plan de PRs**

1. **`feat(analisis)`: resultados que usan otros resultados (F4, 1/2).**
   - **Motor** (`derive_results` en `engines.rs`): grafo de los símbolos de resultado de cada
     fórmula y orden topológico. Un ciclo deja un aviso y saltea los resultados involucrados.
     Cada resultado se propaga con una sola llamada a `propagate` sobre sus hojas (magnitudes,
     `slope`/`intercept`, escalares de curva). El closure evalúa las dependencias en orden y
     después la fórmula propia, sin reescribir texto, así se conservan las correlaciones. Sirve
     para estadística, regresión, curva y operadores.
   - **Partes** (`parts.rs`): un resultado que usa uno de la parte X va a X (`inferred_part`
     resuelve en orden topológico). `part_view` incluye los resultados compartidos, y `analyze`
     los saca de la salida de la parte, como hace con las magnitudes compartidas.
   - **Validación** (`practice_admin.rs`): `result_scope_in` suma los resultados visibles en cada
     view (los compartidos en el compartido; en una parte, los suyos más los compartidos).
     `formula_problems` rechaza ciclos, así que crear, editar, mover o borrar un resultado pasa
     por `guard_change`.
   - **Admin**: un texto de ayuda en el alta de mensurandos, que dice que se pueden usar otros
     mensurandos.
   - **Tests**: ciclo rechazado; correlación (`A + B` con `A = 2x` y `B = 3x` da la misma `u`
     que `5x`); un compartido usado en una parte sin quedar repetido; inferencia de parte; borrar
     un resultado usado. Golden sin cambios.
2. **`refactor(practicas)`: fórmulas de `ca-rlc` e `hidrostatica` con encadenamiento (F4, 2/2).**
   - **Fórmulas nuevas en la siembra**:
     - `ca-rlc`: `Z`, `I_teo = Vg/Z`, `VR/VC/VL_teo = I_teo*R/XC/XL`, `phiR_teo` sobre `XL-XC`, y
       `phiC/phiL_teo = phiR_teo ∓ 90`;
     - `hidrostatica`: `E_medio`, `rho_medio` y `gamma_medio` como promedio de los tres
       resultados.
   - **Migración de una sola vez**: un `UPDATE … WHERE formula = <vieja sembrada>`, así no pisa
     las ediciones del docente.
   - **Golden**: se regeneran. Con un script se comprueba que solo cambia `formula` y que `value` y
     `u` dan iguales.
   - **Doc**: F4 se marca como hecha.

### F5 — Versionado ✅ HECHA

1. Schema: `practice_versions`, `practices.current_version_id`,
   `submissions.practice_version_id`. Migración: versión 1 de cada práctica con la definición
   actual; todas las entregas apuntan a su versión 1.
2. Relajar la FK `submission_measurements.quantity_id` (reconstruir la tabla en SQLite, en
   transacción) y marcar `removed` en vez de borrar magnitudes referenciadas por versiones.
3. `analyze` y todas las lecturas de una entrega usan la definición de su versión. El borrador
   solo lo ven el editor y `analyze-preview`.
4. Flujo en la UI: "Cambios sin publicar" → "Publicar versión N+1" con resumen de diferencias y
   cantidad de entregas que siguen en la versión anterior; cambios cosméticos sin versión nueva
   (3.3). Las entregas no se migran entre versiones.

**Aceptación**: editar estructuralmente una práctica con entregas no altera el análisis de las
entregas existentes (test); una entrega nueva usa la versión nueva; golden idénticos.
**Hito**: a partir de acá es seguro que el docente edite en producción prácticas con entregas.

**Plan de PRs**

Lo que se relevó en el código (2026-11-03):
- **Hoy todo lee la definición en vivo.**
  - `update_form_submission` vuelve a correr `analyze` contra la definición actual.
  - La vista de la entrega (`submissions.js` → `analysis.js`) y el formulario de edición
    (`forms.js`) traen `/practices/{id}/definition`.
  - `submission_detail` lee las tolerancias de `practice_results`.
  - `is_point_based_practice` lee `practices.analysis_kind`.
- **Borrar una magnitud con mediciones da un 500** por la FK de `submission_measurements`.

Decisiones y desvíos respecto de los puntos 1–4:
- **El snapshot es el `PracticeDefinition` serializado.**
  - Hay que sumar `Deserialize` a sus structs: `PracticeDefinition`, `PracticePart`,
    `PracticeSection`, `PracticeCurve`, `PracticeIntermediate`, `PracticePointResult`,
    `PracticeAggregate`, `PracticeQuantity` y `PracticeResult`.
  - `effective_kind` y los símbolos de cada sección ya salen calculados de `definition()`.
- **`analyze` se parte en dos:**
  - `analyze_definition(pool, &def, …)` calcula sobre una definición dada;
  - `analyze(pool, practice_id, …)` queda como envoltorio que calcula sobre el borrador (golden,
    tests).
- **No se reconstruye `submission_measurements`** (punto 2).
  - Una magnitud que alguna versión usa no se borra: se marca `removed`.
  - Su símbolo se renombra (`{símbolo}~{id}`) para liberar el `UNIQUE(practice_id, symbol)`.
  - La FK sigue siendo válida, y la versión conserva el símbolo original en su JSON.
- **El alumno ve la versión vigente, no el borrador.** El formulario nuevo, `analyze-preview` y
  `/practices/{id}/definition` para estudiantes leen la versión vigente; el borrador lo ve solo el
  editor.

1. **`feat(practicas)`: versiones publicadas y entregas atadas a su versión (F5, 1/3).**
   - **Schema:**
     - tabla `practice_versions(id, practice_id, number, definition_json, published_at, published_by)`;
     - columnas `practices.current_version_id` y `submissions.practice_version_id`.
   - **Migración de una sola vez:**
     - versión 1 de cada práctica publicada o archivada, con su definición actual;
     - todas sus entregas apuntan a esa versión 1.
   - **Crear versiones:**
     - publicar un borrador crea la versión 1;
     - `POST /practices/{id}/versions` crea la N+1 desde el borrador, con los chequeos de
       `publish_problems`;
     - en el admin, un botón simple "Publicar versión N+1".
   - **Entregas:**
     - `create_form_submission` guarda la versión vigente;
     - todo lo que lee una entrega usa la definición de su versión: `update_form_submission`,
       `validate_student_results`, `set_student_results`, las tolerancias de `submission_detail`
       e `is_point_based_practice`.
   - **Frontend:**
     - endpoint nuevo `GET /submissions/{id}/definition`, que usan `submissions.js` y `forms.js`
       al editar una entrega;
     - `/practices/{id}/definition` y `analyze-preview` devuelven la versión vigente a los
       estudiantes.
   - **Tests:**
     - editar estructuralmente una práctica con entregas no cambia la vista ni la edición de una
       entrega vieja;
     - después de publicar la N+1, una entrega nueva la usa;
     - la migración de una base con entregas;
     - golden idénticos.
2. **`feat(practicas)`: magnitudes retiradas en vez de borradas (F5, 2/3).**
   - **Borrar una magnitud:** si una versión la usa, se marca `removed` y se le libera el símbolo
     renombrándola a `{símbolo}~{id}`; si no, se borra como hoy. Se acaba el 500. Si solo la usan
     mediciones de entregas sin versión, se rechaza con un 400 hasta publicar la práctica.
   - **Filtrar las retiradas** en `definition()` y en `duplicate_practice`.
     `symbol_taken_in_practice` no las filtra porque el renombre ya libera el símbolo.
   - **Tests:**
     - la entrega vieja sigue mostrando la magnitud retirada;
     - el símbolo se puede reusar;
     - duplicar no copia las retiradas.
3. **`feat(practicas)`: cambios sin publicar en el admin (F5, 3/3).**
   - **Aviso de "Cambios sin publicar"** cuando el borrador difiere de la versión vigente. Se
     comparan los JSON sin los campos cosméticos.
   - **Resumen de diferencias:** magnitudes y mensurandos agregados, quitados y con fórmula
     cambiada; partes; tipo de análisis.
   - **Cantidad de entregas** que quedan en versiones anteriores.
   - **Probar el borrador antes de publicar:** el docente puede ver el formulario y calcular la
     vista previa con el borrador, sin crear una versión ni guardar una entrega. Hoy el formulario
     y `analyze-preview` usan siempre la versión publicada, así que la única forma de ver un cambio
     funcionando es publicarlo.
   - **Cambios cosméticos:** si solo cambian nombres, descripción, tolerancias, títulos o el orden,
     "Publicar" actualiza la versión vigente sin crear otra y registra quién y cuándo (3.3).
   - **E2E de aceptación:**
     1. el alumno entrega;
     2. el docente cambia una fórmula y publica la versión 2;
     3. la entrega vieja muestra los mismos números;
     4. una entrega nueva usa la fórmula nueva.
   - **Doc:** F5 queda marcada como hecha.

**Ojo para las fases siguientes**: las migraciones de las siembras (como la de F4) solo tocan el
borrador. Si una cambia números de una práctica publicada, tiene que crear la versión siguiente.

**Regla desde 2026-11-04: arrancar no modifica las definiciones.** `seed_definitions` corre una sola
vez (`applied_migrations`, `2026-11-definiciones-sembradas`) y los backfills de `schema.rs` también:
después el servidor toma lo que está en la base. Un cambio futuro a una práctica sembrada ya no se
aplica editando un `seed_*`: tiene que ser una migración de una sola vez. Esto resuelve R3 y R4 de la
sección 2.5. En una base sin usuarios, `seed_initial_accounts` crea en una transacción el admin y, solo con
`SEED_DEMO_DATA=true` (dev, E2E), el docente y el estudiante de prueba, la inscripción y las entregas de prueba. Producción
arranca con el admin solo (`SEED_ADMIN_PASSWORD` es obligatoria salvo con `SEED_DEMO_DATA=true`) y reiniciar no
recrea nada. Siguen corriendo en cada arranque
`seed_practices`, `seed_course` y `seed_instruments`, que se retiran con las siembras en F8.

### F6 — Sin casos especiales en el frontend ni en las siembras

1. Propiedades de presentación (3.8) en schema, API y admin.
2. Migración de una sola vez que traduce cada caso especial a datos: J2 (`show_live`), J3/J7
   (`input_widget`), J4 (`label_mode`), J5 (`display_symbol`), J6 (`compare_with`).
3. Borrar `SERIES_LIVE_COLUMNS`, `PRACTICES_WITHOUT_CHRONO_HELPER`, `SYMBOL_FIRST_QUANTITIES`,
   `CC_PART_SUFFIX`/`CC_BASE_SYMBOLS` y los casos especiales de `symbolHtml`, y la rama por id de
   `updateSeriesLive` (el cálculo en vivo pasa al backend).
4. ✅ R3: los backfills por id de `schema.rs` se convierten en migraciones de una sola vez. Ya está
   hecho: `backfill_result_flags` corre una sola vez.

**Aceptación**: `grep` de ids de práctica en `static/` y en `src/` (fuera de siembras/migraciones)
devuelve vacío; capturas de `visual-forms.mjs` y `visual-teacher.mjs` sin cambios visibles.

**Plan de PRs**

Lo que se relevó en el código (2026-11-05):
- **J2–J7 siguen como los describe la sección 2.5.** En Rust no quedan ids de práctica fuera de
  siembras, migraciones y tests: todo lo que falta está en `static/`.
- **`help_text` no reemplaza ningún caso especial**: pasa a F7. `section_id` ya existe.
- **Las vistas de alumnos y de entregas viejas leen el JSON de su versión**
  (`practice_versions.definition_json`), no el borrador. Si una migración solo toca el borrador, al
  borrar el JS se pierde la presentación en las versiones publicadas.
- **`run_seeds` solo corre en bases nuevas.** Una migración agregada ahí nunca corre en una base
  existente.
- **`analyze-preview` no sirve para las columnas en vivo**: necesita al menos dos filas completas y
  no devuelve los valores de los intermedios por punto.
- **No hay comparación de capturas contra una referencia**: `tests/e2e/artifacts/` está ignorado y
  los E2E solo comprueban el DOM.

Decisiones:
- **Todas las propiedades nuevas son de presentación**: van a `COSMETIC_KEYS`
  (`src/practices/changes.rs`). Cambiarlas actualiza la versión vigente sin crear otra.
- **Cada migración actualiza el borrador y el JSON de las versiones**, para que la práctica no quede
  con "Cambios sin publicar". Va en `seed_definitions` (`src/practices/seed.rs`), después del bloque
  de `2026-11-definiciones-sembradas` y antes de `ensure_versions`, en una transacción, y solo toca
  filas en estado sembrado.
- **Columnas en vivo desde los intermedios.** El motor ya evalúa cada intermedio réplica por réplica
  y después promedia (`point_intermediate`, `src/computation/engines.rs`). Se expone ese cálculo:
  fluidos-1 sigue mostrando Q_1, Q_2 y Q_medio, ahora desde el intermedio `Q = V/t`.
- **Viscosidad gana el intermedio `v = dx/t`** y el eje y pasa a usarlo. Es un cambio estructural:
  viscosidad queda en la versión 2, con los mismos números.
- **Cronómetro**: el widget no cambia. Cuándo aparece lo decide `input_widget` de cada magnitud en vez
  de la regla fija. El instrumento preseleccionado es el primero de tiempo del curso.
- **`compare_with` guarda el símbolo de la magnitud, no su id**, porque duplicar una práctica
  regenera los ids.

Cada PR de implementación sigue el mismo patrón:
- columna nueva con `add_column_if_missing` y campo con `#[serde(default)]`, para que los JSON de las
  versiones viejas sigan leyéndose;
- validación en `practice_admin.rs` y campo en el formulario de `practices-admin.js`;
- los literales de los structs que haya que tocar pasan a `..Default::default()`;
- test de la migración que borra su fila de `applied_migrations` y la vuelve a correr;
- golden idénticos, aserciones nuevas en los E2E y capturas de antes y después comparadas a ojo.

1. **`feat(practicas)`: símbolo a mostrar configurable (F6, 1/4).**
   - **`display_symbol`** en magnitudes, resultados, intermedios, resultados por punto y agregados.
     Vacío = se deriva del símbolo con la regla genérica de `symbolHtml`.
   - **Migración**: traduce los especiales de `symbolHtml` (`tmedio`, `T_oc`, `Vg`, `RA`, `*pp`,
     `*_teo`/`*_exp`, `phi*`) y los sufijos `_s`/`_p`/`_c` de P2-CC.
   - **Frontend**: al cargar una definición se arma un mapa símbolo → `display_symbol` que usa
     `symbolHtml`, así no cambian sus ~35 llamados. Se borran los especiales, `CC_PART_SUFFIX` y
     `CC_BASE_SYMBOLS` de `static/lib.js`.
   - **Tests**: los de `symbolHtml` en `tests/lib.test.js` pasan a cubrir el mapa.
2. **`feat(practicas)`: rótulo y entrada de cada magnitud (F6, 2/4).**
   - **`label_mode`**: `nombre` \| `simbolo` \| `nombre_simbolo` (este último es el caso de `T_oc`).
   - **`input_widget`**: `teclado` \| `cronometro`. Con `cronometro`, una magnitud con varias lecturas
     muestra el widget y una con una sola, el ayudante. En series, el ayudante aparece si alguna
     magnitud lo tiene.
   - **Migración**: traduce lo que se ve hoy, sin ids de práctica. `cronometro` en las magnitudes de
     tiempo que no son dato dado y tienen réplicas o están en una práctica que no es de serie: P3 y
     fluidos-2 quedan en `teclado`. `simbolo` en los símbolos de `SYMBOL_FIRST_QUANTITIES`, también en
     filtros; `nombre_simbolo` en `T_oc`.
   - **Se borran** `PRACTICES_WITHOUT_CHRONO_HELPER`, `SYMBOL_FIRST_QUANTITIES`, la rama de `T_oc` en
     `quantityNameHtml` y los imports muertos de `forms.js`. La regex `/cron[oó]metro/` ya no decide si
     aparece el cronómetro; queda solo para preseleccionar el instrumento por defecto del widget.
3. **`feat(practicas)`: comparación medido vs teórico configurable (F6, 3/4).**
   - **`compare_with`** en resultados: el símbolo de una magnitud de la práctica, validado. Renombrar
     la magnitud actualiza la referencia.
   - **Migración**: los `VR*_t` de P2-CC apuntan a su `VR*`.
   - **Frontend**: `compareMeasuredVsTheoretical` empareja por `compare_with` en vez del sufijo `_t`.
4. **`feat(practicas)`: columnas en vivo desde la definición (F6, 4/4).**
   - **`show_live`** en intermedios.
   - **`POST /practices/{id}/live-values`**, con los mismos permisos que `analyze-preview`. Evalúa
     cada fila por separado, sin ajuste, así que anda desde la primera fila. Devuelve, por fila, el
     valor de cada intermedio con `show_live` en cada réplica y su promedio; una fila incompleta
     vuelve vacía.
   - **Migración**: `show_live` en `P` (P2-CC) y `Q` (fluidos-1). En viscosidad agrega `v = dx/t`, pasa
     el eje y a `v`, la marca y publica la versión 2.
   - **Frontend**: `updateSeriesLive` llama al endpoint con debounce. Se borran `SERIES_LIVE_COLUMNS`,
     las ramas por id, `fluidosCaudalLiveValue`, `viscosidadVelocityLiveValue`, y `pointPower` y
     `flowRate` si quedan sin uso.
   - **E2E**: la potencia sigue dando 25 y fluidos-1 muestra Q_1, Q_2 y Q_medio.
   - **Doc**: F6 queda marcada como hecha.

### F7 — Editor completo en la UI

Sobre la ficha actual (bloques apilados de `practices-admin.js`, #69):

1. **Asistente de creación**: datos generales → partes (agregar, ordenar, tipo de análisis) →
   datos compartidos → contenido de cada parte (magnitudes con el selector de rol 3.6,
   intermedias, resultados, curvas/ejes) → secciones → revisar y publicar.
2. **Panel de validación** siempre visible (3.9), con enlaces al elemento con problema.
3. **Editor de fórmulas** con autocompletado de símbolos visibles en ese alcance, catálogo de
   funciones y vista previa del símbolo renderizado.
4. **Vista previa "ver como alumno"**: el formulario real del alumno sobre el borrador, con datos de
   prueba y resultados vía `analyze-preview`; botón "rellenar con datos de ejemplo".
5. Ordenar por arrastre (o subir/bajar) partes, secciones, magnitudes y resultados.
6. **`help_text`** (3.8) en la definición, las partes y las secciones, con su campo en el asistente.

**Aceptación**: crear desde la UI, sin ayuda, una réplica funcional de Fluidos I (regresión con
réplicas por punto, intermedia, derivada por punto) y de Hidrostática (dos partes, compartidos);
E2E que recorre el asistente.

### F8 — Plantillas y compartir

1. Formato de intercambio JSON con `schema_version` (definición completa, sin ids internos).
2. `GET /practices/{id}/export`, `POST /practices/import` (crea un borrador; resuelve choques de id
   y símbolos), como el exportar/importar de instrumentos.
3. Las 10 prácticas pasan a ser plantillas JSON en el repo (`templates/practices/*.json`) que se
   importan una sola vez en bases nuevas. Se retiran las funciones `seed_*` de `seed.rs` y todas
   sus autorreparaciones (R4, R5); las bases existentes ya tienen los datos.
4. `seed_academic` (R2) habilita en el curso de ejemplo lo que se importó.
5. UI: "Importar práctica" y "Exportar", y "Nueva desde plantilla".

**Aceptación**: exportar e importar cada una de las 10 prácticas produce golden idénticos; una base
nueva arranca con las 10 desde plantillas; `seed.rs` ya no contiene definiciones.

### F9 — Determinaciones repetidas con promedio

Generaliza el motor D (operadores) y el patrón de Hidrostática (3 determinaciones independientes,
derivado por determinación y promedio con propagación):

1. Una parte estadística declara `determinations: { count, label, average: bool, display }` en
   `config_json`, con `display` = `pestanas` (una pestaña por determinación, como P1 hoy) o
   `bloques` (todas en la misma pestaña, como Hidrostática).
2. Cada magnitud se marca "repetida por determinación" o "común"; los resultados se derivan por
   determinación y, si `average`, se agrega el promedio con incertidumbre propagada desde las
   determinaciones (lo que pide la técnica de Hidrostática).
3. Migrar P1 (3 operadores, sin promedio, **en pestañas**) e Hidrostática (3 determinaciones con
   promedio, en bloques) a este modelo.

**Aceptación**: golden idénticos para P1 e Hidrostática con el modelo nuevo; Hidrostática pasa de
48 magnitudes a ~16.

### F10 — Futuro

Sin orden fijo; cada ítem se planifica cuando haya un caso real. **Química es la candidata más
probable**, así que lo que la habilita va primero:

**Química (prioritario dentro de F10)**

- **Unidades compuestas y de concentración.** Hoy `unit` es texto libre y el formulario solo
  convierte prefijos SI. Pasar a unidades estructuradas (base + prefijo + exponentes) con un
  catálogo que incluya `mol`, `L`/`mL`, `mol/L` (M, mM), `g/mol`, `g/L`, `ppm`/`ppb`, `% m/m`,
  `% m/v`, `% v/v`, `°C`/`K`. Conversión automática al ingresar y al mostrar (p. ej. el alumno
  carga mL y la fórmula recibe L).
- **Chequeo dimensional opcional** de fórmulas al guardar (avisa si `c = n/V` no da mol/L), para
  detectar errores de unidades en el editor.
- **Funciones y constantes.** `math::log10` y `math::exp` documentadas en el editor (pH =
  `-math::log10(c_H)`), y un catálogo de constantes con incertidumbre (R, N_A, F, masas molares
  de uso común) que se pueden agregar a una práctica como dato de cátedra con un clic.
- **Curvas de calibración.** Ya cubiertas por `regresion_lineal` (p. ej. absorbancia vs
  concentración, Beer-Lambert); falta la **interpolación inversa** como resultado (concentración
  de una muestra a partir de su absorbancia, con incertidumbre de la recta), que es un agregado
  con `slope`/`intercept` y se puede expresar con el modelo actual.
- **Material volumétrico** (bureta, pipeta, matraz) como instrumentos: ya entra en el catálogo con
  `apreciacion` o tolerancia de clase; validar con el primer caso real.

**General**

- Interfaz común de motores (`PartEngine`, 3.10) y primer motor nuevo (ajuste exponencial no
  lineal o regresión ponderada).
- Migración opcional de entregas entre versiones (hoy descartada: se quedan en su versión).
- Tipos de entrada sin cálculo (texto, opción, imagen) para preguntas de la hoja.
- Biblioteca compartida de prácticas entre instalaciones.

---

## 6. Orden, dependencias y riesgos

```
F0 → F1 → F2 → F3 → F4
            └────→ F5 → F6 → F7 → F8 → F9 → F10
```

- **F0 es obligatoria primero**: sin golden no hay forma de saber si F2–F9 cambiaron números.
- **F1 da valor inmediato** (crear prácticas simples) y no depende de nada más.
- **F5 (versionado) antes de F7 (editor completo)**: el editor invita a cambiar estructuras y, sin
  versiones, esos cambios recalculan distinto las entregas viejas. Hasta F5, editar en producción
  una práctica con entregas se hace con cuidado (como hoy).
- **F6 después de F3**: muchas propiedades de presentación dependen de que las partes existan.
- **F8 retira las siembras**: recién cuando las plantillas reproducen las 10 prácticas con golden
  idénticos.

**Riesgos**

| Riesgo | Mitigación |
|--------|------------|
| Cambiar números de prácticas en uso | Golden (F0) en cada PR; migraciones de una sola vez que solo tocan filas en estado sembrado. |
| Migraciones de SQLite con tablas reconstruidas (FK) | No hay datos de producción que conservar: cuando simplifica, la migración borra y vuelve a sembrar. Igual corren en transacción. |
| Correlaciones al encadenar resultados (F4) | Propagación compuesta contra entradas, con test de equivalencia contra la fórmula expandida. |
| El editor se vuelve inmanejable | Asistente por pasos, panel de validación y vista previa como alumno (F7); duplicar práctica como forma principal de crear. |
| Autorreparaciones de siembras pisando ediciones del docente | Se eliminan en F8; hasta entonces, no editar los símbolos que tocan (listados en R4). |

---

## 7. Decisiones del docente

Respondidas el 2026-11-02 (incorporadas en la [sección 1](#1-objetivo-y-decisiones) y en las fases):

1. **Entregas viejas de P3** → no hay datos de producción; se borran.
2. **Quién crea y publica** → docente y admin.
3. **Alcance** → catálogo global, para reutilizar prácticas entre cursos.
4. **P1** → se mantienen las pestañas por operador.
5. **Versionado** → las entregas se quedan en su versión.
6. **Otras materias** → todavía no hay un caso concreto; la candidata es química (concentraciones,
   unidades compuestas, pH).
7. **CA (RLC) en pestañas** → se deja como está: **una sola pestaña**. (La pregunta era si separar
   en una pestaña por componente las mediciones de Lissajous sobre la resistencia, el condensador
   y la bobina — `a_R/b_R`, `a_C/b_C`, `a_L/b_L` → `phiR`, `phiC`, `phiL` —, con los datos del
   circuito compartidos. Se puede revisar más adelante sin cambiar el modelo.)
