# Cómo contribuir

## Ramas y flujo de trabajo

```
feat/*, fix/*, …  →  development  →  staging  →  main
                                                  ↓
                                           release-please
                                         (versión, tag, CHANGELOG)
```

| Rama | Para qué | Cómo entra código |
|------|----------|-------------------|
| `development` | Integración diaria. Es la rama **default**: toda PR de trabajo apunta acá. | PR desde una rama de trabajo, **squash** |
| `staging` | Pre-producción: se prueba lo que va a salir. Se llena promoviendo `development` entero. | PR desde `development`, **merge commit** |
| `main` | Producción. Acá corre release-please. | PR desde `staging`, **merge commit** |

Nadie pushea directo a ninguna de las tres: todo entra por PR (las reglas del repo lo exigen).

```bash
git checkout development
git pull
git checkout -b feat/crear-practicas
# ...cambios y commits...
git push -u origin feat/crear-practicas
gh pr create --base development
```

Las ramas de trabajo llevan el mismo prefijo que el tipo del commit: `feat/...`, `fix/...`,
`refactor/...`, `docs/...`, `chore/...`, `test/...`.

### Promociones

```bash
gh pr create --base staging --head development --title "chore: promueve development a staging"
gh pr create --base main --head staging --title "chore: promueve staging a main"
```

Se mergean con **merge commit**, no con squash: un squash juntaría todas las funcionalidades del
ciclo en un solo commit y release-please leería un único `chore`, con el CHANGELOG vacío. Con merge
commit cada `feat:` y `fix:` que entró a `development` llega intacto a `main`.

## Commits

Usamos [Conventional Commits](https://www.conventionalcommits.org/es/v1.0.0/), en español, en
minúscula y sin punto final:

```
<tipo>(<scope opcional>): <descripción>
```

No es cosmético: release-please lee el historial de `main` para decidir la versión y armar el
CHANGELOG.

| Tipo | Cuándo | Efecto en la versión |
|------|--------|----------------------|
| `feat` | Funcionalidad nueva | minor (0.1.0 → 0.2.0) |
| `fix` | Corrección de un error | patch (0.1.0 → 0.1.1) |
| `perf` | Mejora de rendimiento | patch |
| `refactor` | Cambio de estructura sin cambiar comportamiento | ninguno |
| `docs` | Documentación | ninguno |
| `test` | Tests | ninguno |
| `chore` | Build, CI, dependencias | ninguno |
| `style` | Formato, sin cambios de lógica | ninguno |

El scope es el área que toca: `cursos`, `practicas`, `hidrostatica`, `ui`, `e2e`, `plan`, etc.
Un cambio que rompe compatibilidad lleva `!` (`feat(api)!: ...`) o un footer `BREAKING CHANGE:`.
Un breaking change sube la major aunque la versión sea 0.x: de 0.4.2 pasa a 1.0.0. Usalo solo para
un cambio que de verdad rompa algo para quien usa la app.

## Pull requests

- **El título sigue las mismas reglas que un commit.** Las PRs a `development` se mergean con
  squash, así que el título es lo único que queda en el historial. El check `Conventional commit`
  lo valida.
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

- Los checks de CI y `Conventional commit` en verde.
- Todas las conversaciones resueltas.
- En `development`: squash (`gh pr merge --squash`). En `staging` y `main`: merge commit
  (`gh pr merge --merge`).

## Releases

release-please corre en cada push a `main` y mantiene abierta una PR `chore(main): release X.Y.Z`
que sube la versión de `Cargo.toml`/`Cargo.lock` y escribe el `CHANGELOG.md` con los `feat`, `fix`
y `perf` desde el último release. Mergearla crea el tag `vX.Y.Z` y el release en GitHub.

Esa PR la abre el token de Actions, así que la CI no corre sobre ella: como solo cambia la versión
y el CHANGELOG, el admin la mergea salteando los checks obligatorios (las reglas de `main` le
permiten ese bypass solo por PR).
