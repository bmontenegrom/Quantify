# Quantify

Web app para laboratorio de fisica. Permite que estudiantes carguen medidas en CSV, calcula controles automaticos basicos y deja las entregas disponibles para revision docente.

## Stack inicial

- Backend: Rust + Axum
- Base de datos local para MVP: SQLite
- Frontend: HTML/CSS/JS estatico servido por el backend
- Deploy local: binario Rust o Docker Compose en Ubuntu

SQLite simplifica el arranque en Windows y el primer deploy en una maquina local. La capa de persistencia esta concentrada en `src/db.rs`, por lo que migrar a PostgreSQL mas adelante es directo si el uso crece.

## Desarrollo en Windows

Requisitos:

- Rust stable
- PowerShell

Ejecutar:

```powershell
cargo run
```

Abrir:

```text
http://localhost:8080
```

Variables utiles:

```powershell
$env:APP_BIND_ADDR="127.0.0.1:8080"
$env:DATABASE_URL="sqlite:data/quantify.db"
$env:UPLOAD_DIR="data/uploads"
cargo run
```

## Formato CSV

La primera fila debe contener encabezados. El sistema acepta columnas numericas y calcula:

- cantidad de filas validas
- promedio y desviacion estandar por columna
- regresion lineal si hay al menos dos columnas numericas
- advertencias por celdas vacias o valores no numericos

Ejemplo:

```csv
largo_m,periodo_s
0.20,0.91
0.30,1.10
0.40,1.27
```

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

Los datos persistentes quedan en `./data`.

## Endpoints principales

- `GET /api/health`
- `GET /api/practices`
- `GET /api/submissions`
- `GET /api/submissions/:id`
- `POST /api/submissions`
- `POST /api/submissions/:id/review`

## Siguiente iteracion sugerida

- Login con roles estudiante/docente.
- Cursos y grupos reales.
- Rubricas configurables por practica.
- Exportacion de entregas.
- Migracion a PostgreSQL si se requiere concurrencia alta o integracion institucional.
