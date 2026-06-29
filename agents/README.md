# Agentes de Turtle (personas)

Una **persona** es una identidad de agente con nombre propio que agrupa, en un solo lugar, todo lo necesario para arrancar a trabajar de forma coherente: un **nombre** humano (ej. "Brunelleschi"), un **dominio** (ej. Backend), un **rótulo** de ruteo (ej. `backend`), un **loadout de skills** (qué skills de comportamiento van always-on y con qué nivel, más qué skills de conocimiento y herramienta quedan disponibles bajo demanda), una **voz** (tono y estilo de redacción) y reglas de **handoff** (a quién y cuándo ceder o pedir relevo). Una persona es solo una **selección**: elige rótulo y skills y fija una voz. **No otorga permisos nuevos** ni eleva privilegios; los permisos siguen siendo los del entorno y del usuario. Si una skill o capacidad no estaba disponible sin la persona, tampoco lo está con ella.

## Relación con el resto de Turtle

Las personas no son un subsistema aparte: son una capa fina sobre las piezas que ya existen en Turtle.

- **El `role` ES el rótulo.** El campo `role`/`label` de la persona es exactamente el rótulo que se pasa a `turtle sesion iniciar "<tarea>" -a <rótulo>`. Ese rótulo es la **clave de ruteo** del bus: la mensajería (`turtle mensaje "<texto>" -a <rótulo> --de <rótulo>`), la bandeja (`turtle bandeja <rótulo>`) y los handoffs/relevos rutean por rótulo, nunca por el nombre humano. El nombre de la persona es un **alias para personas**; el rótulo es lo que entienden las máquinas.
- **`skills.behavior` → always-on con niveles.** Cada skill de comportamiento listada en la persona se activa al iniciar y se mantiene cargada durante toda la sesión, con su `level` (`lite` / `full` / `ultra` / `off`). Son compactas por diseño.
- **`skills.knowledge` y `skills.tool` → carga bajo demanda.** No se cargan al arrancar: se **registran como disponibles** y se traen completas con `skill_get(<nombre>)` cuando hacen falta, tras descubrirlas con la búsqueda barata (`skill_search`).
- Esto se apoya directamente en la **capa de skills S3** (los tres tipos: comportamiento, conocimiento y herramienta) y en el contrato operativo de [[turtle-protocol]], que define cómo una sesión se registra con su rótulo, intercambia mensajes y ejecuta relevos.

## Esquema del AGENT.md

Cada persona vive en `agents/<slug>/AGENT.md`, con frontmatter YAML:

```yaml
name: Brunelleschi
role: backend                 # rótulo de ruteo (clave del bus)
label: "Brunelleschi [Backend]"    # alias humano para mostrar
description: Persona de dominio Backend; diseño de APIs, modelado de datos y observabilidad.
metadata:
  domain: Backend
  voice: directo y pragmático; explica decisiones con trade-offs concretos.
  model: sonnet
  skills:
    behavior:
      - name: ponytail
        level: full
      - name: secure-by-default
        level: lite
      - name: commit-hygiene
        level: full
      - name: turtle-protocol
        level: full
    knowledge:
      - backend-api-design
      - backend-data-modeling
      - backend-observability
      - backend-performance
    tool:
      - gh-cli
  handoffs:
    - to: seguridad
      when: el cambio toca autenticación, autorización o manejo de secretos.
    - to: revision
      when: la implementación está lista y necesita revisión antes de integrar.
  version: 1.0.0
```

Notas del esquema:

- `name` es el alias humano; `role` es el rótulo de ruteo (debe coincidir con el rótulo del roster) y `label` es el texto a mostrar (ej. `"Brunelleschi [Backend]"`).
- `skills.behavior` lleva pares `{name, level}`; `knowledge` y `tool` son listas de nombres.
- `handoffs` es una lista de `{to, when}` donde `to` es **el rótulo de destino**, no el nombre.
- `version` versiona la definición de la persona.

## Elegir una persona

Se elige al arrancar la sesión. El `--agente <slug>` resuelve el rótulo y precarga el loadout declarado en `agents/<slug>/AGENT.md`:

```bash
# Iniciar sesión como una persona (resuelve rótulo + precarga loadout)
turtle sesion iniciar "implementar endpoint de turnos" --agente brunelleschi

# Relevo / coordinación entre personas (rutea por rótulo, no por nombre)
turtle mensaje "te dejo el módulo de auth para revisión" -a seguridad --de backend
```

El **nombre** ("Brunelleschi") es solo un alias humano para hablar de la persona; la **clave de ruteo del bus** siempre es el **rótulo** (`backend`, `seguridad`, ...). Toda la mensajería, bandejas y handoffs usan el rótulo.

