# RustFlow performance measurements

Estado: Phase 8.1 sigue en progreso. `main` contiene la infraestructura inicial y esta continuación amplía la medición de memoria, profundidad de componentes y presupuestos. El baseline versionado es candidato hasta revisar ejecuciones comparables y los artefactos instrumentados. Los números de un runner son observaciones de ese entorno, no garantías universales.

## Alcance medido

| Área | Escenarios | Naturaleza |
|---|---|---|
| Reactividad | Runtime, creación/lectura/escritura/actualización de señales, efectos, dependencias dinámicas, memos, batching, fan-out de 1 a 1.024 suscriptores, 1.000 señales y limpieza de owners | Microbenchmarks Rust; el escenario fan-out configura su límite de ejecuciones por descarga al número de suscriptores |
| SSR | Árbol simple, contenido dinámico, árboles anidados, árboles amplios y listas de 100, 1.000 y 10.000 elementos | API pública render_to_string; incluye Runtime::new, scope raíz, render y dispose por llamada |
| DOM | Actualizaciones del contador, inserciones incrementales de 100 filas, montaje inicial de listas keyed de 100, 1.000 y 10.000 filas (3 muestras en CI para 10k), reordenamiento y eliminación | RustFlow WASM ejecutado en Chromium mediante Playwright |
| Bundle | WASM de entrada y assets de producción emitidos por Vite; JS, CSS, WASM y otros assets | Artefactos reales de vite build |
| Inicialización | Marca desde el inicio de init() WASM hasta que termina la inicialización y el montaje inicial | Navegador Chromium |
| Memoria nativa | Conteos de alloc/dealloc/realloc y bytes solicitados; delta neto mientras viven y después de destruir árboles de scopes que representan owners de componentes con 3 señales por owner | Benchmark separado con stats_alloc sobre System; no altera el allocator de producción |
| Memoria de proceso/navegador | RSS máximo de los procesos de benchmark tras precompilar; observación opcional del heap JS de Chromium | El RSS no identifica objetos RustFlow; performance.memory no mide el heap WASM |
| Componentes anidados | Montaje de árboles reales de componentes recursivos en browser a profundidad 5, 20 y 80 | Chromium; la marca incluye inicialización WASM y el montaje inicial del árbol aislado |

El escenario de fan-out usa un Runtime aislado con `Runtime::with_cycle_limit(fanout)`: el scheduler debe ejecutar una vez cada suscriptor en una misma descarga y el límite predeterminado de 1.000 rechazaría el caso de 1.024 aunque no exista un ciclo. Este ajuste pertenece solo al microbenchmark; `Runtime::new()` y el límite de seguridad de producción no cambian.

Los benchmarks de reactividad y SSR son mediciones nativas del runtime/modelo de vistas; no representan tiempos del navegador. El benchmark DOM usa el WASM real y elementos DOM reales. Cada render SSR usa la API pública render_to_string, que crea y elimina un Runtime por llamada: por tanto, el tiempo incluye el ciclo de vida del runtime y la generación HTML, no es una medición pura del serializador. El benchmark SSR no es una prueba HTTP completa ni una medición de concurrencia.

## Ejecutar

Desde la raíz del repositorio:

~~~bash
RUSTFLOW_BENCH_ITERATIONS=10000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow --bench reactivity
RUSTFLOW_BENCH_ITERATIONS=1000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow-ssr --bench rendering
RUSTFLOW_MEMORY_SAMPLES=25 cargo bench -p rustflow --bench memory

# Precompila antes de medir RSS para no incluir consumo de compilación:
cargo bench -p rustflow --bench reactivity > /dev/null
/usr/bin/time -v -o reactivity-rss.txt cargo bench -p rustflow --bench reactivity
/usr/bin/time -v -o ssr-memory.txt cargo bench -p rustflow-ssr --bench rendering
~~~

Los benchmarks imprimen líneas JSONL. Cada medición separa preparación de escenario y operación medida, excepto los nombres que explícitamente describen create_and_drop, create_and_dispose o lifecycle. RUSTFLOW_BENCH_ITERATIONS controla operaciones por muestra; RUSTFLOW_BENCH_SAMPLES controla el número de muestras. Los percentiles p50, p90 y p95 se calculan sobre el tiempo medio por operación de cada muestra. p99 se emite solo con al menos 100 muestras.

