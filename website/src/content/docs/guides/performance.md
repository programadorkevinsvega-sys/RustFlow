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
| Browser / DOM | WASM real en Chromium, inserción incremental de 100 filas, altas/bajas individuales sobre listas preexistentes de 100, 1.000 y 5.000 filas, montajes keyed, reordenamiento y eliminación | Una garantía para todos los navegadores |
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

La suite de Playwright inicia la aplicación compilada con Vite Preview y opera sobre el DOM real mediante los eventos del demo RustFlow. Mide cinco muestras de inserción incremental de 100 filas y el montaje inicial de listas keyed de 100 y 1.000 filas. Para esos montajes iniciales utiliza el parámetro de benchmark `?bench_list=N`, que precarga los datos en el runtime antes del primer render; así no confunde el coste de montar una lista grande con el coste de miles de altas consecutivas. La lista inicial de 10.000 filas se conserva como escenario de estrés informativo de CI: se ejecuta una vez sobre la versión optimizada, con un calentamiento y siete muestras; no se repite con el reconciliador base dentro del A/B/A. El mismo comando permite ejecutarlo manualmente:

~~~bash
RUSTFLOW_BENCH_BROWSER_LARGE_LIST=1 RUSTFLOW_BENCH_BROWSER_KEYED_OPERATIONS=1 npm run bench:browser
~~~

El informe de bundle separa JavaScript, CSS, WASM y otros assets de dist/assets, con tamaños raw, gzip y Brotli. Aplica presupuestos de bytes iniciales para WASM (raw/gzip/Brotli), JavaScript, CSS y assets comprimidos totales; superar cualquiera falla el job de bundle.

La suite también carga `?bench_depth=5`, `?bench_depth=20` y `?bench_depth=80`. Cada escenario monta una cadena de componentes RustFlow recursivos registrados, valida el número exacto de instancias y mide la inicialización/montaje en Chromium. Profundidades 20 y 80 usan tres calentamientos descartados y 25 muestras; profundidad 5 conserva un calentamiento y siete muestras. El montaje inicial de 100 filas ahora usa tres calentamientos y 25 muestras; el límite A/A sigue siendo 15% y no se reduce.

## Operaciones keyed individuales

Con `RUSTFLOW_BENCH_BROWSER_KEYED_OPERATIONS=1`, Playwright precarga listas con baseline de 100, 1.000 o 5.000 filas y mide 20 pares complementarios por escala: una inserción N→N+1 y, a continuación, una eliminación N+1→N. Cada par devuelve el fixture al tamaño estable sin clicks adicionales de restauración. La eliminación empieza con una fila más que el baseline; el JSON declara el número de filas al inicio y al final de cada operación para mantener esa diferencia explícita. El cronómetro rodea únicamente el click de Add o el botón de eliminación localizado previamente. La preparación del input, el despacho de eventos, los selectores y la verificación quedan fuera. Las eliminaciones cubren posiciones inicial, media y final. Se conservan 20 muestras de cada operación y se informan mediana, p90 y p95.

Los contadores de atributos, movimientos DOM, creación de nodos y listeners se recogen en una pasada aparte para no contaminar las latencias. No se envuelven globalmente `Reflect.get/set`: ese hook añade overhead a cada acceso JavaScript y no corresponde a una mutación DOM concreta. Los contadores restantes tampoco equivalen a contadores internos de renders Rust ni a mediciones de clones de señales.

## Interpretar los resultados

Compara el mismo escenario con igual número de operaciones, muestras, toolchain, sistema operativo, runner y build de producción. Revisa la mediana junto con p90/p95 y la variabilidad: una diferencia pequeña puede ser ruido de planificación, temperatura, carga del runner o compilación.

La inserción incremental de 100 filas mide también la reconciliación que ocurre después de cada alta. Las mediciones de montaje inicial de 100 y 1.000 filas (y 10.000 en modo opt-in) son escenarios distintos: incluyen la inicialización WASM y el primer montaje reactivo del DOM, no una secuencia de altas individuales.

El benchmark SSR invoca render_to_string en cada iteración. Esa API crea y elimina el Runtime en cada llamada, de modo que la medición incluye el ciclo de vida del runtime y la generación del HTML; no es el coste aislado del serializador.

CI conserva los informes como artefactos de cada ejecución. Descarga los artefactos de dos commits para comparar sus archivos JSON/JSONL. Los resultados de runners distintos no constituyen una comparación controlada.

