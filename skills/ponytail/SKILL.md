---
name: ponytail
description: >
  Anti-sobre-ingeniería. Antes de escribir código, recorré la escalera de
  decisión: ¿debe existir? → ¿hay biblioteca estándar? → ¿hay característica
  nativa? → ¿se resuelve en una línea? → mínimo viable. Marca atajos con un
  comentario que nombre su ruta de mejora.
license: Apache-2.0
metadata:
  type: comportamiento
  origin: DietrichGebert/ponytail
  activation: permanente
  levels: [lite, full, ultra, off]
  size_budget_tokens: 600
  version: "1.0"
---

# Ponytail

Atá la solución corta antes de soltarla larga. Cada vez que vayas a escribir
código, bajá por esta escalera y parate en el primer escalón que resuelva el
problema.

## Escalera de decisión

1. **¿Debe existir?** Si nadie lo va a usar más de una vez, probablemente no.
2. **Biblioteca estándar.** Si lo trae el lenguaje, usalo. Sin envoltorios.
3. **Característica nativa del framework.** Si el framework ya lo hace, dejalo.
4. **Una línea.** ¿Se resuelve con una expresión? Hacelo.
5. **Mínimo viable.** Lo más simple que funcione para el caso real, no para el imaginado.

## Reglas de atajo

- Si tomás un atajo, **comentalo con su ruta de mejora**:

  `// atajo: validar en cliente; mover a backend si se reutiliza`

- No introduzcas abstracciones por "futura flexibilidad". Tres usos antes de extraer.
- No agregues banderas/features/configs por escenarios que aún no pasan.

## Niveles

- **lite** — solo la escalera, sin marcar atajos.
- **full** — escalera + atajos comentados (por defecto).
- **ultra** — además, rechazá cualquier abstracción nueva sin tres casos reales.
- **off** — desactivada.

## Cuándo NO aplicar

- Bordes del sistema (entrada de usuario, API pública): ahí sí validás y abstraés.
- Código crítico de seguridad: la simplicidad no justifica saltar verificaciones.
