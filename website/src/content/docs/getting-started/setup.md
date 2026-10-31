---
title: Empezar con RustFlow
description: Prepara Rust, prueba el runtime, ejecuta el servidor y compila la demo interactiva.
---

Esta guía te ayuda a comprobar el código real de RustFlow: ejecutar tests del workspace, probar el ejemplo HTTP y levantar el ejemplo WASM del navegador. El CLI experimental de la Fase 6.1 puede crear una aplicación de servidor y generar archivos; la [guía del CLI](/guides/cli/) y la [guía de configuración](/guides/configuration/) explican sus límites y el contrato de perfiles. RustFlow sigue siendo pre-1.0 y no es una plataforma lista para producción.

## 1. Requisitos

Para el workspace necesitas Git, Rust 1.85 o Rust stable y Cargo:

~~~sh
rustc --version
cargo --version
git --version
~~~

Para el ejemplo del navegador necesitas además Node.js 22/npm, `wasm-pack`, el target `wasm32-unknown-unknown` y Chromium si vas a ejecutar E2E. Los pasos están en [README del ejemplo de navegador](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md).

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

## 3. Crear una aplicación con el CLI

Desde la raíz del checkout instala el binario y crea una aplicación de servidor:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow new hello-rustflow
cd hello-rustflow
rustflow dev
~~~

El scaffold es un servidor nativo. Si quieres recorrer las APIs del repositorio sin crear una carpeta nueva, continúa con los ejemplos de esta guía. El [tutorial de primera aplicación](/tutorials/first-cli-app/) explica el flujo completo, los generadores y las colisiones.

## 4. Ejecutar el servidor HTTP

Desde la raíz del repositorio:

~~~sh
cargo run -p rustflow-server --example basic
~~~

El ejemplo escucha en `127.0.0.1:3000`. Incluye una página HTML, una ruta de artículo con componentes SSR, un layout compartido, endpoints JSON, middleware y apagado por Ctrl+C. Prueba `http://127.0.0.1:3000/`, `/articles/demo`, `/api/health` y `/users/42?detailed=true`.

Para ejecutar las pruebas de integración HTTP/TCP:

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

Para probar la API JSON con peticiones reales, sigue [Tutorial: una API JSON](/tutorials/json-api/). Para ejecutar el tutorial de producto, detén el primer servidor y ejecuta `cargo run -p rustflow-server --example product_page`; abre `/products/keyboard` o `/products/mouse`. Después continúa con [Servidor HTTP](/guides/server/), [SSR](/guides/ssr/) y [Layouts](/guides/layouts/).

## 5. Ejecutar la interfaz interactiva WASM

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

Esta demo muestra Counter, estado reactivo, eventos, una lista de tareas con claves, montaje/desmontaje condicional y navegación optativa. Las fixtures E2E también prueban hidratación en HTML preparado para ese contrato; no quiere decir que el servidor de demo sea una aplicación SSR integrada. El recorrido guiado está en [Tutorial: interfaz reactiva de tareas](/tutorials/reactive-todo/).

## 6. Conceptos que conviene aprender

1. [Reactividad](/concepts/reactivity/): qué es una señal, cómo se registran dependencias y cuándo se ejecuta un efecto.
2. [Crear componentes](/guides/components/): props, `view!`, eventos y actualización de UI.
3. [Servidor HTTP](/guides/server/): rutas, handlers, parámetros y respuestas.
4. [SSR y documentos HTML](/guides/ssr/): escape, catálogo de componentes e hidratación optativa.
5. [Layouts](/guides/layouts/): composición de páginas.
6. [Página de producto](/tutorials/product-page/): tutorial integrado que conecta las piezas.
7. [Catálogo de ejemplos](/reference/examples/): mapa de todos los ejecutables verificados del repositorio.

## Instalación y límites del CLI

Todavía no hay una versión estable publicada en crates.io. El método disponible es instalar desde el checkout con `cargo install --path crates/rustflow-cli --locked`. El scaffold genera un servidor nativo con configuración, pool SQLx y runner de migraciones; la plantilla `model` sigue creando un struct de dominio plano y no genera una entidad ORM ni integra un repositorio persistente. Consulta [CLI y generadores](/guides/cli/) y [Estado del proyecto](/project-status/).
