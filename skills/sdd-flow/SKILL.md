---
name: sdd-flow
description: >
  Flujo de desarrollo dirigido por especificación (SDD) anclado a estándares IEEE: especificar requisitos, diseñar, planificar por fases, trazar y verificar — antes de implementar.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: propia (adapta el concepto de spec-driven-development-orchestrator de mcpmarket, en clave local-first)
  activation: bajo_demanda
  version: "1.0"
---

# Flujo SDD (desarrollo dirigido por especificación)

El desarrollo dirigido por especificación (SDD) ancla cada línea de código a un requisito explícito, verificable y trazable. La especificación precede al diseño, el diseño al plan y el plan a la implementación. Nada se construye sin una entrada que lo justifique y un criterio que pruebe que está completo.

## Cuándo usar

Usa SDD cuando:
- Arrancas una feature o módulo nuevo cuyo comportamiento aún no está formalizado.
- Vas a planificar antes de implementar (más de un PR, dependencias entre tareas, varios agentes involucrados).
- Necesitas formalizar o renegociar requisitos (contrato de API, reglas de negocio, criterios de aceptación).
- El cambio tiene impacto transversal (datos, seguridad, contratos públicos) y exige trazabilidad entre sesiones.

NO uses el flujo SDD completo cuando:
- Es un fix trivial (typo, ajuste de copy, corrección de un bug aislado con causa evidente).
- El cambio no altera requisitos ni contratos y cabe en un solo PR pequeño.
- Es trabajo exploratorio descartable (spike) que no llega a producción.

Regla práctica: si el cambio modifica QUÉ hace el sistema, necesita spec; si solo corrige CÓMO lo hace sin cambiar el comportamiento observable, basta con disciplina de commit ([[commit-hygiene]]).

## Estándares de referencia

| Estándar | Qué gobierna |
|---|---|
| ISO/IEC/IEEE 29148:2018 | Ingeniería de requisitos y estructura del SRS (especificación de requisitos de software). |
| IEEE 1016 | Software Design Descriptions (SDD): cómo se documenta el diseño y sus vistas. |
| IEEE 1012 | Verificación y Validación (V&V): qué se verifica, cómo y con qué evidencia. |
| ISO/IEC/IEEE 29119 | Testing de software: procesos, técnicas de diseño de pruebas y documentación de casos. |
| ISO/IEC/IEEE 12207 | Procesos del ciclo de vida del software: marco que encuadra y conecta las fases. |

## Fases

El flujo es lineal con compuertas (gates): no se pasa a la fase siguiente hasta cumplir el criterio de salida de la actual.

### 1. Especificar
- **Entrada:** necesidad, problema u objetivo de negocio; restricciones conocidas.
- **Actividad:** elicitar y redactar requisitos atómicos, verificables y no ambiguos (funcionales y no funcionales); definir criterios de aceptación.
- **Artefacto:** SRS conforme a 29148.
- **Criterio de salida (gate):** cada requisito tiene id único, método de verificación (I/A/D/P) y criterio de aceptación; pasa el autocontrol de "buen requisito"; no quedan ambigüedades ni requisitos sin verificación.

### 2. Diseñar
- **Entrada:** SRS aprobado.
- **Actividad:** decidir estructura, componentes, interfaces y contratos que satisfacen los requisitos; registrar decisiones y alternativas descartadas.
- **Artefacto:** SDD conforme a IEEE 1016 (incluye contratos de API como parte de la spec).
- **Criterio de salida (gate):** cada elemento de diseño referencia el o los requisitos que cubre; todo requisito del SRS tiene al menos un elemento de diseño que lo realiza; decisiones clave registradas.

### 3. Planificar
- **Entrada:** SDD aprobado.
- **Actividad:** descomponer en tareas atómicas (una unidad de trabajo = un PR); ordenar por dependencias; asignar a rótulos vía el bus.
- **Artefacto:** plan por fases con tareas atómicas y orden de ejecución.
- **Criterio de salida (gate):** cada tarea mapea a elementos de diseño y requisitos; dependencias explícitas; ninguna tarea sin entrada en la matriz de trazabilidad.