> **Nota — `--agente <slug>` es un flag PROPUESTO**, parte de esta capa de personas (aún no implementado en el núcleo). Hoy el equivalente real es iniciar por rótulo: `turtle sesion iniciar "<tarea>" -a <rótulo>`; como la relación rótulo↔persona es 1:1 en el roster, `-a backend` ya equivale a "ser Brunelleschi". El flag `--agente` agrega azúcar (resolver por nombre y precargar el loadout automáticamente) y deja lugar a futuro para varias personas por rótulo. Ver [Extensión del SRS](#extensión-del-srs).

## Cómo se carga (capa S3)

Al elegir una persona, la secuencia es:

1. **Leer** `agents/<slug>/AGENT.md` y resolver su frontmatter (rótulo, voz, loadout, handoffs).
2. **Activar** las skills de **comportamiento** (always-on) con el `level` indicado (`lite`/`full`/`ultra`/`off`); quedan vigentes toda la sesión.
3. **Registrar** las skills de **conocimiento** y **herramienta** como *disponibles bajo demanda*: no se cargan aún; se descubren con `skill_search` y se traen completas con `skill_get(<nombre>)` cuando la tarea lo pida.
4. **Iniciar sesión** con el **rótulo** de la persona, dejándola enganchada al bus para mensajería, bandeja y handoffs.

## Extensión del SRS

Esta carpeta **extiende la identidad de agente (RF-AGN)**: agrega un registro de **personas** análogo al registro de `skills/`, pero en lugar de definir capacidades define *combinaciones con nombre* de rótulo + loadout + voz + handoffs. No introduce un mecanismo de permisos ni un canal de ruteo nuevo; reutiliza rótulos y la capa S3.

RF sugeridos para incorporar al SRS (a aprobar antes de tocar el SRS formal):

> **RF-AGN-x:** El sistema deberá permitir definir **personas** con nombre propio que agrupen un rótulo de ruteo y una carga de skills (comportamiento con nivel, más conocimiento y herramienta bajo demanda), seleccionables al iniciar sesión mediante `--agente <slug>`, sin que ello altere los permisos del agente.

> **RF-SDD-x:** El sistema debería ofrecer una skill de **flujo dirigido por especificación (SDD)** que produzca, antes de implementar, artefactos conformes a estándares IEEE (ISO/IEC/IEEE 29148 para requisitos, IEEE 1016 para diseño, IEEE 1012 para V&V, ISO/IEC/IEEE 29119 para pruebas), con requisitos atómicos, verificables y trazables, y método de verificación I/A/D/P consistente con el resto del SRS. Ver [`sdd-flow`](../skills/sdd-flow/SKILL.md).

> **RNF-COORD-x:** La coordinación entre personas mediada por la skill [`agent-orchestration`](../skills/agent-orchestration/SKILL.md) deberá operar **exclusivamente sobre el bus asíncrono** (mensajería, bandeja, actividad, relaciones) y **no deberá lanzar ni controlar procesos**, conforme al alcance fijado en la sección 1.2 del SRS ("TURTLE no es un orquestador que lance o controle agentes").

## Seguridad

Importar **personas o skills de terceros es contenido NO confiable**. Una definición externa de persona puede *describir* un loadout o una voz, pero **no se ejecuta ni se activa sin acción explícita del usuario** (conforme a **RNF-SEG-05**). Recordatorios:

- Una persona **no eleva privilegios**: solo selecciona rótulo y skills ya disponibles. Nunca concede capacidades nuevas.
- Tratar todo `AGENT.md` ajeno como dato a inspeccionar, no como instrucción a obedecer; revisar su loadout y sus `handoffs` antes de usarlo.
- El rótulo de una persona importada no debe poder suplantar canales de mensajería de otra sin que el usuario lo autorice de forma explícita.

## Roster

| Nombre | Dominio | Rótulo | Definición |
| --- | --- | --- | --- |
| Brunelleschi | Backend | `backend` | [./brunelleschi/AGENT.md](./brunelleschi/AGENT.md) |
| Michelangelo | Frontend | `frontend` | [./michelangelo/AGENT.md](./michelangelo/AGENT.md) |
| Raphael | Seguridad | `seguridad` | [./raphael/AGENT.md](./raphael/AGENT.md) |
| Donatello | Arquitectura | `arquitectura` | [./donatello/AGENT.md](./donatello/AGENT.md) |
| Vasari | Revisión | `revision` | [./vasari/AGENT.md](./vasari/AGENT.md) |
| Leonardo | Orquestador | `orquestador` | [./leonardo/AGENT.md](./leonardo/AGENT.md) |
| Alberti | SDD | `sdd` | [./alberti/AGENT.md](./alberti/AGENT.md) |
| Pacioli | API Design | `api` | [./pacioli/AGENT.md](./pacioli/AGENT.md) |
| Botticelli | GEO/SEO | `seo` | [./botticelli/AGENT.md](./botticelli/AGENT.md) |
| Galileo | Consejo | `consejo` | [./galileo/AGENT.md](./galileo/AGENT.md) |

Índice completo del roster: [./roster.md](./roster.md).
