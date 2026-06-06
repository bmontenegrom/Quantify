# Quantify

Web app para el laboratorio de Física 103 (Física Experimental I). Los estudiantes cargan sus
medidas **a mano en un formulario**, el sistema calcula las incertidumbres (tipo A, tipo B,
combinada y expandida) y los mensurandos derivados por propagación, y deja las entregas
disponibles para revisión docente.

## Stack

- **Backend**: Rust + Axum 0.8, SQLite vía `sqlx` 0.8, Tokio.
- **Frontend**: HTML/CSS/JS estático (`static/`) servido por el backend con `ServeDir`. La lógica
  pura y los selectores se extraen a `static/lib.js` (ES module) para poder testearlos.
- **Auth**: sesiones por cookie `quantify_session` (12 h); hash de contraseña SHA-256 con salt.
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

Para **empezar de cero** (regenerar los datos sembrados), detené el server y borrá la base:

```powershell
Remove-Item data\quantify.db* -Force
```

## Cómo se carga una entrega

La carga es **por formulario**, no por archivo. En la pestaña **Entregas** el estudiante:

1. Elige **Curso → Grupo → Práctica → Mesa**.
2. Según la práctica, aparecen los campos de cada **magnitud** a medir. Carga las **lecturas
   crudas** a mano (una o varias réplicas) y, cuando corresponde, elige **instrumento** y **escala**
   del catálogo.
3. Al enviar (`POST /api/submissions/form`), el sistema calcula:
   - por cada magnitud: media, incertidumbre tipo A (réplicas) y tipo B (resolución / apreciación /
     especificación de fabricante, según la escala), combinada y expandida;
   - los **mensurandos derivados** por propagación de varianzas evaluando la fórmula de la práctica;
   - en prácticas de **regresión lineal**, el ajuste `y = m·x + b` (pendiente, intercepto, sus
     incertidumbres y R²) sobre los puntos cargados.

El docente puede luego habilitar que el estudiante vea el cálculo automático, y el estudiante puede
guardar sus propios resultados finales (`valor ± U`) para **compararlos** con el cálculo automático.

> Existe además un endpoint heredado de carga por CSV (`POST /api/submissions`, multipart) que
> calcula estadística básica por columna y una regresión entre las dos primeras columnas numéricas.
> La UI actual usa el formulario; el CSV se mantiene por compatibilidad.

## Prácticas sembradas (Física 103)

| id | Nombre | Tipo de análisis |
|----|--------|------------------|
| `p1-estadistica` | Tratamiento Estadístico de Datos | estadístico |
| `p2-corriente-continua` | Circuitos de Corriente Continua | estadístico |
| `p3-relajacion` | Relajación Exponencial (parte 1: medida directa de τ) | estadístico |
| `p3-relajacion-desfasaje` | Relajación Exponencial — Desfasaje (parte 2) | regresión lineal |

- **P1** mide `l`, `a`, `b` (con réplicas) y deriva el área del cordón `Q = l*a + l*b`.
- **P2** mide `Vg`, `R1`, `R2`, `R3`, `RA` y deriva `Req = R1 + RA + 1/(1/R2 + 1/R3)` e `I = Vg/Req`.
- **P3 parte 1** mide `R`, `Rint`, `C`, `tmedio` y deriva `τ_teórico = (R + Rint)·C` y
  `τ_exp = tmedio / ln2`.
- **P3 parte 2** carga series de `f`, `a`, `b` por punto; ajusta `tg(φ) = b/√(a²−b²)` contra
  `ω = 2·π·f`, y la pendiente del ajuste es `τ = RC`.

Las definiciones de magnitudes y mensurandos de cada práctica son **editables** por el docente desde
la pestaña **Prácticas** (símbolos, unidades, fórmulas, tipo de análisis y fórmulas de eje para
regresión).

## Modelo académico

El sistema guarda las entregas contra entidades reales: **cursos**, **grupos de laboratorio**,
**estudiantes** asignados a grupos y **prácticas habilitadas** por curso. Cada entrega registra
`course_id`, `group_id`, `practice_id`, `submitted_by_user_id` y `entry_mode` (`form` o CSV).

Para desarrollo se siembra automáticamente:

- Curso: `Física Experimental I` (2026)
- Grupo: `Grupo 1` (4 mesas)
- Estudiante: `estudiante@quantify.local`, inscripto en el curso y el grupo
- Prácticas habilitadas: las cuatro de la tabla anterior

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
