---
title: Probar y depurar aplicaciones
description: Ejecuta la verificación de Rust, pruebas HTTP y browser E2E, y localiza fallos sin asumir que CI despliega la app.
---

La forma más fiable de aprender el contrato actual de RustFlow es ejecutar los ejemplos y las pruebas en la misma revisión del repositorio que piensas utilizar. Las APIs siguen siendo pre-1.0, y pasar las pruebas no significa que una aplicación tenga seguridad, disponibilidad o compatibilidad de producción garantizadas.

## 1. Verificar el workspace Rust

Desde la raíz del repositorio ejecuta:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Estas tareas comprueban formato, advertencias de Clippy, tests del workspace y la construcción de documentación Rust. Si falla una, corrige el primer error y repite la orden pertinente; no ocultes advertencias con allow globales solo para obtener una CI verde.

## 2. Probar el servidor HTTP

Levanta el ejemplo ejecutable:

~~~sh
cargo run -p rustflow-server --example basic
~~~

En otra terminal, prueba rutas de diferentes clases:

~~~sh
curl -i http://127.0.0.1:3000/
curl -i http://127.0.0.1:3000/articles/demo
curl -i http://127.0.0.1:3000/api/health
curl -i 'http://127.0.0.1:3000/users/42?detailed=true'
curl -i -X POST http://127.0.0.1:3000/api/echo \
  -H 'content-type: application/json' \
  -d '{"message":"hola"}'
~~~

El ejemplo demuestra documento HTML, componente SSR, catálogo y layout compartidos, JSON, parámetros de ruta/query y middleware. No escribe en una base de datos. Para la suite de integración de transporte y configuración HTTP:

~~~sh
cargo test -p rustflow-server --test http_integration
~~~

La suite prueba rutas por TCP local, datos malformados, grupos, middleware, request IDs, páginas de error, límites, timeouts, TLS local, proxies confiables, abortos de cliente y concurrencia acotada. Los casos de la Fase 6.3 incluyen formularios tipados, validación JSON/query, errores estructurados 422, content-types no admitidos, campos inesperados, CSRF ausente/inválido/válido y redirecciones de retorno seguras. Consulta también la [guía de formularios y validación](/guides/forms-validation/).

## 3. Compilar y probar el navegador WebAssembly

Desde la raíz:

~~~sh
cd examples/browser
npm install
npm run build
npm run check:bundle
npm run dev
~~~

Para ejecutar E2E instala Chromium y corre Playwright:

~~~sh
npx playwright install --with-deps chromium
npm run test:e2e
~~~

La demo cubre el contador, eventos, lista de tareas, identidad de nodos keyed, montaje/desmontaje y navegación cliente optativa. Las fixtures E2E prueban también hidratación sobre HTML preparado para ese contrato. Esto no convierte la demo completa en una aplicación servida por SSR.

Si falla la compilación de WASM, verifica el target y las herramientas antes de investigar el código:

~~~sh
rustup target add wasm32-unknown-unknown
cargo check -p rustflow-dom --target wasm32-unknown-unknown
~~~

## 4. Diagnóstico rápido

| Síntoma | Comprobación inicial |
|---|---|
| Un snippet no compila | Compara la firma con la revisión de RustFlow; el website explica la API, pero no todos los fragmentos son programas independientes. |
| No se actualiza una vista | Comprueba que la lectura se rastrea como señal y que las expresiones arbitrarias están dentro de una región dinámica compatible. |
| Falla un componente | Revisa props requeridas/tipos, registro del componente y compatibilidad del target de render. |
| Una respuesta HTML falla | Separa fragmento SSR de documento completo y revisa tags/atributos rechazados por la allowlist. |
| No intercepta un enlace | Confirma origen, target, modificadores, fragmento, download y exclusiones descritas en [Navegación](/guides/navigation/). |
| Un formulario sigue enviándose de forma normal | Es el comportamiento esperado si no lleva opt-in o usa un método/codificación/control no compatible. |
| Falla E2E antes de abrir el navegador | Instala la versión de Chromium que requiere Playwright y revisa el output del runner/trace. |
| Un ejemplo no es persistente | El ejemplo mantiene sus datos en memoria; no se debe inferir almacenamiento integrado. |

## 5. ¿Qué significa una CI verde?

Una CI verde significa que los jobs definidos para esa revisión pasaron. No comprueba automáticamente los dominios, variables, certificados, secretos, políticas de acceso, persistencia o escalado de tu despliegue. Por separado, la publicación del website depende de la acción de documentación y del proveedor de Pages.

Antes de presentar una nueva guía como ejemplo canónico, verifica que el comando existe, que la API está implementada, que el ejemplo relevante compila o tiene una prueba que lo cubre y que se explican los límites conocidos. Sigue las [normas de documentación](/contributing/documentation/).

## Referencias

- [Empezar con RustFlow](/getting-started/setup/)
- [Catálogo de ejemplos](/reference/examples/)
- [README del ejemplo browser](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md)
- [Workflow de CI](https://github.com/programadorkevinsvega-sys/RustFlow/actions)