La referencia preoptimización [continuation-candidate-2026-11-03.json](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/continuation-candidate-2026-11-03.json) registró 631,9 ms de mediana para 100 inserciones keyed incrementales (p95 662,2 ms). La optimización posterior ya fue comprobada en el mismo runner mediante A/B/A en el CI [#893](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/41589448890): la inserción incremental de 100 filas mejoró 45,30% (311,90 → 170,60 ms) y reverse keyed 54,67% (7,50 → 3,40 ms), con deriva A/A inferior al 0,24%. En mutaciones aisladas, las mejoras de mediana estuvieron entre 53,52% y 95,83% para inserciones y entre 54,79% y 95,71% para eliminaciones sobre listas existentes de 100, 1.000 y 5.000 filas. El [registro histórico versionado](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/runs/2026-11-03-ci-893-phase-8-1.md) incluye p50/p95, conteos DOM, memoria, SSR, reactividad y bundle. El montaje inicial de 10.000 filas permanece informativo (644,20 ms de mediana, siete muestras optimizadas) y no forma parte del A/B/A. Estos resultados aceptan la mejora interna de RustFlow, pero todavía no comparan React, Vue y Svelte.

## Presupuestos y límites actuales

El build aplica límites deterministas de bytes para WASM raw/gzip/Brotli, JavaScript, CSS y assets comprimidos totales. Los límites actuales son guardrails iniciales con margen por encima del artefacto medido: WASM 600/220/160 KiB (raw/gzip/Brotli), JavaScript 24/7/6 KiB, CSS 32/8/8 KiB y total gzip/Brotli 220/170 KiB. El límite histórico de 1,5 MiB para WASM raw queda documentado, pero ya no es el umbral activo. Los umbrales de tiempo para browser, reactividad y SSR son advertencias, no fallos, porque deben revisarse con varias ejecuciones comparables y ruido del runner.

El benchmark `memory` usa stats_alloc en un ejecutable separado. La referencia inicial mide 25 ciclos de árboles de `Scope` con 3 señales String por owner y el saldo después de `dispose` fue 0. La continuación también mide `ComponentInstance` reales (profundidades 1, 5, 20 y 80), con setup, render, señales y `unmount`; cada muestra debe liberar todo el outstanding solicitado. Los techos de allocaciones/bytes de estos escenarios reales ya se derivaron del artefacto nativo de 25 muestras: para profundidades 1/5/20/80 se aplican máximos de 32/120/440/1.720 allocaciones, 3.072/13.312/54.272/217.856 bytes asignados y 3.072/10.496/40.192/158.976 bytes outstanding tras mount. El gate exige además outstanding min/max/promedio = 0 después de cada `unmount`. Estos valores se vuelven a verificar en CI sobre el HEAD del PR antes de la aceptación final. El saldo outstanding se calcula como `bytes_allocated - bytes_deallocated`; `stats_alloc` ya registra el cambio de `realloc` dentro de esos contadores, por lo que `bytes_reallocated` se mantiene como dato diagnóstico y no se suma dos veces. No se confunde esta métrica con RSS ni memoria física. RSS se toma después de precalentar la compilación. performance.memory, cuando Chromium la expone, observa el heap JS y no representa la memoria total de Rust/WASM. Consulta [docs/PERFORMANCE.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/PERFORMANCE.md) y el [baseline de rendimiento](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/baseline-2026-11-03.json).

El job nativo compara las medianas de tres ejecuciones de reactividad y SSR contra el baseline versionado con tolerancia de +25%. Los avisos se escriben en `native-budget-review.json` dentro del artefacto CI. El job de memoria valida allocations, bytes solicitados y outstanding después de `dispose` mediante `memory-budget-review.json`; los límites actuales vienen de 25 ciclos iniciales. El baseline es candidato, no una garantía universal; debe revisarse tras acumular varias ejecuciones equivalentes. Los límites de bundle y de allocator son gates deterministas.

## Qué no debes concluir

- Un benchmark nativo no mide el navegador.
- Un benchmark SSR aislado no mide HTTP, base de datos ni concurrencia.
- El heap JS no mide toda la memoria WASM.
- Una sola ejecución no demuestra escalabilidad ni ausencia de regresiones.
- Un saldo de bytes allocator netos no equivale al tamaño RSS o a memoria WASM residente.
- Los resultados de benchmark no son garantías de producción.

La comparación inicial entre jobs no se acepta como evidencia porque el control y el candidato se ejecutaron en CPU distintas (AMD EPYC 7763 y AMD EPYC 9V45). CI está configurado para realizar un A/B/A en el mismo runner: versión optimizada, versión base de `browser.rs`, y versión optimizada otra vez. La comparación solo se considera aceptada cuando existe el artefacto final y el comparador confirma todos los umbrales. El informe A/B/A incluye medianas y p95 de montajes de 100 y 1.000 filas y componentes anidados a profundidades 5, 20 y 80. Los escenarios de montaje de 100/1.000 filas y profundidad 5 usan un calentamiento no contado y siete muestras por variante; profundidades 20/80 usan tres calentamientos descartados y 25 muestras medidas por variante. El montaje de 10.000 filas permanece en el artefacto informativo optimizado y queda fuera de la comparación controlada. Todos exigen deriva A/A máxima del 15% y se rechaza una regresión mediana superior al 25%; las muestras inestables impiden aceptar la comparación. La comparación exige al menos 20% de mejora mediana tanto en las 100 inserciones como en el reverse keyed de 100 filas, y deriva máxima de 15% entre las dos muestras optimizadas que rodean al control; el reporte se conserva como `browser-ab-comparison.json`.


## Comparación con React, Vue y Svelte

La referencia upstream está fijada al commit `fa510a76ffe066af1d30f3e75fa51d3fcbc14009` de `krausest/js-framework-benchmark`. Las implementaciones keyed seleccionadas fijan React 19.2.0, Vue 3.5.39 y Svelte 5.42.1 en sus lockfiles originales. La auditoría detectó diferencias entre el README y las operaciones que ejecuta realmente el runner oficial: por eso RustFlow separará la reproducción compatible con el runner de los escenarios extendidos de 10.000 filas. Consulta el [contrato de referencia y las discrepancias](https://github.com/programadorkevinsvega-sys/RustFlow/blob/feat/phase-8-1-measurement-completion/docs/performance/FRAMEWORK_BENCHMARK_REFERENCE.md).

Esta auditoría **no es una comparación de rendimiento**. La suite ejecutable, sus artefactos en el mismo runner y los resultados por operación siguen pendientes. No se afirma superioridad competitiva ni se importan tiempos del leaderboard público como mediciones de RustFlow.

## Suite extendida de listas

Además de la suite estándar, RustFlow dispone de un estrés específico de listas de 100, 1.000, 5.000 y 10.000 filas. Incluye altas/bajas aisladas, cambios de fila, actualizaciones de cada décima fila, reordenamientos, append de 1.000, reemplazo total, eliminación de secciones y cleanup de todos los componentes. El protocolo comprueba identidad de nodos y el recuento de owners desmontados.

Se puede ejecutar desde GitHub Actions mediante `workflow_dispatch` marcando `extended_browser_benchmarks`; también se ejecuta semanalmente. Los PR habituales no lanzan este conjunto pesado. No es el benchmark oficial comparativo y sus resultados no se mezclan con el índice de React/Vue/Svelte.


## Comparación oficial keyed

El job del PR de Phase 8.1, además de las ejecuciones manuales/semanales, compara las implementaciones keyed del benchmark oficial fijado y la aplicación RustFlow WASM bajo el mismo runner y Chromium. Usa la traza oficial para distinguir trabajo total, JavaScript y paint; conserva muestras crudas por operación, p50/p90/p95 y el índice de media geométrica. Los artefactos se publican como `phase-8-1-browser-performance`. La comparación no se ejecuta en cada PR ordinario. Consulta el contrato técnico de [comparación oficial](../../../../../docs/performance/FRAMEWORK_BENCHMARK_REFERENCE.md) y el [historial de medición](../../../../../docs/PERFORMANCE.md).

No se publicará una victoria si falta alguna de las nueve operaciones, un framework no llega a producir muestras o el navegador/runner no coincide con los requisitos.


El informe comparativo también resume memoria (p50/p90/p95), tamaño sin comprimir, Brotli y first paint por implementación. Estas métricas se mantienen separadas del índice geométrico de latencia CPU.

### Operaciones comparativas de 10.000 filas

El mismo runner de trazas ejecuta además, para React, Vue, Svelte y RustFlow, actualización cada décima fila sobre 10.000, append de 1.000 a 10.000 y clear de 10.000. Los resultados forman un índice extendido de tres operaciones y otro combinado de doce operaciones (nueve upstream más tres extensiones). También se activan los escenarios de memoria tras cinco updates y cinco reemplazos.

### Arranque y primera interacción

El informe `startup-comparison.json` utiliza el mismo runner y Chromium, con tres calentamientos descartados y 15 navegaciones medidas por framework en orden rotatorio. Incluye response start/end, DOMContentLoaded, load event, first paint/FCP, recursos transferidos y el tiempo hasta la primera acción real que crea 1.000 filas. La marca de inicialización interna WASM se publica para RustFlow cuando está disponible; no se equipara con internals no expuestos de los otros frameworks. El desglose de parseo/compilación sigue pendiente de tracing dedicado.
