---
name: agent-orchestration
description: >
  Coordinar varias personas/agentes sobre el bus asíncrono de Turtle (mensajería, bandeja, actividad, relaciones) respetando que Turtle no lanza ni controla procesos.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: propia (adapta patrones de maestro-orchestrator y Agent Mail al bus asíncrono de Turtle, SIN spawning)
  activation: bajo_demanda
  version: "1.0"
---

# Orquestación de agentes (sobre el bus de Turtle)

Coordinar varias personas/agentes que trabajan hacia una misma meta, secuenciando, relevando y desbloqueando el trabajo a través del bus asíncrono de Turtle, sin pisarse y dejando rastro observable.

## Cuándo usar

Cuando dos o más personas (rótulos distintos) avanzan sobre una misma meta y hace falta:
- ordenar quién hace qué y en qué momento (secuenciar),
- pasar trabajo de una persona a otra sin perder contexto (relevar),
- destrabar a alguien que espera un insumo (desbloquear),
- evitar que dos personas toquen el mismo módulo a la vez (no pisarse).

Si una sola persona resuelve la meta de punta a punta, no necesitas orquestación: trabaja directo.

## Límite de alcance (leer primero)

Turtle NO lanza, spawnea, mata ni controla procesos. El SRS (sección 1.2) lo fija sin ambigüedad: "TURTLE no es un orquestador que lance o controle agentes; la comunicación entre agentes es asíncrona y mediada por la base de datos, no un canal de baja latencia."

Por lo tanto, en Turtle **orquestar = secuenciar y relevar trabajo por el bus**. Quien ejecuta cada agente es el usuario o el runtime de cada cliente, nunca Turtle.

Está prohibido en esta skill:
- proponer spawnear, lanzar, reiniciar o matar procesos o agentes,
- asumir un canal síncrono de baja latencia entre agentes,
- diseñar cualquier "control" sobre el ciclo de vida de otro agente.

Todo lo que sigue se apoya solo en mensajes asíncronos persistidos en la base.

## Primitivas del bus

- **message_send** — envía un mensaje dirigido por rótulo (relevo a una persona) o en difusión (convocatoria a varias). Es el acto central de coordinar.
- **inbox / bandeja** — la persona destino lee su bandeja cuando arranca o retoma; ahí encuentra relevos y solicitudes pendientes. Reemplaza al polling.
- **agents_list** — muestra qué rótulos están activos en este momento; sirve para saber a quién puedes delegar o convocar sin escribir al vacío.
- **events_list / actividad** — feed observable de lo que ocurre; el coordinador lo vigila para detectar avances, huecos y bloqueos.
- **relation_add / relations_list** — declara relaciones entre memorias/áreas para deduplicar y marcar conflictos; es como se señala "esto choca con aquello" en vez de bloquear.

## Patrones (adaptados, sin spawning)

- **Relevo dirigido (handoff por rótulo)**: una persona termina su parte y hace `message_send` al rótulo siguiente con el contexto necesario (qué quedó hecho, qué falta, dónde está). El destino lo recoge en su bandeja al retomar.
- **Convocatoria / difusión**: para arrancar una meta o pedir insumos a varios dominios, se difunde un mensaje a los rótulos involucrados; cada uno responde por el bus cuando puede.
- **Pistas paralelas por dominio**: rótulos distintos (p. ej. backend, frontend, seguridad) avanzan en paralelo en sus áreas y se sincronizan con mensajes en los puntos de encuentro (contratos, esquemas, fechas de integración).
- **Reserva de área** (adaptación del file-reservation de Maestro): antes de tocar un módulo o archivo, la persona lo **anuncia en la actividad** ("tomo el módulo X"). No se bloquea ningún proceso. Si otra persona ya lo anunció, el solape se marca con `relation_add` como conflicto y se arbitra por el bus.
- **Secuenciación SDD**: la meta avanza por fases spec → diseño → implementación → revisión. Cada gate se confirma por mensaje: la fase siguiente no arranca hasta recibir la confirmación de la anterior en bandeja.

## El rol coordinador

La persona orquestadora (rótulo `orquestador`):
- secuencia el trabajo: define el orden de relevos y abre cada fase con un mensaje,
- vigila `events_list` para ver el avance real y detectar bloqueos o huecos,
- despeja bloqueos: identifica quién espera qué y dirige el insumo al rótulo correcto,
- arbitra conflictos: cuando dos áreas chocan, marca el conflicto con relations y decide la prioridad de orden.

Lo que el coordinador **no** hace: no implementa código, no define el diseño ni decide por las otras personas dentro de su dominio. Coordina el flujo, no el contenido técnico ajeno.

## Anti-patrones

- **Intentar lanzar o controlar procesos**: está fuera de alcance (SRS 1.2); Turtle no tiene esa capacidad ni debe simularla.
- **Coordinar fuera del bus** (chat lateral, acuerdos verbales): se pierde la trazabilidad y la actividad queda con huecos.
- **Hacer polling** preguntando "¿ya?" en vez de confiar en la bandeja: genera ruido y asume baja latencia que no existe.
- **Coordinador que además implementa**: mezcla árbitro y parte; pierde la vista global y sesga las decisiones.
- **Resolver conflictos en silencio**: editar el área de otro sin marcar el solape con relations rompe la dedup.

## Integración con Turtle

- [[sdd-flow]] define la secuencia de fases (spec → diseño → implementación → revisión) que el coordinador hace cumplir gate por gate.
- [[turtle-protocol]] fija el contrato de sesión y mensajería: cómo registrarse con rótulo y cómo enviar/leer mensajes correctamente.
- [[commit-hygiene]] asegura que cada relevo deje un rastro limpio: el trabajo entregado en un handoff llega con commits ordenados y atribuibles.

## Reglas duras

- Toda coordinación pasa por el bus (message_send, bandeja, actividad, relations). Nada de canales laterales.
- Ningún proceso se lanza ni se mata desde aquí. Jamás.
- Cada relevo nombra el **rótulo destino** y entrega **contexto** suficiente (estado, pendientes, ubicación).
- Los conflictos se marcan con `relation_add`, no se resuelven en silencio ni editando el área ajena sin aviso.
- El coordinador secuencia y arbitra; no implementa ni decide diseño por otras personas.

## Validación

La coordinación funcionó si:
- cada relevo aparece entregado en la **bandeja** del rótulo destino, con contexto legible,
- la **actividad** (`events_list`) refleja el flujo sin huecos: cada fase abierta tiene su confirmación de cierre por mensaje,
- no quedan **conflictos sin marcar**: todo solape de área figura en `relations_list`,
- ningún registro indica intento de lanzar o controlar procesos,
- el rótulo `orquestador` no aparece como autor de implementación ni de decisiones de diseño ajenas.
