---
name: security-secrets
description: >
  Manejo de secretos: variables de entorno, .env, gestores (Vault, AWS Secrets,
  GCP SM, doppler), rotación, detección de fugas, qué hacer si se filtra uno.
  Cargá al introducir nuevos secretos o si encontrás uno en el repo.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Manejo de secretos

## Cuándo usar

- Introducir o rotar un secreto (API key, token, conexión a BD, clave de firma).
- Encontrar un secreto commiteado en el repo o en los logs.
- Diseñar el flujo de despliegue.

## Reglas duras

- **NUNCA** commitear secretos. Ni de "test", ni de "staging", ni "es solo lectura".
- **NUNCA** en logs, mensajes de error, tickets, chats, prompts a un agente.
- **NUNCA** como argumento de CLI (queda en `ps`, en history). Usá `--secret-file` o stdin.
- **NUNCA** en URL (queda en logs de proxy, navegador, referrer).
- **NUNCA** un único secreto compartido por persona. Identidad por persona/servicio.

## Dónde guardarlos

Por orden de preferencia:

1. **Gestor dedicado**: HashiCorp Vault, AWS Secrets Manager, GCP Secret Manager, Azure Key Vault, Doppler, 1Password Secrets Automation.
2. **Variables de entorno** inyectadas por el orquestador (Kubernetes Secrets + sealed-secrets, Docker secrets, systemd `EnvironmentFile` con permisos restrictivos).
3. **Archivo `.env` local** solo para desarrollo, en `.gitignore`, **nunca** subido.

## .env

- `.gitignore` incluye `.env`, `.env.*`, `*.pem`, `*.key`, `id_rsa*`, `secrets.*`.
- Commiteá `.env.example` con **claves vacías**, sin valores.
- `chmod 600` en archivos con secretos.

## Rotación

- **Toda credencial tiene fecha de expiración** y un dueño.
- Rotá: al sospechar fuga, al irse alguien con acceso, periódicamente (≤90 días para producción).
- Diseñá la app para tolerar **dos secretos válidos a la vez** durante la rotación (old + new), para no romper en el corte.

## Detección de fugas

- **Pre-commit hook**: `gitleaks` o `trufflehog`.
- **CI**: scan en cada PR.
- **Server-side**: GitHub/GitLab secret scanning activado.
- **Monitor de público**: alertas si tu organización aparece en gitleaks/trufflehog feeds.

## Si se filtra uno

1. **Asumí compromiso.** Si estuvo en GitHub público aunque sea 1 min, asumilo.
2. **Rotá** ya. No esperes "a ver si alguien lo usó".
3. **Revocá** el viejo. No solo crear nuevo.
4. **Auditá uso**: logs del servicio para ver si hubo acceso anómalo desde que se expuso.
5. **Reescribir historia** del git (BFG / git-filter-repo) **no reemplaza** la rotación. El secreto ya salió.
6. **Postmortem**: ¿cómo entró al repo? Bloquear la causa.

## Buenas prácticas por tipo

### API keys de terceros

- Una por entorno (dev/staging/prod).
- Mínimos permisos requeridos (no "admin").
- Restringir por IP o dominio cuando el proveedor lo permita.

### Conexiones a BD

- Usuario por servicio, no usuario compartido.
- Permisos mínimos (no `SUPERUSER`).
- Conexión por socket o TLS, nunca claro.

### Claves de firma (JWT, webhooks)

- Asimétricas (RS/Ed) cuando distribuís verificación.
- Rotación con `kid` (key ID) para soportar varias activas.

### Tokens personales (PAT, deploy keys)

- A nombre de **un service account**, no de una persona.
- Expiración corta (días, no años).

## Reglas duras (recordatorio)

- **Sin secretos en código.** Ni en tests, ni en fixtures, ni en CI scripts.
- **Sin secretos en imágenes Docker.** Usá build secrets / mount en runtime.
- **Sin secretos en URLs ni en argv.**
- **Sin claves SSH compartidas.**

## Validación

- Pre-commit + CI con gitleaks/trufflehog activo y bloqueante.
- Inventario de secretos con dueño, expiración, último uso.
- Drill: rotar un secreto en producción sin downtime, una vez por trimestre.

## Relacionadas

[[security-owasp]] · [[security-authn-authz]] · [[security-supply-chain]] · [[backend-observability]]
