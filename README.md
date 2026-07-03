# Quantify

Web app para el laboratorio de Física 103 (Física Experimental I). Los estudiantes cargan sus
medidas **a mano en un formulario**, el sistema calcula las incertidumbres (tipo A, tipo B,
combinada y expandida) y los mensurandos derivados por propagación, y deja las entregas
disponibles para revisión docente.

## Stack

- **Backend**: Rust + Axum 0.8, SQLite vía `sqlx` 0.8, Tokio.
- **Frontend**: HTML/CSS/JS estático (`static/`) servido por el backend con `ServeDir`. La lógica
  pura y los selectores se extraen a `static/lib.js` (ES module) para poder testearlos.
- **Auth**: sesiones por cookie `quantify_session` (12 h); hash de contraseña Argon2id (re-hash
  transparente de hashes SHA-256 legacy en el primer login).
- **Motor de cálculo**: `src/uncertainty.rs` (tipo A/B/combinada/expandida + propagación numérica)
  y `src/computation.rs` (entrega por formulario: cablea la definición de la práctica, el catálogo
  de instrumentos y las lecturas crudas, y evalúa las fórmulas con `evalexpr`).
- **Tests**: backend con `cargo test`; frontend con `node --test` sobre `static/lib.js`.

SQLite simplifica el arranque en Windows y el primer deploy local. La persistencia está concentrada
en `src/db.rs`, así que migrar a PostgreSQL más adelante es directo si el uso crece.

## Desarrollo en Windows

Requisitos:

- Rust stable
- PowerShell

Ejecutar:

```powershell
cargo run
```

Al arrancar, la app **se configura sola**: crea `data/quantify.db`, corre las migraciones y siembra
prácticas, usuarios y el contexto académico de prueba. Cuando veas en el log
`Quantify listening on http://127.0.0.1:8080`, ya está arriba.

Abrir:

```text
http://localhost:8080
```

Usuarios iniciales de desarrollo:

```text
admin@quantify.local       / admin123
docente@quantify.local     / docente123
estudiante@quantify.local  / estudiante123
```

Variables útiles:

```powershell
$env:APP_BIND_ADDR="127.0.0.1:8080"
$env:DATABASE_URL="sqlite:data/quantify.db"
$env:UPLOAD_DIR="data/uploads"
$env:SEED_ADMIN_PASSWORD="cambiar-esto"
$env:SEED_TEACHER_PASSWORD="cambiar-esto"
$env:SEED_STUDENT_PASSWORD="cambiar-esto"
cargo run
```

Para **empezar de cero** (regenerar los datos sembrados), detené el server y corré:

```powershell
.\scripts\dev-reset.ps1    # en Linux/macOS: ./scripts/dev-reset.sh
```

El script borra la base (`data\quantify.db*`) y los uploads (`data\uploads`), respetando
`DATABASE_URL`/`UPLOAD_DIR` si están definidas; el próximo `cargo run` vuelve a sembrar todo.

## Cómo se carga una entrega

La carga es **por formulario**, no por archivo. En la pestaña **Entregas** el estudiante:

1. Elige **Curso → Grupo → Práctica → Mesa**.
2. Aparecen los campos de la práctica según su tipo de análisis:

   **Prácticas estadísticas** — un bloque por magnitud. Cada bloque tiene una o más filas de
   réplica (el alumno agrega las que midió) y un selector de instrumento y escala del catálogo.
   Los campos `is_given` (dato aportado por la cátedra como `valor ± U`) se muestran pre-cargados
   y no requieren instrumento.

   **Prácticas de regresión lineal** — una tabla de puntos: el alumno agrega una fila por
   medición (p. ej. una frecuencia), cargando los valores crudos de ese punto. Las fórmulas de
   eje `x_formula` / `y_formula` (definidas en la práctica) derivan las coordenadas `(x, y)` de
   cada punto; el ajuste lineal `y = m·x + b` se calcula sobre esa serie.

3. Al enviar (`POST /api/submissions/form`), el sistema calcula automáticamente:
   - por cada magnitud: media, **incertidumbre tipo A** (`s/√n`, solo si hay réplicas) y **tipo B**
     (según el modelo de la escala elegida: `resolución` → `paso/(2√3)`, `apreciación` →
     `apreciación/√6`, `fabricante` → `(pct·|v| + coef·paso + fijo) / 2`), combinada `u_c` y
     expandida `U = 2·u_c`;
   - los **mensurandos derivados** evaluando la fórmula de la práctica con propagación numérica de
     varianzas (diferencias finitas centradas);
   - en regresión: **pendiente**, **intercepto**, sus incertidumbres (`u_slope`, `u_intercept`),
     **R²**, y los mensurandos que dependan de `slope` o `intercept`; más un **gráfico SVG**
     con la nube de puntos y la recta ajustada.

