---
name: backend-observability
description: >
  Logs estructurados, métricas de los Four Golden Signals, trazas distribuidas,
  correlación por request-id, niveles correctos, qué NUNCA loguear. Cargá al
  instrumentar un servicio o investigar un incidente.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Backend observability

## Cuándo usar

- Crear un servicio nuevo o un endpoint sensible.
- Investigar un incidente y faltan datos.
- Definir alertas o dashboards.

## Pilares

1. **Logs** — eventos discretos, con contexto, estructurados (JSON).
2. **Métricas** — series temporales agregables.
3. **Trazas** — el camino de un request por varios servicios.

Sin los tres, falta una pata: logs sin métricas no escalan, métricas sin trazas no explican causa.

## Logs

- **Estructurados** (`{"ts":..., "level":..., "msg":..., "request_id":..., ...}`), nunca strings sueltos.
- **Niveles bien usados**:
  - `DEBUG` — detalle de desarrollo. Off en producción.
  - `INFO` — eventos esperados (request servido, job ok). Bajo volumen.
  - `WARN` — anomalía recuperada (reintento, fallback). Mirar tendencias.
  - `ERROR` — fallo no recuperado en un request/job. Debería alertar si sube.
  - `FATAL` — el proceso no puede continuar.
- **Un log por evento de negocio.** No logs decorativos cada 3 líneas.
- **`request_id`** propagado en todo log de la cadena. Si no podés correlacionar, no podés debuggear.

### Qué NUNCA loguear

- Contraseñas, tokens, claves, header `Authorization`, cookies de sesión.
- PII completa: número de tarjeta, RUT/DNI, mail, teléfono. Hashear o truncar si hace falta.
- Cuerpo de request/response salvo en debug local explícito.
- Stack traces a clientes (sí internamente).

## Métricas — Four Golden Signals

Para todo servicio:

1. **Latency** — p50, p95, p99 por endpoint. Promedio miente.
2. **Traffic** — requests/seg, por ruta y método.
3. **Errors** — tasa de 5xx (y 4xx interesantes como 429).
4. **Saturation** — CPU, memoria, conexiones a BD, cola.

Para jobs: éxito/falla/duración/cola.

### Convenciones

- Histogramas, no solo contadores, para latencias.
- Cardinalidad bajo control: `user_id` como label rompe Prometheus.
- Nombres consistentes: `http_request_duration_seconds`, `db_query_duration_seconds`.

## Trazas

- OpenTelemetry como estándar. Cliente + auto-instrumentación + exportador a Jaeger/Tempo/Honeycomb.
- Propagá `traceparent` entre servicios.
- Span por unidad de trabajo significativa (request, query, llamada externa, job).
- Atributos útiles: `http.route`, `db.statement` (sanitizado), `messaging.destination`.

## Health checks

- `/health/live` — el proceso responde. Para Kubernetes liveness.
- `/health/ready` — listo para tráfico (BD ok, dependencias críticas ok). Para readiness.
- **No mezclar**. `live` no debería tocar BD.

## Alertas

- Alertá sobre **síntomas del usuario** (latencia alta, error rate >X%), no sobre causas (CPU 80%).
- Toda alerta tiene runbook con: qué significa, qué chequear, qué hacer.
- Umbrales basados en SLO, no en intuición.

## Reglas duras

- **Sin `console.log` ni `print` directos** en producción.
- **Sin loguear en loop apretado** sin sampling.
- **Sin métrica nueva sin dashboard.** Si no se mira, no existe.
- **Sin alerta sin runbook.** Ruido garantizado.

## Validación

- Generá un request en local y seguilo por log, métrica y traza.
- Caos test: matá la BD, ¿la alerta dispara en <5 min?
- Revisión periódica: ¿qué alertas dispararon? ¿cuáles fueron ruido?

## Relacionadas

[[backend-api-design]] · [[backend-performance]] · [[security-secrets]]
