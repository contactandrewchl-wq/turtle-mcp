---
name: backend-data-modeling
description: >
  Modelado de datos relacional: esquema, claves, índices, migraciones seguras,
  transacciones, integridad referencial, tipos, soft delete, timestamps,
  estrategias contra N+1. Cargá al crear tablas o modificar el esquema.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Backend data modeling

## Cuándo usar

- Crear o modificar tablas, columnas, índices, constraints.
- Diseñar una migración que se va a correr en producción.
- Resolver un problema de performance que apunta a la base.

## Reglas de esquema

- **Una clave primaria por tabla.** UUID v7 / ULID si necesitás generarla del lado app; `bigserial` si la base la genera.
- **`NOT NULL` por defecto.** Permitir `NULL` es una decisión, no el descuido.
- **`DEFAULT` explícito** en columnas booleanas y de estado.
- **Tipos correctos**: `text` (no `varchar(N)` salvo restricción real), `timestamptz` (no `timestamp`), `numeric(p,s)` para dinero (**nunca** `float`/`double`).
- **`CHECK` constraints** para invariantes simples (`status IN (...)`, `monto >= 0`).
- **FK con `ON DELETE`** explícito: `CASCADE` / `SET NULL` / `RESTRICT`. Sin pensar = bomba de tiempo.

## Convenciones

- Nombres en `snake_case`, plural para tablas (`usuarios`, `sesiones`).
- PK: `id`. FK: `<tabla_singular>_id` (`usuario_id`).
- Timestamps en toda tabla: `created_at`, `updated_at` (`timestamptz NOT NULL DEFAULT now()`).
- Soft delete solo si hay requisito real de recuperación; preferí archivado a `deleted_at`.

## Índices

- **Toda FK lleva índice** (no se crea solo en Postgres).
- Índice compuesto en el orden de la query (`WHERE a=? AND b=?` → `(a,b)`, no `(b,a)`).
- Índices parciales para flags raros: `CREATE INDEX ... WHERE status='active'`.
- **No indexar por reflejo.** Cada índice cuesta en escritura; medí con `EXPLAIN ANALYZE` antes.

## Migraciones seguras (zero-downtime)

Para tablas grandes, dividí en pasos:

1. **Agregar columna `NULL`** (instantáneo en Postgres ≥11).
2. **Backfill por lotes** con cursor o batch.
3. **Agregar `NOT NULL`** y `DEFAULT` cuando esté lleno.
4. **Cambio de aplicación** que la use.
5. **Drop de la vieja** en migración separada y posterior.

**Nunca** en una sola migración: `ALTER ... ADD COLUMN NOT NULL DEFAULT ...` en tabla grande sin pensar; bloquea por minutos en algunos motores.

## Transacciones

- Toda operación que toca varias tablas y debe ser atómica: **transacción explícita**.
- Mantenelas **cortas**. Sin llamadas HTTP/red adentro.
- Nivel de aislamiento: `READ COMMITTED` por defecto; `SERIALIZABLE` solo donde hay condición de carrera real, y manejá `serialization_failure` con retry.
- Lockeo pesimista (`SELECT ... FOR UPDATE`) o optimista (versión + `WHERE version=?`) según el patrón.

## Integridad referencial

- **FKs activas en producción.** El argumento "lo valida la app" se rompe el día que dos apps escriben.
- Borrado: `RESTRICT` por defecto, `CASCADE` solo si la dependencia es de composición real.

## N+1 y carga

- Detectalo: query log + un test que cuente queries por endpoint.
- Resolvelo: `JOIN`, `IN`, o capas tipo DataLoader / Sequelize `include` / Prisma `include`.
- Paginá antes de consultar relaciones.

## Datos sensibles

- Hash (no cifrado) para contraseñas: `argon2id` o `bcrypt(12+)`.
- Cifrado a nivel columna para PII regulada (Ley 21.719, GDPR).
- Auditá quién accedió a tablas sensibles (trigger o middleware).

## Validación

- Migración reversible si es viable; si no, documentar por qué.
- Probar `up` y `down` en CI contra una base limpia.
- `EXPLAIN ANALYZE` en queries que toquen tablas >100k filas.
- Backup verificado antes de migraciones destructivas.

## Reglas duras

- **Sin migración a mano en producción.** Todo cambio en un archivo versionado.
- **Sin `SELECT *`** en código de aplicación (rompe contratos con cambios de esquema).
- **Sin queries dinámicas concatenadas.** Parámetros siempre.
- **Sin datos de prueba** en producción ni seeds de dev en migraciones compartidas.

## Relacionadas

[[backend-api-design]] · [[backend-performance]] · [[security-owasp]]
