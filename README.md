# RustFlow

> **Framework web en Rust: reactividad, componentes, renderizado HTML y servidor HTTP experimental.**

RustFlow es un proyecto de framework pre-1.0. Combina un runtime reactivo propio, un modelo tipado de vistas y componentes, un renderer DOM para WebAssembly, serialización SSR y un adaptador HTTP basado en Axum/Tokio/Tower.

**Estado del producto:** prototipo activo con APIs inestables; todavía no está listo para producción ni ofrece el flujo integrado de un framework maduro. Las capacidades descritas como disponibles aquí tienen implementación en el repositorio; los objetivos del roadmap no deben interpretarse como funciones publicadas.

## Qué está implementado hoy

- **Reactividad:** señales tipadas, lecturas con seguimiento de dependencias, escrituras, efectos, scheduler, batching, scopes de propietario y limpieza.
- **Componentes:** macro <code>#[rustflow::component]</code>, props tipados dentro del subconjunto documentado, callbacks tipados y sintaxis <code>view!</code>.
- **DOM/WASM:** montaje y reconciliación de texto, elementos, fragmentos, componentes, regiones dinámicas y listas con claves; eventos y limpieza de recursos.
- **Servidor HTTP:** rutas y grupos, handlers asíncronos, extracción tipada, validación 422, CSRF optativo, sesiones server-side experimentales, cookies seguras, guards de autenticación, políticas deny-by-default, IDs de petición, límites, timeouts, apagado graceful y TLS local.
- **Base de datos:** integración experimental. La implementación de Phase 6.7.2 en la PR #59 incorpora SeaORM 1.1.20 en `rustflow-server` sobre un único pool SQLx tipado por backend (SQLite, PostgreSQL y protocolo MySQL compartido por MySQL/MariaDB); ambos usan el mismo pool, sin duplicarlo. `DatabasePool::entity` ofrece un `EntityRepository` con CRUD/bulk y consultas SeaORM tipadas; los loaders de relaciones son explícitos y no ejecutan I/O perezoso. Las transacciones admiten operaciones ORM y SQL parametrizado sobre la misma conexión. La subfase 6.7.2 quedó fusionada mediante Squash Merge en `main` el 2026-10-31, con commit `8d387943d5f6ccb4d3f33ba3708311442e238c5f`; las pruebas de los cuatro motores y los 8 checks requeridos pasaron en el HEAD final de la PR.  La API sigue siendo experimental y pre-1.0: las pruebas cubren los cuatro motores, pero la matriz de tipos tiene límites documentados (por ejemplo, Decimal/BigDecimal de SQLite pasa por `f64`, y `u64` completo solo en MySQL/MariaDB). MongoDB mantiene su adaptador BSON independiente; su ODM de 6.7.3 está aceptado por CI en PR #60 (HEAD `c91a558`), aunque la PR todavía no está fusionada en `main`. Consulta [`docs/DATABASES.md`](docs/DATABASES.md), [`docs/ORM_MODEL_CONTRACT.md`](docs/ORM_MODEL_CONTRACT.md), [`docs/DATABASE_SUPPORT_MATRIX.md`](docs/DATABASE_SUPPORT_MATRIX.md) y [`docs/adr/0021-mongodb-odm-architecture.md`](docs/adr/0021-mongodb-odm-architecture.md).
- **SSR:** serialización de vistas con escape, documentos HTML completos, páginas de error configurables, componentes de servidor y catálogo compartido seguro para uso concurrente.
- **Composición de páginas:** <code>HtmlPage</code>, propiedades de ruta explícitas y layouts de componentes anidados.
- **Hidratación optativa:** límites estructurales, adjunción de eventos al DOM compatible y recuperación conservadora ante discrepancias.
- **Navegación cliente optativa:** intercepción conservadora de enlaces del mismo origen, History API y mejora de formularios GET compatibles con fallback nativo.

Consulta el [estado público de las capacidades](website/src/content/docs/project-status.md), la [guía de componentes](website/src/content/docs/guides/components.md), la [guía HTTP](website/src/content/docs/guides/server.md), la [guía de formularios y validación](website/src/content/docs/guides/forms-validation.md), la [guía SSR](website/src/content/docs/guides/ssr.md), la [guía de layouts](website/src/content/docs/guides/layouts.md) y el [tutorial de página de producto](website/src/content/docs/tutorials/product-page.md).

## Qué sigue pendiente o tiene alcance limitado

