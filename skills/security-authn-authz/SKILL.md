---
name: security-authn-authz
description: >
  Autenticación y autorización: hashing de contraseñas, sesiones vs tokens
  (JWT, opaque), OAuth2/OIDC, MFA, RBAC vs ABAC, principio de menor privilegio,
  manejo de sesión y logout. Cargá al implementar login, registro o permisos.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Autenticación y autorización

## Cuándo usar

- Implementar registro, login, logout, reset de contraseña, MFA.
- Diseñar el modelo de permisos.
- Resolver un bug que toca sesión o autorización.

## Distinguir

- **Autenticación (authn)**: ¿quién sos? (login).
- **Autorización (authz)**: ¿qué podés hacer? (permisos).
- Son independientes. Casi todo el daño viene de mezclarlas.

## Contraseñas

- Hash con **`argon2id`** (preferido, parámetros: 64 MB, 3 iter, 4 paralelismo como base) o **`bcrypt` con cost ≥12**.
- **Nunca** MD5, SHA-1, SHA-256, ni "salt + sha" caseros.
- Reglas: longitud mínima 12, sin requerir mezcla forzada de mayúsculas/símbolos (NIST 800-63B). Sí permitir hasta ≥64 chars, incluir emojis.
- Verificá contra **breached password lists** (HaveIBeenPwned API con k-anonymity).
- **Reset por email**: link de un solo uso, expira en ≤1 h, invalida sesiones activas.

## Sesiones

Dos modelos, elegí uno y respetalo:

### Server session (recomendado para apps web)

- Cookie `Set-Cookie: session=<opaque-id>; HttpOnly; Secure; SameSite=Lax; Path=/`.
- Sesión almacenada server-side (Redis, BD). Invalidable instantánea.
- Rotación de ID al elevar privilegios (post-login, post-MFA).

### JWT (para APIs stateless o SPAs)

- Access token corto (5–15 min) + refresh token largo (días/semanas).
- **Firmá** con RS256/EdDSA (asymétrico) si distribuís verificación a varios servicios; HS256 si todo es un solo servicio.
- **Nunca confíes en `alg: none`.** Verificá `alg` en una whitelist.
- Refresh token rotativo y revocable (almacenado server-side).
- **JWT en `localStorage` queda expuesto a XSS.** Para web, preferí cookie HttpOnly.

## MFA

- TOTP (RFC 6238) por defecto: apps tipo Authy/Google Authenticator.
- WebAuthn / passkeys para experiencia moderna y phishing-resistant.
- SMS solo como fallback, no como factor principal (SIM swap).
- Códigos de recuperación: 8 códigos one-time, mostrados una vez, hash en BD.

## OAuth2 / OIDC

- **Authorization Code + PKCE** para cualquier app pública (SPA, mobile, CLI).
- **Client Credentials** para service-to-service.
- `state` siempre presente (anti-CSRF en el flow).
- `nonce` en OIDC para prevenir replay.
- Tokens **nunca** en URL, salvo el `code` corto.

## Modelos de permisos

| Modelo | Cuándo |
|---|---|
| **RBAC** (roles) | Sistema con pocos roles definidos (admin/user/guest). |
| **ABAC** (atributos) | Permisos por atributos del recurso, contexto, tenant. |
| **ReBAC** (relaciones) | Grafos de permisos (Google Drive, GitHub). Considerá OpenFGA / SpiceDB. |

**Por defecto denegar**: si no hay regla que permita, se rechaza.

### Verificación

Cada handler protegido pregunta lo mismo:

1. ¿Está autenticado? → 401 si no.
2. ¿Tiene permiso para esta acción sobre este recurso específico? → 403 si no.

**Sin** confiar en hidden inputs, query params, o ausencia del botón en UI.

## Logout y revocación

- Logout invalida la sesión **server-side** (no solo borra la cookie).
- "Cerrar sesión en todos los dispositivos" → invalidar todas las sesiones del usuario.
- Cambio de contraseña → invalidar todas las sesiones activas.

## Reglas duras

- **Sin esquemas de auth caseros.** Usá una librería probada y reciente.
- **Sin secretos hardcodeados** ni en repos. Ver [[security-secrets]].
- **Sin permisos verificados solo en el frontend.**
- **Sin tokens con expiración infinita.**
- **Sin mensajes que distingan** "usuario no existe" de "contraseña incorrecta" en login.

## Validación

- Test: usuario A no puede leer/modificar recurso de usuario B (IDOR).
- Test: token expirado/manipulado/firmado por otro emisor → 401.
- Test: tras cambio de contraseña, token viejo → 401.
- Auditoría manual del flujo de reset y MFA.

## Relacionadas

[[security-owasp]] · [[security-secrets]] · [[secure-by-default]]
