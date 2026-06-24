---
name: backend-performance
description: >
  Diagnóstico y optimización de performance backend: medir antes de optimizar,
  cachés correctas, batching, índices, conexiones, async, perfiles de CPU/IO.
  Cargá cuando hay un cuello reportado o un endpoint crítico.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Backend performance

## Cuándo usar

- Latencia o throughput por debajo del SLO.
- Endpoint nuevo en ruta crítica.
- Antes de un evento de tráfico alto.

**No cargar** para optimizaciones especulativas sin medición.

## Disciplina

1. **Medí primero.** Sin perfil, no hay diagnóstico, hay opinión.
2. **Optimizá el cuello.** Mejorar lo que ya es rápido es trabajo perdido.
3. **Comprobá el efecto.** Si no medís después, no sabés si ayudó.

## Mediciones base

- **Latencia** p50/p95/p99 por endpoint.
- **Throughput** (req/s) sostenido sin degradar.
- **Recursos**: CPU, memoria, IO, conexiones a BD, cola.
- **Distribución**: ¿es lento siempre o por colas? Histograma > promedio.

## Herramientas

| Capa | Herramienta |
|---|---|
| CPU/IO/lock | `perf`, `pprof`, `py-spy`, `clinic.js`, `async-profiler` |
| Queries | `EXPLAIN ANALYZE`, `pg_stat_statements`, slow query log |
| Trazas | OpenTelemetry + Jaeger/Tempo (ver [[backend-observability]]) |
| HTTP | k6, vegeta, wrk para carga |
| Memoria | heap dump + analizador del runtime |

## Patrones que suelen pagar

### Base de datos (suele ser el 80%)

- **N+1** → eager loading / JOIN. Detectá con conteo de queries por request.
- **Índice faltante** en `WHERE`/`JOIN`/`ORDER BY` frecuente. `EXPLAIN` para confirmar uso.
- **`SELECT *`** → seleccioná solo lo que usás (menos IO, menos transferencia).
- **Pool de conexiones** del tamaño correcto (no infinito). `(núcleos * 2) + spindles` es una base.
- **Read replicas** para queries pesadas de lectura, si la consistencia eventual sirve.

### Caché

Niveles, del más barato al más caro:

1. **Memoization del proceso** — para cálculos puros, invalida con LRU.
2. **Caché distribuida** (Redis) — para datos compartidos. TTL razonable.
3. **CDN / edge** — para respuestas idempotentes públicas.

**Toda caché necesita estrategia de invalidación.** Sin esa, es bug en cámara lenta.
Cache stampede → `single-flight` o lock corto.

### Batching y bulk

- Operaciones en N items en una sola query, no N queries.
- `INSERT ... VALUES (...), (...), (...)` o `COPY` en Postgres.
- Para HTTP: agrupá calls a APIs externas en uno cuando lo permite.

### Async y colas

- Tareas que **no necesitan respuesta inmediata** → cola (BullMQ, Sidekiq, Celery, Tigris, jobs propios).
- Reintentos exponenciales con tope; DLQ para los que fallan persistente.
- Timeouts en toda llamada a red (HTTP, BD, cache). Sin timeout = hang propagado.

### Concurrencia

- Limitá la concurrencia entrante (rate limit, semáforo) para no inundar el downstream.
- En Node/Python async: cuidá CPU-bound dentro del event loop → worker threads / multiprocess.
- En Go/Rust: cuidá bloqueos largos en pools de goroutines/tasks.

## Reglas duras

- **Sin optimización sin perfil.** Microbench ≠ producción.
- **Sin caché sin TTL e invalidación pensadas.**
- **Sin llamada de red sin timeout.**
- **Sin retry sin backoff y tope.** Tormenta de retries = autoataque.

## Validación

- Test de carga reproducible (k6 script en repo) antes y después.
- p95 dentro de SLO bajo carga objetivo.
- Sin regresión de error rate ni de uso de recursos en el ambiente de stress.

## Relacionadas

[[backend-data-modeling]] · [[backend-observability]] · [[backend-api-design]]