El docente puede habilitar que el estudiante vea el cálculo automático desde la revisión.
Una vez habilitado, el estudiante puede guardar sus propios resultados (`valor ± U`)
para **compararlos** con el cálculo automático (tabla de diferencias absolutas y relativas).

> Existe además un endpoint heredado de carga por CSV (`POST /api/submissions`, multipart) que
> calcula estadística básica por columna y una regresión entre las dos primeras columnas numéricas.
> La UI actual usa el formulario; el CSV se mantiene por compatibilidad.

## Prácticas sembradas (Física 103)

| id | Nombre | Tipo de análisis |
|----|--------|------------------|
| `p1-estadistica` | Tratamiento Estadístico — Péndulo Simple | estadístico |
| `p2-cc` | Corriente continua (serie + paralelo + curva de potencia, una sola entrega) | curva |
| `p3-relajacion` | Relajación Exponencial (parte 1 — medida directa de τ) | estadístico |
| `p3-relajacion-desfasaje` | Relajación Exponencial (parte 2 — desfasaje) | regresión lineal |

> **Migración**: las prácticas viejas `p2-serie`, `p2-corriente-continua` y `p2-potencia` ya no se
> siembran (las reemplaza `p2-cc`). En una base de desarrollo existente, borrá `data/quantify.db`
> antes de arrancar para re-seedear limpio.

### P1 — Tratamiento estadístico

Magnitudes medidas (con réplicas):

| símbolo | nombre | unidad |
|---------|--------|--------|
| `l` | Longitud del cordón | m |
| `a` | Ancho del cordón | m |
| `b` | Espesor del cordón | m |

Mensurando derivado: `Q = l·a + l·b` (área de la sección transversal, m²).

### P2 — Corriente continua (`p2-cc`, una sola entrega)

Una sola práctica real con tres partes temáticas (tabs **Serie**, **Paralelo** y **Curva de
potencia** que alternan secciones del mismo formulario, sin cambiar de entrega):

- **Compartidas** (medidas UNA vez con el óhmetro, valen para las tres partes): `R1`, `R2`, `R3`.
- **Por parte** (la fuente y el amperímetro pueden cambiar entre armados): `Vg_s`/`RA_s` (serie),
  `Vg_p`/`RA_p` (paralelo), `Vg_c`/`RA_c` (curva de potencia).
- **Tensiones experimentales** medidas con multímetro en cada circuito: `VR1_s`…`VR3_s` y
  `VR1_p`…`VR3_p`. El análisis las compara contra las teóricas automáticas en la tabla
  **"Medido vs teórico"** (valor ± U del instrumento vs valor ± U propagada).
- **Curva de potencia** (por punto): `R` (carga variable) e `I`, con la columna **P = I²·R**
  calculada en vivo al tipear y la curva P vs R.

Mensurandos (los `*_t` teóricos y los finales los entrega también el alumno como resultado final):

```
Serie:     I_s     = Vg_s / (R1 + R2 + R3 + RA_s)
           VRi_s_t = Vg_s · Ri / (R1 + R2 + R3 + RA_s)          (i = 1, 2, 3)
Paralelo:  Req     = R1 + RA_p + R2·R3 / (R2 + R3)
           I_p     = Vg_p / Req
           VR1_p_t = Vg_p · R1 / Req
           VR2_p_t = VR3_p_t = Vg_p · (R2·R3/(R2+R3)) / Req
Potencia:  RP_max_t = RA_c + R2·R3 / (R2 + R3)                  (Rth)
           P_max_t  = Vg_c² / (4·Rth)
           P_max_e  = máx(P) de la tabla                        (alias P_max)
           RP_max_e = R en el punto de máx(P)                   (alias R_at_P_max)
```

`P_max_e`, `P_max_t`, `RP_max_e` y `RP_max_t` se entregan y muestran **sin incertidumbre**
(`RESULTS_WITHOUT_U` en `static/constants.js`). Los alias de extremos (`{S}_max`,
`{T}_at_{S}_max`) los inyecta el camino `curva` del motor con U = 0; como `check_formula` de la
UI admin no los conoce, las fórmulas de `P_max_e`/`RP_max_e` no son editables desde la pestaña
Prácticas.

### P3 — Relajación exponencial

#### Parte 1 — Medida directa de τ (estadístico)

| símbolo | nombre | unidad | nota |
|---------|--------|--------|------|
| `R` | Resistencia | Ω | medida por el alumno |
| `Rint` | Resistencia interna de la fuente | Ω | dato dado por la cátedra |
| `C` | Capacitancia | F | medida por el alumno |
| `T_oc` | Período de la onda cuadrada | s | referencia: debe ser ≈ 10·τ_exp |
| `tmedio` | Tiempo de semidescarga t₁/₂ | s | medido sobre la curva exponencial |

Mensurandos derivados:

