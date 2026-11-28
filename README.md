# Quantify

Web app para cursos de laboratorio de cualquier disciplina experimental (física, química, biología,
ingeniería…). El docente define cada práctica en un editor: qué se mide, con qué instrumentos, qué
fórmulas derivan los resultados y qué análisis se hace (estadístico, ajuste lineal o curva). Los
estudiantes cargan sus medidas **a mano en un formulario**, y el sistema calcula los resultados con
sus incertidumbres según la GUM (tipo A, tipo B por instrumento, combinada y expandida, propagadas
analíticamente) y deja las entregas disponibles para revisión docente.

Nada del motor es específico de una materia: las prácticas, las magnitudes, las unidades y los
instrumentos son datos que configura el docente. Las prácticas de ejemplo que trae la biblioteca son
las del curso de Física 103, y sirven como referencia para armar las propias.

## Funcionalidades

**Prácticas configurables**
- Editor de prácticas: magnitudes medidas, datos de cátedra (con o sin incertidumbre), intermedias
  calculadas por réplica o sobre los promedios, mensurandos, derivadas por punto y agregados.
- Tres tipos de análisis: estadístico (réplicas por magnitud), regresión lineal (fórmulas de eje
  sobre una serie de puntos) y curva (dispersión sin ajuste).
- Prácticas en varias partes, cada una con su análisis, y determinaciones (la misma medida repetida
  por operador, muestra o condición, cada una con su instrumento).
- Borrador y publicación con versiones: las entregas quedan en la versión con que se hicieron.
- Biblioteca de plantillas, "Nueva desde plantilla" y exportar/importar una práctica como archivo.
- Modo guiado para armar una práctica paso a paso, y "Probar" con datos de ejemplo antes de publicar.
- Avisos del editor: unidades no reconocidas, chequeo dimensional de las fórmulas y casos en que la
  incertidumbre puede no ser exacta.
- Catálogo de constantes físicas y químicas, y sinónimos de magnitudes físicas.

**Carga de medidas**
- Formulario por práctica, con filas de réplica, tabla de puntos para la regresión y columnas que se
  calculan en vivo mientras el alumno carga.
- Cronómetro integrado para medir tiempos desde el formulario.
- Unidades estructuradas: el alumno carga en la unidad que elija del desplegable y el motor convierte.
- Catálogo de instrumentos por curso, con escalas, modelo de incertidumbre tipo B (resolución,
  apreciación o fabricante) y calibración; exportable e importable.

**Incertidumbres**
- Tipo A (`s/√n`), tipo B según la escala del instrumento, combinada y expandida (`k = 2`).
- Propagación analítica con números duales (derivadas exactas, sin diferencias finitas), con las
  correlaciones como fuentes compartidas: una magnitud que entra por varios caminos cuenta una vez.
- U de los mensurandos, de las intermedias y derivadas por punto (tabla "Por corrida") y de los
  agregados; las intermedias por réplica respetan que las lecturas de una réplica se tomaron juntas.
- Regresión lineal con pendiente, ordenada, sus incertidumbres y R²; interpolación inversa con la U
  de la recta; barras de error en el gráfico; las magnitudes compartidas de los ejes se suman a la U
  de la recta (el panel muestra la U del ajuste y la total).
- Avisos cuando una U no se puede calcular o cae en un quiebre de la fórmula.

**Curso y revisión**
- Cursos, grupos, mesas, estudiantes y prácticas habilitadas por curso; roles de admin, docente y
  estudiante.
- Revisión docente de cada entrega; el docente puede habilitar que el alumno vea el cálculo
  automático y compare sus propios resultados, con una tolerancia por mensurando.
- Notas por curso: preguntas, informes y parciales, con normalización a un total.

## Stack

- **Backend**: Rust + Axum 0.8, SQLite vía `sqlx` 0.8, Tokio.
- **Frontend**: HTML/CSS/JS estático (`static/`) servido por el backend con `ServeDir`. La lógica
  pura y los selectores se extraen a `static/lib.js` (ES module) para poder testearlos.
