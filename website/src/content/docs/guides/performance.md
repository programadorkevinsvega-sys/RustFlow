---
title: Medir el rendimiento
description: Ejecuta y entiende las mediciones de reactividad, SSR, DOM, bundle e inicialización de RustFlow.
---

RustFlow incluye benchmarks para observar el coste de su runtime reactivo, el renderizado SSR y las actualizaciones del DOM en el navegador. Estas mediciones sirven para comparar revisiones y detectar posibles regresiones; **no son garantías universales de latencia ni sustituyen pruebas funcionales**.

## Qué mide cada suite

| Suite | Qué ejecuta | Qué no representa |
|---|---|---|
| Reactividad | Señales, efectos, dependencias dinámicas, memos, batching, fan-out de hasta 1.024 suscriptores y limpieza de owners | Actualizaciones del DOM o tiempos del navegador |
| SSR | Árboles simples, dinámicos, amplios, profundos y listas, mediante la API pública render_to_string | Una solicitud HTTP completa o concurrencia; el tiempo incluye la creación y eliminación del Runtime por render |
| Browser / DOM | WASM real en Chromium, inserción incremental de 100 filas, montaje inicial de listas keyed de 100 y 1.000 filas, reordenamiento y eliminación | Una garantía para todos los navegadores |
| Bundle | WASM y assets de producción emitidos por Vite | Tamaño estimado a partir del código fuente |
| Inicialización | Desde el inicio de init() WASM hasta que termina el montaje inicial | Tiempo hasta que una aplicación real obtiene datos de red |
| Memoria | stats_alloc en benchmark nativo para alloc/dealloc/realloc y saldo de bytes solicitados de owners de componentes | No es RSS ni memoria física; no mide el allocator WASM live |
| Componentes anidados | Árbol recursivo de componentes reales en Chromium, profundidades 5, 20 y 80 | No sustituye pruebas en Firefox/WebKit |

## Ejecutar las mediciones nativas

Desde la raíz del repositorio:

~~~bash
RUSTFLOW_BENCH_ITERATIONS=10000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow --bench reactivity
RUSTFLOW_BENCH_ITERATIONS=1000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow-ssr --bench rendering
RUSTFLOW_MEMORY_SAMPLES=25 cargo bench -p rustflow --bench memory
~~~

Cada benchmark informa mínimo, mediana, p90, p95, throughput y parámetros de muestra en líneas JSONL. p99 solo se calcula con al menos 100 muestras. El escenario de fan-out configura el límite de ejecuciones por descarga del runtime aislado al número de suscriptores, porque 1.024 ejecuciones legítimas superan el límite predeterminado de 1.000. Este ajuste solo existe en el benchmark; no cambia la protección predeterminada de las aplicaciones. Los árboles y señales del escenario se preparan fuera de la operación medida cuando el propósito es medir una operación aislada.

## Medir el navegador y el bundle

Desde examples/browser/:

~~~bash
npm install --no-package-lock --no-audit --no-fund
npm run build
npm run check:bundle
npx playwright install chromium
npm run bench:browser
~~~

La suite de Playwright inicia la aplicación compilada con Vite Preview y opera sobre el DOM real mediante los eventos del demo RustFlow. Mide cinco muestras de inserción incremental de 100 filas y el montaje inicial de listas keyed de 100 y 1.000 filas. Para esos montajes iniciales utiliza el parámetro de benchmark `?bench_list=N`, que precarga los datos en el runtime antes del primer render; así no confunde el coste de montar una lista grande con el coste de miles de altas consecutivas. La lista inicial de 10.000 filas es un escenario de estrés opt-in:

~~~bash
RUSTFLOW_BENCH_BROWSER_LARGE_LIST=1 npm run bench:browser
~~~

El informe de bundle separa JavaScript, CSS, WASM y otros assets de dist/assets, con tamaños raw, gzip y Brotli. Aplica presupuestos de bytes iniciales para WASM (raw/gzip/Brotli), JavaScript, CSS y assets comprimidos totales; superar cualquiera falla el job de bundle.

La suite también carga `?bench_depth=5`, `?bench_depth=20` y `?bench_depth=80`. Cada escenario monta una cadena de componentes RustFlow recursivos registrados, valida el número exacto de instancias y mide la inicialización/montaje en Chromium.

## Interpretar los resultados

