---
title: Catálogo de ejemplos
description: Encuentra los ejemplos ejecutables del repositorio y el caso de uso que demuestra cada uno.
---

Esta página es un índice de los ejemplos que existen en el repositorio. Los comandos se deben ejecutar desde la raíz del checkout, salvo cuando se indique otra cosa. Como RustFlow es pre-1.0, ejecuta los ejemplos de la misma revisión que estás leyendo: las firmas y las APIs pueden cambiar.

## CLI y proyecto generado

Desde un checkout del repositorio, instala el binario y crea una aplicación de servidor:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow new hello-rustflow
cd hello-rustflow
rustflow dev
~~~

Desde la raíz de la nueva aplicación, puedes generar `component ProductCard`, `page about`, `controller health`, `model User` o `migration create_users` con `rustflow generate <tipo> <nombre>`. La implementación de plantillas está en [`crates/rustflow-cli/src/lib.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-cli/src/lib.rs). Modelos y migraciones son estructurales y no integran persistencia. Más detalles en la [guía del CLI](/guides/cli/).

## Ejemplos ejecutables

| Ejemplo | Comando | Casos de uso |
|---|---|---|
| Servidor HTTP básico | `cargo run -p rustflow-server --example basic` | Página HTML, componente SSR, catálogo compartido, layout, grupos HTTP, middleware y endpoints JSON. |
| Página de producto SSR | `cargo run -p rustflow-server --example product_page` | Datos en memoria, ruta con ID de producto, 404, props públicas, catálogo compartido y layout. |
| Interfaz browser/WASM | Ver [README del browser](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) | Counter, señales, callbacks, lista de tareas, claves, montaje/desmontaje, navegación optativa y fixtures E2E de hidratación. |

## 1. Servidor HTTP básico

El archivo canónico es [`crates/rustflow-server/examples/basic.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs). Arranca en localhost:3000 y permite probar:

- `/`: documento HTML generado desde una vista estructural.
- `/articles/demo`: componente SSR y layout de catálogo compartido.
- `/api/health`: respuesta JSON.
- `/users/42?detailed=true`: parámetro de ruta y query deserializada.
- `POST /api/echo`: cuerpo JSON y respuesta JSON.

Tutorial: [Construir una API JSON](/tutorials/json-api/). Guías: [Servidor HTTP](/guides/server/) y [Errores, límites y seguridad](/guides/errors-and-security/).

## 2. Página de producto SSR

El ejemplo [`product_page.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs) es un servidor separado, también en el puerto local 3000. Detén el servidor básico antes de iniciarlo. Las rutas `/products/keyboard` y `/products/mouse` enseñan un catálogo fijo y cómo se convierte un registro encontrado en props públicas.

Tutorial: [Página de producto SSR](/tutorials/product-page/). Guías: [SSR](/guides/ssr/) y [Layouts](/guides/layouts/).

## 3. Interfaz reactiva browser/WASM

El ejemplo de [`examples/browser`](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/examples/browser) se ejecuta con Vite y Rust compilado a WebAssembly. Consulta su README para requisitos de Rust, target WASM, Node/npm, build, presupuesto de bundle y Playwright E2E.

Tutorial: [Interfaz reactiva de tareas](/tutorials/reactive-todo/). Guías: [Estado y reactividad](/concepts/reactivity/), [Componentes](/guides/components/) y [Navegación](/guides/navigation/).

## Ejemplos SSR, layouts y rutas JSON

La composición SSR con catálogo compartido y layout está en [`crates/rustflow-server/examples/basic.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs) y [`product_page.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs). Para JSON sigue el [tutorial de API JSON](/tutorials/json-api/). Para UI reactiva de navegador usa [`examples/browser/src/app.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/src/app.rs), no el scaffold de servidor.

## Verificación compartida

Desde la raíz del repositorio puedes ejecutar `cargo test --workspace --all-features`. Para comprobar lint, formato y documentación Rust, consulta [Probar y depurar aplicaciones](/guides/testing/). Para disponibilidad y límites por API, usa [Referencia de disponibilidad de API](/reference/api-status/).

## Lo que los ejemplos no proporcionan

Los datos del catálogo de productos y de la lista de tareas no son persistentes. No hay una demo canónica que incluya una base de datos, autenticación, sesiones, autorización, CSRF, pagos o server actions. No extrapoles esos casos desde un ejemplo de SSR, una ruta JSON o una interfaz de navegador.