### 4. Implementar
- **Entrada:** plan aprobado y tarea seleccionada.
- **Actividad:** escribir código y pruebas para una tarea; un PR por tarea.
- **Artefacto:** código + pruebas + actualización de la matriz de trazabilidad.
- **Criterio de salida (gate):** el PR referencia su tarea/requisito; pruebas que cubren el requisito existen y pasan; la fila de trazabilidad correspondiente queda completa.

### 5. Verificar
- **Entrada:** implementación de una o varias tareas.
- **Actividad:** ejecutar el plan de V&V; verificar cada requisito por su método (I/A/D/P); validar contra criterios de aceptación.
- **Artefacto:** resultados de V&V y evidencia por requisito.
- **Criterio de salida (gate):** todo requisito verificado con evidencia; trazabilidad cerrada de requisito a test; criterios de aceptación cumplidos.

## Artefactos

- **SRS (29148):** requisitos atómicos con id, prioridad, método de verificación y criterio de aceptación.
- **SDD / descripción de diseño (IEEE 1016):** vistas de estructura, componentes, interfaces y contratos; cada elemento ligado a requisitos.
- **Plan por fases:** tareas atómicas (un PR por tarea), orden y dependencias.
- **Matriz de trazabilidad:** liga requisito → diseño → código → test (bidireccional).
- **Plan de V&V (IEEE 1012):** qué se verifica, método y evidencia esperada por requisito.
- **Casos de prueba (29119):** diseño de pruebas que cubre cada requisito verificable por Prueba (P).

Los artefactos viven en una carpeta de specs del repositorio (por ejemplo `specs/`), versionados junto al código.

## Trazabilidad y buen requisito

Todo requisito debe ser:
- **Atómico:** una sola obligación por requisito.
- **Verificable:** existe un método que prueba si se cumple.
- **No ambiguo:** una única interpretación posible.

Método de verificación por requisito (convención del SRS de Turtle):
- **I — Inspección:** examen del artefacto sin ejecutarlo.
- **A — Análisis:** razonamiento, modelos o cálculo.
- **D — Demostración:** observación del comportamiento en operación.
- **P — Prueba:** ejecución de casos de prueba con resultado medible.

La matriz de trazabilidad liga requisito ↔ artefacto de diseño ↔ código ↔ test, y debe ser navegable en ambos sentidos: de cada requisito a su evidencia, y de cada test al requisito que justifica su existencia. Un requisito sin fila completa no está terminado.

## Integración con Turtle

- Guarda requisitos, decisiones de diseño y arquitectura en memoria con `memory_save` usando los tipos `decision` y `architecture`, para preservar trazabilidad entre sesiones.
- Recupera contexto previo con `memory_search` y `memory_get` antes de re-especificar.
- Los artefactos (SRS, SDD, plan, matriz, V&V, casos) viven en la carpeta de specs del repo; la memoria de Turtle guarda las decisiones y enlaces, no reemplaza los archivos.
- Usa `relation_add` para enlazar requisitos con decisiones y diseño en el grafo de memoria.
- Los contratos de API son parte de la spec: ver [[backend-api-design]].
- Para registro de sesión, mensajería y handoffs durante el flujo: [[turtle-protocol]].

## Reglas duras

- Sin spec no hay código.
- Todo requisito debe ser verificable; si no se puede verificar, no es un requisito válido.
- Nada se implementa sin una entrada en la matriz de trazabilidad.
- El plan precede al PR; no se abre PR sin tarea planificada.
- Un cambio de requisito actualiza la spec y la trazabilidad en el mismo PR; nunca por separado.

## Validación

La spec y el plan están completos cuando:
- **Autocontrol 29148:** cada requisito pasa atómico + verificable + no ambiguo, tiene id, método (I/A/D/P) y criterio de aceptación.
- **Cobertura de trazabilidad:** 100% de requisitos con fila completa (diseño → código → test); cero requisitos huérfanos y cero diseño/código sin requisito que lo justifique.
- **Criterios de aceptación verificables:** cada criterio expresa un resultado observable y medible, no una intención.
- **Cierre de V&V:** existe método y evidencia para cada requisito; el plan de V&V (1012) no deja requisitos sin verificar.
