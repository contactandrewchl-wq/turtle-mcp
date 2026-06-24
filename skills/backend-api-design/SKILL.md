---
name: backend-api-design
description: >
  Diseño de APIs HTTP/REST y JSON: recursos, verbos, códigos de estado,
  versionado, paginación, idempotencia, errores estructurados, contratos
  documentados. Cargá al diseñar un endpoint nuevo o cambiar un contrato.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Backend API design

## Cuándo usar

- Crear un endpoint nuevo o cambiar uno existente.
- Diseñar un contrato entre frontend y backend o entre servicios.
- Revisar un PR que toca rutas, payloads o códigos de estado.

## Recursos y verbos

Pensá en **recursos** (sustantivos), no en acciones:

| Acción | Verbo | Ruta |
|---|---|---|
| Listar | `GET` | `/usuarios` |
| Obtener | `GET` | `/usuarios/{id}` |
| Crear | `POST` | `/usuarios` |
| Reemplazar | `PUT` | `/usuarios/{id}` |
| Modificar parcial | `PATCH` | `/usuarios/{id}` |
| Borrar | `DELETE` | `/usuarios/{id}` |
| Sub-recurso | `GET` | `/usuarios/{id}/sesiones` |

Acciones que no son CRUD: `POST /usuarios/{id}/activar`. No `POST /activarUsuario`.

## Códigos de estado

Los más usados, sin inventar:

- **200** OK — éxito con cuerpo.
- **201** Created — creado, devolvé `Location` con la URL.
- **204** No Content — éxito sin cuerpo (típico `DELETE`).
- **400** Bad Request — payload o parámetro inválido.
- **401** Unauthorized — falta credencial o es inválida.
- **403** Forbidden — autenticado, pero no autorizado.
- **404** Not Found — el recurso no existe.
- **409** Conflict — conflicto de estado (duplicado, versión).
- **422** Unprocessable — sintaxis ok, semántica no.
- **429** Too Many Requests — rate limit.
- **500** Internal Server Error — bug nuestro.
- **503** Service Unavailable — caído / mantenimiento.

**Nunca 200 con `{ "error": ... }`.** El estado va en el HTTP.

## Errores estructurados

Adoptá un formato único en toda la API. Recomendado: RFC 9457 (Problem Details):

```json
{
  "type": "https://api.tu-app.com/errors/usuario-no-encontrado",
  "title": "Usuario no encontrado",
  "status": 404,
  "detail": "No existe un usuario con id 'abc123'.",
  "instance": "/usuarios/abc123"
}
```

Incluí campos extras útiles: `errors` (lista de fallos por campo), `traceId`.
**Nunca** devuelvas stack traces ni nombres de tablas a un cliente.

## Versionado

- **En la URL** (`/v1/usuarios`) — simple, cacheable, visible.
- O **en el header** (`Accept: application/vnd.tuapp.v1+json`) — para puristas.
- Subí versión solo en cambio incompatible. Agregar campo opcional **no** rompe.

## Paginación

Siempre paginá listas con potencial de crecer:

```
GET /usuarios?cursor=eyJpZCI6MTAwfQ&limit=50
```

Respuesta:

```json
{ "items": [...], "next_cursor": "eyJpZCI6MTUwfQ", "limit": 50 }
```

Cursor > offset: offset rompe si entran/salen filas. `limit` con tope server-side.

## Idempotencia

- `GET`, `PUT`, `DELETE` deben ser idempotentes por contrato HTTP.
- `POST` que crea recurso: aceptá header `Idempotency-Key` y deduplicá por N horas. Pagos, mensajes, jobs **siempre** idempotentes.

## Filtros, orden, búsqueda

- Filtros: `?status=active&role=admin`.
- Orden: `?sort=-created_at` (`-` = desc).
- Búsqueda: `?q=...`. Diferenciá de filtros exactos.

## Validación y contrato

- Defina el contrato en **OpenAPI / JSON Schema** y generá tipos para el cliente.
- Validá entrada **en el borde** (DTO/schema), no dentro de la lógica.
- Documentá ejemplos en la spec, no solo tipos.

## Reglas duras

- **Sin secretos en URLs** (van en headers o body).
- **Sin verbos en rutas** (`/getUser` ❌ → `GET /users/{id}` ✅).
- **Sin breaking changes silenciosos.** Deprecar primero (header `Deprecation`, `Sunset`), remover después.
- **Toda mutación loguea** quién + qué + cuándo, sin PII completa.
- **Rate limit por defecto.** 429 con header `Retry-After`.

## Validación

- Tests por endpoint: happy path + cada código de error.
- Tests E2E del contrato (Pact, Dredd, schemathesis).
- Docs actualizadas en el **mismo PR** que cambia el contrato.

## Relacionadas

[[backend-data-modeling]] · [[backend-observability]] · [[security-authn-authz]] · [[security-owasp]]
