# Plan de implementación — Quantify

Plan para continuar el desarrollo de **Quantify**, la app de laboratorio de
**Física 103** (Facultad de Química, UdelaR). Este directorio documenta el
estado actual, el dominio extraído de la cuaderneta del curso y el roadmap de
las próximas iteraciones.

## Índice

1. [`01-estado-actual.md`](01-estado-actual.md) — qué hay implementado hoy en el repo.
2. [`02-referencia-fisica-103.md`](02-referencia-fisica-103.md) — prácticas, esquema de evaluación y modelo de incertidumbres/instrumentos del curso.
3. [`03-modelo-datos-y-motor.md`](03-modelo-datos-y-motor.md) — diseño del esquema de base de datos, API y motor de incertidumbres.
4. [`04-roadmap-fases.md`](04-roadmap-fases.md) — fases con tareas concretas, archivos a tocar y criterios de aceptación.

## Decisiones tomadas (29-05-2026)

| Tema | Decisión |
|------|----------|
| **Prácticas a soportar al inicio** | Las 3 reales del primer bloque: **P1** Tratamiento Estadístico de Datos, **P2** Corriente Continua, **P3** Relajación Exponencial. Reemplazan los placeholders `pendulo`/`hooke`/`caida-libre`. |
| **Foco de la próxima iteración** | **Motor de incertidumbres + catálogo de instrumentos.** Pasar del análisis CSV genérico al modelo propio de Física 103 (tipo A, tipo B, combinada, expandida, propagación de varianzas). |
| **Modelo de instrumentos** | **Catálogo gestionable por el docente, por curso**: nombre, tipo (analógico/digital), magnitud, escalas con resolución/apreciación. Con **exportar/importar** para reutilizarlo entre cursos/años. El estudiante elige el instrumento usado por cada magnitud al cargar datos. |
| **Ingreso de datos** | **Formulario guiado en la app** por práctica (magnitudes, réplicas, instrumento por magnitud). La importación CSV pasa a ser secundaria/legacy. |

## Fuera de alcance de esta iteración (futuro)

- Alineación del modelo de notas al esquema real del curso (Preguntas 20 / Hojas de
  resultados 30 por mesa / Controles 50 con mínimos, asistencia y regla de ganancia ≥51).
  Ver [`02-referencia-fisica-103.md`](02-referencia-fisica-103.md#evaluación-del-curso).
- Prácticas 4 a 9 (Corriente Alterna, Filtro, Hidrostática, Viscosidad, Fluidos I/II).
- Notificaciones por correo, exportación de entregas, migración a PostgreSQL.

## Definiciones confirmadas con el docente

- **Modelos de incertidumbre tipo B** (todos confirmados):
  - Analógico (regla, calibre, aguja): `u_B = A/√6` (triangular).
  - Digital simple (cronómetro, balanza): `u_B = R/(2√3)` (rectangular).
  - **Fabricante** (tester y osciloscopio): la hoja/técnica da la **U expandida (k=2)** como
    `U_spec = pct·|valor| + coef·step + fijo` → `u_B = U_spec/2`. Casos reales cargados:
    testers A830L y EXTECH MN35 (`coef`=dgt, `fijo`=0); osciloscopio GDS-1052-U eje Y
    (`3% + 0.1·(VOLTS/DIV) + 1 mV`). En el osciloscopio la tipo A es despreciable.
- **Propagación de varianzas**: **numérica** (diferencias finitas centradas). Sin derivada simbólica.

## Pendientes menores

- Valores de **apreciación** concretos de los instrumentos analógicos del laboratorio (para el seed).
- Incertidumbre del **eje X (tiempo)** del osciloscopio para P3 (tiempo de descarga por CURSOR).
- Magnitudes y fórmulas exactas de **P2/P3** (las de P1 sirven de ejemplo guía).
