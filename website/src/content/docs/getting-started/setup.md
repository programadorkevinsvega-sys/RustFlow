---
title: Empezar con RustFlow
description: Prepara Rust, prueba el runtime, ejecuta el servidor y compila la demo interactiva.
---

Esta guía te ayuda a comprobar el código real de RustFlow. Distingue tres tareas: ejecutar los tests del workspace, probar el ejemplo HTTP y levantar el ejemplo WASM del navegador. **No es todavía un instalador de aplicaciones**: RustFlow es pre-1.0, no tiene un CLI propio ni un workflow estable de scaffolding.

## 1. Requisitos

Para el workspace necesitas Git, Rust 1.85 o Rust stable y Cargo:

~~~sh
rustc --version
cargo --version
git --version
~~~

Para el ejemplo del navegador necesitas además Node.js 22/npm, <code>wasm-pack</code>, el target <code>wasm32-unknown-unknown</code> y Chromium si vas a ejecutar E2E. Los pasos están en [README del ejemplo de navegador](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md).

## 2. Clonar y validar el workspace

~~~sh
git clone https://github.com/programadorkevinsvega-sys/RustFlow.git
cd RustFlow

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Estas comprobaciones verifican la revisión del repositorio que tienes clonada. No prueban por sí solas el despliegue, la estabilidad de APIs ni la preparación para producción.

## 3. Ejecutar el servidor HTTP

Desde la raíz del repositorio:

~~~sh
cargo run -p rustflow-server --example basic
~~~

El ejemplo escucha en <code>127.0.0.1:3000</code>. Incluye una página HTML, una ruta de artículo con componentes SSR, un layout compartido, endpoints JSON, middleware y apagado por Ctrl+C. Prueba <code>http://127.0.0.1:3000/</code>, <code>/articles/demo</code>, <code>/api/health</code> y <code>/users/42?detailed=true</code>.

Para ejecutar las pruebas de integración HTTP/TCP:

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

Para ejecutar el tutorial de producto, detén el primer servidor y ejecuta `cargo run -p rustflow-server --example product_page`; abre `/products/keyboard` o `/products/mouse`. Después continúa con [Servidor HTTP](/guides/server/), [SSR](/guides/ssr/) y [Layouts](/guides/layouts/).

## 4. Ejecutar la interfaz interactiva WASM

~~~sh
cd examples/browser
npm install
npm run build
npm run dev
~~~

Abre la URL mostrada por Vite. Para los tests de navegador:

~~~sh
npx playwright install --with-deps chromium
npm run test:e2e
~~~

Esta demo muestra Counter, estado reactivo, eventos, una lista de tareas con claves, montaje/desmontaje condicional y navegación optativa. Las fixtures E2E también prueban hidratación en HTML preparado para ese contrato; no quiere decir que el servidor de demo sea una aplicación SSR integrada.

## 5. Conceptos que conviene aprender

1. [Reactividad](/concepts/reactivity/): qué es una señal, cómo se registran dependencias y cuándo se ejecuta un efecto.
2. [Crear componentes](/guides/components/): props, <code>view!</code>, eventos y actualización de UI.
3. [Servidor HTTP](/guides/server/): rutas, handlers, parámetros y respuestas.
4. [SSR y documentos HTML](/guides/ssr/): escape, catálogo de componentes e hidratación optativa.
5. [Layouts](/guides/layouts/): composición de páginas.
6. [Página de producto](/tutorials/product-page/): tutorial integrado que conecta las piezas.

## ¿Puedo crear un proyecto con cargo add o rustflow new?

No hay una versión estable publicada ni un CLI propio verificado para crear proyectos, ejecutar <code>rustflow dev</code> o generar componentes. No asumas que esos comandos existen. Por ahora, prueba los ejemplos dentro del workspace y consulta [Estado del proyecto](/project-status/) antes de adoptar una capacidad.
