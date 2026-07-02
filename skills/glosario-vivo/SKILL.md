---
name: glosario-vivo
description: >
  Glosario del proyecto mantenido en la memoria de Turtle: cada término propio o ambiguo que la persona y el agente acuerdan se guarda como una memoria evolutiva (`topic_key: glosario/<término>`), se consulta antes de rediscutir significados y se actualiza cuando el significado cambia. Evita que las mismas palabras signifiquen cosas distintas en sesiones distintas.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: propia (local-first, sin spawnear procesos)
  activation: bajo_demanda
  version: "1.0"
---

# Glosario vivo — las mismas palabras, el mismo significado

## El problema que resuelve
En un proyecto largo, la persona y el agente desarrollan un vocabulario propio ("el bus", "la malla", "el consejo", "perfil"). Sin registro, cada sesión nueva reinterpreta esos términos desde cero, y dos sesiones pueden entender cosas distintas por la misma palabra. El glosario convierte ese vocabulario en memoria consultable.

## Cuándo guardar un término
- La persona usa un término propio del proyecto que no es obvio por el código.
- Un término genérico tiene un significado específico aquí (p. ej. "spawn" = proceso del SO, NO sub-agente).
- Hubo un malentendido por una palabra: la señal más fuerte de que falta la entrada.

## Cómo guardarlo
Una memoria por término, evolutiva (se actualiza, no se duplica):

```
memory_save(
  tipo: "convention",
  topic_key: "glosario/<término-en-minúsculas>",
  titulo: "Glosario: <término>",
  contenido: "<término> = <definición en una o dos frases>. Ejemplo: <uso real>. NO confundir con: <el malentendido que motivó la entrada>."
)
```

## Cómo consultarlo
- Al arrancar una tarea que toca un área con vocabulario propio: `memory_search("glosario <área o término>")`.
- Ante la duda de qué quiso decir la persona con una palabra: busca el término en el glosario ANTES de preguntar; pregunta solo si no está.
- Si la persona usa el término de forma incompatible con la entrada, muéstrale la definición guardada y pregunta cuál vale: puede que el significado haya evolucionado — actualiza la entrada con el mismo `topic_key` (el historial queda en `memory_history`).

## Regla de oro
Una palabra con entrada en el glosario se usa SIEMPRE con ese significado, en la salida a la persona y en las memorias nuevas. Si el significado cambió, se cambia la entrada, no el uso.
