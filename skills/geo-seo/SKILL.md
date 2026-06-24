---
name: geo-seo
description: >
  GEO + SEO: optimizar para motores generativos (ChatGPT, Claude, Perplexity, Gemini) y buscadores tradicionales — citability, llms.txt, JSON-LD/schema, acceso de crawlers de IA, brand mentions y contenido autocontenido.
license: Apache-2.0
metadata:
  type: conocimiento
  origin: propia (destila zubair-trabzada/geo-seo-claude, MIT)
  activation: bajo_demanda
  version: "1.0"
---

# GEO + SEO (optimización para motores generativos y buscadores)

Guía de conocimiento para optimizar sitios de modo que sean **citados por motores generativos** (ChatGPT, Claude, Perplexity, Gemini) y **rankeen en buscadores tradicionales**. Cubre citability, llms.txt, datos estructurados, acceso de crawlers de IA, brand mentions y SEO técnico base.

## Cuándo usar

Usa esta skill cuando trabajes en:

- **Landings y sitios de cliente** que dependen de tráfico orgánico o de visibilidad pública.
- **Contenido que debe ser citado por IA**: artículos, guías, páginas de producto, FAQs, documentación de marca.
- **Contenido que debe rankear** en Google/Bing por keywords con intención comercial o informacional.
- **Negocios locales** que quieren aparecer en respuestas de IA y en el paquete local de buscadores.

**Cuándo NO usar:**

- App interna sin tráfico público (dashboards admin, paneles privados, herramientas de equipo).
- Endpoints/APIs detrás de auth: no se indexan ni se citan, y exponerlos es riesgo de seguridad.
- Páginas transaccionales sensibles (checkout, perfil de usuario): se excluyen de indexación a propósito.

Regla general: si la página no busca audiencia externa, GEO+SEO no aplica y agregar marcado solo añade ruido.

## GEO vs SEO

**SEO (Search Engine Optimization)** optimiza para que un buscador tradicional (Google, Bing) liste tu URL en su página de resultados. El usuario hace clic y llega a tu sitio. Métricas: posición, CTR, tráfico orgánico.

**GEO (Generative Engine Optimization)** optimiza para que un motor generativo (ChatGPT, Claude, Perplexity, Gemini, AI Overviews de Google) **mencione tu marca o cite tu contenido** dentro de su respuesta. Muchas veces el usuario NUNCA visita tu sitio: la IA resume y atribuye. La meta es ser la fuente que el modelo elige citar.

**Por qué GEO importa cada vez más:** la búsqueda asistida por IA desplaza tráfico de la búsqueda tradicional. Cuando el modelo responde directo, el clic clásico desaparece (zero-click) y la visibilidad pasa a depender de **ser citado**, no de rankear. Quien no aparece en las respuestas generativas pierde presencia aunque rankee bien en Google.

**Se complementan, no compiten:** un sitio bien estructurado para buscadores (HTML semántico, contenido claro, datos estructurados, buen rendimiento) es también más fácil de parsear y citar para un modelo. La base técnica de SEO es prerrequisito de GEO. La diferencia está en cómo redactas y estructuras el contenido para que sea **extraíble y atribuible**.

## Citability (que la IA te cite)

La citability es la propiedad de que un motor generativo pueda extraer un hecho de tu página y atribuírtelo con confianza. Claves:

- **Contenido autocontenido y rico en hechos.** Cada bloque debe entenderse sin contexto externo. La IA extrae fragmentos sueltos; si tu párrafo depende de tres secciones anteriores, no es citable. Incluye el sujeto explícito en cada afirmación (no "esto mejora un 40%" sino "el plan Pro reduce el tiempo de respuesta un 40%").
- **Responder la pregunta directo en el primer párrafo.** Patrón de "respuesta primero": la definición o la cifra clave va al inicio, no como conclusión al final. Los modelos priorizan el contenido que resuelve la intención de inmediato.
- **Bloques óptimos de ~134-167 palabras.** Secciones de ese tamaño son la unidad ideal para que el modelo las cite enteras: suficientes para tener contexto, lo bastante acotadas para ser un fragmento limpio. Divide el contenido en bloques temáticos de ese rango con un encabezado descriptivo cada uno.
- **Datos, fechas y fuentes verificables.** Cifras concretas, fechas explícitas ("según el informe X de marzo de 2025"), estadísticas con su origen y citas a fuentes primarias aumentan la probabilidad de cita. La IA prefiere atribuir hechos comprobables. Evita afirmaciones vagas sin respaldo.
- **Estructura escaneada:** encabezados claros (H2/H3), listas, tablas y definiciones. La estructura ayuda al modelo a localizar la respuesta exacta.

## llms.txt

`llms.txt` es un estándar emergente: un archivo de texto en la raíz del sitio (`https://tudominio.com/llms.txt`) pensado para que los modelos de IA descubran y prioricen tu contenido más relevante, en lenguaje legible por máquina y humano.

**Qué incluir** (formato markdown):