El benchmark de memoria usa `stats_alloc 0.1.10` como wrapper del allocator del sistema, declarado únicamente en el ejecutable `benches/memory.rs`. No agrega código inseguro al framework ni cambia el allocator de una aplicación. Reporta operaciones de alloc/dealloc/realloc, bytes solicitados/liberados y el saldo solicitado tras crear el árbol y después de ejecutar la limpieza del owner. El saldo es una métrica de solicitudes del allocator: no equivale a páginas RSS, fragmentación interna ni memoria física residente. Los escenarios crean 1, 5, 20 y 80 owners anidados, cada uno con tres señales de estado; destruir el owner superior debe liberar sus descendientes y señales.

El paso de CI precompila los benchmarks de tiempo antes de envolver las ejecuciones con `/usr/bin/time -v`. Esto evita contabilizar la compilación inicial como parte del RSS del benchmark. Las mediciones instrumentadas de memoria se ejecutan en un proceso aparte y no se usan para inferir tiempos de latencia.

Para el navegador, desde examples/browser/:

~~~bash
npm install --no-package-lock --no-audit --no-fund
npm run build
npm run check:bundle
npx playwright install chromium
npm run bench:browser
~~~

El suite usa el preview de Vite configurado en playwright.config.js, espera el montaje real del demo y despacha eventos a través de los listeners del runtime. La inserción incremental mide 100 altas reales y la reconciliación posterior a cada una. Para medir listas grandes sin convertir el tiempo de preparación en cientos o miles de altas repetidas, el demo acepta el parámetro de benchmark `?bench_list=N`: el runtime crea la lista inicial y Playwright mide la inicialización WASM junto con el montaje reactivo del DOM para 100 y 1.000 filas. El parámetro está limitado a 10.000 elementos. CI también ejecuta el montaje inicial de 10.000 filas en tres muestras por variante como escenario de estrés; se activa con `RUSTFLOW_BENCH_BROWSER_LARGE_LIST=1` (el mismo comando sirve para ejecutarlo manualmente):

~~~bash
RUSTFLOW_BENCH_BROWSER_LARGE_LIST=1 npm run bench:browser
~~~

Para medir profundidad real de componentes en Chromium, el fixture admite `?bench_depth=5`, `?bench_depth=20` y `?bench_depth=80`. La ruta de benchmark contiene únicamente una cadena recursiva de componentes RustFlow registrados, cuenta las instancias montadas y mide el montaje inicial de cada profundidad; no construye solo elementos HTML anidados.

## Artefactos y comparación

El workflow CI guarda artefactos de medición para cada ejecución:

- performance-results/native/: metadatos, JSONL de reactividad y SSR, y reportes /usr/bin/time -v.
- examples/browser/performance-results/: bundle-size.json y browser.json.

Los metadatos incluyen commit, sistema operativo, arquitectura, toolchain/Node cuando corresponde y parámetros. Para comparar revisiones, descarga los artefactos de ambas ejecuciones y compara el mismo escenario, toolchain, runner, número de muestras y configuración. No mezcles tiempos de diferentes runners o familias de hardware como si fueran una comparación controlada.

Los artefactos CI son evidencia de una ejecución y tienen la retención configurada en el workflow; no son por sí mismos un baseline histórico permanente dentro del repositorio. Una referencia oficial de larga duración debe aprobarse y guardarse junto con su commit, hardware, sistema operativo, toolchain y comandos.

## Memoria: qué significa cada dato

La medición distingue cuatro magnitudes que no deben mezclarse:

- **Allocaciones por ciclo de vida:** conteos y bytes solicitados/liberados por el allocator de sistema mientras se crea y destruye el árbol de owners.
- **Bytes solicitados netos tras setup:** bytes de alloc y realloc que siguen outstanding según los contadores después de montar el árbol. Es una aproximación a memoria solicitada por las estructuras vivas del escenario, no al RSS ni a bytes físicos.
- **Bytes solicitados netos tras dispose:** el mismo saldo después de disponer el owner raíz y liberar las referencias del benchmark. Un saldo distinto de cero debe investigarse; no se debe convertir automáticamente en un diagnóstico de fuga porque también existen referencias y estado de proceso fuera del escenario.
- **RSS máximo:** máximo del proceso completo en Linux. Se recoge después de precompilar para excluir la memoria del compilador, pero sigue incluyendo harness, runtime y allocator. Es un dato auxiliar, no una métrica por componente.
- **Heap JS de Chromium:** observación de `performance.memory` cuando está disponible; la resolución puede ser gruesa. No incluye toda la memoria lineal de WASM.

El benchmark de memoria usa un allocator instrumentado exclusivamente en el ejecutable nativo `benches/memory.rs`. Está separado de los microbenchmarks de tiempo porque el wrapper introduce overhead y no debe contaminar las latencias. No se incorpora al runtime ni a los binarios de las aplicaciones.

