---
name: commit-hygiene
description: >
  Reglas compactas de commit y PR. Mensajes en conventional commits, un cambio
  por commit, sin co-autoría automática del agente, sin --no-verify. Aplica
  cada vez que vayas a registrar o publicar cambios.
license: Apache-2.0
metadata:
  type: comportamiento
  activation: permanente
  levels: [lite, full, ultra, off]
  size_budget_tokens: 500
  version: "1.0"
---

# Commit hygiene

## Mensaje

Formato conventional commits, en imperativo, en el idioma del proyecto:

```
<tipo>(<scope opcional>): <qué cambió, no por qué>
```

Tipos: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `build`, `ci`.

El **por qué** va en el cuerpo, no en el título. El título describe el cambio,
no la tarea ni el PR.

## Reglas

- **Un cambio lógico por commit.** Si no se puede describir en una frase, son dos.
- **No mezclar** refactor con cambio de comportamiento en el mismo commit.
- **Sin `--no-verify`** salvo pedido explícito del usuario. Si el hook falla, arreglá la causa.
- **Sin co-autoría automática** del agente. No agregar `Co-Authored-By: Claude` u otra atribución salvo pedido explícito.
- **Sin amend a commits publicados.** Nuevo commit, no reescritura.

## PR

- Título = título del commit principal.
- Cuerpo: **qué**, **por qué**, **cómo probarlo** (lista verificable).
- Un PR ≈ un objetivo. Si el diff toca varias áreas no relacionadas, dividilo.

## Niveles

- **lite** — solo el formato del título.
- **full** — formato + cuerpo + reglas duras (por defecto).
- **ultra** — además, bloquear PRs sin "cómo probarlo" y sin tests cuando hay cambio de comportamiento.
- **off** — desactivada.
