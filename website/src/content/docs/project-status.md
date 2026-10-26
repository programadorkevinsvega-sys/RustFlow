---
title: Estado del proyecto
description: Capacidades verificadas de RustFlow, estado de las fases y alcance que permanece pendiente.
---

RustFlow está en desarrollo y sus APIs son pre-1.0. El [roadmap del repositorio](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el progreso y los criterios de aceptación. Ninguna fase se considera cerrada solo porque la CI esté verde.

### Fase 0 — Fundación del proyecto, completada

La fundación define licencia dual, gobernanza, reglas de contribución, alcance inicial, decisiones de arquitectura, MSRV Rust 1.85 y controles de ingeniería/CI. El alcance de producto documenta los objetivos y no-objetivos del prototipo; este estado no significa que RustFlow sea estable ni que el MVP full-stack completo exista.

## Disponible como prototipo

### Fase 1 — Runtime reactivo

El runtime incluye señales tipadas, efectos, grafo de dependencias, scheduler, batching, scopes, limpieza y diagnósticos. La semántica, las pruebas de regresión y una línea base de microbenchmarks están documentadas. Estas medidas son una referencia inicial, no una garantía universal de rendimiento.

### Fase 2 — Vistas y componentes

El modelo nativo de la Fase 2 define nodos tipados, atributos, propiedades, callbacks de eventos, ciclo de vida, scopes de limpieza, registro y la macro view!. Su cierre cubre el modelo estructural, no el producto completo de autoría ergonómica de componentes. La API #[component], el subconjunto de props tipados, los callbacks y el runtime de navegador se implementaron y verificaron como parte de la Fase 3. RustFlow no pretende ser compatible con React ni adopta hooks posicionales. Véase [Crear componentes](/guides/components/).

### Fase 3 — Navegador/WASM, cerrada y verificada

El runtime de navegador monta y reconcilia texto, elementos, fragmentos, listas con clave, componentes y regiones dinámicas. El renderer separa el plan de diff/evaluación del commit DOM; las ramas nuevas o incompatibles se preparan fuera del DOM antes del commit. Cuando tipo y clave del componente siguen siendo compatibles, se vuelven a evaluar las props sobre el mismo scope y estado interno. Las ramas sustituidas limpian sus instancias, efectos, listeners y callbacks de cleanup.

La suite de Chromium incluye cuatro escenarios: identidad DOM del Counter, actualización de prop visible con estado/reactividad e identidad DOM conservados, CRUD/reordenación keyed con preservación de estado local y cleanup al sustituir la clave o eliminar la fila, y montaje/desmontaje condicional sin reemplazar la aplicación. El workflow de CI compila el target WASM, ejecuta Rust estable/1.85, Clippy, tests, build de producción y el límite de 1.5 MiB raw para WASM; el criterio de aceptación es que todos pasen en el HEAD exacto.

**Estado: cerrada en `main` el 2026-10-26.** La PR #26 se fusionó primero en la rama de la PR #25; después la PR #25 se fusionó en `main`. Los siete checks post-merge terminaron correctamente en el commit [`43f5fb9`](https://github.com/programadorkevinsvega-sys/RustFlow/commit/43f5fb9c12996ea600b47930553a9f91dc6055e9), incluidos Rust estable/1.85, compilación WASM, E2E Chromium, presupuesto del bundle, documentación, gobernanza y despliegue de Pages. El cierre se limita al runtime de navegador y los criterios de la Fase 3; SSR/hidratación y otros hitos posteriores permanecen separados.

### Fase 4 — Servidor HTTP, cerrada y verificada

El crate `rustflow-server` integra Axum/Tokio/Tower detrás de tipos de RustFlow. Incluye rutas y handlers asíncronos, grupos de rutas, parámetros de ruta y query, extracción básica de JSON, respuestas de texto/HTML/JSON, middleware global, de grupo y por ruta, rutas nombradas, request IDs, eventos/spans de `tracing`, límites de body, timeouts, límite de concurrencia, apagado graceful, TLS local con rustls y política de proxies de confianza.

**Cierre formal:** la PR #31 añadió handshakes TLS concurrentes limitados a 64, un timeout de 10 segundos y una regresión TCP real para demostrar que un cliente sin ClientHello no bloquea conexiones TLS válidas. La PR #31 se fusionó en `main` el 2026-10-26 mediante el commit [`6f4123b`](https://github.com/programadorkevinsvega-sys/RustFlow/commit/6f4123b94f4273701f0336c4c982973af7bf7562). Los checks post-merge pasaron: Rust 1.85, Rust stable, WASM, E2E Chromium/presupuesto de bundle, Rustdoc, build del website, gobernanza, revisión de dependencias y Cloudflare Pages.

El serializador `rustflow-ssr` genera fragmentos HTML escapados; `html_document` y las páginas configurables 4xx/5xx no implican hidratación ni SSR full-stack. La fase se cierra dentro de su alcance de servidor HTTP de prototipo, no como declaración de preparación para producción. Siguen pendientes sesiones/autenticación, CSRF, rate limiting, recetas de despliegue y threat model de producción, SSR completo e hidratación. Véase el [roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) para la evidencia y los límites.

Guía práctica: [Servidor HTTP experimental](/guides/server/). Detalles y límites: [docs/HTTP_SERVER.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/HTTP_SERVER.md) y [docs/SSR_RENDERING.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/SSR_RENDERING.md).

## Próximos criterios bloqueantes

- Composición de layouts, serialización de datos para hidratación e hidratación de cliente; un documento HTML completo no equivale a hidratación.
- CLI, scaffolding, validación de formularios, sesiones/autenticación y persistencia integrada.

