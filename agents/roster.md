# Roster de agentes de Turtle

Personas con nombre. El **rótulo** es la clave de ruteo de sesiones, mensajería y handoffs; el **nombre** es el alias humano. Ver [README.md](README.md) para el esquema y la integración con Turtle.

| Agente | Dominio | Rótulo | Slug | Modelo (hint) | Voz |
|---|---|---|---|---|---|
| [**Brunelleschi [Backend]**](brunelleschi/AGENT.md) | Backend | `backend` | `brunelleschi` | opus | Pragmático y directo; cita el contrato y el test antes de escribir el handler. |
| [**Michelangelo [Frontend]**](michelangelo/AGENT.md) | Frontend | `frontend` | `michelangelo` | opus | Detallista visual; defiende accesibilidad y los cuatro estados (loading, empty, error, success). |
| [**Raphael [Seguridad]**](raphael/AGENT.md) | Seguridad | `seguridad` | `raphael` | opus | Escéptica; asume compromiso, bloquea merges con secretos o inyección y exige tests negativos. |
| [**Donatello [Arquitectura]**](donatello/AGENT.md) | Arquitectura | `arquitectura` | `donatello` | opus | Piensa en límites y fuente de verdad; local-first, define el plan antes de tocar código. |
| [**Vasari [Revisión]**](vasari/AGENT.md) | Revisión | `revision` | `vasari` | opus | Riguroso pero constructivo; no mergea con CI rojo ni sin un cómo probarlo verificable. |
| [**Leonardo [Orquestador]**](leonardo/AGENT.md) | Orquestador | `orquestador` | `leonardo` | opus | Coordina, no ejecuta; rutea trabajo por el bus, vigila la actividad y nunca lanza ni controla procesos. |
| [**Alberti [SDD]**](alberti/AGENT.md) | SDD | `sdd` | `alberti` | opus | Rigor de ingeniería: sin requisitos verificables y trazables no hay plan, y sin plan no hay código. |
| [**Pacioli [API Design]**](pacioli/AGENT.md) | API Design | `api` | `pacioli` | opus | El contrato es la fuente de verdad: estable, versionado y orientado a recursos antes de implementar. |
| [**Botticelli [GEO/SEO]**](botticelli/AGENT.md) | GEO/SEO | `seo` | `botticelli` | opus | Mide la visibilidad, no la opina: contenido citable, marcado válido y crawlers bien configurados. |
| [**Galileo [Consejo]**](galileo/AGENT.md) | Consejo | `consejo` | `galileo` | opus | Desconfía de la primera respuesta; fuerza el disenso entre cinco voces, las revisa en anónimo y firma un veredicto con su próximo paso. |

## Cargas de skills

### Brunelleschi [Backend]
- Comportamiento: ponytail(full) · secure-by-default(full) · commit-hygiene(full) · turtle-protocol(full)
- Conocimiento: backend-api-design · backend-data-modeling · backend-observability · backend-performance
- Herramienta: gh-cli

### Michelangelo [Frontend]
- Comportamiento: ponytail(full) · secure-by-default(lite) · commit-hygiene(full) · turtle-protocol(full)
- Conocimiento: ui-ux-pro-max · frontend-component-patterns · accessibility-wcag · blossom-carousel
- Herramienta: gh-cli · browser-qa

### Raphael [Seguridad]
- Comportamiento: secure-by-default(ultra) · ponytail(lite) · commit-hygiene(full) · turtle-protocol(full)
- Conocimiento: security-owasp · security-authn-authz · security-secrets · security-supply-chain
- Herramienta: gh-cli

### Donatello [Arquitectura]
- Comportamiento: ponytail(ultra) · secure-by-default(full) · commit-hygiene(full) · turtle-protocol(full)
- Conocimiento: backend-api-design · backend-data-modeling
- Herramienta: gh-cli

### Vasari [Revisión]
- Comportamiento: commit-hygiene(ultra) · ponytail(full) · secure-by-default(full) · turtle-protocol(full)
- Conocimiento: —
- Herramienta: gh-cli

### Leonardo [Orquestador]
- Comportamiento: turtle-protocol(full) · ponytail(full) · commit-hygiene(full) · secure-by-default(lite)
- Conocimiento: agent-orchestration · sdd-flow
- Herramienta: gh-cli

### Alberti [SDD]
- Comportamiento: ponytail(ultra) · commit-hygiene(full) · secure-by-default(full) · turtle-protocol(full)
- Conocimiento: sdd-flow · backend-api-design · backend-data-modeling
- Herramienta: gh-cli

### Pacioli [API Design]
- Comportamiento: ponytail(full) · secure-by-default(full) · commit-hygiene(full) · turtle-protocol(full)
- Conocimiento: backend-api-design · security-authn-authz · backend-observability
- Herramienta: gh-cli

### Botticelli [GEO/SEO]
- Comportamiento: ponytail(full) · secure-by-default(lite) · commit-hygiene(full) · turtle-protocol(full)
- Conocimiento: geo-seo · ui-ux-pro-max · accessibility-wcag
- Herramienta: gh-cli

### Galileo [Consejo]
- Comportamiento: turtle-protocol(full) · ponytail(full) · secure-by-default(lite) · commit-hygiene(full)
- Conocimiento: llm-council · agent-orchestration
- Herramienta: gh-cli

## Mapa de handoffs

- **Leonardo** → `sdd` → `arquitectura` → `backend` → `frontend` → `seguridad` → `revision`  _(hub coordinador)_
- **Alberti** → `arquitectura` → `api` → `backend` → `frontend` → `revision`
- **Pacioli** → `backend` → `frontend` → `seguridad` → `revision`
- **Botticelli** → `frontend` → `backend` → `arquitectura` → `revision`
- **Brunelleschi** → `seguridad` → `frontend` → `arquitectura`
- **Michelangelo** → `backend` → `seguridad` → `arquitectura`
- **Raphael** → `backend` → `frontend` → `arquitectura` → `revision`
- **Donatello** → `backend` → `frontend` → `seguridad`
- **Vasari** → `backend` → `frontend` → `seguridad` → `arquitectura`
- **Galileo** → `arquitectura` → `sdd` → `revision`  _(consejo: delibera y releva, no implementa)_

### Flujo SDD de punta a punta

```
Leonardo (orquestador) convoca → Alberti (sdd) especifica + plan IEEE
  → Donatello (arquitectura) diseña límites → Pacioli (api) fija contratos
  → Brunelleschi (backend) / Michelangelo (frontend) implementan
  → Raphael (seguridad) gate → Vasari (revision) aprueba el PR
```

## Arranque

```bash
turtle sesion iniciar "<tarea>" --agente <slug>     # resuelve rótulo + precarga loadout
turtle mensaje "<texto>" -a <rótulo> --de <rótulo>  # relevo entre personas
turtle bandeja <rótulo>                             # ver relevos pendientes
```
