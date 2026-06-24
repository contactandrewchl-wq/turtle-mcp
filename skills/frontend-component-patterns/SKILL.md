---
name: frontend-component-patterns
description: >
  Patrones de componentes para React, Vue y Svelte: composición sobre herencia,
  estado local vs global, props vs slots, controlled vs uncontrolled, listas
  con keys estables, memoización dirigida, manejo de async/loading/error.
  Cargá al diseñar o refactorizar componentes.
license: Apache-2.0
metadata:
  type: conocimiento
  activation: bajo_demanda
  version: "1.0"
---

# Frontend component patterns

## Cuándo usar

- Crear un componente nuevo de UI compleja (formulario, tabla, modal, lista virtualizada).
- Refactorizar un componente que creció a >300 líneas o tiene >5 props booleanos.
- Resolver problemas de rerender, claves o estado compartido.

## Composición

- **Slots/children sobre props de configuración.** Si vas por `variant="primary-large-icon-loading"`, partilo en partes componibles.
- **Headless + estilos** para libraries reutilizables (Radix, Headless UI, shadcn): la lógica vive en un hook, el render en JSX.
- **Compound components** para grupos: `<Tabs><TabList><Tab/></TabList><TabPanel/></Tabs>`. Mejor API que props array.

## Estado

| Decisión | Regla |
|---|---|
| ¿Local o compartido? | Local hasta que tres componentes no relacionados lo necesiten. |
| ¿Server o cliente? | Server state (datos remotos) → React Query / SWR / TanStack Query. Client state → useState/signals/stores. **No mezclar.** |
| ¿Global? | Solo si cruza rutas o capas. Zustand/Pinia/Svelte stores antes que Redux. |
| ¿Form? | React Hook Form / VeeValidate / Felte. Nunca `useState` por campo en formularios >3 campos. |

## Controlled vs uncontrolled

- **Controlled** por defecto en formularios con validación viva.
- **Uncontrolled** (refs + `defaultValue`) para inputs simples sin reactividad — más rápido, menos rerenders.
- **Nunca** mezclar: o `value` o `defaultValue`, no ambos.

## Listas

- `key` **estable y único**, nunca el índice si la lista cambia de orden.
- Listas >100 ítems: virtualizar (`react-window`, `vue-virtual-scroller`, `svelte-virtual-list`).
- Paginación o scroll infinito >50 ítems. No render todo "porque cabe".

## Memoización

- **No memoizar por reflejo.** `useMemo`/`memo`/`computed` solo cuando hay profiler que lo justifique.
- Sí memoizar: cálculos pesados (>10 ms), referencias pasadas a `React.memo`, dependencias de `useEffect` que disparan red.
- Vue 3 / Svelte: la reactividad fina ya memoiza; intervenir solo en cuellos medidos.

## Async: loading · success · error · empty

Toda vista que carga datos tiene **4 estados** explícitos, no tres:

```tsx
if (isLoading) return <Skeleton />
if (error) return <ErrorState onRetry={retry} />
if (!data?.length) return <EmptyState />
return <List items={data} />
```

`empty` ≠ `loading`. Mostrar lista vacía es peor UX que un empty state con acción.

## Reglas duras

- **Una responsabilidad por componente.** Si nombrarlo cuesta, está haciendo dos cosas.
- **Props ≤ 7.** Más que eso → compound component o config object.
- **Sin lógica de negocio en JSX/template.** Extraé a hook/composable/store.
- **Sin fetch directo en componentes** salvo prototipo. Pasá por capa de query o action.

## Validación

- Tests con Testing Library: rol + texto, no clases CSS ni IDs internos.
- Storybook por estado del componente (loading/error/empty/success).
- Lighthouse a11y ≥95 en cada vista nueva.

## Patrones avanzados

### Render props
- Usa render props cuando el componente posee estado/lógica pero el consumidor decide el markup: `<Toggle>{(on, toggle) => ...}</Toggle>`. Da control total del render sin acoplar UI.
- Hoy, en React, prefiere un **hook** (`useToggle()`) para compartir lógica: menos anidamiento, sin "wrapper hell", composición más limpia. Reserva render props para cuando necesitas inyectar JSX en un punto específico del árbol (ej. una librería headless que renderiza su propio contenedor).
- Vue/Svelte: el equivalente idiomático es **slot con scope** (`v-slot="{ on, toggle }"` / `let:on`), no render props. Úsalos ahí.
- Regla: si solo compartes estado/efectos → hook/composable; si además controlas *dónde* va el markup → slot con scope o render prop.

### Compound components (profundizar)
- Un padre expone subcomponentes (`<Tabs><Tabs.List><Tabs.Tab/>`) que se comunican por **Context** compartido, no por props perforadas. El consumidor compone libremente el orden y el markup.
- API por contexto:
  - El padre crea el contexto y provee `{ state, actions }`; los hijos lo consumen con un hook interno (`useTabsContext`).
  - El hook debe **fallar con mensaje claro** si se usa fuera del provider (`throw new Error('Tabs.Tab debe ir dentro de <Tabs>')`).
  - Expón los hijos como propiedades estáticas (`Tabs.Tab = Tab`) o vía un objeto namespaced; documenta cuáles son válidos.
