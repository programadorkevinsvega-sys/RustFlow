---
title: Estado del proyecto
description: Capacidades verificadas de RustFlow y criterios funcionales que aún bloquean el cierre de fases.
---

RustFlow está en desarrollo y sus APIs son pre-1.0. El [roadmap del repositorio](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el progreso y los criterios de aceptación. Ninguna fase se considera cerrada solo porque la CI esté verde.

## Disponible como prototipo

### Fase 1 — Runtime reactivo

El runtime incluye señales tipadas, efectos, grafo de dependencias, scheduler, batching, scopes, limpieza y diagnósticos. La semántica, las pruebas de regresión y una línea base de microbenchmarks están documentadas. Estas medidas son una referencia inicial, no una garantía universal de rendimiento.

### Fase 2 — Vistas y componentes

El modelo nativo incluye nodos tipados, atributos, propiedades, callbacks de eventos, ciclo de vida, scopes de limpieza, registro de componentes y la macro view!. El contrato de autoría de componentes se implementa parcialmente en la Fase 3: #[component], un subconjunto de props tipados, callbacks y registro generado ya funcionan, pero la API no pretende ser compatible con React ni adopta hooks posicionales. Véase [Crear componentes](/guides/components/).

### Fase 3 — Navegador/WASM, implementación en integración

El runtime de navegador monta y reconcilia texto, elementos, fragmentos, listas con clave, componentes y regiones dinámicas. El renderer separa el plan de diff/evaluación del commit DOM; las ramas nuevas o incompatibles se preparan fuera del DOM antes del commit. Cuando tipo y clave del componente siguen siendo compatibles, se vuelven a evaluar las props sobre el mismo scope y estado interno. Las ramas sustituidas limpian sus instancias, efectos, listeners y callbacks de cleanup.

La suite de Chromium incluye cuatro escenarios: identidad DOM del Counter, actualización de prop visible con estado/reactividad e identidad DOM conservados, CRUD/reordenación keyed con preservación de estado local y cleanup al sustituir la clave o eliminar la fila, y montaje/desmontaje condicional sin reemplazar la aplicación. El workflow de CI compila el target WASM, ejecuta Rust estable/1.85, Clippy, tests, build de producción y el límite de 1.5 MiB raw para WASM; el criterio de aceptación es que todos pasen en el HEAD exacto.

**Estado de integración:** esta implementación se mantiene en la PR apilada #26, cuya base es la rama de la PR #25. Aunque se implementen los criterios técnicos en esta rama, la Fase 3 no se da por cerrada en `main` hasta que #26 se revise e integre en #25 y la PR #25 se cierre mediante la revisión/fusión autorizada. La Fase 4 sigue bloqueada hasta entonces.

### Fase 4 — Servidor HTTP, en prototipo

El crate rustflow-server integra Axum/Tokio/Tower detrás de tipos de RustFlow. Incluye rutas y handlers asíncronos, parámetros de ruta y query, extracción básica de JSON, respuestas de texto/HTML/JSON, middleware global ordenado, rutas nombradas, request IDs, eventos/spans de tracing, límites de body, timeouts, límite de concurrencia y apagado graceful mediante una future proporcionada por la aplicación.

El serializador rustflow-ssr genera fragmentos HTML escapados. Desde el servidor se pueden generar documentos HTML completos mediante html_document y páginas de error HTML configurables para estados 4xx/5xx. Las pruebas cubren dispatch in-memory y casos representativos por TCP local. Esto no implica que SSR, hidratación o despliegue estén completos.

Guía práctica: [Servidor HTTP experimental](/guides/server/). Detalles y límites: [docs/HTTP_SERVER.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/HTTP_SERVER.md) y [docs/SSR_RENDERING.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/SSR_RENDERING.md).

## Próximos criterios bloqueantes

- Completar la separación entre evaluación/diff y commit DOM; reconciliar todos los tipos de subárbol sin perder identidad cuando corresponda.
- Preservar el estado/identidad de los componentes al actualizar sus descriptores y props.
- Ampliar las pruebas de navegador para cubrir estos casos de reconciliación y limpieza antes de cerrar la Fase 3.
- Composición de layouts, serialización de datos para hidratación e hidratación de cliente; un documento HTML completo no equivale a hidratación.
- Grupos de rutas, middleware por ruta, configuración de producción y más pruebas de red para TLS/reverse proxy, cabeceras de proxy confiables, abortos de cliente y carga/concurrencia.
- CLI, scaffolding, validación de formularios, sesiones/autenticación y persistencia integrada.