- Un `# Título` con el nombre del sitio/marca.
- Un blockquote con una descripción breve de qué es el proyecto/empresa.
- Secciones con listas de enlaces a las páginas y documentos clave (docs, guías, productos, políticas), cada enlace con una descripción de una línea.
- Opcionalmente un `## Optional` con recursos secundarios que el modelo puede omitir si tiene poco contexto.

**Dónde se sirve:** en la raíz del dominio como archivo estático, accesible sin autenticación. Variante `llms-full.txt` para volcar el contenido completo concatenado cuando el sitio es chico. No reemplaza a `sitemap.xml` ni a `robots.txt`: es complementario y orientado a consumo por LLM.

El estándar aún no es universal, pero su costo es bajo y algunos crawlers de IA ya lo consideran. Inclúyelo en sitios donde la citability es prioridad.

## Datos estructurados / JSON-LD

Los datos estructurados con vocabulario **schema.org** (formato **JSON-LD** en un `<script type="application/ld+json">`) le dicen a buscadores y a la IA qué representa cada entidad de la página. Habilitan rich results y mejoran la extracción.

Schema por tipo de negocio (elige los que apliquen):

- **Organization** — identidad de la empresa: nombre, logo, URL, redes sociales (`sameAs`), datos de contacto. Base para casi todo sitio de marca.
- **LocalBusiness** — negocio con ubicación física: dirección, horarios, teléfono, geocoordenadas, rango de precios. Clave para visibilidad local.
- **Product** — producto: nombre, descripción, precio (`offers`), disponibilidad, reseñas (`aggregateRating`).
- **FAQPage** — preguntas frecuentes: cada par pregunta/respuesta marcado. Muy citable por IA, porque ya viene en formato Q&A.
- **Article** — contenido editorial: titular, autor, fecha de publicación/actualización, editor. Refuerza autoría y frescura.
- **BreadcrumbList** — ruta de navegación jerárquica; ayuda a buscadores a entender la estructura del sitio.

**El marcado lo implementa frontend.** Esta skill define qué tipos usar y qué propiedades importan; la inserción del JSON-LD en el HTML/componentes es trabajo de frontend (ver [[frontend-component-patterns]]). Todo JSON-LD debe validarse antes de publicar (ver Validación).

## Crawlers de IA

`robots.txt` controla qué bots pueden rastrear tu sitio. Los crawlers de IA son agentes específicos que debes decidir conscientemente si permitir o bloquear. Hay 14+ relevantes; los principales:

- **GPTBot** (OpenAI, entrenamiento) y **OAI-SearchBot** / **ChatGPT-User** (búsqueda y navegación en vivo de ChatGPT).
- **ClaudeBot** / **Claude-Web** / **anthropic-ai** (Anthropic).
- **PerplexityBot** / **Perplexity-User** (Perplexity).
- **Google-Extended** (controla uso por Gemini/Vertex sin afectar el ranking en Search) y **Googlebot** (búsqueda clásica).
- **Bingbot** (alimenta Bing y Copilot), **Applebot** / **Applebot-Extended**, **CCBot** (Common Crawl, alimenta a muchos modelos), **Amazonbot**, **Meta-ExternalAgent**, **Bytespider** (TikTok/ByteDance), **cohere-ai**, **Diffbot**.

**Cuándo permitir o bloquear:**

- **Permitir** (lo habitual para sitios de cliente que buscan visibilidad): dejas que los crawlers de IA accedan a tu contenido público para poder ser citado.
- **Bloquear** un crawler concreto solo cuando hay una razón explícita: contenido propietario que no quieres en datasets de entrenamiento, costos de ancho de banda, o política del cliente.

**El costo de bloquear:** si bloqueas GPTBot, ClaudeBot, PerplexityBot, etc., **pierdes visibilidad en IA**: esos motores ya no podrán citarte ni recomendarte. Bloquear entrenamiento es distinto de bloquear navegación en vivo (p. ej. `ChatGPT-User` o `Perplexity-User`), y conviene separarlos: puedes impedir entrenamiento pero permitir que te citen en tiempo real. Nunca bloquees por descuido un crawler de IA en un sitio cuyo objetivo es ser descubierto.

## Brand mentions

Las **menciones de marca** (que tu nombre aparezca en webs, foros, reseñas, medios y comunidades, con o sin enlace) correlacionan aproximadamente **3x más que los backlinks** con la visibilidad en respuestas de IA. Los modelos aprenden asociaciones de entidades a partir de cómo y dónde se nombra tu marca en el corpus, no solo de la estructura de enlaces.

Cómo cultivarlas:

- **Presencia en fuentes que los modelos consumen mucho:** Wikipedia (si aplica notabilidad), Reddit, foros del nicho, listados y directorios reputados, reseñas de terceros, prensa y blogs del sector.
- **Consistencia de entidad:** usa el mismo nombre, descripción y datos en todos lados (web, redes, directorios). Refuerza la propiedad `sameAs` del schema Organization y ayuda al modelo a fusionar las menciones en una sola entidad.
- **Contenido digno de ser citado y compartido:** datos originales, estudios, comparativas y definiciones claras generan menciones naturales.
- **Relaciones públicas y participación en comunidad** generan menciones contextuales (no solo enlaces), que es lo que más mueve la visibilidad en IA.