- Mantén el estado en el padre (o acepta `value`/`onChange` para versión controlada, ver controlled/uncontrolled). No dupliques estado en los hijos.
- No iteres `children` con `cloneElement` para inyectar props: es frágil al anidamiento. Context escala mejor.
- Vue/Svelte: mismo patrón con `provide`/`inject` (Vue) o context API (`setContext`/`getContext` en Svelte).

### Error boundaries
- Capturan errores lanzados **durante el render** (y en lifecycles/constructores) de su subárbol, muestran un fallback y evitan el "pantallazo en blanco". No capturan errores async, en event handlers ni en SSR; esos van con `try/catch` o estado de error explícito.
- React: clase con `getDerivedStateFromError` + `componentDidCatch` (no hay equivalente en hooks; usa `react-error-boundary` para una API declarativa con `onReset` y `resetKeys`).
- Combinación con Suspense: envuelve `<ErrorBoundary><Suspense fallback={...}>...</Suspense></ErrorBoundary>`. Suspense maneja el estado de carga; el boundary, el de fallo. Para reintentar, resetea el boundary y vuelve a montar.
- Granularidad: pon boundaries por sección (sidebar, widget, ruta), no solo uno global, para degradar parcialmente. Loguea el error (Sentry, etc.) en el handler.
- Equivalentes por framework:
  - **Vue**: hook `onErrorCaptured` en un ancestro, o `app.config.errorHandler` global.
  - **Svelte**: `<svelte:boundary>` con snippet `failed` (Svelte 5); en versiones previas, manejo manual por estado.

### Animaciones
- Anima para comunicar (cambio de estado, jerarquía, continuidad espacial, feedback), no como adorno. Animaciones de entrada/salida de listas, transiciones de ruta y microinteracciones de feedback son las de mayor retorno.
- **Respeta `prefers-reduced-motion`**: es obligatorio, no opcional. Desactiva o reduce a un fade mínimo cuando esté activo.
  - CSS: `@media (prefers-reduced-motion: reduce) { * { animation: none; transition: none; } }` acotado a lo que aplique.
  - JS: `matchMedia('(prefers-reduced-motion: reduce)').matches` antes de disparar animaciones imperativas.
- Por framework:
  - **React / Framer Motion**: `<motion.div>` con `initial/animate/exit`; `<AnimatePresence>` para desmontajes; `layout` para transiciones de posición automáticas. Usa `useReducedMotion()` para cortar o suavizar.
  - **Vue**: `<Transition>` / `<TransitionGroup>` con clases `*-enter-active`/`*-leave-to`; anima `transform`/`opacity`.
  - **Svelte**: `svelte/transition` (`fade`, `fly`, `slide`) con la directiva `transition:`/`in:`/`out:`; `animate:flip` para reordenamientos de lista.
- Performance: anima solo `transform` y `opacity` (compositables, sin reflow). Evita animar `width`, `height`, `top`, `left`. No animes durante el scroll en el hilo principal si puedes usar transiciones CSS.

### Hooks de utilidad
- **Debounce** (espera a que pare la actividad: búsqueda, autosave) vs **throttle** (limita la frecuencia: scroll, resize, mousemove). Elige según el evento.
- Hazlo bien:
  - Limpia el timer en el cleanup del efecto/unmount para no llamar con el componente desmontado.
  - Mantén la referencia del callback fresca (`useRef` al último callback) para no capturar props/estado viejos por cierre.
  - Para *valores* prefiere `useDebouncedValue(value, delay)` (deriva un valor diferido); para *acciones* prefiere `useDebouncedCallback(fn, delay)`.
  - No recrees el debounced en cada render sin memoizar: pierdes el timer interno. Memoiza y respeta las deps.
- Vue: composable que limpia en `onScopeDispose`. Svelte: store derivado/función que limpia en el cleanup del efecto. Antes de escribirlo, considera una utilidad probada (`lodash.debounce`, `@vueuse/core`).

### Específicos de Next.js / RSC
- **Server Components (por defecto en App Router)**: corren solo en el servidor, no envían JS al cliente, pueden ser `async` y leer datos directo (DB, fetch, secrets). Úsalos para todo lo que no necesite interactividad.
- **Client Components (`'use client'`)**: necesarios para estado, efectos, event handlers, hooks del navegador y APIs del DOM. Marca la frontera lo más **abajo y chica** posible; pasa Server Components como `children`/props a los Client para no convertir todo el árbol.
- **Data fetching**: hazlo en el Server Component más cercano al uso (colocation), no lo subas con prop-drilling. `fetch` se deduplica y cachea; configura revalidación (`revalidate`, `cache: 'no-store'`) por dato. No traigas datos en un Client Component con `useEffect` si un Server Component puede hacerlo.
- **Server Actions (`'use server'`)**: funciones server-side invocables desde el cliente para mutaciones (forms, botones) sin construir un endpoint manual. Valida y autoriza **siempre dentro** de la action (es una superficie pública); después llama `revalidatePath`/`revalidateTag` para refrescar.
- No pongas secrets ni lógica sensible en Client Components: termina en el bundle. Serializa solo datos planos a través de la frontera server→client (sin funciones ni clases).

## Relacionadas

[[ui-ux-pro-max]] · [[accessibility-wcag]]
