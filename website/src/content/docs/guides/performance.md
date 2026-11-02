---
title: Medir el rendimiento
description: Ejecuta y entiende las mediciones de reactividad, SSR, DOM, bundle e inicialización de RustFlow.
---

RustFlow incluye benchmarks para observar el coste de su runtime reactivo, el renderizado SSR y las actualizaciones del DOM en el navegador. Estas mediciones sirven para comparar revisiones y detectar posibles regresiones; **no son garantías universales de latencia ni sustituyen pruebas funcionales**.

## Qué mide cada suite

| Suite | Qué ejecuta | Qué no representa |
|---|---|---|
| Reactividad | Señales, efectos, dependencias dinámicas, memos, batching, fan-out y limpieza de owners | Actualizaciones del DOM o tiempos del navegador |
| SSR | Árboles simples, dinámicos, amplios, profundos y listas, mediante la API pública render_to_string | Una solicitud HTTP completa o concurrencia; el tiempo incluye la creación y eliminación del Runtime por render |
| Browser / DOM | WASM real en Chromium, actualizaciones del contador, listas keyed, reordenamiento y eliminación | Una garantía para todos los navegadores |
| Bundle | WASM y assets de producción emitidos por Vite | Tamaño estimado a partir del código fuente |
| Inicialización | Desde el inicio de init() WASM hasta que termina el montaje inicial | Tiempo hasta que una aplicación real obtiene datos de red |

## Ejecutar las mediciones nativas

Desde la raíz del repositorio:

~~~bash
RUSTFLOW_BENCH_ITERATIONS=10000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow --bench reactivity
RUSTFLOW_BENCH_ITERATIONS=1000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow-ssr --bench rendering
~~~

Cada benchmark informa mínimo, mediana, p90, p95, throughput y parámetros de muestra en líneas JSONL. p99 solo se calcula con al menos 100 muestras. Los árboles y señales del escenario se preparan fuera de la operación medida cuando el propósito es medir una operación aislada.

## Medir el navegador y el bundle

Desde examples/browser/:

~~~bash
npm install --no-package-lock --no-audit --no-fund
npm run build
npm run check:bundle
npx playwright install chromium
npm run bench:browser
~~~

La suite de Playwright inicia la aplicación compilada con Vite Preview y opera sobre el DOM real mediante los eventos del demo RustFlow. Incluye listas de 100 y 1.000 filas; la lista de 10.000 filas es un escenario de estrés opt-in:

~~~bash
RUSTFLOW_BENCH_BROWSER_LARGE_LIST=1 npm run bench:browser
~~~

El informe de bundle separa JavaScript, CSS, WASM y otros assets de dist/assets, con tamaños raw, gzip y Brotli. También conserva el tamaño del WASM de entrada usado por la comprobación histórica.

## Interpretar los resultados

Compara el mismo escenario con igual número de operaciones, muestras, toolchain, sistema operativo, runner y build de producción. Revisa la mediana junto con p90/p95 y la variabilidad: una diferencia pequeña puede ser ruido de planificación, temperatura, carga del runner o compilación.

La inserción de listas del navegador es incremental y mide también la reconciliación que ocurre después de cada alta. No la interpretes como el coste de construir toda la lista en una única operación.

El benchmark SSR invoca render_to_string en cada iteración. Esa API crea y elimina el Runtime en cada llamada, de modo que la medición incluye el ciclo de vida del runtime y la generación del HTML; no es el coste aislado del serializador.

CI conserva los informes como artefactos de cada ejecución. Descarga los artefactos de dos commits para comparar sus archivos JSON/JSONL. Los resultados de runners distintos no constituyen una comparación controlada.

## Presupuestos y límites actuales

El único límite numérico que se sigue aplicando es el presupuesto heredado de **1,5 MiB para el WASM raw de entrada**. Los tamaños de JS/CSS, la latencia DOM, SSR, reactividad e inicialización se registran de forma informativa. No hay nuevos límites de milisegundos porque deben derivarse primero de varias mediciones comparables y de una tolerancia aprobada.

La medición RSS de /usr/bin/time -v corresponde al proceso completo del benchmark, no a memoria retenida por una señal ni a bytes asignados por operación. performance.memory, cuando Chromium la expone, observa el heap JS y no representa la memoria total de Rust/WASM. La documentación técnica explica estas diferencias y las limitaciones actuales en [docs/PERFORMANCE.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/PERFORMANCE.md).

## Qué no debes concluir

- Un benchmark nativo no mide el navegador.
- Un benchmark SSR aislado no mide HTTP, base de datos ni concurrencia.
- El heap JS no mide toda la memoria WASM.
- Una sola ejecución no demuestra escalabilidad ni ausencia de regresiones.
- Los resultados de benchmark no son garantías de producción.

La optimización sigue siendo una decisión posterior a perfilar un caso reproducible y comparar antes/después. La implementación de mediciones no autoriza optimizaciones especulativas.
