---
name: ui-ux-pro-max
description: >
  Guía integral de UI/UX para web y mobile. Sistemas de color, tipografía
  pareada, jerarquía visual, espaciado 8-pt, estados de interacción, dark mode,
  accesibilidad de contraste, y patrones por tipo de producto (landing,
  dashboard, e-commerce, SaaS, app móvil). Cargá esta skill al diseñar o
  implementar interfaz visible.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: nextlevelbuilder/ui-ux-pro-max-skill
  activation: bajo_demanda
  version: "1.0"
---

# UI/UX Pro Max

## Cuándo usar

- Diseñar o reescribir una página, pantalla, modal, drawer o panel completo.
- Elegir paleta, tipografía, espaciado o estados de un componente.
- Auditar una UI existente contra estándares de accesibilidad y consistencia.

Si solo vas a tocar lógica sin afectar la vista, no cargues esta skill.

## Fundamentos

### Color

- **Token primario** + **secundario** + **neutros** (5–7 grises) + **semánticos** (success, warning, danger, info).
- Contraste **AA mínimo** (4.5:1 texto normal, 3:1 texto grande y componentes UI). Verificá con [[accessibility-wcag]].
- Dark mode no es invertir colores: rediseñá superficies con elevación por opacidad/luminosidad, no por sombras duras.

### Tipografía

- **Una familia** para UI (sans-serif moderna: Inter, IBM Plex Sans, Geist) + opcional **una** para display.
- Escala modular (1.125, 1.2, 1.25 o 1.333). Mínimo 14 px para texto largo, 16 px para body.
- `line-height` 1.4–1.6 en párrafos, 1.1–1.25 en headings.

### Espaciado

- Grid de **8 px** (o 4 px para componentes densos). Todo padding/margin múltiplo del token.
- Densidad por contexto: dashboard denso (8/12/16), landing aireado (24/40/64).

### Jerarquía

- Una sola acción primaria por vista. Si hay dos, una es secundaria (ghost/outline).
- Z-index documentado: 0 base · 10 sticky · 100 dropdown · 1000 modal · 10000 toast.

## Estados de interacción

Todo componente clickeable tiene **5 estados** definidos:

`default · hover · active/pressed · focus (visible, ≥2 px outline) · disabled`

Y para inputs: `default · focus · filled · error · disabled · readonly`.

Sin `focus` visible no pasa a11y. Nunca `outline: none` sin reemplazo.

## Patrones por tipo de producto

- **Landing** — hero con propuesta clara en ≤8 palabras, una CTA dominante, prueba social arriba del fold, secciones de máximo 3 columnas.
- **Dashboard** — sidebar persistente, breadcrumbs, KPI cards arriba, tablas con paginación + búsqueda + filtros, skeletons en carga.
- **E-commerce** — grid 2/3/4 columnas responsive, badge de stock, precio claro, CTA "agregar" visible sin hover.
- **SaaS** — onboarding en 3 pasos máximo, empty states con acción, command palette `⌘K`.
- **App móvil** — tab bar inferior (≤5 ítems), gestos nativos, áreas tocables ≥44×44 px.

## Reglas duras

- **Sin valores mágicos** en CSS: todo color/tamaño/espaciado viene de tokens.
- **Sin texto incrustado** en imágenes (rompe a11y, traducción, dark mode).
- **Skeletons o placeholders** en toda carga >200 ms.
- **Feedback** en toda acción: optimistic UI, toast o estado del botón.
- **Errores** dicen qué pasó y qué hacer, no códigos crudos.

## Validación

- Lighthouse ≥90 en Performance, Accessibility, Best Practices.
- `axe-core` sin violaciones críticas.
- Render en mobile (375 px), tablet (768 px), desktop (1440 px) sin scroll horizontal.
- Probar con teclado: tab por toda la página, foco siempre visible.

## Relacionadas

[[accessibility-wcag]] · [[frontend-component-patterns]] · [[blossom-carousel]]