- **Auth**: sesiones por cookie `quantify_session` (12 h); hash de contraseña Argon2id (re-hash
  transparente de hashes SHA-256 legacy en el primer login).
- **Motor de cálculo**: `src/uncertainty.rs` (tipo A/B/combinada/expandida y combinación de fuentes)
  y `src/computation.rs` (entrega por formulario: cablea la definición de la práctica, el catálogo
  de instrumentos y las lecturas crudas, evalúa las fórmulas con `evalexpr` y propaga las
  incertidumbres con números duales).
- **Tests**: backend con `cargo test`; frontend con `node --test` sobre `static/lib.js`.

SQLite simplifica el arranque en Windows y el primer deploy local. La persistencia está concentrada
en `src/db.rs`, así que migrar a PostgreSQL más adelante es directo si el uso crece.

## Desarrollo en Windows

Requisitos:

- Rust stable
- PowerShell

Ejecutar:

```powershell
$env:SEED_DEMO_DATA="true"
cargo run
```

Al arrancar, la app **se configura sola**: crea `data/quantify.db`, corre las migraciones y siembra
prácticas, usuarios y el contexto académico de prueba. Cuando veas en el log
`Quantify listening on http://127.0.0.1:8080`, ya está arriba.

Abrir:

```text
http://localhost:8080
```

