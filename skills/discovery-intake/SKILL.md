---
name: discovery-intake
description: >
  Discovery previo al SDD: convierte una idea vaga en un problema claro y verificable antes de escribir la especificación. Entrevista breve por cinco ejes, caza de "palabras comadreja" (términos ambiguos que no son verificables) con el tool `spec_lint`, y un artefacto de problema afilado que se entrega al flujo SDD.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: propia (local-first, sin spawnear procesos)
  activation: bajo_demanda
  version: "1.0"
---

# Discovery — afila la idea antes del SDD

## Cuándo usar esta skill
Cuando la persona llega con una idea o pedido **sin especificar** ("quiero construir X", "necesito un sistema que…") y todavía no hay requisitos claros. Es el **paso 0**, antes de `propose`/`spec` del flujo SDD. Si el pedido ya trae requisitos verificables, omite este paso.

## Objetivo
Convertir una idea vaga en un **enunciado de problema claro + criterios de éxito verificables + incógnitas abiertas**, para que el SDD arranque sobre base sólida. IEEE 29148 exige requisitos **no ambiguos y verificables**: esta skill produce esa base.

## Cómo conducir la entrevista
Pregunta de a dos o tres, no todo junto. Refleja de vuelta lo que entendiste. Cinco ejes:

1. **Problema y usuario** — ¿qué problema se resuelve y para quién? (el dolor, no la solución)
2. **Éxito** — ¿cómo sabremos que quedó bien? (criterios observables o medibles)
3. **Restricciones** — límites reales (tiempo, stack, datos, integraciones, "esto no va")
4. **Fuera de alcance** — qué queda explícitamente afuera (evita el crecimiento del alcance)
5. **Incógnitas y riesgo** — qué es lo más incierto o peligroso

## Caza de palabras ambiguas (obligatoria)
Antes de cerrar, revisa el borrador con el detector de Turtle: llama al tool MCP `spec_lint` (o corre `turtle spec-lint`) sobre el texto. Por cada término que devuelva —rápido, escalable, fácil, manejar, algunos, "según sea necesario", etc.— exige un **número o un criterio observable**. No cierres el Discovery con "palabras comadreja" sin concretar.

Ejemplo: *"debe ser rápido"* → ¿qué latencia objetivo? (p. ej. p95 < 200 ms en el checkout). *"manejar los pagos"* → ¿qué operaciones exactas? (crear intento, confirmar, reembolsar).

## La compuerta
No avances al SDD hasta que se cumplan las tres:
- El problema y el usuario están claros.
- Hay al menos un criterio de éxito **verificable**.
- `spec_lint` no marca términos ambiguos sin concretar (o los que quedan están listados como **incógnitas abiertas** explícitas).

Si algo falta, dilo y vuelve a preguntar. La compuerta protege al SDD de arrancar sobre una idea borrosa.

## El artefacto (guardar en memoria)
Al pasar la compuerta, guarda el resultado con `memory_save` (tipo `note`, `topic_key: sdd/<cambio>/discovery`):

```
Discovery: <título del cambio>
- Problema: …
- Usuario/beneficiario: …
- Criterios de éxito (verificables): …
- Restricciones: …
- Fuera de alcance: …
- Incógnitas abiertas: …
```

## Handoff al SDD
Con el artefacto guardado, entrega el control a la fase `propose`/`spec` del flujo SDD (skill `sdd-flow`), que lo levanta por su `topic_key`. La persona de SDD ya no arranca en borroso: parte de un problema afilado y verificable.
