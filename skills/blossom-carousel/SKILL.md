---
name: blossom-carousel
description: >
  Carrusel nativo con scroll horizontal + snap, sin JS pesado, gratis en táctil.
  Componentes para React, Vue y Svelte. Útil para landings, galerías y
  agencias. Cargá al construir un slider/carousel/galería horizontal.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: jespervos/blossom-carousel
  activation: bajo_demanda
  version: "1.0"
---

# Blossom carousel

## Cuándo usar

- Galería horizontal en landing.
- Carrusel de testimonios, productos, casos.
- Cualquier slider donde el gesto táctil deba ser nativo (no simulado).

Si necesitás autoplay complejo, paginación con dots animados o transiciones 3D,
usá Swiper. Blossom es para lo simple, rápido y accesible.

## Idea

Scroll horizontal del navegador + `scroll-snap-type`. Sin listeners de drag,
sin librerías de touch. Animaciones nativas, momentum nativo, accesible por defecto.

## CSS base

```css
.carousel {
  display: flex;
  gap: 1rem;
  overflow-x: auto;
  scroll-snap-type: x mandatory;
  scroll-behavior: smooth;
  scrollbar-width: none;
  -webkit-overflow-scrolling: touch;
}
.carousel::-webkit-scrollbar { display: none; }
.carousel > * {
  flex: 0 0 auto;
  scroll-snap-align: start;
}
```

## React

```tsx
export function Carousel({ children }: { children: React.ReactNode }) {
  return <div className="carousel" role="region" aria-label="Carrusel">{children}</div>
}
```

## Vue

```vue
<template>
  <div class="carousel" role="region" aria-label="Carrusel">
    <slot />
  </div>
</template>
```

## Svelte

```svelte
<div class="carousel" role="region" aria-label="Carrusel">
  <slot />
</div>
```

## Botones prev/next (opcional)

```ts
const scrollByCard = (el: HTMLElement, dir: 1 | -1) => {
  const card = el.querySelector(':scope > *') as HTMLElement | null
  if (!card) return
  el.scrollBy({ left: dir * (card.offsetWidth + 16), behavior: 'smooth' })
}
```

## A11y

- `role="region"` + `aria-label` en el contenedor.
- Cada slide focusable (`tabindex="0"`) si tiene contenido interactivo dentro.
- Botones prev/next con `aria-label="Anterior"` / `"Siguiente"`.
- Indicar progreso si hay >5 slides: "3 de 8".

## Reglas

- **Sin librerías** salvo que necesites algo que CSS no da.
- **Sin altura fija** por slide; dejar que el contenido mande.
- **Lazy load** de imágenes (`loading="lazy"`).

## Relacionadas

[[ui-ux-pro-max]] · [[accessibility-wcag]]