Usuarios iniciales de desarrollo. En una base sin usuarios se crea siempre el admin; el docente, el
estudiante, su inscripción y las entregas de prueba se crean solo con `SEED_DEMO_DATA=true`. Sin
esa variable, el primer arranque exige `SEED_ADMIN_PASSWORD`:

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
$env:SEED_DEMO_DATA="true"            # cuentas y entregas de prueba
$env:SEED_ADMIN_PASSWORD="cambiar-esto"
$env:SEED_TEACHER_PASSWORD="cambiar-esto"
$env:SEED_STUDENT_PASSWORD="cambiar-esto"
cargo run
```

Para **empezar de cero** (volver a importar las prácticas de la biblioteca), detené el server y corré:

```powershell
.\scripts\dev-reset.ps1    # en Linux/macOS: ./scripts/dev-reset.sh
```

El script borra la base (`data\quantify.db*`) y los uploads (`data\uploads`), respetando
`DATABASE_URL`/`UPLOAD_DIR` si están definidas; el próximo `cargo run` vuelve a sembrar todo (los datos de prueba, con `SEED_DEMO_DATA=true`).

## Cómo se carga una entrega

La carga es **por formulario**, no por archivo. En la pestaña **Entregas** el estudiante:

1. Elige **Curso → Grupo → Práctica → Mesa**.
2. Aparecen los campos de la práctica según su tipo de análisis:

   **Prácticas estadísticas** — un bloque por magnitud. Cada bloque tiene una o más filas de
   réplica (el alumno agrega las que midió) y un selector de instrumento y escala del catálogo;
   una magnitud que se repite por determinación tiene un selector en cada determinación.
   Los campos `is_given` (dato aportado por la cátedra como `valor ± U`) se muestran pre-cargados
   y no requieren instrumento.

   **Prácticas de regresión lineal** — una tabla de puntos: el alumno agrega una fila por
   medición (p. ej. una frecuencia), cargando los valores crudos de ese punto. Las fórmulas de
   eje `x_formula` / `y_formula` (definidas en la práctica) derivan las coordenadas `(x, y)` de
   cada punto; el ajuste lineal `y = m·x + b` se calcula sobre esa serie.

   **Prácticas de curva** — la misma tabla de puntos, pero sin ajuste: el resultado es el gráfico de
   dispersión de una o más curvas (con eje x logarítmico si la práctica lo pide).

3. Al enviar (`POST /api/submissions/form`), el sistema calcula automáticamente:
   - por cada magnitud: media, **incertidumbre tipo A** (`s/√n`, solo si hay réplicas) y **tipo B**
     (según el modelo de la escala elegida: `resolución` → `paso/(2√3)`, `apreciación` →
     `apreciación/√6`, `fabricante` → `(pct·|v| + coef·paso + fijo) / 2`), combinada `u_c` y
     expandida `U = 2·u_c`;
   - los **mensurandos derivados** evaluando la fórmula de la práctica y propagando las
     incertidumbres con números duales (derivadas exactas), con las correlaciones entre entradas;
   - en regresión: **pendiente**, **intercepto**, sus incertidumbres (la del ajuste y, si un eje usa
     magnitudes compartidas, la total), **R²**, los mensurandos que dependan de `slope` o
     `intercept`, la U de cada intermedia y derivada por punto y de los agregados, y un **gráfico
     SVG** con la nube de puntos, sus barras de error y la recta ajustada.

El docente puede habilitar que el estudiante vea el cálculo automático desde la revisión.
Una vez habilitado, el estudiante puede guardar sus propios resultados (`valor ± U`)
para **compararlos** con el cálculo automático (tabla de diferencias absolutas y relativas).

> Existe además un endpoint heredado de carga por CSV (`POST /api/submissions`, multipart) que
> calcula estadística básica por columna y una regresión entre las dos primeras columnas numéricas.
> La UI actual usa el formulario; el CSV se mantiene por compatibilidad.

## Prácticas de la biblioteca (ejemplos de Física 103)

Las prácticas de ejemplo son las del curso de Física 103: plantillas JSON
en `templates/practices/<id>.json`. Una base nueva (sin prácticas) las importa al arrancar,
publicadas y habilitadas en el curso de ejemplo; después cada práctica se edita desde el admin como
cualquier otra. El docente también puede crear un
borrador desde una plantilla ("Nueva desde plantilla") o exportar e importar una práctica como
archivo.

| id | Nombre | Tipo de análisis |
|----|--------|------------------|
| `p1-estadistica` | Tratamiento estadístico de Datos (péndulo, tres operadores) | estadístico |
| `p2-cc` | Corriente continua (serie, paralelo y curva de potencia) | curva |
| `p3-relajacion` | Relajación Exponencial (medida directa y desfasaje, en dos partes) | regresión lineal |
| `fluidos-1` | Fluidos I | regresión lineal |
| `fluidos-2` | Fluidos II | regresión lineal |
| `viscosidad` | Viscosidad | regresión lineal |
| `filtros` | Filtros | curva |
| `ca-rlc` | Corriente Alterna (RLC) | estadístico |
| `hidrostatica` | Hidrostática y Tensión Superficial | estadístico |

> Una base existente no vuelve a importar la biblioteca. Para empezar de cero en desarrollo, borrá
> `data/quantify.db` antes de arrancar.

### P1 — Tratamiento estadístico (`p1-estadistica`)

Péndulo simple medido por **3 operadores**, cada uno en su pestaña (determinaciones "Operador",
sin promedio). Datos comunes: `L` (longitud del péndulo, dato de cátedra con U) y `t_med`
(tiempo de semiamplitud, sin U). Cada operador mide su serie de períodos `T` con el cronómetro;
ninguno es obligatorio.

Mensurandos: `gamma = 2·ln2/t_med` (común), y por operador `g = 4π²·L/T²` y
`Q = π·t_med/(T·ln2)`. El alumno informa `g` y `Q` de cada operador (`g#1`, `Q#1`, …).

### P2 — Corriente continua (`p2-cc`, una sola entrega)

Una sola práctica real con tres partes temáticas (tabs **Serie**, **Paralelo** y **Curva de
potencia** que alternan secciones del mismo formulario, sin cambiar de entrega):

