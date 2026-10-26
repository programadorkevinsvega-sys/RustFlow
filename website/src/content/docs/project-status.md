---
title: Estado del proyecto
description: Capacidades de RustFlow verificadas en el repositorio y trabajo que sigue pendiente.
---

RustFlow está en desarrollo activo y sus APIs son pre-1.0. El [roadmap del repositorio](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el progreso y los criterios de aceptación.

## Disponible como prototipo

### Fase 1 — Runtime reactivo

El runtime incluye señales tipadas, efectos, grafo de dependencias, scheduler, batching, scopes y diagnósticos. La semántica, la cobertura de regresión y una línea base inicial de microbenchmarks están documentadas. La CI ha verificado los checks configurados para Rust 1.85 y stable; las mediciones de benchmark son una referencia inicial, no una garantía de rendimiento.

Consulta la [implementación](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow/src/reactivity.rs), la [guía técnica del runtime](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_RUNTIME.md) y la [verificación](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_VERIFICATION.md).

### Fase 2 — Vistas y componentes

El modelo nativo incluye nodos tipados, atributos y propiedades, callbacks de eventos, ciclo de vida, scopes de limpieza, registro de componentes y la macro `view!`. Esto es un modelo estructural de prototipo, no todavía la experiencia React-like integrada que se busca. La API `signal(value)`, `Signal::update`, `#[component]`, props/callbacks tipados e interpolación reactiva automática siguen pendientes. Consulta [Crear componentes](/guides/components/) para diferenciar lo disponible del objetivo futuro.

### Fase 3 — Navegador/WASM, parcialmente implementada

El renderer inicial puede montar vistas y manejar listeners; las comprobaciones de CI compilan el adaptador para `wasm32-unknown-unknown`. Todavía faltan el bundle cargable por un navegador, actualizaciones DOM dirigidas, renderizado condicional, reconciliación keyed, ejemplos interactivos completos y pruebas E2E.

### Fase 4 — Servidor HTTP, en prototipo

El crate `rustflow-server` integra Axum/Tokio/Tower detrás de tipos de RustFlow. Incluye rutas y handlers asíncronos, parámetros de ruta y query, extracción básica de JSON, respuestas de texto/HTML/JSON, middleware global ordenado, rutas nombradas, request IDs, eventos/spans de `tracing`, límites de body, timeouts, límite de concurrencia y apagado graceful mediante una future proporcionada por la aplicación.

El serializador `rustflow-ssr` genera fragmentos HTML escapados. Desde el servidor se pueden generar documentos HTML completos mediante `html_document` y páginas de error HTML configurables para estados 4xx/5xx. Las pruebas cubren dispatch in-memory y casos representativos por TCP local: HTML/JSON, path/query, errores 400/404/405, body 413, timeout 504, cabeceras de request ID y apagado graceful.

Guía práctica: [Servidor HTTP experimental](/guides/server/). Detalles y límites: [docs/HTTP_SERVER.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/HTTP_SERVER.md) y [docs/SSR_RENDERING.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/SSR_RENDERING.md).

## Qué sigue pendiente

- La API ergonómica de componentes React-like: `signal(value)` con `get/set/update`, `#[component]`, props y callbacks tipados, e interpolación reactiva automática en `view!`.
- El vocabulario de API familiar en Rust `snake_case`: `use_state`, `use_effect`, `use_memo`, `use_ref` y `use_context` son nombres propuestos, no APIs disponibles todavía. La intención es conservar semántica signal-first, no copiar las reglas de hooks por orden de llamada de React.
- Un bundle de navegador, actualización DOM por subárbol, condicionales/listas keyed y tests E2E; el renderer actual reemplaza el rango montado en actualizaciones reactivas.
- Composición avanzada de layouts, serialización de datos para hidratación e hidratación de cliente; los documentos HTML completos no equivalen a hidratación.
- Grupos de rutas, middleware por ruta, configuración de producción y más pruebas de red para TLS/reverse proxy, cabeceras de proxy confiables, abortos de cliente y carga/concurrencia.
- CLI, scaffolding, validación de formularios, sesiones/autenticación y persistencia integrada.

No interpretes el HTML estructural seguro ni el servidor experimental como garantía de seguridad para despliegue o como declaración de producción. Verifica el código, los límites documentados y el estado actual de los checks antes de depender de una capacidad.