Las menciones complementan al SEO clásico: los backlinks siguen importando para ranking en buscadores, pero para GEO la mención de marca pesa más.

## SEO técnico base

Fundamentos que sostienen tanto SEO como GEO:

- **Meta tags:** `<title>` único y descriptivo por página, `meta description` relevante, Open Graph y Twitter Cards para previsualizaciones al compartir.
- **HTML semántico:** un solo `<h1>`, jerarquía correcta de encabezados, `<nav>`, `<main>`, `<article>`, `<header>`, `<footer>`. La semántica ayuda a buscadores y modelos a entender la estructura (ver [[accessibility-wcag]]: lo que mejora accesibilidad también mejora SEO).
- **sitemap.xml:** lista de URLs indexables con fecha de última modificación; referenciado desde `robots.txt`.
- **Core Web Vitals / performance:** LCP, INP y CLS en verde; rendimiento es factor de ranking y de experiencia. Optimiza imágenes, JS y carga crítica.
- **Mobile-first:** diseño responsive; el índice de Google es mobile-first.
- **canonical:** `<link rel="canonical">` para evitar contenido duplicado y consolidar señales en la URL preferida.
- **HTTPS, URLs limpias, hreflang** (si hay multi-idioma) y manejo correcto de códigos 301/404.

## Integración con Turtle

- **Guía PORTABLE y sin Python.** Esta skill es conocimiento puro: principios, checklists y criterios. No requiere Python ni Node ni ninguna herramienta del núcleo (RNF-RES-04). Se aplica leyendo y decidiendo, no ejecutando.
- **El marcado/JSON-LD lo implementa frontend (handoff).** Como skill de conocimiento, define el QUÉ (tipos de schema, estructura de contenido, llms.txt). La implementación concreta del JSON-LD y los meta tags en el código la hace el rótulo `frontend` siguiendo [[frontend-component-patterns]] y [[ui-ux-pro-max]]. Coordina el handoff por la mensajería de Turtle.
- **Sinergia con accesibilidad y UI/UX.** Enlaza [[accessibility-wcag]] (la semántica HTML que mejora accesibilidad también mejora SEO y citability), [[ui-ux-pro-max]] y [[frontend-component-patterns]] para que la estructura visible y la estructura semántica vayan alineadas.
- **Tooling opcional del repo origen.** El repositorio de origen (licencia MIT) ofrece utilidades opcionales en Python/PDF para auditoría y reportes, útiles para quien trabaje con Claude Code. Son opcionales y externas: el núcleo de Turtle no las necesita ni las ejecuta (RNF-SEG-05: contenido importado no se ejecuta sin acción explícita del usuario).

## Reglas duras

- **Sin contenido autocontenido no hay citability.** Si los bloques dependen de contexto externo, la IA no puede extraerlos ni atribuirlos. Cada bloque se sostiene solo o no sirve.
- **Nunca bloquear crawlers de IA sin decisión explícita.** Bloquear GPTBot, ClaudeBot, PerplexityBot, etc. por descuido borra tu visibilidad en motores generativos. Cualquier bloqueo debe ser una decisión consciente y documentada del cliente, no un default.
- **Todo JSON-LD validado.** Ningún marcado se publica sin pasar validación (Rich Results Test / Schema Markup Validator). Schema inválido no genera rich results y puede penalizar.
- **Sin keyword stuffing.** Repetir keywords de forma antinatural daña ranking y citability. Escribe para personas y modelos, no para algoritmos: contenido claro, honesto y rico en hechos.

## Validación

Checklist de auditoría GEO+SEO antes de dar por terminada una página o sitio:

**Citability**
- [ ] La pregunta/intención se responde en el primer párrafo (respuesta primero).
- [ ] Bloques temáticos de ~134-167 palabras con encabezado descriptivo.
- [ ] Cada bloque es autocontenido (sujeto explícito, sin depender de otras secciones).
- [ ] Datos, fechas y fuentes verificables presentes.

**llms.txt**
- [ ] `llms.txt` presente en la raíz, accesible sin auth, con enlaces y descripciones a las páginas clave.

**Datos estructurados**
- [ ] JSON-LD con los tipos schema.org correctos para el negocio (Organization/LocalBusiness/Product/FAQ/Article/BreadcrumbList).
- [ ] Schema validado sin errores (Rich Results Test / Schema Validator).

**Crawlers**
- [ ] `robots.txt` revisado: los crawlers de IA están permitidos (o su bloqueo es una decisión explícita y documentada).
- [ ] `sitemap.xml` presente y referenciado en `robots.txt`.

**SEO técnico**
- [ ] `<title>`, meta description, Open Graph y canonical correctos por página.
- [ ] HTML semántico (un `<h1>`, jerarquía de encabezados, landmarks).
- [ ] Core Web Vitals en verde (LCP, INP, CLS) y diseño mobile-first.

**Brand**
- [ ] Estrategia de brand mentions definida; datos de entidad consistentes y `sameAs` poblado.
