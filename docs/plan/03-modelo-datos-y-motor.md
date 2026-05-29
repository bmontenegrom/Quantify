# 03 — Modelo de datos, API y motor de incertidumbres

Diseño técnico para soportar: catálogo de instrumentos, definición de prácticas con
magnitudes, carga por formulario y cálculo de incertidumbres (tipo A/B/combinada/expandida
y propagación). Compatible con SQLite y con el patrón actual de `db.rs` (migraciones
idempotentes + `add_column_if_missing`).

## 1. Nuevas tablas

### Catálogo de instrumentos (gestionable por docente)

```sql
CREATE TABLE IF NOT EXISTS instruments (
    id          TEXT PRIMARY KEY,
    course_id   TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,  -- catálogo por curso
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('analogico','digital')),
    quantity    TEXT NOT NULL,           -- magnitud: longitud, masa, tiempo, voltaje, corriente...
    unit        TEXT NOT NULL,           -- unidad base (m, kg, s, V, A...)
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS instrument_scales (
    id             TEXT PRIMARY KEY,
    instrument_id  TEXT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    label          TEXT NOT NULL,        -- "0-25 mm", "200 mV", "20 V"...
    full_scale     REAL,                 -- valor máximo de la escala (fondo de escala)
    step           REAL NOT NULL,        -- resolución (digital) o menor división (analógico)
    appreciation   REAL,                 -- analógico: apreciación efectiva (default = step)
    internal_res   REAL,                 -- resistencia interna (P2; ohm). NULL si no aplica
    internal_res_u REAL,                 -- incertidumbre de la resistencia interna (p. ej. ±10 en 1002±10)
    -- step: resolución (digital), menor división (analógico) o VOLTS/DIV (osciloscopio)
    -- Modelo de incertidumbre tipo B de la escala:
    b_model        TEXT NOT NULL DEFAULT 'resolucion'
                     CHECK(b_model IN ('resolucion','apreciacion','fabricante')),
    -- Solo para b_model = 'fabricante' (tester y osciloscopio): U_spec = pct*|v| + coef*step + fijo
    spec_pct_reading REAL,               -- % del valor leído (3.0 osc; 1.0/2.0 tester)
    spec_step_coeff  REAL,               -- coeficiente que multiplica step (5 = "5 dgt"; 0.1 osc)
    spec_fixed       REAL,               -- término fijo en unidad base (0.001 V = 1 mV en osc; 0 tester)
    unit           TEXT NOT NULL,
    position       INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL
);
```

La incertidumbre tipo B de una lectura depende del `b_model` de la escala:
- `resolucion` (digital simple: cronómetro, balanza digital) → `u_B = step / (2*sqrt(3))`  (rectangular)
- `apreciacion` (analógico: regla, calibre, aguja) → `u_B = appreciation / sqrt(6)`  (triangular; default `appreciation = step`)
- `fabricante` (tester **y osciloscopio**) → depende del **valor leído**:
  ```
  U_spec = (spec_pct_reading/100)*|valor| + spec_step_coeff*step + spec_fixed   # U expandida (k=2)
  u_B    = U_spec / 2                                                            # confirmado por el docente
  ```

> Por defecto, `kind='digital'` usa `b_model='resolucion'` y `kind='analogico'` usa
> `'apreciacion'`; testers y osciloscopios se cargan con `b_model='fabricante'`. Para el
> osciloscopio la tipo A es despreciable (solo tipo B) según la técnica del curso.

### Definición de prácticas (magnitudes y mensurandos)

Amplía `practices` con el tipo de análisis y agrega las magnitudes:

```sql
ALTER TABLE practices ADD COLUMN analysis_kind TEXT;  -- 'estadistico' | 'regresion_lineal' | 'relajacion_exponencial'

-- Variables de entrada que el estudiante mide
CREATE TABLE IF NOT EXISTS practice_quantities (
    id           TEXT PRIMARY KEY,
    practice_id  TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
    symbol       TEXT NOT NULL,          -- 'l', 'a', 'T', 'V', 'i'
    name         TEXT NOT NULL,
    unit         TEXT NOT NULL,
    repeated     INTEGER NOT NULL DEFAULT 1,  -- 1 = admite n réplicas (tipo A); 0 = medida única
    quantity     TEXT,                   -- magnitud física (para sugerir instrumentos compatibles)
    position     INTEGER NOT NULL DEFAULT 0
);

-- Mensurandos derivados (determinación indirecta)
CREATE TABLE IF NOT EXISTS practice_results (
    id           TEXT PRIMARY KEY,
    practice_id  TEXT NOT NULL REFERENCES practices(id) ON DELETE CASCADE,
    symbol       TEXT NOT NULL,          -- 'Q', 'g', 'tau'
    name         TEXT NOT NULL,
    unit         TEXT NOT NULL,
    formula      TEXT NOT NULL,          -- expresión en función de los symbols, ej "l*a + l*b"
    position     INTEGER NOT NULL DEFAULT 0
);
```

### Datos de las entregas (carga por formulario)

```sql
ALTER TABLE submissions ADD COLUMN entry_mode TEXT;  -- 'form' | 'csv' (legacy)

-- Cada medición individual cargada en el formulario
CREATE TABLE IF NOT EXISTS submission_measurements (
    id              TEXT PRIMARY KEY,
    submission_id   TEXT NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
    quantity_id     TEXT NOT NULL REFERENCES practice_quantities(id),
    instrument_id   TEXT REFERENCES instruments(id),
    scale_id        TEXT REFERENCES instrument_scales(id),
    replicate_index INTEGER NOT NULL DEFAULT 0,  -- 0..n-1 para réplicas (tipo A)
    value           REAL NOT NULL
);
```

El `analysis_json` de `submissions` se sigue usando para **cachear el resultado calculado**
(ahora con incertidumbres), de modo que listar entregas no recalcule. El cálculo se hace al
crear/editar la entrega.

## 2. Motor de incertidumbres (`src/uncertainty.rs`, módulo nuevo)

Funciones puras y testeables, independientes de la base:

```rust
pub struct QuantityResult {
    pub symbol: String,
    pub n: usize,
    pub mean: f64,
    pub s: f64,        // desviación estándar muestral (/(n-1))
    pub u_a: f64,      // s / sqrt(n)
    pub u_b: f64,      // del instrumento/escala
    pub u_c: f64,      // sqrt(u_a^2 + u_b^2)
    pub u_expanded: f64, // 2 * u_c
}

pub struct DerivedResult {
    pub symbol: String,
    pub value: f64,
    pub u: f64,        // propagación
    pub u_expanded: f64,
}

pub fn type_a(values: &[f64]) -> (f64 /*mean*/, f64 /*s*/, f64 /*u_a*/);
// Despacha según b_model. 'fabricante' depende del valor leído, por eso recibe `value`.
//   resolucion  -> step/(2*sqrt(3))
//   apreciacion -> appreciation/sqrt(6)          (analógico)
//   fabricante  -> ((pct_reading/100)*|value| + step_coeff*step + fixed) / 2   (tester y osciloscopio; U k=2 -> u_B)
pub fn type_b(scale: &ScaleSpec, value: f64) -> f64;
pub fn combine(u_a: f64, u_b: f64) -> f64;            // sqrt cuadrática
pub fn expand(u_c: f64, k: f64) -> f64;               // k=2

// Propagación: f evaluada y derivadas numéricas (diferencias finitas centradas)
pub fn propagate(formula: &Expr, vars: &[(&str, f64 /*mean*/, f64 /*u*/)]) -> (f64, f64);
```

### Propagación

Recomendación: **propagación numérica por diferencias finitas centradas**, evaluando la
fórmula con un crate de expresiones (`evalexpr` o `meval`). Evita implementar derivación
simbólica y cubre las fórmulas del curso (productos, sumas, potencias, log/exp).

```
∂f/∂xᵢ ≈ ( f(...,xᵢ+h,...) − f(...,xᵢ−h,...) ) / (2h),   h = max(|xᵢ|,1)·1e-6
u_Q²  = Σ (∂f/∂xᵢ)² · uᵢ²
```

⚠️ CONFIRMAR: alternativa simbólica si se requiere mostrar la expresión analítica de la
derivada al estudiante (mayor esfuerzo). Para cálculo numérico, diferencias finitas alcanza.

### Tipos de análisis por práctica

