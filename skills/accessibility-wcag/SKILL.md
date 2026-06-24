---
name: accessibility-wcag
description: >
  Accesibilidad WCAG 2.2 nivel AA aplicada: semántica HTML, ARIA cuando hace
  falta, navegación por teclado, foco visible, contraste, textos alternativos,
  formularios accesibles. Cargá al crear o auditar UI.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Accesibilidad — WCAG 2.2 AA

## Cuándo usar

- Cualquier UI con la que interactúe una persona.
- Antes de mergear un PR que toca componentes interactivos.
- Auditoría de una pantalla existente.

## Principios POUR

1. **Perceivable** — todo contenido tiene alternativa textual o auditiva.
2. **Operable** — todo se puede usar con teclado, sin tiempo límite duro.
3. **Understandable** — lenguaje claro, errores con instrucción.
4. **Robust** — válido sintácticamente, compatible con asistivas.

## Semántica primero

Antes de ARIA, **HTML nativo**:

| Necesitás | Usá | No uses |
|---|---|---|
| Botón clickeable | `<button>` | `<div onClick>` |
| Link | `<a href>` | `<button>` que navega |
| Lista | `<ul>/<ol>` | `<div>` con bullets |
| Encabezado | `<h1>..<h6>` jerárquico | `<div class="title">` |
| Formulario | `<form>` + `<label for>` | inputs sueltos |
| Tabla de datos | `<table>` con `<th>` y `scope` | grid de `<div>` |
| Diálogo modal | `<dialog>` | `<div>` flotante |

ARIA solo cuando el HTML nativo no alcanza. **No ARIA es mejor que ARIA mal.**

## Teclado

- `Tab` recorre todo lo interactivo en orden lógico (DOM = visual).
- `Esc` cierra modales, dropdowns, popovers.
- `Enter`/`Space` activan botones; flechas para menús, tabs, sliders.
- **Foco visible siempre.** Outline ≥2 px, contraste ≥3:1 con el fondo.
- **Sin trampas de foco** salvo dentro de un modal abierto (y restaurar al cerrar).

## Contraste

- Texto normal: **4.5:1** mínimo.
- Texto grande (≥18 pt o ≥14 pt bold): **3:1**.
- Componentes UI y bordes de foco: **3:1** contra fondo adyacente.
- Verificá con axe DevTools o WebAIM Contrast Checker.

## Imágenes y media

- `alt` descriptivo si la imagen aporta info; `alt=""` si es decorativa.
- Video: subtítulos sincronizados. Audio: transcripción.
- No autoplay con sonido. Pausa accesible en cualquier animación >5 s.

## Formularios

- `<label for>` en cada input. No "placeholder como label".
- Errores en texto + ícono + color (no solo color). `aria-invalid` + `aria-describedby` al mensaje.
- Agrupá relacionados con `<fieldset>` + `<legend>`.
- Autocompletar: `autocomplete="email"`, `autocomplete="given-name"`, etc.

## ARIA — reglas básicas

1. No usar ARIA si hay HTML nativo equivalente.
2. No cambiar la semántica nativa (`role="button"` en `<a>` casi nunca).
3. Todo control interactivo debe tener nombre accesible.
4. `aria-hidden="true"` nunca en algo que recibe foco.
5. Usar landmarks: `<header>`, `<nav>`, `<main>`, `<aside>`, `<footer>`.

## Validación

- `axe-core` (DevTools o CLI) sin violaciones críticas/serias.
- Recorrer la página **solo con teclado**: ¿llegás a todo? ¿salís de todo?
- Probar con lector de pantalla (NVDA en Windows, VoiceOver en macOS, TalkBack en Android).
- Lighthouse Accessibility ≥95.

## Reglas duras

- **Nunca `outline: none`** sin reemplazo visible.
- **Nunca `tabindex` >0**: rompe el orden natural.
- **Nunca `<div onClick>`** para algo que se ve como botón o link.
- **Nunca color como único indicador** (rojo = error, verde = ok no alcanza).

## Relacionadas

[[ui-ux-pro-max]] · [[frontend-component-patterns]]
