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

### Fase 3 — Navegador/WASM, parcialmente implementada

El runtime WASM monta vistas, mantiene regiones dinámicas con scopes propios y reconcilia nodos compatibles cuando puede. Los eventos que burbujean se delegan en el límite del montaje y los listeners de otros eventos se registran directamente. La aplicación de ejemplo incluye Counter y una lista de tareas con alta, marcado, eliminación y reordenación keyed.

La CI del branch de implementación ha verificado Rust estable, Rust 1.85, Clippy, compilación WASM y una build de producción de Vite. En el último ciclo de verificación, el WASM de producción fue **229,158 bytes raw y 87,987 bytes gzip**, por debajo del presupuesto de **1.5 MiB raw**, y **3 pruebas E2E de Chromium pasaron** (Counter/identidad de nodo, CRUD/reordenación keyed, y renderizado condicional). La CI comprueba el bundle estático de producción, no únicamente el servidor de desarrollo.

**La Fase 3 permanece abierta.** La actualización de árboles que contienen componentes todavía puede reemplazar rangos completos; falta completar el diff/commit explícito y preservar la identidad de instancia al cambiar componentes/props. Estos son bloqueos deliberados, no tareas que se puedan saltar para empezar la fase siguiente.

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

