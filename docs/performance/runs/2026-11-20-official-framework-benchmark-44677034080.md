# Comparación oficial de frameworks - 2026-11-20

**Estado:** válida y aceptada por el agregador; no declara superioridad competitiva.
**Workflow:** [Official framework benchmarks, run 44677034080](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/44677034080)
**RustFlow:** `5360963ae7bab3be2bc497eee5a2f62e3c6bc136`
**Upstream:** `fa510a76ffe066af1d30f3e75fa51d3fcbc14009`

El artifact informa `status: "valid"`, `valid: true`, `accepted: true`, cero errores de validación y `competitive_superiority_claimed: false`. Los cuatro frameworks completaron las muestras requeridas de CPU, memoria, tamaño y startup. La comparación usa el mismo job, runner y navegador. El reporte y los datos crudos están en el artifact del workflow.

## Resultados CPU

El índice es la media geométrica de `mediana(RustFlow) / mediana(framework)` por operación; valores mayores que 1 indican más tiempo mediano de RustFlow en el conjunto indicado. Los índices son específicos de este workload y no una medición de todo el framework.

| Conjunto CPU | React | Vue | Svelte |
|---|---:|---:|---:|
| 9 operaciones upstream | 1.6176 | 2.2548 | 2.3599 |
| 3 extensiones Phase 8.1 | 2.1303 | 2.8747 | 3.1031 |
| 12 operaciones combinadas | 1.7329 | 2.3960 | 2.5271 |

El resultado no respalda una afirmación de superioridad de RustFlow en estos conjuntos de latencia CPU. Memoria, tamaño y startup se reportan por separado y no forman parte del índice CPU.

## Startup y primera interacción

`startup-comparison.json` registra tres calentamientos y 15 navegaciones medidas por framework. Los valores siguientes son p50:

| Métrica | React | Vue | Svelte | RustFlow |
|---|---:|---:|---:|---:|
| Response start (ms) | 3.85 | 4.47 | 4.03 | 3.57 |
| DOMContentLoaded (ms) | 79.24 | 50.23 | 49.53 | 43.07 |
| Load event end (ms) | 86.45 | 63.17 | 58.66 | 45.68 |
| First paint / FCP (ms) | 94.14 | 66.92 | 60.08 | 76.65 |
| First interaction + 1.000 filas (ms) | 115.41 | 101.21 | 101.96 | 154.03 |
| Resource transfer (bytes) | 335883 | 206690 | 168193 | 726362 |

Startup es una medición separada de la CPU keyed. RustFlow tuvo menor `DOMContentLoaded` y `load event end` en esta ejecución, pero mayor first paint que Vue/Svelte, una primera interacción más lenta que los tres y más bytes transferidos. No se generaliza este resultado a otras aplicaciones o entornos.

## Entorno y alcance

- Sistema: Linux x86_64, runner GitHub Actions, CPU Intel Xeon 6975P-C.
- Node.js `v22.23.4`, npm `10.9.9`, Rust `1.100.0`.
- Google Chrome for Testing `156.0.8078.4`.
- Frameworks fijados por el workflow: React `19.2.0`, Vue `3.5.39`, Svelte `5.42.1` y el commit RustFlow indicado arriba.
- Se validó el conteo exacto de muestras: 15 por operación CPU salvo `04_select1k` con 25; cinco muestras por operación de memoria; una por operación de tamaño; 15 navegaciones medidas por framework para startup.
- El resultado cubre el benchmark keyed fijado y sus tres extensiones, no rendimiento HTTP, SSR de producción, concurrencia ni todas las aplicaciones.
- La validez del run no completa Phase 8.1. Continúan pendientes los criterios A/B/A de listas grandes contra la implementación base y la revisión del conjunto restante de requisitos de la fase.
