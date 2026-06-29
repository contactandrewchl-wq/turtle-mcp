---
name: llm-council
description: >
  Someter una decisión de peso a un consejo de cinco voces adversariales que discuten, se revisan en anónimo y entregan un veredicto sintetizado y trazable, para contrarrestar la complacencia del agente y la respuesta apresurada. Cargá al enfrentar una decisión importante con riesgo de sesgo, encuadre dudoso o "primera respuesta" demasiado cómoda.
license: MIT
metadata:
  type: conocimiento
  origin: adapta tenfoldmarc/llm-council-skill (MIT) al bus asíncrono de Turtle, SIN spawning
  activation: bajo_demanda
  version: "1.0"
---

# Consejo deliberativo (LLM Council, sobre Turtle)

Forzar el desacuerdo productivo antes de decidir. En vez de confiar en la primera respuesta (cómoda y complaciente por defecto), la decisión se somete a **cinco voces con perspectivas enfrentadas** que la atacan desde ángulos distintos, se **revisan entre sí en anónimo**, y un **presidente** sintetiza un veredicto con su próximo paso. El objetivo no es consenso tibio: es exponer el modo de falla, el encuadre equivocado y el punto ciego antes de que cuesten caro.

## Cuándo usar

Cuando una decisión cumple al menos una:
- **Pesa y es difícil de revertir** (arquitectura, modelo de datos, contrato público, elección de stack, pricing).
- **Tenés una respuesta y te convence demasiado rápido** — señal de juicio rápido, no de análisis.
- **El encuadre del problema es dudoso**: quizá estás resolviendo la pregunta equivocada.
- **Hay sesgo de complacencia**: el agente tiende a darte la razón en lugar de discutir.

Si la decisión es trivial, reversible de un commit o ya tiene un camino obvio y barato, **no convoques al consejo**: decidí directo (ver [[ponytail]]). El consejo cuesta deliberación; usalo donde el error es caro.

## Límite de alcance (leer primero)

Este método **no spawnea, lanza ni controla procesos**, en línea con el SRS §1.2 y con [[agent-orchestration]]:

> "TURTLE no es un orquestador que lance o controle agentes; la comunicación entre agentes es asíncrona y mediada por la base de datos."

Las "cinco voces" **no son cinco procesos**. Son cinco **perspectivas**. Se materializan de una de dos formas, nunca spawneando:

- **Modo mono-sesión (por defecto):** una sola sesión recorre las cinco voces como *lentes secuenciales* (estilo "sombreros de pensamiento"). Barato, sin coordinación externa, sirve para la mayoría de las decisiones.
- **Modo por el bus (cuando hay personas activas):** el presidente **convoca por difusión** (`message_send`) a personas reales del roster que aportan su voz por la **bandeja**, de forma asíncrona. Útil cuando la decisión cruza dominios reales (ej. que [[security-owasp]] hable por boca de Raphael). Sigue siendo bus asíncrono: nadie lanza a nadie.

## Las cinco voces

Cada voz tiene un mandato único y **debe** discrepar de las demás; si todas coinciden, el consejo falló (no hubo presión real).

1. **La Contraria** — abogada del diablo. Busca el modo de falla: ¿cómo se rompe esto, qué supuesto es frágil, qué pasa en el peor caso? No propone; ataca.
2. **El de Primeros Principios** — cuestiona el encuadre. ¿Es este el problema correcto? ¿Qué damos por sentado que no deberíamos? Reconstruye desde cero.
3. **La Expansionista** — saca a la luz el upside que nadie miró: oportunidades, efectos de segundo orden positivos, la versión ambiciosa de la idea.
4. **El Forastero** — mirada sin contexto. Llega de cero y dice lo obvio que el experto ya naturalizó. Detecta la complejidad innecesaria y la jerga que esconde un hueco.
5. **La Ejecutora** — aterriza. ¿Qué hago el lunes a la mañana? Pasos concretos, costo real, primer entregable. Sin esto, el consejo es filosofía.

## Proceso

