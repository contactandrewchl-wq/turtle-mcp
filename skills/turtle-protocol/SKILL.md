---
name: turtle-protocol
description: Protocolo de memoria de Turtle. Usalo en cualquier proyecto con Turtle instalado para decidir cuándo buscar y guardar memorias, cómo recuperar contexto tras una compactación, y para responder en español latino neutro.
license: Apache-2.0
metadata:
  type: comportamiento
  origin: propia
  activation: permanente
  size_budget_tokens: 800
  version: "1.0"
---

# Protocolo de memoria de Turtle

Turtle te da **memoria persistente** entre sesiones y **coordinación** con otros agentes, vía sus herramientas MCP. Seguí este protocolo.

## Al empezar (o tras una compactación)

- El hook `session-start` ya te inyectó las memorias recientes del proyecto. Si necesitás más, usá `memory_search` y `context_get`.
- Tras una **compactación de contexto**, no reconstruyas de memoria: recuperá con `context_get` (proyecto + tarea) y `memory_search` lo que te falte.

## Antes de responder algo no trivial

- Buscá primero: `memory_search "<términos>"` en el proyecto actual. Si hay una decisión, convención o corrección previa, respetala.
- Recuperá el contenido completo solo cuando lo necesites: `memory_get(id)`. Cuidá el presupuesto de tokens; el índice ya trae título y resumen.

## Cuándo guardar (`memory_save`)

Guardá cuando aparezca algo que valga la pena recordar entre sesiones:

- **decision** — una decisión de diseño y su porqué.
- **architecture** — cómo está estructurado algo.
- **correction** — un error y cómo se corrigió (para no repetirlo).
- **convention** — una convención del proyecto.
- **note** — cualquier otra cosa útil.

No guardes lo trivial ni lo que ya está en el código/README. Un buen `summary` de una línea hace barata la búsqueda.

## Coordinación entre agentes

- Si trabajás como un rol declarado, los relevos pendientes te llegan al iniciar sesión.
- Para dejarle algo a otro rol: `message_send` (dirigido a un rótulo o por difusión). Para ver actividad: `events_list`; para ver quién está activo: `agents_list`.

## Idioma

Respondé a la persona en **español latino neutro** (sin voseo regional marcado ni anglicismos innecesarios), salvo que te pida otro idioma.
