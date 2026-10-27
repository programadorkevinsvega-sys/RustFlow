# RustFlow

> **Framework web en Rust: reactividad, componentes, renderizado HTML y servidor HTTP experimental.**

RustFlow es un proyecto de framework pre-1.0. Combina un runtime reactivo propio, un modelo tipado de vistas y componentes, un renderer DOM para WebAssembly, serialización SSR y un adaptador HTTP basado en Axum/Tokio/Tower.

**Estado del producto:** prototipo activo con APIs inestables; todavía no está listo para producción ni ofrece el flujo integrado de un framework maduro. Las capacidades descritas como disponibles aquí tienen implementación en el repositorio; los objetivos del roadmap no deben interpretarse como funciones publicadas.

## Qué está implementado hoy

- **Reactividad:** señales tipadas, lecturas con seguimiento de dependencias, escrituras, efectos, scheduler, batching, scopes de propietario y limpieza.
- **Componentes:** macro <code>#[rustflow::component]</code>, props tipados dentro del subconjunto documentado, callbacks tipados y sintaxis <code>view!</code>.
- **DOM/WASM:** montaje y reconciliación de texto, elementos, fragmentos, componentes, regiones dinámicas y listas con claves; eventos y limpieza de recursos.
- **Servidor HTTP:** rutas y grupos, handlers asíncronos, parámetros de ruta/query, decodificación básica de JSON, middleware, IDs de petición, límites, timeouts, apagado graceful y TLS local.
- **SSR:** serialización de vistas con escape, documentos HTML completos, páginas de error configurables, componentes de servidor y catálogo compartido seguro para uso concurrente.
- **Composición de páginas:** <code>HtmlPage</code>, propiedades de ruta explícitas y layouts de componentes anidados.
- **Hidratación optativa:** límites estructurales, adjunción de eventos al DOM compatible y recuperación conservadora ante discrepancias.
- **Navegación cliente optativa:** intercepción conservadora de enlaces del mismo origen, History API y mejora de formularios GET compatibles con fallback nativo.

Consulta el [estado público de las capacidades](website/src/content/docs/project-status.md), la [guía de componentes](website/src/content/docs/guides/components.md), la [guía SSR](website/src/content/docs/guides/ssr.md), la [guía de layouts](website/src/content/docs/guides/layouts.md), la [guía HTTP](website/src/content/docs/guides/server.md) y el [tutorial de página de producto](website/src/content/docs/tutorials/product-page.md).

## Qué sigue pendiente o tiene alcance limitado

- <code>use_memo</code>, <code>use_ref</code> y <code>use_context</code>.
- CLI propio, scaffolding y comandos <code>rustflow new/dev/build</code>.
- Validación de formularios de servidor, sesiones, autenticación/autorización, CSRF, server actions y persistencia integrada.
- Un router general integrado: el callback de navegación cliente y las rutas HTTP de servidor no son un router full-stack unificado.
- Un flujo de desarrollo y despliegue de producción validado, una política completa de compatibilidad y la revisión de seguridad de producción.

La hidratación y la navegación cliente existen como APIs explícitas y optativas; no activan automáticamente todas las páginas ni convierten por sí solas un proyecto en una aplicación full-stack lista para producción. Las cargas de archivo y los métodos de mutación no forman parte de la mejora progresiva de formularios actual.

## Estructura del workspace

- <code>crates/rustflow</code>: facade, runtime reactivo y componentes.
- <code>crates/rustflow-view</code>: modelo de vistas tipado, independiente del destino.
- <code>crates/rustflow-macros</code>: macros <code>view!</code> y <code>#[component]</code>.
- <code>crates/rustflow-dom</code>: runtime DOM/WASM, reconciliación, hidratación y navegación cliente.
- <code>crates/rustflow-ssr</code>: serialización segura de vistas a fragmentos HTML y datos acotados para hidratación.
- <code>crates/rustflow-server</code>: adapter HTTP, documentos HTML, páginas compuestas y servidor.
- <code>examples/browser</code>: aplicación de navegador con Counter, lista de tareas y cobertura Playwright.
- <code>website/</code>: documentación pública para desarrolladores.
- <code>docs/</code>: contratos técnicos, semántica, decisiones arquitectónicas y detalles de ingeniería.

## Probar el código

Requisitos generales: Git, Rust 1.85 o Rust stable y Cargo. Para el ejemplo del navegador se necesita además Node.js/npm, <code>wasm-pack</code>, el target WebAssembly y Chromium para E2E.

Desde la raíz del repositorio:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Para ejecutar el servidor HTTP de ejemplo:

~~~sh
cargo run -p rustflow-server --example basic
~~~

Abre <code>http://127.0.0.1:3000/</code>. Para compilar y ejecutar el ejemplo del navegador, consulta [examples/browser/README.md](examples/browser/README.md). Para ejecutar el website, consulta [website/README.md](website/README.md).

No hay todavía un flujo recomendado de instalación de una aplicación nueva desde un paquete estable publicado en crates.io ni un CLI propio. La forma más fiable de evaluar las APIs es usar el repositorio y los ejemplos de la misma revisión.

## Calidad, límites y contribuciones

La CI comprueba Rust 1.85 y stable, tests, Clippy, rustdoc, la compilación WASM, la build del ejemplo de navegador, E2E Chromium, el presupuesto de bundle, documentación y gobernanza. Un check verde verifica la revisión concreta; no implica por sí mismo estabilidad de API ni preparación para producción.

Antes de contribuir, consulta [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md), [ARCHITECTURE_RULES.md](ARCHITECTURE_RULES.md), [ROADMAP.md](ROADMAP.md), [docs/PRODUCT_SCOPE.md](docs/PRODUCT_SCOPE.md) y [SECURITY.md](SECURITY.md).

## Licencia

RustFlow se distribuye bajo licencia dual MIT o Apache-2.0. Puedes elegir cualquiera de ellas según los términos de cada licencia.
