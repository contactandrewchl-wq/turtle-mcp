---
name: security-owasp
description: >
  OWASP Top 10 (2021) aplicado: cómo detectarlas y prevenirlas en el código que
  escribís. Inyección, autenticación rota, exposición de datos, XXE, control de
  acceso, configuración insegura, XSS, deserialización, dependencias, logging.
  Cargá al diseñar o revisar código expuesto a entrada externa.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Security — OWASP Top 10 aplicado

## Cuándo usar

- Diseñar o revisar un endpoint público.
- Auditoría de seguridad antes de release.
- Triage de un reporte de vulnerabilidad.

Complementa a [[secure-by-default]] (lista corta always-on); esta es la profunda.

## A01 — Broken Access Control

**Síntoma:** un usuario accede o modifica algo que no debería.

- **Por defecto denegar.** Permisos explícitos, no implícitos.
- Autorización **en el servidor**, en cada handler. Cliente solo oculta UI.
- **IDOR**: nunca confíes en IDs del cliente. Verificá ownership en cada acceso a recurso (`WHERE id=? AND owner_id=?`).
- Sin métodos HTTP sin protección: `PUT/DELETE/POST` por error rutean a un handler abierto.

Ver [[security-authn-authz]] para el detalle.

## A02 — Cryptographic Failures

- **HTTPS siempre.** Sin "interno está bien sin TLS".
- **Hash de contraseñas:** `argon2id` (preferido) o `bcrypt(12+)`. Nunca MD5, SHA1, SHA-256 pelado.
- **Cifrado:** AES-GCM, ChaCha20-Poly1305. Sin ECB, sin CBC sin MAC.
- **TLS ≥1.2**, idealmente 1.3. Cipher suites modernas.
- Datos sensibles **cifrados en reposo** (BD, backups, logs).
- Tokens con suficiente entropía: 128+ bits, generados con CSPRNG (`crypto.randomBytes`, `secrets.token_bytes`).

## A03 — Injection

**SQL, NoSQL, OS command, LDAP, ORM raw, template.**

- **Parametrización SIEMPRE.** Sin `f"SELECT * FROM x WHERE id={id}"`.
- Para queries dinámicas seguras: query builder con bindings o stored procs parametrizados.
- **Shell**: sin `exec(string)`; usá array de args (`spawn(cmd, [args...])`) o evitá del todo.
- **Template injection**: no metas input de usuario en strings que después rendea un template engine sin escape.
- **Validá whitelist** (lo que sí), no blacklist (lo que no).

## A04 — Insecure Design

- Modelá amenazas antes de codear lo crítico (STRIDE como base).
- Límites por defecto: rate limit, max size, max depth, timeout.
- Patrones de fallo seguro: si el authz falla → denegar, no permitir.

## A05 — Security Misconfiguration

- **Sin defaults inseguros en producción.** Admin/admin, debug=true, CORS `*`.
- **Headers de seguridad**: `Strict-Transport-Security`, `Content-Security-Policy`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy`.
- Errores de servidor → mensaje genérico al cliente, detalle al log.
- Endpoints de debug, métricas, profilers → cerrados o autenticados.

## A06 — Vulnerable Components

- Inventario de dependencias (SBOM). Ver [[security-supply-chain]].
- Actualizá; no quedes en versiones EOL.
- Auditorías automáticas en CI: `npm audit`, `cargo audit`, `pip-audit`, `osv-scanner`.

## A07 — Identification and Authentication Failures

Ver [[security-authn-authz]]. En corto:

- MFA disponible y empujado para roles sensibles.
- Sesiones con expiración + rotación al cambio de privilegio.
- Bloqueo gradual de fuerza bruta (rate + delay + CAPTCHA), no bloqueo permanente fácil de explotar.

## A08 — Software and Data Integrity Failures

- Verificá firmas/checksums de artefactos descargados.
- No `curl | sh` sin pinning ni checksum (a menos que sea instalación interactiva con consentimiento).
- Lockfiles commiteados (`package-lock.json`, `Cargo.lock`, `poetry.lock`).
- Webhooks verificados con HMAC.

## A09 — Security Logging and Monitoring

- Logueá: login (éxito/fallo), cambios de permisos, accesos a datos sensibles, errores 5xx.
- **NO logueés** contraseñas, tokens, PII completa. Ver [[security-secrets]] y [[backend-observability]].
- Alertas accionables (no ruido).
- Retención auditable según regulación (Ley 21.719 en Chile: revisar requisitos).

## A10 — Server-Side Request Forgery (SSRF)

- Si tu servidor hace requests HTTP a URLs que vienen del cliente: peligro.
- **Whitelist de dominios** o **bloqueá** rangos privados (10.x, 172.16-31, 192.168, 169.254, localhost).
- Resolución DNS controlada (sin TOCTOU).
- Timeout corto, sin seguir redirects a otros dominios.

## Validación

- Tests negativos: payloads de inyección, IDs ajenos, tokens manipulados.
- ZAP / Burp en staging para fuzz básico.
- `semgrep --config p/owasp-top-ten` en CI.
- Revisión de seguridad documentada en cada cambio que toca authz, criptografía o input externo.

## Relacionadas

[[secure-by-default]] · [[security-authn-authz]] · [[security-secrets]] · [[security-supply-chain]]
