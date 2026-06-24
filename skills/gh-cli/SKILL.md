---
name: gh-cli
description: >
  Operar GitHub desde la terminal con `gh`: PRs (crear, revisar, merge),
  issues, runs de CI, releases, gists, API cruda. Cargá cuando vayas a
  interactuar con GitHub desde la línea de comandos en lugar del navegador.
license: Apache-2.0
metadata:
  type: herramienta
  origin: cli/cli
  activation: bajo_demanda
  requires: ["gh autenticado en el entorno"]
  version: "1.0"
---

# GitHub CLI (`gh`)

## Cuándo usar

- Abrir o revisar un PR.
- Chequear estado de CI o logs de un run fallido.
- Crear o triagear issues.
- Hacer una llamada cruda a la API de GitHub sin escribir cURL.

## Setup mínimo

```bash
gh auth status           # ¿estás logueado?
gh auth login            # OAuth interactivo si no lo estás
gh repo set-default      # fijar el repo "current" del directorio
```

## Pull requests

```bash
# Crear PR desde la rama actual (abre editor; --fill toma title/body del commit)
gh pr create --base main --fill
gh pr create --title "feat: x" --body-file PR_BODY.md

# Listar y filtrar
gh pr list --state open --author "@me"
gh pr list --label bug --assignee "@me"

# Ver detalle / diff / checks
gh pr view 123
gh pr view 123 --web                 # abrir en navegador
gh pr diff 123
gh pr checks 123

# Revisar
gh pr review 123 --approve -b "LGTM"
gh pr review 123 --request-changes -b "Ver comentarios"
gh pr comment 123 -b "Texto"

# Merge (squash por defecto en muchos repos)
gh pr merge 123 --squash --delete-branch
gh pr merge 123 --merge                # commit de merge
gh pr merge 123 --rebase               # rebase
```

## Issues

```bash
gh issue list --state open --label bug
gh issue create --title "..." --body "..." --label bug --assignee "@me"
gh issue view 42
gh issue comment 42 -b "Texto"
gh issue close 42 --reason "not planned"
```

## CI / runs

```bash
gh run list                              # últimos workflow runs
gh run list --workflow=ci.yml --branch main
gh run view <run-id>                     # detalle
gh run view <run-id> --log               # logs completos
gh run view <run-id> --log-failed        # solo lo que falló
gh run rerun <run-id>                    # reintentar
gh run rerun <run-id> --failed           # solo jobs fallidos
gh run watch <run-id>                    # esperar a que termine
```

## Releases

```bash
gh release create v1.2.0 --generate-notes
gh release create v1.2.0 ./dist/* --notes "Cambios..."
gh release list
gh release view v1.2.0
gh release upload v1.2.0 ./dist/extra.tar.gz
```

## API cruda

Cuando `gh` no tiene el comando:

```bash
gh api repos/{owner}/{repo}/pulls/123/comments
gh api -X POST repos/{owner}/{repo}/issues -f title="..." -f body="..."
gh api graphql -f query='query{viewer{login}}'
```

`{owner}/{repo}` se autocompletan con el repo actual si usás `gh repo set-default`.

## Flujo típico para un cambio

```bash
git switch -c feat/x
# ...código + commits limpios (ver [[commit-hygiene]])...
git push -u origin feat/x
gh pr create --fill
gh pr checks --watch                   # esperar CI
gh pr merge --squash --delete-branch
```

## Reglas

- **`gh auth status` antes** si no sabés en qué cuenta estás (especialmente con varias cuentas/orgs).
- **Sin `--force` en merges/pushes** salvo pedido explícito.
- **Sin merge si CI rojo.** Arreglar primero.
- **PR description** es el espacio para el "por qué"; el commit es para el "qué".

## Validación

- `gh --version` reciente (≥2.x).
- `gh auth status` con scopes mínimos necesarios (`repo`, `workflow` si corresponde).

## Relacionadas

[[commit-hygiene]]
