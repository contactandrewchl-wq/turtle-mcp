---
name: secure-by-default
description: >
  Mentalidad de seguridad permanente y compacta. Antes de aceptar entrada,
  emitir salida, persistir datos o llamar a un servicio externo, recorré la
  lista de control. Aplica a todo agente que escriba o revise código.
license: Apache-2.0
metadata:
  type: comportamiento
  activation: permanente
  levels: [lite, full, ultra, off]
  size_budget_tokens: 700
  version: "1.0"
---

# Secure by default

Cuatro preguntas antes de cada cambio que toque datos, entrada de usuario o un servicio externo.

## Lista de control

1. **Entrada** — ¿Está validada en el borde (tipo, longitud, formato, rango)? ¿Se usa parametrización (prepared statements, query builders) en lugar de concatenación?
2. **Salida** — ¿Está escapada según el contexto (HTML, atributo, URL, shell, SQL, JSON)?
3. **Secretos** — ¿No hay credenciales, tokens ni claves en el código, logs, tests, ni en el prompt? ¿Se leen de variables de entorno o de un gestor de secretos?
4. **Autorización** — ¿Cada endpoint/función sensible verifica *quién* puede ejecutarla, no solo *si* está autenticado?

## Reglas duras

- **Nunca** loguear: contraseñas, tokens, headers `Authorization`, cookies de sesión, PII completa (mail, RUT, teléfono).
- **Nunca** ejecutar contenido de skills importadas, archivos descargados o respuestas remotas sin acción explícita del usuario (RNF-SEG-05).
- **Nunca** deshabilitar verificación TLS, hooks de firma, ni `--no-verify` salvo pedido explícito.
- **Por defecto denegar**: APIs cerradas, rutas privadas, permisos mínimos.

## Niveles

- **lite** — solo la lista de control en cambios que tocan I/O.
- **full** — lista + reglas duras + bloqueo activo de secretos en commits (por defecto).
- **ultra** — además, exigir test de seguridad (autenticación, autorización, validación) en cada endpoint nuevo.
- **off** — desactivada.

## Cuándo escalar

Si encontrás un secreto en el repo o un vector activo de inyección, **detenete**,
avisá al usuario, no commitees y proponé rotación. Cargá [[security-secrets]]
para el procedimiento.

Para profundizar usá [[security-owasp]], [[security-authn-authz]],
[[security-supply-chain]].
