# Ejemplo de RustFlow en navegador

Este paquete construye una aplicación WebAssembly que se puede cargar en un navegador con el runtime DOM de RustFlow. Incluye:

- Un componente Counter con señal local y handler tipado.
- Una región de ayuda que se monta/desmonta condicionalmente.
- Una lista de tareas con alta, marcado, eliminación y reordenación mediante claves.
- Preservación de identidad DOM/estado para componentes compatibles.
- Un adaptador de navegación cliente optativo para las rutas de demostración.
- Fixtures E2E específicos que ejercitan la API de hidratación sobre HTML preparado por servidor; la fixture no debe confundirse con que el demo entero se sirva desde un backend SSR.

Este ejemplo demuestra capacidades de desarrollo y pruebas. No es una plantilla de aplicación lista para producción ni un tutorial completo por sí solo. Las guías públicas explican los conceptos paso a paso: [componentes](../../website/src/content/docs/guides/components.md), [reactividad](../../website/src/content/docs/concepts/reactivity.md), [SSR](../../website/src/content/docs/guides/ssr.md) y [navegación](../../website/src/content/docs/guides/navigation.md).

## Requisitos en Ubuntu

- Ubuntu 22.04/24.04 (o un entorno equivalente).
- Git y toolchain Rust 1.85 o posterior, con rustfmt y Clippy.
- Target <code>wasm32-unknown-unknown</code>.
- Node.js 22 con npm.
- Acceso de red para descargar dependencias de Cargo/npm y Chromium de Playwright.

Instala las herramientas nativas y el target WebAssembly:

~~~sh
sudo apt-get update
sudo apt-get install -y build-essential curl pkg-config libssl-dev
rustup toolchain install stable --component rustfmt clippy
rustup default stable
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked --version 0.13.1
~~~

## Compilar y ejecutar

Desde la raíz del repositorio:

~~~sh
cd examples/browser
npm install
npm run build
npm run check:bundle
npm run dev
~~~

Abre la URL que Vite imprime en el terminal. El punto de entrada Rust está marcado con <code>#[wasm_bindgen(start)]</code>; Vite carga el glue JavaScript y <code>rustflow_browser_bg.wasm</code>. <code>npm run build</code> produce los assets estáticos en <code>dist/</code> y source maps.

Para un artefacto WASM orientado a depuración, ejecuta <code>npm run build:wasm:debug</code> antes de <code>npm run dev</code>. Ese comando sustituye el glue/WASM generado en <code>pkg/</code>; ejecuta otra vez <code>npm run build</code> para regenerar el build release.

## Tests E2E

~~~sh
npx playwright install --with-deps chromium
npm run test:e2e
~~~

Playwright verifica el Counter, la propagación de eventos desde nodos anidados, el CRUD/reordenación de tareas con identidad keyed, el montaje/desmontaje condicional, el historial atrás/adelante, la navegación directa y recarga, los formularios GET con opt-in, el envío nativo cuando no se cumplen los criterios y los fallbacks por errores HTTP. La CI construye el bundle de producción, compila WASM, ejecuta E2E y aplica un presupuesto de 1.5 MiB al WASM raw.

## Diagnóstico

- **Falta <code>wasm32-unknown-unknown</code>:** ejecuta <code>rustup target add wasm32-unknown-unknown</code>.
- **Cargo no resuelve features de <code>web-sys</code>:** actualiza la revisión y ejecuta <code>cargo check -p rustflow-dom --target wasm32-unknown-unknown</code>.
- **Falta el artefacto del bundle:** ejecuta <code>npm run build</code> antes de <code>npm run check:bundle</code>.
- **Playwright no inicia Chromium:** repite <code>npx playwright install --with-deps chromium</code> e inspecciona el trace del test.
- **Falla una actualización reactiva:** revisa la consola; el <code>ReactiveMountedDom</code> permite inspeccionar el último error con <code>take_error()</code>.

El ejemplo del navegador no sustituye al ejemplo de servidor HTTP. Para documentos SSR, layouts y catálogo compartido, ejecuta también <code>cargo run -p rustflow-server --example basic</code> desde la raíz.
