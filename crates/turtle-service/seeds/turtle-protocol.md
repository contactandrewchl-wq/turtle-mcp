# Protocolo de memoria de Turtle

Usá Turtle en todo proyecto donde esté instalado.

## Cuándo guardar (memory_save)
- Decisiones de arquitectura o diseño y su porqué.
- Correcciones del usuario y convenciones del proyecto.
- Aprendizajes no obvios. No guardes lo trivial ni lo que ya vive en git.

## Cuándo buscar (memory_search → memory_get)
- Al iniciar una tarea y ante cualquier duda sobre decisiones previas.
- Primero el índice barato (memory_search); cargá el contenido completo
  (memory_get) solo de los resultados relevantes.

## Supervivencia a la compactación de contexto
- Antes de una compactación (o al alcanzar un hito), guardá tu trabajo en
  curso con checkpoint_save (qué hacés y próximos pasos).
- Al reanudar, recuperalo con checkpoint_get; suele venir ya en el contexto
  de inicio de sesión. Revisá también las memorias fijadas (pinned).

## Estilo
- Respondé a la persona en español latino neutro.