Compara el mismo escenario con igual número de operaciones, muestras, toolchain, sistema operativo, runner y build de producción. Revisa la mediana junto con p90/p95 y la variabilidad: una diferencia pequeña puede ser ruido de planificación, temperatura, carga del runner o compilación.

La inserción incremental de 100 filas mide también la reconciliación que ocurre después de cada alta. Las mediciones de montaje inicial de 100 y 1.000 filas (y 10.000 en modo opt-in) son escenarios distintos: incluyen la inicialización WASM y el primer montaje reactivo del DOM, no una secuencia de altas individuales.

El benchmark SSR invoca render_to_string en cada iteración. Esa API crea y elimina el Runtime en cada llamada, de modo que la medición incluye el ciclo de vida del runtime y la generación del HTML; no es el coste aislado del serializador.

CI conserva los informes como artefactos de cada ejecución. Descarga los artefactos de dos commits para comparar sus archivos JSON/JSONL. Los resultados de runners distintos no constituyen una comparación controlada.

La referencia preoptimización [continuation-candidate-2026-11-03.json](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/continuation-candidate-2026-11-03.json) registró 631,9 ms de mediana para 100 inserciones keyed incrementales (p95 662,2 ms). La inspección detectó búsquedas de claves y comprobaciones de identidad DOM anidadas, además de reinserciones innecesarias. La continuación propone resolver claves con un mapa, retención de nodos mediante identidad DOM y colocación estable desde el final. Aún no se considera una mejora aceptada: se debe comparar el mismo escenario después del cambio y volver a pasar E2E de identidad de filas/estado.

## Presupuestos y límites actuales

El build aplica límites deterministas de bytes para WASM raw/gzip/Brotli, JavaScript, CSS y assets comprimidos totales. Los límites actuales son guardrails iniciales con margen por encima del artefacto medido: WASM 600/220/160 KiB (raw/gzip/Brotli), JavaScript 24/7/6 KiB, CSS 32/8/8 KiB y total gzip/Brotli 220/170 KiB. El límite histórico de 1,5 MiB para WASM raw queda documentado, pero ya no es el umbral activo. Los umbrales de tiempo para browser, reactividad y SSR son advertencias, no fallos, porque deben revisarse con varias ejecuciones comparables y ruido del runner.

El benchmark `memory` usa stats_alloc en un ejecutable separado. Su primera referencia mide 25 ciclos para owner trees de 1, 5, 20 y 80 componentes, con 3 señales String por owner. El saldo de bytes solicitados después de `dispose` fue 0 en los cuatro tamaños. CI aplica límites de asignaciones y bytes con al menos 25% de margen y exige que el outstanding after-dispose mínimo y máximo sean cero por muestra. No se confunde esta métrica con RSS ni memoria física. RSS se toma después de precalentar la compilación. performance.memory, cuando Chromium la expone, observa el heap JS y no representa la memoria total de Rust/WASM. Consulta [docs/PERFORMANCE.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/PERFORMANCE.md) y el [baseline de rendimiento](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/baseline-2026-11-03.json).

El job nativo compara las medianas de tres ejecuciones de reactividad y SSR contra el baseline versionado con tolerancia de +25%. Los avisos se escriben en `native-budget-review.json` dentro del artefacto CI. El job de memoria valida allocations, bytes solicitados y outstanding después de `dispose` mediante `memory-budget-review.json`; los límites actuales vienen de 25 ciclos iniciales. El baseline es candidato, no una garantía universal; debe revisarse tras acumular varias ejecuciones equivalentes. Los límites de bundle y de allocator son gates deterministas.

## Qué no debes concluir

- Un benchmark nativo no mide el navegador.
- Un benchmark SSR aislado no mide HTTP, base de datos ni concurrencia.
- El heap JS no mide toda la memoria WASM.
- Una sola ejecución no demuestra escalabilidad ni ausencia de regresiones.
- Un saldo de bytes allocator netos no equivale al tamaño RSS o a memoria WASM residente.
- Los resultados de benchmark no son garantías de producción.

La primera comparación candidata registrada en [optimized-candidate-2026-11-03.json](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/optimized-candidate-2026-11-03.json) muestra que las 100 inserciones bajaron de 631,9 a 278,0 ms de mediana (−56,0%) y el reverse keyed de 100 filas de 13,8 a 5,5 ms (−60,1%); los 20 E2E browser pasaron en esa ejecución. Es un resultado inicial, no todavía un presupuesto aceptado: se debe repetir con el hardware/versions del runner capturados y comparar la variabilidad antes de cerrar la fase.
