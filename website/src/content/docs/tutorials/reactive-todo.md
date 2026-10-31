---
title: "Tutorial: interfaz reactiva de tareas"
description: Aprende el runtime DOM de RustFlow con estado, eventos, listas con claves y ciclo de vida en WebAssembly.
---

Este tutorial te guía por la demo ejecutable del navegador. El código completo está en [`examples/browser/src/app.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/src/app.rs), y las herramientas necesarias y los escenarios E2E están en su [README](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md).

**Relación con el CLI:** `rustflow new` crea una aplicación de servidor nativa, no un proyecto browser/WASM. Esta demo conserva su flujo independiente de wasm-pack, Vite y Playwright.

**Alcance:** las tareas existen en la memoria de la pestaña durante la ejecución de la demo. Este ejemplo no las guarda en un servidor ni en una base de datos y no implementa usuarios ni permisos.

## 1. Ejecutar la demo

Desde la raíz:

~~~sh
cd examples/browser
npm install
npm run build
npm run dev
~~~

Abre la URL que imprime Vite. El proyecto usa Rust compilado a WebAssembly; `npm run build` prepara el bundle para que Vite lo sirva.

## 2. Explorar los comportamientos

La demo combina piezas pequeñas para mostrar el contrato del runtime:

| Interacción | Qué observar |
|---|---|
| Pulsar el contador | Un evento actualiza una señal y la región de texto dependiente se vuelve a renderizar. |
| Mostrar/ocultar ayuda | La región condicional monta o desmonta contenido. |
| Añadir una tarea | El estado de la lista cambia y aparece una fila con una clave estable. |
| Completar o eliminar una tarea | El evento actualiza o filtra la colección reactiva. |
| Reordenar la lista | La reconciliación keyed conserva la identidad de los elementos compatibles. |
| Cambiar la identidad de una fila | La instancia antigua se desmonta y se ejecuta la limpieza asociada a su scope. |
| Usar enlaces de la demo | La navegación cliente optativa aplica sus reglas de mismo origen y fallback. |

No confundas la lista interactiva del browser demo con una aplicación conectada al servidor HTTP: los dos ejemplos son programas separados.

## 3. Cómo se organiza el código

En `app.rs` se pueden identificar cuatro piezas:

- `Counter`: un componente pequeño que recibe props y mantiene un contador reactivo.
- `TodoRow`: la vista de una tarea; enlaza callbacks de completar/eliminar y registra cleanup ligado al ciclo de vida.
- `todo_component`: crea un descriptor de componente con sus props para cada tarea.
- `start`: crea el runtime, declara señales para la colección/entrada/ruta, registra las definiciones y monta el árbol de vistas.

La sintaxis `#[rustflow::component]` y `view!` está limitada a la gramática documentada. La guía [Crear componentes](/guides/components/) describe props, eventos y regiones dinámicas; [Estado y reactividad](/concepts/reactivity/) explica cómo se calculan las dependencias y cómo se limpian los scopes.

## 4. ¿Por qué importan las claves?

Una clave estable representa la identidad de una tarea, no su posición actual. Cuando la lista cambia de orden, la reconciliación puede reutilizar una instancia compatible en lugar de destruir y crear todas las filas. Si la clave cambia deliberadamente, la demo permite observar que se crea una instancia diferente y que se limpia la anterior.

Usa IDs estables para registros, filas y tarjetas que cambien de orden. No uses índices de array como identidad de una fila si las operaciones de inserción, eliminación o reordenamiento pueden cambiar su posición.

## 5. Ejecutar las pruebas de navegador

Desde `examples/browser`:

~~~sh
npx playwright install --with-deps chromium
npm run test:e2e
~~~

Las pruebas E2E comprueban el contador, propagación de eventos, operaciones de la lista, identidad keyed, montaje/desmontaje, historial, navegación directa, formularios GET con opt-in y fallback nativo cuando una combinación no es compatible. También existen fixtures separadas para probar la hidratación de HTML preparado para ese protocolo.

Estas pruebas son una fuente de comportamiento verificable, pero no demuestran compatibilidad con todos los navegadores ni una integración completa entre SSR y el runtime de servidor.

## 6. Extendiendo el ejemplo sin asumir funciones futuras

Un buen ejercicio es añadir un campo visual a cada tarea (por ejemplo, una etiqueta), pasarla como prop y conservar la clave existente. Otro es añadir un filtro local a la lista usando una señal y una región dinámica. Ejecuta los tests después de cambiar la estructura DOM, los eventos o las claves.

Este ejemplo mantiene la lista en memoria y no conecta persistencia, sesiones ni guards. RustFlow ofrece APIs experimentales separadas para ORM SQL (SeaORM), ODM MongoDB, sesiones y control de acceso; todavía no hay proveedor de usuarios, almacenamiento SQL durable de sesiones ni server actions, y el scaffold no genera automáticamente una aplicación CRUD completa. Para patrones de ruta y datos HTTP, sigue el [tutorial de API JSON](/tutorials/json-api/); para SSR y catálogo de páginas, sigue el [tutorial de producto](/tutorials/product-page/); para persistencia, consulta la [guía de base de datos](/guides/database/).

## Referencias

- [Fuente browser WASM](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/examples/browser)
- [Servidor HTTP y JSON](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs)
- [Catálogo de ejemplos](/reference/examples/)
