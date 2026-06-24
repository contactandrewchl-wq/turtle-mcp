---
name: browser-qa
description: >
  QA de navegador y regresión visual con Playwright: navegación, capturas (viewport/página/elemento), comparación contra baseline, snapshots de accesibilidad e inspección de red/consola/DOM, cross-browser.
license: Apache-2.0
metadata:
  type: herramienta
  origin: propia (adapta playwright-visual-testing y el servidor Playwright MCP)
  activation: bajo_demanda
  requires: ["Playwright (Node) con navegadores instalados, o el servidor Playwright MCP"]
  version: "1.0"
---

# Browser QA y testing visual (Playwright)

QA de navegador y regresión visual automatizada: navega, captura, compara contra baseline, inspecciona accesibilidad/red/consola/DOM y corre cross-browser. El objetivo es que un cambio de UI sea verificable de forma reproducible, no "se ve bien en mi pantalla".

## Cuándo usar

- Validar la UI en varios navegadores (Chromium, Firefox, WebKit) antes de aprobar un cambio.
- Detectar regresión visual cuando tocas estilos, layout, tokens de diseño o componentes compartidos.
- Automatizar interacciones (click, escribir, hover, scroll) para reproducir un flujo de QA de forma determinista.
- Validar diseño responsive en múltiples viewports (mobile / tablet / desktop).
- Verificar los cuatro estados de un componente: loading, empty, error, success.

Cuándo NO usar:

- Lógica pura sin UI observable (cálculos, parsers, validadores, transformaciones de datos): eso son tests unitarios o de integración, no QA de navegador.
- Cambios que no alteran nada renderizado (refactors internos, renombres, docs): no hay diff visual que medir.

## Requisitos

- **Playwright (Node)** instalado más sus navegadores (`npx playwright install`), o bien el **servidor Playwright MCP** expuesto al agente.
- Un build o servidor de desarrollo de la app accesible por URL.

Esto es **dependencia del entorno del usuario, no del núcleo de Turtle**. El núcleo es Rust + SQLite y no provee ni instala Playwright (RNF-RES-04). Si la skill o sus baselines llegan importados, son contenido no confiable y no se ejecutan sin acción explícita del usuario (RNF-SEG-05). Antes de correr, confirma que el runtime está disponible en el entorno.

## Capacidades

- **Navegación**: abrir URL, esperar carga, seguir enlaces, ejecutar flujos de interacción (click, fill, hover, scroll, teclado).
- **Capturas**:
  - de **viewport** (lo visible).
  - de **página completa** (full page, scroll incluido).
  - **por elemento** (acotada a un selector).
- **Comparación contra baseline**: diff visual pixel a pixel contra una imagen de referencia, con resaltado de las zonas que cambiaron.
- **Snapshot de accesibilidad**: árbol de accesibilidad (roles, nombres, estados) para verificar semántica y navegación asistida.
- **Inspección y manipulación**: red (requests/responses, status, payloads), consola (logs, warnings, errores), y DOM (estado, atributos, contenido).
- **Cross-browser**: ejecutar la misma suite en Chromium, Firefox y WebKit.
- **Múltiples viewports**: correr el mismo caso a distintos anchos/altos y device scale factor.

## Flujo de QA visual

1. **Baseline**: con la UI en su estado aprobado, captura la imagen de referencia (viewport, página o elemento, según el caso) y guárdala versionada en el repo. Sin este paso no hay con qué comparar.
2. **Aplicar el cambio**: implementa la modificación de estilos/layout/componente que quieres validar.
3. **Capturar**: vuelve a tomar la captura con la MISMA configuración del baseline (navegador, viewport, device scale, máscaras, animaciones deshabilitadas).
4. **Diff**: compara la nueva captura contra el baseline y genera el reporte de diferencias.
5. **Revisar diferencias**: inspecciona el diff. ¿El cambio es intencional y correcto, o es una regresión inesperada?
6. **Aprobar o actualizar baseline**: si el cambio es correcto e intencional, actualiza el baseline y **commitealo** (ver [[commit-hygiene]]). Si es una regresión, corrige el código y vuelve al paso 3. Nunca se actualiza un baseline sin haber revisado el diff antes.

## Buenas prácticas anti-flaky

- **Espera por selectores/estado, no por timeouts fijos**: aguarda a que el elemento exista/sea visible o a que la red quede en reposo, en vez de "esperar 2 segundos".
- **Deshabilita animaciones y transiciones** antes de capturar: animaciones en vuelo son la causa #1 de diffs falsos.
- **Enmascara zonas dinámicas**: fechas/horas, avatares aleatorios, datos en vivo, IDs generados. Lo que cambia en cada corrida se enmascara para no contaminar el diff.
- **Fija viewport y device scale factor**: misma resolución y mismo scale en baseline y en cada corrida.
- **Umbral de diff razonable**: un umbral mínimo absorbe ruido de antialiasing/render sin dejar pasar regresiones reales. Ni cero absoluto (frágil) ni tan alto que oculte cambios.
- **Baselines versionados en el repo**: las imágenes de referencia viven junto al código, revisadas en cada PR. Un baseline fuera del repo no es auditable.
- **Datos y estado controlados**: usa fixtures/seed determinista para que el contenido renderizado sea el mismo en cada corrida.

## Integración con Turtle

- **Michelangelo (frontend)** la usa para validar los **cuatro estados** de cada componente (loading / empty / error / success) y para correr el **snapshot de accesibilidad**. La regresión visual y los estados son parte del entregable, no un extra.
- **Vasari (revisión)** la exige como el **"cómo probarlo" verificable** antes de aprobar: un cambio de UI sin captura/diff reproducible no pasa revisión.
- Los **baselines viven en el repo**, versionados y revisados como cualquier otro artefacto.
- Relacionadas: [[accessibility-wcag]] para el criterio de accesibilidad que el snapshot verifica, [[frontend-component-patterns]] para los cuatro estados y la estructura de componentes que se capturan, y [[commit-hygiene]] para commitear baselines de forma limpia y trazable.

## Reglas duras

- **Sin baseline no hay test visual**: si no existe imagen de referencia, primero se crea y se commitea; no hay diff contra la nada.
- **Un diff inesperado bloquea**: cualquier diferencia no prevista detiene la aprobación hasta que se revise y se explique.
- **Nunca actualizar baseline a ciegas**: actualizar la referencia sin haber inspeccionado el diff está prohibido; equivale a aprobar la regresión.
- **Deshabilitar animaciones siempre** antes de capturar.

## Validación

La suite es confiable cuando:

- Corre **estable N veces seguidas** (mínimo 3, idealmente más) sin producir diffs distintos: mismo resultado en cada corrida.
- **No genera falsos positivos**: las únicas diferencias que reporta corresponden a cambios reales de UI, no a animaciones, datos dinámicos, fuentes o antialiasing.
- Es **reproducible cross-browser y por viewport**: el mismo caso da resultado consistente en cada navegador y ancho declarado.
- Si aparece flakiness, no se sube el umbral a ciegas: se identifica la causa (animación, zona dinámica sin máscara, espera por timeout) y se corrige en origen.