- **Compartidas** (medidas UNA vez con el óhmetro, valen para las tres partes): `R1`, `R2`, `R3`.
- **Por parte** (la fuente y el amperímetro pueden cambiar entre armados): `Vg_s`/`RA_s` (serie),
  `Vg_p`/`RA_p` (paralelo), `Vg_c`/`RA_c` (curva de potencia).
- **Voltajes experimentales** medidos con multímetro en cada circuito: `VR1_s`…`VR3_s` y
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
(el mensurando tiene `has_uncertainty = 0`). Los alias de extremos (`{S}_max`,
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

### Hidrostática y Tensión Superficial (`hidrostatica`, una sola entrega, dos partes)

#### Parte 1 — Densidad de la goma por Arquímedes (balanza de Mohr)

Se pesa la goma (`m_goma`, balanza digital) y se mide la densidad del fluido (`rho_f`,
densímetro). Con la goma sumergida se reequilibra la balanza colocando pesas en las **9 ranuras
calibradas** (`b1..b9` = 1 cm .. 9 cm; el brazo fijo del que pende la goma es `b_E` = 10 cm). El
balance de torques da:

```
E · b_E = Σ mᵢ · g · bᵢ        ⇒  E = (Σ mᵢ·bᵢ) · g / b_E
E / P = ρ_f / ρ_goma           ⇒  ρ_goma = ρ_f · m_goma · b_E / (Σ mᵢ·bᵢ)
```

El procedimiento se repite **3 veces** con pesas y posiciones distintas: son 3 determinaciones
("Medida 1..3", en bloques), no réplicas. Las masas de las ranuras `m_1..m_9` se cargan en cada
medida; la goma, el fluido y los brazos son comunes. Salida: `E` y `rho` por medida, más
`E_medio` y `rho_medio`, que se calculan solos y propagan la U desde las tres medidas con los
datos comunes una sola vez, como pide la técnica.

Las 9 ranuras se cargan siempre: la que no se usó queda en **masa 0** (el formulario ya la trae en
0) y **sin instrumento**: cada medida elige su propio instrumento, así que una ranura vacía no
aporta ni valor ni incertidumbre aunque otra medida use la balanza en esa misma ranura. El
instrumento se guarda junto con las lecturas: una medida que se entrega vacía no lo conserva, y si
después se edita la entrega para cargarla hay que volver a elegirlo.

#### Parte 2 — Tensión superficial por el método de Wilhelmy

Se hace sobre la **misma balanza de Mohr**, así que las distancias son las de sus ranuras
calibradas, no medidas con regla: el marco de base `l` cuelga del brazo fijo `d` (10 cm, como la
goma en la parte 1) y la pesa `mw` va en una de las 9 ranuras, `dw` = 1..9 cm. El torque de la
película se sustituye por el de la pesa:

```
2·γ·l·d = mw·g·dw        ⇒  γ = mw·g·dw / (2·l·d)
```

`mw` y `dw` se cargan en cada una de las 3 medidas: `gamma` por medida más `gamma_medio`. La técnica indica expresamente que esta
parte **no informa incertidumbre**, así que estos mensurandos se muestran sin ±U.

> Unidades: masas en g y brazos/distancias en cm (como se leen). Las fórmulas de `E` y `γ` llevan
> los factores de conversión a kg/m explícitos para dar N y N/m; en `rho_goma` se cancelan.

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

Sobre el modelo base, una escala puede declarar además su **calibración** (`u_cal_pct`, % del valor
leído, y `u_cal_fixed`, término fijo), que se suma **en cuadratura**:

```
u_B = √( base² + (u_cal_pct/100 · |valor|)² + u_cal_fixed² )
```

Con ambos en 0 (el default) el resultado es el del modelo base de siempre. Lo usan los instrumentos
de Hidrostática: balanza digital (truncamiento `R = 0,1 g` ⊕ 3 % de calibración) y densímetro
(apreciación ⊕ `0,001 g/cm³` fijos).

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
