# Catálogo de skills de Turtle

Índice de las skills semilla incluidas en Turtle, clasificadas según
RF-SKL-01 / Apéndice A del SRS. La búsqueda barata (`skill_search`) devuelve
estos metadatos; el contenido completo se carga con `skill_get(<nombre>)`
(RF-SKL-03 / RF-SKL-04).

## Tipos

- **comportamiento** — siempre activas, compactas, niveles `lite/full/ultra/off` (RF-SKL-07, RF-SKL-08).
- **conocimiento** — guías técnicas, bajo demanda.
- **herramienta** — instructivo para operar una herramienta externa, bajo demanda.

## Comportamiento (always-on)

| Nombre | Activación / niveles | Uso |
|---|---|---|
| [`ponytail`](ponytail/SKILL.md) | permanente · lite/full/ultra/off | Anti-sobre-ingeniería. Escalera de decisión + atajos comentados. |
| [`secure-by-default`](secure-by-default/SKILL.md) | permanente · lite/full/ultra/off | Mentalidad de seguridad compacta: validar entrada, escapar salida, sin secretos en logs, denegar por defecto. |
| [`commit-hygiene`](commit-hygiene/SKILL.md) | permanente · lite/full/ultra/off | Conventional commits, un cambio por commit, sin co-autoría automática del agente, sin `--no-verify`. |
| [`turtle-protocol`](turtle-protocol/SKILL.md) | permanente | Cuándo buscar/guardar memoria, contexto tras compactación, coordinación entre agentes y respuesta en español latino neutro. |

## Conocimiento — frontend / UX (bajo demanda)

| Nombre | Origen | Uso |
|---|---|---|
| [`ui-ux-pro-max`](ui-ux-pro-max/SKILL.md) | `nextlevelbuilder/ui-ux-pro-max-skill` | Sistemas de color, tipografía, grid 8-pt, jerarquía, estados, dark mode, patrones por tipo de producto. |
| [`frontend-component-patterns`](frontend-component-patterns/SKILL.md) | propia | React/Vue/Svelte: composición, estado, async loading/error/empty, memoización dirigida. |
| [`accessibility-wcag`](accessibility-wcag/SKILL.md) | propia | WCAG 2.2 AA aplicada: semántica, teclado, foco, contraste, formularios accesibles. |
| [`blossom-carousel`](blossom-carousel/SKILL.md) | `jespervos/blossom-carousel` | Carrusel nativo con scroll-snap, sin JS pesado, accesible. |

## Conocimiento — backend (bajo demanda)

| Nombre | Uso |
|---|---|
| [`backend-api-design`](backend-api-design/SKILL.md) | Diseño REST/HTTP: recursos, verbos, códigos, versionado, paginación, idempotencia, errores estructurados. |
| [`backend-data-modeling`](backend-data-modeling/SKILL.md) | Esquema relacional, índices, migraciones zero-downtime, transacciones, integridad referencial. |
| [`backend-observability`](backend-observability/SKILL.md) | Logs estructurados, Four Golden Signals, trazas, qué nunca loguear. |
| [`backend-performance`](backend-performance/SKILL.md) | Medir antes de optimizar, N+1, caché con TTL e invalidación, batching, async, timeouts. |

## Conocimiento — ciberseguridad (bajo demanda)

| Nombre | Uso |
|---|---|
| [`security-owasp`](security-owasp/SKILL.md) | OWASP Top 10 (2021) aplicado a cómo detectar y prevenir cada categoría. |
| [`security-authn-authz`](security-authn-authz/SKILL.md) | Hashing, sesiones vs JWT, OAuth2/OIDC, MFA, RBAC/ABAC, IDOR. |
| [`security-secrets`](security-secrets/SKILL.md) | Gestores, rotación, detección de fugas, qué hacer si se filtra uno. |
| [`security-supply-chain`](security-supply-chain/SKILL.md) | Lockfiles, auditorías, SBOM, firmas, CI/CD seguro, typosquatting. |

## Conocimiento — proceso / coordinación (bajo demanda)

| Nombre | Origen | Uso |
|---|---|---|
| [`sdd-flow`](sdd-flow/SKILL.md) | propia (adapta `spec-driven-development-orchestrator`) | Desarrollo dirigido por especificación anclado a IEEE (29148, 1016, 1012, 29119, 12207): especificar → diseñar → planificar → implementar → verificar, con trazabilidad y método I/A/D/P. |
| [`agent-orchestration`](agent-orchestration/SKILL.md) | propia (adapta `maestro-orchestrator` / Agent Mail, sin spawning) | Coordinar varias personas sobre el bus asíncrono de Turtle (mensajería, bandeja, actividad, relaciones). Respeta el límite del SRS: nunca lanza ni controla procesos. |
| [`llm-council`](llm-council/SKILL.md) | adapta `tenfoldmarc/llm-council-skill` (MIT), sin spawning | Consejo deliberativo: somete una decisión a cinco voces adversariales que discuten, se revisan en anónimo y entregan un veredicto trazable (memoria `decision`). Contrarresta la complacencia; nunca lanza procesos. |

## Conocimiento — growth / SEO (bajo demanda)

| Nombre | Origen | Uso |
|---|---|---|
| [`geo-seo`](geo-seo/SKILL.md) | propia (destila `zubair-trabzada/geo-seo-claude`, MIT) | GEO + SEO: citability para IA, llms.txt, JSON-LD/schema, crawlers de IA, brand mentions, SEO técnico. Portable, sin Python. |

## Herramienta (bajo demanda)

| Nombre | Requiere | Uso |
|---|---|---|
| [`gh-cli`](gh-cli/SKILL.md) | `gh` autenticado | PRs, issues, runs de CI, releases, API cruda de GitHub desde la terminal. |
| [`browser-qa`](browser-qa/SKILL.md) | Playwright (Node) + navegadores, o Playwright MCP | QA de navegador y regresión visual: capturas, diff contra baseline, snapshot a11y, cross-browser. |

---

## Convenciones

Cada `SKILL.md` lleva frontmatter YAML compatible con el ecosistema de
skills (RF-SKL-10 / RNF-COM-02):

```yaml
---
name: <kebab-case>
description: >
  <descripción + disparador>
license: Apache-2.0
metadata:
  type: comportamiento | conocimiento | herramienta
  origin: <repo o "propia">
  activation: permanente | bajo_demanda
  levels: [lite, full, ultra, off]      # solo en comportamiento
  size_budget_tokens: <int>             # solo en comportamiento
  version: "<semver>"
---
```

Las skills **importadas** se tratan como contenido no confiable: no se ejecutan
sin acción explícita del usuario (RNF-SEG-05, RNF-RES-04).

## Cómo se enlazan

Las referencias entre skills usan la sintaxis `[[nombre]]` para mantener el
formato portable. El cargador resuelve el nombre contra el registro local.
