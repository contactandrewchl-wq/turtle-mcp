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

## Fases nombradas sobre el roster (orquestación SDD)

Las cinco fases IEEE de arriba son la columna vertebral. Para **orquestar** el trabajo entre personas, el flujo se descompone en **fases nombradas**, cada una con un dueño del roster real y un artefacto persistido. Es un **guion del orquestador, no un proceso del sistema**: cada fase corre como **sub-agente Task del CLI** (donde el CLI lo soporta) o como **relevo por el bus** (`turtle mensaje ... -a <rótulo> --de sdd`). En CLIs single-agent (Codex/OpenCode) el flujo degrada a un guion secuencial en una sola sesión. Nunca se lanza un proceso del SO.

| # | Fase | Dueño (rótulo) | Fase IEEE | Artefacto (`topic_key`) | Gate de salida |
|---|---|---|---|---|---|
| 0 | init | Leonardo (`orquestador`) | — | `sdd/<cambio>/config` | cambio nombrado; reglas por fase y modo de rigor fijados |
| 1 | explore | sub-agente de exploración, tier barato | (entrada a Especificar) | `sdd/<cambio>/explore` | mapa del código y restricciones, solo lectura |
| 2 | propose | Alberti (`sdd`) | Especificar | `sdd/<cambio>/proposal` | problema, alcance y motivación acordados |
| 3 | spec | Alberti (`sdd`) | Especificar | `sdd/<cambio>/specs` | SRS 29148: cada requisito atómico, con id, método I/A/D/P y criterio de aceptación |
| 4 | design | Donatello (`arquitectura`) | Diseñar | `sdd/<cambio>/design` | SDD 1016: cada elemento de diseño referencia su(s) requisito(s) |
| 4b | contracts (si hay API) | Pacioli (`api`) | Diseñar | `sdd/<cambio>/contracts` | contrato versionado, orientado a recursos, fijado antes de implementar |
| 5 | tasks | Alberti (`sdd`) + Leonardo (`orquestador`) | Planificar | `sdd/<cambio>/tasks` | tareas atómicas (un PR c/u), dependencias explícitas, todas en la matriz |
| 6 | apply | Brunelleschi (`backend`) / Michelangelo (`frontend`) | Implementar | `sdd/<cambio>/apply-progress` | tareas en `[x]`; con modo de rigor, evidencia de test (rojo→verde) por tarea |
| 7 | verify | Vasari (`revision`) + Raphael (`seguridad`) | Verificar | `sdd/<cambio>/verify-report` | V&V 1012: cada requisito verificado por su método; sin FAIL/BLOCKED/PENDING |
| 8 | judge (T3) | Galileo (`consejo`) | Verificar (gate adversarial) | `sdd/<cambio>/verdict` (memoria `decision`) | veredicto APROBADO o ESCALADO con próximo paso |
| 9 | archive | cierre con `memory_save` | — | `sdd/<cambio>/archive-report` | cambio marcado completo; trazabilidad cerrada y persistida |

(El paso de inducción de un SDD por fases convencional no tiene fase propia en Turtle: las AGENT.md de cada persona ya enseñan identidad, voz y flujo, así que la inducción no necesita un paso separado.)

### Handoff entre fases por memoria (no por proceso)

El relevo entre fases es **por memoria**, no por orquestación de procesos. Cada fase:

1. **Asienta su salida** con `memory_save` usando `topic_key: "sdd/<cambio>/<artefacto>"`. La clave de tema hace **upsert**: re-correr una fase actualiza su artefacto en vez de duplicarlo, y el historial versionado queda disponible (`memory_history`).
2. **La fase siguiente la levanta** con `memory_search` por la misma clave (`sdd/<cambio>/<artefacto>`) y trae el detalle con `memory_get`.

Regla dura: **toda fase que produce un artefacto DEBE persistirlo**; saltearlo rompe la cadena, porque la fase siguiente no tiene de dónde leer. Este es el equivalente Turtle del handoff por memoria de un SDD por fases, con dos ventajas: la **temporalidad versionada** (cada upsert deja historia recuperable) y las **relaciones** entre artefactos (`relation_add` para enlazar requisito ↔ diseño ↔ tarea ↔ verificación), no solo el último estado.

### Gates y relevos por el bus

El paso de una fase a la siguiente es una **compuerta**: no se releva hasta cumplir el gate de salida de la fase actual. El relevo se hace por rótulo:

- **init → explore:** `turtle mensaje "cambio <X> abierto; barré el código en modo lectura" -a <tier-barato> --de orquestador`
- **explore → propose/spec:** el sub-agente asienta `sdd/<X>/explore`; Leonardo releva a `sdd`.
- **spec → design:** `turtle mensaje "SRS 29148 de <X> en sdd/<X>/specs; diseñá los límites" -a arquitectura --de sdd`
- **design → contracts (si hay API):** `turtle mensaje "diseño en sdd/<X>/design; fijá y versioná los contratos" -a api --de sdd`
- **tasks → apply:** `turtle mensaje "plan en sdd/<X>/tasks; implementá la tarea N con tests" -a backend --de sdd` (o `-a frontend`)
- **apply → verify:** `turtle mensaje "apply-progress en sdd/<X>/apply-progress; verificá V&V" -a revision --de sdd`
- **verify → judge (T3):** `turtle mensaje "V&V en verde; someté la decisión al consejo adversarial" -a consejo --de revision`
- **judge → archive:** con el veredicto APROBADO, se cierra el cambio y se asienta `sdd/<X>/archive-report`.

**Dónde supera a un SDD por fases convencional:** el gate de verificación **T3 = Galileo (`consejo`)** somete la decisión a cinco voces adversariales con peer-review anónimo y **persiste el veredicto** como memoria `decision`, contra los dos jueces ciegos y **efímeros** de un gate convencional que no deja rastro. Más voces, anónimo y trazable entre sesiones ([[llm-council]]).

### Modo de rigor (equivalente liviano a strict-TDD)

El modo de rigor se fija en la fase **init**, dentro de `sdd/<cambio>/config` (campo `rigor: estricto|normal`). Cuando está en estricto:

- **Sin requisitos verificables no hay plan:** la fase `tasks` no abre hasta que cada requisito del `spec` tiene método de verificación (I/A/D/P) y criterio de aceptación.
- **Sin tests no se cierra `apply`:** cada tarea aplicada debe traer su evidencia de prueba (test que falla primero, luego pasa) en `apply-progress`; `verify` **rechaza** una tarea sin esa evidencia.

Es la versión liviana y honesta del `strict_tdd` de una config de SDD por fases convencional: vive en memoria (`topic_key`), no necesita un parser que lo valide y es prompt-driven.

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
- En el flujo por fases, cada fase asienta su artefacto con `topic_key: "sdd/<cambio>/<artefacto>"` (upsert) y la siguiente lo levanta por la misma clave; ver «Fases nombradas sobre el roster».
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