- <code>use_memo</code>, <code>use_ref</code> y <code>use_context</code> siguen pendientes.
- La configuración de la Fase 6.2 cubre solo perfil, host y puerto; no es un sistema general ni un gestor de secretos.
- La Fase 6.4 de sesiones y control de acceso ya está integrada en `main` y sus checks post-merge pasaron. Las APIs siguen siendo experimentales: `InMemorySessionStore` es volátil y local al proceso, y la Fase 6.5 no añade un adaptador SQL durable para sesiones. No se incluyen proveedor de usuarios, OAuth/OIDC, MFA, recuperación de cuentas ni rate limiting.
- Un router general integrado: el callback de navegación cliente y las rutas HTTP de servidor no son un router full-stack unificado.
- La Fase 6.5 añadió pools, transacciones y migraciones SQLx; las fases 6.6.1–6.6.3 completaron el soporte documentado de SQLite, PostgreSQL, MySQL, MariaDB y el adaptador MongoDB. La Fase 6.7 está en progreso: 6.7.1 (ADR 0020 y contrato de modelos) ya está integrada en `main`; 6.7.2 (SeaORM, pools tipados compartidos, CRUD/bulk, consultas, loaders explícitos, codecs y transacciones mixtas) está integrada en `main` mediante PR #59. La subfase 6.7.3 (ODM MongoDB) está implementada y aceptada por CI en la PR #60, sobre el HEAD `c91a558934851ea17e10dd9ebde13b8869fbbcf1`; la PR sigue abierta y aún no está fusionada en `main`. El ODM tipado reutiliza el cliente MongoDB existente y mantiene separada la semántica documental de SQL. La API permanece experimental; las subfases posteriores cubren extensiones avanzadas, la aplicación CRUD generada y el gate global. Siguen pendientes sesiones SQL durables, proveedor de usuarios y una receta de despliegue de producción validada.
- Un flujo de desarrollo y despliegue de producción validado, una política completa de compatibilidad y la revisión de seguridad de producción.

La hidratación y la navegación cliente existen como APIs explícitas y optativas; no activan automáticamente todas las páginas ni convierten por sí solas un proyecto en una aplicación full-stack lista para producción. Las cargas de archivo y los métodos de mutación no forman parte de la mejora progresiva de formularios actual.

## Estructura del workspace

- <code>crates/rustflow</code>: facade, runtime reactivo y componentes.
- <code>crates/rustflow-view</code>: modelo de vistas tipado, independiente del destino.
- <code>crates/rustflow-macros</code>: macros <code>view!</code> y <code>#[component]</code>.
- <code>crates/rustflow-dom</code>: runtime DOM/WASM, reconciliación, hidratación y navegación cliente.
- <code>crates/rustflow-ssr</code>: serialización segura de vistas a fragmentos HTML y datos acotados para hidratación.
- <code>crates/rustflow-server</code>: adapter HTTP, documentos HTML, páginas compuestas y servidor.
- <code>crates/rustflow-cli</code>: binario <code>rustflow</code>, scaffolding de servidor y generadores protegidos contra colisiones.
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
cargo test -p rustflow-cli
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Para ejecutar el servidor HTTP de ejemplo:

~~~sh
cargo run -p rustflow-server --example basic
~~~

Abre <code>http://127.0.0.1:3000/</code>. Para compilar y ejecutar el ejemplo del navegador, consulta [examples/browser/README.md](examples/browser/README.md). Para ejecutar el website, consulta [website/README.md](website/README.md).

No existe una publicación estable de RustFlow en crates.io. Desde un checkout de este repositorio puedes instalar el CLI experimental y crear una aplicación de servidor:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow new hello-rustflow
cd hello-rustflow
rustflow dev
~~~

Consulta [docs/CLI.md](docs/CLI.md), [docs/CONFIGURATION.md](docs/CONFIGURATION.md), la [guía pública del CLI](website/src/content/docs/guides/cli.md) y la [guía pública de configuración](website/src/content/docs/guides/configuration.md) para conocer comandos, perfiles y límites de la aplicación generada.

## Calidad, límites y contribuciones

La CI comprueba Rust 1.85 y stable, tests, Clippy, rustdoc, la compilación WASM, la build del ejemplo de navegador, E2E Chromium, el presupuesto de bundle, documentación y gobernanza. Un check verde verifica la revisión concreta; no implica por sí mismo estabilidad de API ni preparación para producción.

Antes de contribuir, consulta [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md), [ARCHITECTURE_RULES.md](ARCHITECTURE_RULES.md), [ROADMAP.md](ROADMAP.md), [docs/PRODUCT_SCOPE.md](docs/PRODUCT_SCOPE.md) y [SECURITY.md](SECURITY.md).

## Licencia

RustFlow se distribuye bajo licencia dual MIT o Apache-2.0. Puedes elegir cualquiera de ellas según los términos de cada licencia.

