# Cómo contribuir

## Ramas

Se trabaja en una rama a partir de `main`, con el mismo prefijo que el tipo del commit:
`feat/...`, `fix/...`, `refactor/...`, `docs/...`, `chore/...`, `test/...`.

## Commits

Usamos [Conventional Commits](https://www.conventionalcommits.org/es/v1.0.0/), en español, en
minúscula y sin punto final:

```
<tipo>(<scope opcional>): <descripción>
```

| Tipo | Cuándo |
|------|--------|
| `feat` | Funcionalidad nueva |
| `fix` | Corrección de un error |
| `refactor` | Cambio de estructura sin cambiar comportamiento |
| `docs` | Documentación |
| `test` | Tests |
| `chore` | Build, CI, dependencias |
| `style` | Formato, sin cambios de lógica |
| `perf` | Mejora de rendimiento |

El scope es el área que toca: `cursos`, `practicas`, `hidrostatica`, `ui`, `e2e`, `plan`, etc.
Un cambio que rompe compatibilidad lleva `!` (`feat(api)!: ...`).

## Pull requests

- **El título sigue las mismas reglas que un commit.** Las PRs a `main` se mergean con squash, así
  que el título es lo único que queda en el historial.
- **Completá la plantilla** (`.github/pull_request_template.md`): issue, tipo de cambio,
  descripción, capturas y cómo probarlo. La descripción explica qué cambia y por qué, las
  decisiones tomadas y lo que quedó afuera; si hay migraciones de base, cambios de API o de la
  siembra, se dicen explícitamente.
- Un cambio por PR. Si una PR crece mucho, partila.
- Sumá tests para lo que agregues o arregles.
- Antes de pushear corré lo mismo que corre CI:

  ```bash
  cargo fmt --check
  cargo clippy --all-targets --locked -- -D warnings
  cargo test --locked
  npm test
  npm run test:e2e
  ```

## Para mergear

- Todos los checks de CI en verde.
- Todas las conversaciones resueltas.
- Squash merge (`gh pr merge --squash`).