La memoria live del allocator WASM aún no se mide por componente: el tamaño de `WebAssembly.Memory.buffer`, cuando esté disponible, representa capacidad lineal reservada/crecida y no bytes vivos internos del allocator. No se debe presentar como memoria retenida por un componente.

## Bundle e inicialización

npm run check:bundle lee pkg/rustflow_browser_bg.wasm y la salida de producción dist/assets creada por Vite. Reporta bytes raw, gzip y Brotli por tipo y total. Los tamaños gzip/Brotli son sumas de compresión independiente por archivo; no simulan una conexión HTTP con diccionario compartido.

El checker aplica presupuestos deterministas de bytes para WASM raw/gzip/Brotli (600/220/160 KiB), JavaScript (24/7/6 KiB), CSS (32/8/8 KiB) y total de assets gzip/Brotli (220/170 KiB). Los límites fallan CI cuando se superan. El histórico 1,5 MiB para WASM raw queda documentado, pero ya no es el umbral activo. Los tiempos de inicialización/DOM/SSR/reactividad siguen siendo warning-only porque dependen de la carga y el hardware del runner.

## Baselines y presupuestos

No se incorporan números ficticios ni se elevan resultados de un runner de GitHub a garantías de rendimiento del framework. La primera ejecución de CI de esta rama produce un baseline candidato con commit exacto y configuración. Para aprobar un presupuesto nuevo se deben comparar varias ejecuciones del mismo runner, revisar dispersión y percentiles, decidir una tolerancia y comprobar que la señal supera el ruido normal.

Estado del baseline y del control de regresiones:

| Área | Evidencia actual | Guardrail | Política CI |
|---|---|---|---|
| Reactividad | 3 ejecuciones nativas por job, JSONL por escenario | Mediana candidata +25% | Advertencia; nunca falla por timing del runner |
| SSR | 3 ejecuciones nativas por job, JSONL y bytes HTML | Mediana candidata +25% | Advertencia; nunca falla por timing del runner |
| DOM / profundidad | Chromium, 5 muestras para la mayoría de escenarios y 3 por profundidad | Percentiles p95 por escenario | Advertencia; nunca falla por timing del runner |
| Memoria nativa | 25 muestras por árbol de owner; el saldo outstanding tras dispose fue 0 bytes en las 4 profundidades | Presupuestos de asignación/outstanding setup con margen inicial ≥25%; after-dispose min/max = 0 | Falla si se supera un techo o hay retención tras dispose |
| JS / CSS / WASM | Tamaños raw/gzip/Brotli del build de producción | Límites de bytes definidos abajo | Falla si se supera |
| RSS y heap JS | RSS de proceso precompilado; heap JS opcional | Informativo, no sustituyen allocator/retención | Sin gate propio |

### Perfil de listas keyed y optimización candidata

El artefacto preoptimización [continuation-candidate-2026-11-03.json](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/continuation-candidate-2026-11-03.json) conserva una ejecución del benchmark browser sin las optimizaciones de esta continuación. En Chromium registró:

| Escenario | Mediana | p95 / observación |
|---|---:|---:|
| Inserción reactiva incremental de 100 filas | 631,9 ms | 662,2 ms |
| Reordenar 100 filas keyed | 13,8 ms | 16,7 ms |
| Eliminar 100 filas | 636,9 ms | Una sola muestra; no usar como presupuesto |
| Montaje inicial 1.000 filas | 97,8 ms | Dos muestras |

El perfil de código encontró tres costes repetidos en la ruta de reconciliación: la planificación buscaba cada clave recorriendo toda la lista anterior; la eliminación de nodos obsoletos comparaba cada nodo previo con todos los siguientes; y la colocación ejecutaba `insert_before` para todos los nodos, incluidos los que ya estaban en su posición. La continuación introduce una tabla hash para resolver claves, un `Set` de identidad de objetos DOM para retención/eliminación y colocación de derecha a izquierda que evita mover hermanos ya colocados.

Esta es una optimización candidata basada en el perfil, no una mejora ya aceptada. Debe demostrar resultados antes/después con el mismo fixture, conteo de operaciones, Chromium y runner equivalente, además de mantener las pruebas E2E que comprueban identidad de nodo, estado local, limpieza y reordenamiento. No se modifica la semántica de las claves ni la identidad de componentes. Los cambios solo se aceptarán después de revisar el artefacto posterior.

### Resultado inicial después de la optimización

El primer artefacto posterior, [optimized-candidate-2026-11-03.json](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/optimized-candidate-2026-11-03.json), proviene de una ejecución de CI donde los 20 E2E de browser pasaron. La comparación inicial con el baseline preoptimización mostró diferencias grandes, pero una segunda prueba control reveló que los jobs se ejecutaron en modelos de CPU distintos (AMD EPYC 9V45 frente a AMD EPYC 7763). Esos porcentajes se conservan como observaciones históricas, pero **no se aceptan como comparación de rendimiento controlada ni como evidencia de cierre**.

