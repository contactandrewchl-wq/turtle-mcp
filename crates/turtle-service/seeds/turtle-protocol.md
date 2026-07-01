# Protocolo de memoria de Turtle

Usa Turtle en todo proyecto donde esté instalado.

## Cuándo guardar (memory_save)
- Decisiones de arquitectura o diseño y su porqué.
- Correcciones del usuario y convenciones del proyecto.
- Aprendizajes no obvios. No guardes lo trivial ni lo que ya vive en git.

## Cuándo buscar (memory_search → memory_get)
- Al iniciar una tarea y ante cualquier duda sobre decisiones previas.
- Primero el índice barato (memory_search); carga el contenido completo
  (memory_get) solo de los resultados relevantes.

## Supervivencia a la compactación de contexto
- Antes de una compactación (o al alcanzar un hito), guarda tu trabajo en
  curso con checkpoint_save (qué haces y próximos pasos).
- Al reanudar, recupéralo con checkpoint_get; suele venir ya en el contexto
  de inicio de sesión. Revisa también las memorias fijadas (pinned).

## Estilo
- Responde a la persona en español latino neutro.
