# Phase 8.1 — benchmark comparativo oficial #22 (diagnóstico incompleto)

**Fecha de ejecución:** 2026-11-05  
**Revisión del artefacto:** 2026-11-06  
**Workflow:** [Official framework benchmarks #22](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/41960093893)  
**PR:** [#81](https://github.com/programadorkevinsvega-sys/RustFlow/pull/81)  
**SHA evaluado:** 657eaa6c68d7abecd017568eceabec72749e1eeb  
**SHA upstream:** fa510a76ffe066af1d30f3e75fa51d3fcbc14009  
**Artefacto:** [official-framework-comparison-41960093893](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/41960093893/artifacts/13391764453)  
**Estado:** status=incomplete, valid=false, accepted=false.

Este registro conserva los resultados parciales para diagnóstico. No es una comparación oficial aceptada ni permite cerrar Phase 8.1. Los tiempos pertenecen a un único runner de GitHub Actions.

## Entorno

| Campo | Valor |
|---|---|
| SO / kernel | Linux / 7.0.0-1017-azure |
| CPU | AMD EPYC 9V74, 4 vCPU |
| Memoria disponible reportada | 15 GiB |
| Node / npm | v22.23.4 / 10.9.9 |
| Rust | 1.99.1 (58cf8f9c3 2026-10-13) |
| Chromium | 156.0.8078.4 |
| React | 19.2.0 |
| Vue | 3.5.39 |
| Svelte | 5.42.1 |
| Runner upstream | krausest/js-framework-benchmark en el SHA fijado arriba |

La compilación WASM y la comprobación de Chromium pasaron. El runner completó CPU, memoria y tamaño para React, Vue, Svelte y RustFlow. El paso startup no pudo finalizar y el agregador rechazó el conjunto completo, como exige el contrato fail-closed.

## CPU — mediana total por operación

Todas las cifras están en milisegundos. Se conservan todas las muestras: 15 por escenario CPU, excepto 04_select1k, donde upstream genera 25.

| Operación | React p50 | Vue p50 | Svelte p50 | RustFlow p50 | RF / React | RF / Vue | RF / Svelte |
|---|---:|---:|---:|---:|---:|---:|---:|
| 01_run1k | 66,1 | 64,7 | 58,3 | 86,3 | 1,31× | 1,33× | 1,48× |
| 02_replace1k | 76,7 | 72,3 | 69,2 | 114,4 | 1,49× | 1,58× | 1,65× |
| 03_update10th1k_x16 | 41,8 | 39,8 | 37,9 | 57,7 | 1,38× | 1,45× | 1,52× |
| 04_select1k | 12,1 | 9,2 | 11,3 | 42,1 | 3,48× | 4,58× | 3,73× |
| 05_swap1k | 275,3 | 45,3 | 48,3 | 307,7 | 1,12× | 6,79× | 6,37× |
| 06_remove-one-1k | 31,0 | 33,0 | 29,2 | 57,5 | 1,85× | 1,74× | 1,97× |
| 07_create10k | 858,2 | 714,7 | 665,1 | 1.187,5 | 1,38× | 1,66× | 1,79× |
| 08_create1k-after1k_x2 | 75,7 | 71,2 | 66,2 | 113,6 | 1,50× | 1,60× | 1,72× |
| 09_clear1k_x8 | 47,8 | 35,0 | 27,5 | 90,8 | 1,90× | 2,59× | 3,30× |
| 10_update10th10k | 500,5 | 438,9 | 419,1 | 604,0 | 1,21× | 1,38× | 1,44× |
| 11_append1k-after10k | 175,8 | 169,2 | 161,8 | 347,2 | 1,98× | 2,05× | 2,15× |
| 12_clear10k | 530,2 | 323,7 | 257,2 | 1.657,4 | 3,13× | 5,12× | 6,44× |

Razón geométrica diagnóstica de p50 para las doce operaciones (RustFlow / framework):

| Comparación | Razón geométrica |
|---|---:|
| React | 1,697× |
| Vue | 2,246× |
| Svelte | 2,392× |

Un valor mayor que 1 significa que RustFlow fue más lento. Estas razones son solo para orientar el profiling; no son el índice de una ejecución aceptada porque el reporte integral quedó incompleto.

### Percentiles p50/p90/p95 de RustFlow

| Operación | Muestras | p50 ms | p90 ms | p95 ms |
|---|---:|---:|---:|---:|
| 01_run1k | 15 | 86,3 | 87,3 | 87,4 |
| 02_replace1k | 15 | 114,4 | 117,0 | 120,6 |
| 03_update10th1k_x16 | 15 | 57,7 | 61,9 | 62,0 |
| 04_select1k | 25 | 42,1 | 43,5 | 43,8 |
| 05_swap1k | 15 | 307,7 | 317,5 | 326,0 |
| 06_remove-one-1k | 15 | 57,5 | 59,6 | 59,9 |
| 07_create10k | 15 | 1.187,5 | 1.201,1 | 1.212,7 |
| 08_create1k-after1k_x2 | 15 | 113,6 | 116,4 | 116,6 |
| 09_clear1k_x8 | 15 | 90,8 | 93,2 | 93,8 |
| 10_update10th10k | 15 | 604,0 | 630,2 | 645,6 |
| 11_append1k-after10k | 15 | 347,2 | 362,1 | 368,4 |
| 12_clear10k | 15 | 1.657,4 | 1.703,2 | 1.749,6 |

Las señales de profiling más importantes son selección, swap, append sobre 10.000 filas y clear de 10.000 filas. No se deben ocultar ni compensar con medias globales.

## Memoria — p50 por escenario (MiB)

| Escenario | React | Vue | Svelte | RustFlow |
|---|---:|---:|---:|---:|
| 21_ready-memory | 1,561 | 1,312 | 1,088 | 1,174 |
| 22_run-memory | 4,930 | 4,356 | 3,360 | 3,192 |
| 23_update5-memory | 5,405 | 4,461 | 3,396 | 3,227 |
| 24_run5-memory | 5,511 | 4,521 | 3,497 | 3,491 |
| 25_run-clear-memory | 2,473 | 1,660 | 1,491 | 1,632 |

## Tamaño y first paint

| Métrica | React | Vue | Svelte | RustFlow |
|---|---:|---:|---:|---:|
| Bundle sin comprimir (KiB) | 190,3 | 64,3 | 26,6 | 568,5 |
| Bundle comprimido (KiB) | 51,4 | 23,3 | 9,7 | 152,7 |
| First paint (ms, una muestra) | 439,0 | 201,6 | 130,1 | 1.264,4 |

El tamaño y first paint no se mezclan con el índice CPU. First paint contiene una única muestra por implementación.

## Causas de rechazo y correcciones requeridas

1. **Cantidad esperada para 04_select1k.** El runner upstream fijado declara additionalNumberOfRuns: 10 para esta operación. Con --count 15, su ejecutor calcula 15 + 10 = 25 mediciones. El artefacto contiene exactamente 25 para total/script/paint en los cuatro frameworks. El agregador anterior exigía 15 y rechazó esas 12 series. La corrección acepta 25 únicamente para este ID y conserva todas las mediciones; los otros once IDs CPU siguen exigiendo 15.
2. **Ruta de Vue en startup.** La ruta se construía como /frameworks/keyed/vue/index.html, pero el catálogo upstream /ls declara customURL: "/dist" para Vue. La página está bajo /frameworks/keyed/vue/dist/index.html. El script esperó #run durante 30 segundos y terminó por timeout. La corrección debe resolver los metadatos reales de los cuatro directorios y respetar customURL, sin hardcodear una excepción de Vue.
3. **Regresión protegida.** Las pruebas comprueban los recuentos por ID, rechazan 24 muestras en 04_select1k, conservan las 25 muestras, resuelven rutas estándar y customURL, y fallan cerrado cuando falta o es ambiguo el metadata upstream.

La siguiente ejecución solo será aceptada si el artefacto final informa simultáneamente status: "valid", valid: true y accepted: true, con los cuatro frameworks y todas las operaciones/muestras requeridas. La ejecución #22 permanece diagnóstica y no se reescribe como válida.