- `estadistico` (P1): por cada `practice_quantity` con réplicas → tipo A + tipo B → combinada;
  luego cada `practice_result` → propagación. Soporta también medida única (solo tipo B).
  Para `b_model='fabricante'` (depende del valor), `u_B` se evalúa **en la media** de las
  réplicas de esa magnitud.
- `regresion_lineal` (P1 y P2): reusa la regresión de `analysis.rs`, pero reportando
  incertidumbre de pendiente/intercepto (a agregar). Para P2 incluye selección de escala
  y, si aplica, corrección por resistencia interna del instrumento.
- `relajacion_exponencial` (P3): linealiza `V = V₀·e^(−t/τ)` ⇒ `ln V = ln V₀ − t/τ`,
  ajusta recta `ln V` vs `t`, obtiene `τ = −1/pendiente` y propaga la incertidumbre de la
  pendiente a `τ`.

## 3. Cambios en `analysis.rs`

- Agregar `std_dev` **muestral** (`/(n−1)`) además de la poblacional, o un flag, para
  alimentar tipo A. ⚠️ CONFIRMAR convención del curso.
- Exponer incertidumbre de pendiente/intercepto en `LinearRegression`
  (`u_slope`, `u_intercept`) para P2/P3.
- Mantener la ruta CSV genérica como import "legacy"; el resultado nuevo vive en `uncertainty.rs`.

## 4. Nueva API (`/api`)

Instrumentos (rol docente, **catálogo por curso**):
```
GET    /instruments?course_id=...          -> lista del curso (incluye escalas)
POST   /instruments                         -> crea instrumento (course_id requerido)
POST   /instruments/{id}                    -> edita
POST   /instruments/{id}/scales             -> agrega escala
POST   /instruments/{id}/scales/{sid}       -> edita escala
DELETE /instruments/{id} | /scales/{sid}    -> baja

GET    /instruments/export?course_id=...    -> JSON con instrumentos+escalas del curso
POST   /instruments/import                  -> { course_id, instruments[] } alta masiva
```

**Exportar/importar**: permite reutilizar el catálogo entre cursos/años. El export
devuelve un JSON autocontenido (instrumentos con sus escalas, sin ids internos); el import
recrea todo en el `course_id` destino generando ids nuevos. Útil para clonar el catálogo de
un curso anterior al iniciar una nueva edición.

Definición de prácticas (rol docente):
```
GET    /practices/{id}/definition           -> quantities + results + analysis_kind
POST   /practices/{id}/quantities           -> alta de magnitud
POST   /practices/{id}/results              -> alta de mensurando derivado
```

Entregas por formulario:
```
GET    /submissions/new?practice_id=...     -> formulario (magnitudes + instrumentos compatibles)
POST   /submissions  (entry_mode=form)      -> crea con measurements[] y calcula incertidumbres
```

Las entregas existentes (`GET/POST /submissions`, review) se mantienen; cambia el `analysis`
embebido para incluir `quantity_results[]` y `derived_results[]`.

## 5. Frontend (`static/`)

- **Pestaña "Instrumentos"** (`teacher-only`): catálogo CRUD con escalas.
- **Definición de práctica** dentro de la pestaña Cursos o nueva sub-vista: magnitudes y fórmulas.
- **Formulario de entrega guiado**: por práctica, genera filas por magnitud y réplica, con
  selector de instrumento/escala compatible por magnitud; muestra en vivo `n`, media,
  `s`, `u_A`, `u_B`, `u_c`, `U` y los mensurandos derivados con su `U`.
- El detalle de entrega muestra la tabla de incertidumbres y, si aplica, el gráfico de
  regresión / linealización.

## 6. Estrategia de testing

- **Unit tests** en `uncertainty.rs`: tipo A (con caso `s` conocido), tipo B digital y
  analógico, combinada, expandida, y propagación contra el ejemplo del cordón (`l·a+l·b`).
- **Unit test** de relajación exponencial: datos sintéticos con `τ` conocido ⇒ recuperar `τ`.
- Mantener el test existente de `analysis.rs` y añadir el de incertidumbre de pendiente.
- Test de integración del endpoint de entrega por formulario (crea curso/práctica/instrumento
  en una base temporal con `tempfile` y verifica el JSON calculado).