1. **Reunir contexto.** Antes de deliberar, buscar lo que ya se sabe: `memory_search` sobre el tema, `CLAUDE.md` y memorias del proyecto, restricciones conocidas. El consejo decide informado, no en el vacío.
2. **Deliberar las cinco voces.** Cada voz produce su lectura de la decisión desde su mandato. En mono-sesión, se escriben una tras otra **sin suavizar** las contradicciones. En modo bus, se difunde la consulta y se recogen las respuestas de la bandeja.
3. **Peer-review anónimo.** Cada voz revisa los aportes de las otras **sin saber de quién es cada uno** (se etiquetan A–E, no por nombre). Marca: dónde hay **acuerdo**, dónde **chocan**, qué **punto ciego** comparten todas. El anonimato evita que el peso de una persona/rol gane por autoridad en vez de por argumento.
4. **Síntesis del presidente.** El presidente (rótulo `consejo`) integra todo en un veredicto estructurado:
   - **Zonas de acuerdo** — lo que ninguna voz discute.
   - **Choques** — las tensiones reales y qué las resuelve (o por qué quedan abiertas).
   - **Puntos ciegos** — lo que el grupo casi pasa por alto.
   - **Recomendación** — la decisión, con su razón.
   - **Próximo paso** — la acción accionable inmediata (mandato de la Ejecutora).
5. **Asentar el veredicto.** Guardar el resultado como **memoria tipo `decision`** con `memory_save` (What / Why / Where / Learned). Esa decisión queda buscable y sobrevive a la compactación. Opcionalmente, dejar el transcript de la deliberación como artefacto `.md` y registrar el cierre en la actividad (`events`).

## Salida (mundo Turtle, no HTML)

El council original emite HTML interactivo. En Turtle, terminal-first, la salida es:
- **Veredicto → memoria `decision`** (fuente de verdad, recuperable con `memory_get`).
- **Transcript opcional → `.md`** con las cinco voces y el peer-review, para auditoría.
- **Cierre → evento** en el feed de actividad, para trazabilidad.

Nada de procesos, nada de servicios, nada de claves: solo memoria y bus.

## Integración con Turtle

- [[turtle-protocol]] — cuándo buscar/guardar memoria, supervivencia a la compactación y respuesta en español latino neutro; el veredicto se guarda según su contrato.
- [[agent-orchestration]] — provee el modo por el bus: convocatoria por difusión, bandeja y actividad, **sin spawning**.
- [[ponytail]] — el filtro de entrada: si la decisión no amerita el costo del consejo, se decide directo.
- [[commit-hygiene]] — si el veredicto deriva en cambios, llegan en commits limpios y atribuibles.

## Anti-patrones

- **Consejo de sí-señores:** cinco voces que coinciden. Si no hubo choque, no hubo consejo; volvé a forzar el disenso.
- **Spawnear "agentes":** está fuera de alcance (SRS §1.2). Las voces son perspectivas o personas convocadas por el bus, jamás procesos lanzados.
- **Peer-review con nombres:** revisar sabiendo quién dijo qué reintroduce el sesgo de autoridad que el anonimato evita.
- **Veredicto sin próximo paso:** una recomendación que no dice qué hacer el lunes es filosofía, no decisión.
- **No asentarlo:** un veredicto que no se guarda como `decision` se pierde y se vuelve a deliberar lo mismo.
- **Convocar para todo:** el consejo es caro; usado en decisiones triviales, es teatro.

## Reglas duras

1. **Las cinco voces discrepan.** El valor está en la tensión; el acuerdo unánime es señal de falla, no de éxito.
2. **El peer-review es anónimo.** Las voces se etiquetan A–E; se juzga el argumento, no el autor.
3. **Cero spawning.** Las voces son perspectivas (mono-sesión) o personas convocadas por el bus (asíncrono); nunca procesos.
4. **Todo veredicto termina en una `decision` guardada** con What/Why/Where/Learned, recuperable después.
5. **Todo veredicto trae su próximo paso** accionable; sin eso no se cierra.
6. **El consejo se justifica por el costo del error.** Si la decisión es barata de revertir, no se convoca ([[ponytail]]).

## Validación

La deliberación funcionó si:
- el veredicto nombra **al menos un choque real** entre voces y cómo se resolvió,
- expone **al menos un punto ciego** que la primera respuesta no veía,
- queda **una memoria `decision`** guardada y buscable con su próximo paso,
- en modo bus, cada voz convocada aparece respondida en la **bandeja** y el cierre figura en la **actividad**,
- ningún registro indica intento de lanzar o controlar procesos.