La ejecución control mostró por qué no debe usarse esta comparación entre jobs: ambos usaron Ubuntu 24.04 y Chromium 156.0.8078.4, pero el modelo de CPU difirió (AMD EPYC 9V45 para el optimizado y AMD EPYC 7763 para el control). El job de browser ahora ejecuta un A/B/A dentro del mismo runner: mide optimizado, reconstruye el archivo DOM desde el SHA base de la PR, mide el control y vuelve a medir optimizado. De esa forma CPU, Node, Chromium, Rust y condiciones del proceso permanecen iguales. El resultado controlado se guarda como `browser-ab-comparison.json` y exige una mejora mínima de 20% en la mediana de inserción keyed y reverse; los tiempos absolutos siguen siendo warning-only.


### Presupuestos iniciales y política de regresión

El archivo [baseline-2026-11-03.json](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/performance/baseline-2026-11-03.json) conserva la primera referencia candidata con commit, runner, toolchain, Chromium y muestras reales. Se usa una tolerancia inicial de aproximadamente 25% para permitir variación en runners alojados; los umbrales de tiempo generan advertencias, no fallos, hasta acumular ejecuciones equivalentes. Los límites de bytes del bundle son deterministas y sí fallan CI.

| Área | Presupuesto inicial | Comportamiento CI |
|---|---:|---|
| WASM raw | 600 KiB | Falla si se supera; reemplaza al límite más laxo como guardrail activo, y mantiene documentado el histórico de 1,5 MiB |
| WASM gzip / Brotli | 220 KiB / 160 KiB | Falla si se supera |
| JavaScript raw / gzip / Brotli | 24 KiB / 7 KiB / 6 KiB | Falla si se supera |
| CSS raw / gzip / Brotli | 32 KiB / 8 KiB / 8 KiB | Falla si se supera; la referencia inicial no emite CSS |
| Total assets gzip / Brotli | 220 KiB / 170 KiB | Falla si se supera |
| Reactividad | Baselines representativas +25% | Advertencia informativa en la revisión de resultados |
| SSR | Baselines representativas +25% | Advertencia informativa en la revisión de resultados |
| Browser init, DOM y listas | Baseline por escenario +25–40% según dispersión observada | Advertencia informativa, nunca gate de timing sensible al host |
| Owners / memoria | 25 ciclos por owner tree a profundidades 1, 5, 20 y 80; outstanding después de dispose = 0 en todas las muestras | Presupuestos de allocaciones/bytes con ~25% de margen; outstanding after-dispose min/max = 0 | Gate determinista; falla por retención o por superar cualquier techo |

Los techos de bundle dejan margen aproximado de 20–30% sobre el primer artefacto medido. No son objetivos de tamaño recomendado; son límites de regresión para esta aplicación de demostración. Los presupuestos absolutos de tiempo se revisan con mediana y p90/p95 y permanecen warning-only debido al ruido del runner. La memoria nativa tiene límites deterministas por escenario y exige saldo outstanding cero después de dispose; la reconciliación keyed tiene un gate relativo de mejora mínima de 20% en un A/B/A dentro del mismo runner.

Las mediciones de rendimiento no sustituyen pruebas funcionales. Los benchmarks no provocan fallos de CI por variaciones de latencia.

## Limitaciones conocidas

1. El DOM se mide en Chromium; todavía no es una matriz multi-navegador ni cubre Firefox/WebKit.
2. La profundidad de componentes se mide en Chromium para 5, 20 y 80 niveles; aún no hay comparación multi-navegador.
3. No hay benchmark concurrente de SSR ni benchmark de solicitudes HTTP completas.
4. La memoria por owner se mide en un benchmark nativo separado; la memoria live interna del allocator WASM todavía no se mide por componente.
5. Los artefactos CI requieren descarga para comparación; no se mantiene todavía una serie histórica comprometida en Git.
6. La optimización keyed está pendiente de la primera salida del A/B/A controlado en el mismo runner, que debe demostrar al menos 20% de mejora mediana en inserción y reverse; las comparaciones entre jobs con CPU diferentes se rechazan.

El tiempo de Runtime::new también se mide como microoperación en la suite de reactividad, pero no se resta del tiempo SSR: no se hace una descomposición artificial del coste público por render.

Los puntos pendientes deben permanecer abiertos en ROADMAP.md; crear infraestructura de benchmark no basta para declarar completa Phase 8.1.
