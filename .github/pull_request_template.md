# Issue

<!-- Link al issue o "No aplica". -->

# Tipo de cambio

- [ ] **Funcionalidad** (`feat`): agrega una funcionalidad o capacidad nueva.
- [ ] **Refactor** (`refactor`): reestructura código existente sin cambiar su comportamiento.
- [ ] **Corrección** (`fix`): arregla un error o defecto identificado.
- [ ] **Mantenimiento** (`chore`): build, CI, dependencias, etc.
- [ ] **Documentación** (`docs`): cambios en la documentación o en el README.
- [ ] **Tests** (`test`): agrega o corrige tests sin cambiar código productivo.

# Descripción de los cambios

<!-- Qué cambia y por qué. Contexto del problema, decisiones tomadas y lo que quedó afuera.
     Si hay migraciones de base, cambios de API o de la siembra, decirlo explícitamente. -->

# Capturas

<!-- Capturas o video si cambia la UI (claro y oscuro si aplica). "No aplica" si no hay cambios visuales. -->

# Cómo probar

<!-- Pasos para probarlo a mano: usuario con el que entrar, vista, qué hacer y qué se espera ver.
     Usuarios de prueba (con SEED_DEMO_DATA=true): docente@quantify.local / docente123,
     estudiante@quantify.local / estudiante123. -->

1.

# Checklist

- [ ] El título de la PR sigue [Conventional Commits](https://www.conventionalcommits.org/es/v1.0.0/)
- [ ] Basada en `development` (o es una promoción: `development` → `staging` → `main`)
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y `cargo test` pasan
- [ ] `npm test` y `npm run test:e2e` pasan
- [ ] Agregué o actualicé tests para lo que cambia
- [ ] Actualicé la documentación (`docs/`, README) si corresponde
