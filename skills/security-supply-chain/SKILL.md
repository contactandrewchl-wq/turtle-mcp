---
name: security-supply-chain
description: >
  Seguridad de la cadena de suministro: lockfiles, auditorías de dependencias,
  SBOM, firmas, builds reproducibles, scripts de install, supply-chain attacks
  típicos. Cargá al agregar o actualizar dependencias, o al configurar CI/CD.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Supply chain security

## Cuándo usar

- Agregar o actualizar una dependencia.
- Configurar CI/CD o un pipeline de publicación.
- Diseñar el proceso de release.

## Por qué importa

Una dependencia comprometida ejecuta código en tu build, en tus tests y en
producción. Históricos: `event-stream`, `colors.js`/`faker.js`, `xz-utils` (CVE-2024-3094),
typosquatting masivo en npm/PyPI.

## Reglas duras

- **Lockfile commiteado siempre.** `package-lock.json`, `Cargo.lock`, `poetry.lock`, `pnpm-lock.yaml`, `go.sum`.
- **Sin instalar `latest` en runtime**. Pinneá versiones; actualizá con PR.
- **Sin `npm install` sin lockfile en CI.** Usá `npm ci`, `pnpm install --frozen-lockfile`, `cargo install --locked`.
- **Sin scripts `postinstall` de paquetes no auditados** en entornos sensibles. `npm install --ignore-scripts` cuando se pueda.
- **Sin descargas `curl | sh`** sin checksum / firma verificada.

## Auditoría continua

- `npm audit` / `pnpm audit` / `yarn audit`
- `cargo audit` (Rust)
- `pip-audit` (Python)
- `osv-scanner` (multi-ecosistema, base OSV)
- `govulncheck` (Go)
- `bundler-audit` (Ruby)

Corré en **CI** y bloqueá merges con vulnerabilidades altas/críticas sin justificación.

## SBOM

Generá un **Software Bill of Materials** por release:

- `syft` / `cyclonedx` para generación.
- Formato: CycloneDX o SPDX (estándar).
- Almacenalo junto al artefacto en el release.

Si te llaman por un CVE de una dep, tener SBOM = saber qué releases están afectados en minutos, no días.

## Firmas y verificación

- **Verificá firmas** de artefactos descargados: GPG, sigstore/cosign, Apple notarization.
- **Firmá tus releases**: `cosign sign` para imágenes y binarios.
- Para SLSA L3+: build reproducible + provenance attestation.

## Dependencias — antes de agregar una

Checklist rápido:

- [ ] ¿La necesitamos o lo hace el lenguaje/runtime?
- [ ] ¿Está mantenida? (último commit, issues abiertos, downloads).
- [ ] ¿Cuántas deps trae transitivamente? Más = más superficie.
- [ ] ¿Tiene licencia compatible con la nuestra?
- [ ] ¿Hay versión LTS o solo `0.0.x`?
- [ ] ¿La organización detrás es identificable?
- [ ] ¿Hay alternativa ya en uso en el repo?

## Typosquatting

- Confirmá el nombre exacto del paquete (sin guiones cambiados, sin caracteres unicode lookalike).
- Verificá el repo de origen, no solo el README en el registry.
- Sospechá de paquetes nuevos con muchas descargas y poco código.

## CI/CD seguro

- **Mínimos permisos** al runner: tokens read-only salvo cuando publican.
- **Sin secretos en pull requests de forks.** Usá `pull_request_target` con cuidado.
- **Pin de actions de GitHub por SHA**, no por tag (`uses: actions/checkout@<sha>`).
- **Aislamiento de runners** entre repos / clientes.
- **OIDC para cloud providers** (sin claves estáticas de larga vida).

## Build reproducible

- Lockfile + base image con digest (`FROM node:20@sha256:...`).
- Sin timestamps no determinísticos en artefactos.
- Mismo input → mismo output binario (verificable por terceros).

## Validación

- CI rompe si:
  - Hay vulnerabilidades críticas en deps sin excepción justificada.
  - El lockfile no se actualizó cuando cambió el manifest.
  - Hay archivos con secretos detectados.
- Revisar Dependabot/Renovate weekly; no merges automáticos en deps de alto privilegio sin revisión.

## Si una dep aparece comprometida

1. **Identificá** versiones afectadas (CVE + advisory).
2. **Bloqueá** instalación de esas versiones (`overrides` / `resolutions`).
3. **Actualizá** a fix o forkeá temporal.
4. **Re-buildeá y re-deployá**. Asumí ejecución en CI/dev/prod.
5. **Auditá**: ¿qué corrió esa dep? ¿qué pudo exfiltrar?
6. **Rotá** secretos a los que tuvo acceso.

## Relacionadas

[[security-owasp]] · [[security-secrets]] · [[secure-by-default]] · [[commit-hygiene]]