```
tau_teorico = (R + Rint) · C
tau_exp     = tmedio / ln(2)
```

`T_oc` no entra en las fórmulas; se carga como verificación de la condición experimental
(el capacitor debe descargarse completamente antes del siguiente semiciclo).

#### Parte 2 — Desfasaje por figura de Lissajous (regresión lineal)

El alumno carga **una fila por frecuencia**, con los valores leídos del osciloscopio:

| símbolo | nombre | unidad |
|---------|--------|--------|
| `f` | Frecuencia | Hz |
| `a` | Amplitud de la elipse (semieje vertical) | div |
| `b` | Intersección de la elipse con el eje vertical | div |

Fórmulas de eje (calculadas por punto antes del ajuste):

```
x = 2·π·f              (ω, rad/s)
y = b / √(a² − b²)    (tg φ)
```

El ajuste `tg φ = ω·RC` es lineal en ω con `b_intercept = 0`; la **pendiente es τ = RC**.

Mensurando derivado: `tau = slope` (s).

---

Las definiciones de magnitudes y mensurandos son **editables** por el docente desde la pestaña
**Prácticas** (símbolos, unidades, fórmulas, tipo de análisis y fórmulas de eje para regresión).

## Modelo académico

El sistema guarda las entregas contra entidades reales: **cursos**, **grupos de laboratorio**,
**estudiantes** asignados a grupos y **prácticas habilitadas** por curso. Cada entrega registra
`course_id`, `group_id`, `practice_id`, `submitted_by_user_id` y `entry_mode` (`form` o CSV).

Para desarrollo se siembra automáticamente:

- Curso: `Física Experimental I` (2026)
- Grupo: `Grupo 1` (4 mesas)
- Estudiante: `estudiante@quantify.local`, inscripto en el curso y el grupo
- Prácticas habilitadas: las de la tabla anterior

El docente o admin administra todo esto desde las pestañas **Cursos**, **Grupos**, **Estudiantes** y
**Usuarios**: crear usuarios, asignar estudiantes a grupos, asignar mesas por práctica y resetear
contraseñas. El login es el email (único en la base, pensado para sumar notificaciones por correo
más adelante). Cada usuario puede cambiar su propia contraseña desde la pestaña **Cuenta**.

## Instrumentos

La pestaña **Instrumentos** (docente/admin) administra el catálogo de instrumentos por curso y sus
**escalas**. Cada escala define el modelo de incertidumbre tipo B que aplica (`resolucion`,
`apreciacion` o `fabricante`) y sus parámetros (paso, apreciación, % de lectura, etc.). El catálogo
se puede **exportar e importar** en JSON.

## Notas

La pestaña **Cargar notas** permite a docentes definir componentes evaluables por curso:

- preguntas
- informes
- parciales

Cada componente tiene:

- `Sobre`: puntaje máximo usado al corregir ese ítem.
- `Valor normalizado`: puntos que aporta al total del curso.

La normalización es:

```text
puntos_normalizados = puntos_obtenidos / sobre * valor_normalizado
```

La sección **Notas** muestra al estudiante sus subtotales por tipo y su total normalizado.

## Deploy simple en Ubuntu

Con Docker y Docker Compose:

```bash
cp .env.example .env
docker compose up -d --build
```

La app queda disponible en:

```text
http://IP_DE_LA_MAQUINA:8080
```

Los datos persistentes quedan en `./data`. Ver `deploy/ubuntu.md` para más detalle.

## Endpoints principales (`/api`)

- **Salud**: `GET /health`
- **Auth**: `POST /auth/login`, `POST /auth/logout`, `GET /auth/me`, `POST /auth/profile`,
  `POST /auth/password`
- **Usuarios**: `GET/POST /users`, `POST /users/{id}`, `POST /users/{id}/password`
- **Académico**: `GET /academic/context`; CRUD de cursos, grupos, subgrupos, miembros, prácticas
  habilitadas y mesas (`/academic/courses…`, `/academic/groups…`)
- **Notas**: `GET /grades`, `POST /grades/components`, `POST /grades/scores`
- **Prácticas**: `GET /practices`, `GET /practices/{id}/definition`, y edición de
  `analysis-kind`, `regression-formulas`, `quantities` y `results`
- **Instrumentos**: `GET/POST /instruments`, `POST/DELETE /instruments/{id}`, escalas
  `/instruments/{id}/scales[/{scale_id}]`, `GET /instruments/export`, `POST /instruments/import`
- **Entregas**: `GET /submissions`, `POST /submissions` (CSV heredado),
  `POST /submissions/form` (formulario), `GET /submissions/{id}`,
  `POST /submissions/{id}/review`, `POST /submissions/{id}/student-results`

## Documentación adicional

El plan de fases y la referencia de Física 103 están en `docs/plan/`.
</content>
</invoke>
