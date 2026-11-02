# RustFlow performance measurements

Estado: infraestructura de medición inicial de Phase 8.1. Los números producidos por un runner son observaciones de ese entorno, no garantías universales.

## Alcance medido

| Área | Escenarios | Naturaleza |
|---|---|---|
| Reactividad | Runtime, creación/lectura/escritura/actualización de señales, efectos, dependencias dinámicas, memos, batching, fan-out de 1 a 1.024 suscriptores, 1.000 señales y limpieza de owners | Microbenchmarks Rust |
| SSR | Árbol simple, contenido dinámico, árboles anidados, árboles amplios y listas de 100, 1.000 y 10.000 elementos | API pública render_to_string; incluye Runtime::new, scope raíz, render y dispose por llamada |
| DOM | Actualizaciones del contador, inserciones incrementales de 100 filas, montaje inicial de listas keyed de 100 y 1.000 filas, reordenamiento y eliminación | RustFlow WASM ejecutado en Chromium mediante Playwright |
| Bundle | WASM de entrada y assets de producción emitidos por Vite; JS, CSS, WASM y otros assets | Artefactos reales de vite build |
| Inicialización | Marca desde el inicio de init() WASM hasta que termina la inicialización y el montaje inicial | Navegador Chromium |
| Memoria | RSS máximo del proceso de benchmark en Linux, mediante /usr/bin/time -v; observación JS heap opcional de Chromium | Proceso completo / API específica del navegador |

Los benchmarks de reactividad y SSR son mediciones nativas del runtime/modelo de vistas; no representan tiempos del navegador. El benchmark DOM usa el WASM real y elementos DOM reales. Cada render SSR usa la API pública render_to_string, que crea y elimina un Runtime por llamada: por tanto, el tiempo incluye el ciclo de vida del runtime y la generación HTML, no es una medición pura del serializador. El benchmark SSR no es una prueba HTTP completa ni una medición de concurrencia.

## Ejecutar

Desde la raíz del repositorio:

~~~bash
RUSTFLOW_BENCH_ITERATIONS=10000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow --bench reactivity
RUSTFLOW_BENCH_ITERATIONS=1000 RUSTFLOW_BENCH_SAMPLES=25 cargo bench -p rustflow-ssr --bench rendering

/usr/bin/time -v -o reactivity-memory.txt cargo bench -p rustflow --bench reactivity
/usr/bin/time -v -o ssr-memory.txt cargo bench -p rustflow-ssr --bench rendering
~~~

Los benchmarks imprimen líneas JSONL. Cada medición separa preparación de escenario y operación medida, excepto los nombres que explícitamente describen create_and_drop, create_and_dispose o lifecycle. RUSTFLOW_BENCH_ITERATIONS controla operaciones por muestra; RUSTFLOW_BENCH_SAMPLES controla el número de muestras. Los percentiles p50, p90 y p95 se calculan sobre el tiempo medio por operación de cada muestra. p99 se emite solo con al menos 100 muestras.

Para el navegador, desde examples/browser/:

~~~bash
npm install --no-package-lock --no-audit --no-fund
npm run build
npm run check:bundle
npx playwright install chromium
npm run bench:browser
~~~

El suite usa el preview de Vite configurado en playwright.config.js, espera el montaje real del demo y despacha eventos a través de los listeners del runtime. La inserción incremental mide 100 altas reales y la reconciliación posterior a cada una. Para medir listas grandes sin convertir el tiempo de preparación en cientos o miles de altas repetidas, el demo acepta el parámetro de benchmark `?bench_list=N`: el runtime crea la lista inicial y Playwright mide la inicialización WASM junto con el montaje reactivo del DOM para 100 y 1.000 filas. El parámetro está limitado a 10.000 elementos. La lista de 10.000 elementos es un escenario opt-in de estrés de montaje inicial:

~~~bash
RUSTFLOW_BENCH_BROWSER_LARGE_LIST=1 npm run bench:browser
~~~

## Artefactos y comparación

El workflow CI guarda artefactos de medición para cada ejecución:

- performance-results/native/: metadatos, JSONL de reactividad y SSR, y reportes /usr/bin/time -v.
- examples/browser/performance-results/: bundle-size.json y browser.json.

Los metadatos incluyen commit, sistema operativo, arquitectura, toolchain/Node cuando corresponde y parámetros. Para comparar revisiones, descarga los artefactos de ambas ejecuciones y compara el mismo escenario, toolchain, runner, número de muestras y configuración. No mezcles tiempos de diferentes runners o familias de hardware como si fueran una comparación controlada.

Los artefactos CI son evidencia de una ejecución y tienen la retención configurada en el workflow; no son por sí mismos un baseline histórico permanente dentro del repositorio. Una referencia oficial de larga duración debe aprobarse y guardarse junto con su commit, hardware, sistema operativo, toolchain y comandos.

## Memoria: qué significa cada dato

- Maximum resident set size de GNU time -v es el máximo RSS de todo el proceso durante esa ejecución. Incluye runtime, harness, buffers, allocator y datos del proceso. No es memoria retenida por una señal, memoria por operación ni heap exclusivamente de RustFlow.
- performance.memory.usedJSHeapSize es una observación del heap JS de Chromium cuando el navegador la expone. No mide RSS del proceso del browser ni memoria Rust/WASM de forma completa.
- La infraestructura actual no mide bytes asignados por operación, conteo de asignaciones, memoria retenida después de GC ni memoria de WASM por componente. Esas métricas no deben inferirse de RSS o del heap JS.
- Los ciclos de crear/destruir owners y la eliminación de filas se incluyen como escenarios de comportamiento, pero no equivalen a una prueba de ausencia de fugas. Para certificar retención se necesita un profiler/allocator instrumentado y un protocolo estable.

## Bundle e inicialización

npm run check:bundle lee pkg/rustflow_browser_bg.wasm y la salida de producción dist/assets creada por Vite. Reporta bytes raw, gzip y Brotli por tipo y total. Los tamaños gzip/Brotli son sumas de compresión independiente por archivo; no simulan una conexión HTTP con diccionario compartido.

Se conserva el límite histórico existente de 1,5 MiB para el WASM raw de entrada. Los tamaños de JS, CSS, WASM emitido y el coste de inicialización se publican como mediciones informativas hasta tener suficientes ejecuciones comparables y una decisión aprobada. No se ha añadido un límite arbitrario de milisegundos ni un threshold sensible al hardware.

## Baselines y presupuestos

No se incorporan números ficticios ni se elevan resultados de un runner de GitHub a garantías de rendimiento del framework. La primera ejecución de CI de esta rama produce un baseline candidato con commit exacto y configuración. Para aprobar un presupuesto nuevo se deben comparar varias ejecuciones del mismo runner, revisar dispersión y percentiles, decidir una tolerancia y comprobar que la señal supera el ruido normal.

Estado inicial:

| Área | Baseline | Presupuesto aprobado | Límite CI |
|---|---|---|---|
| Reactividad | Se genera JSONL en CI | Pendiente de revisión de varias ejecuciones | Ninguno nuevo |
| SSR | Se genera JSONL y bytes HTML | Pendiente de revisión de varias ejecuciones | Ninguno nuevo |
| DOM / runtime | Se genera JSON mediante Chromium | Pendiente de revisión de varias ejecuciones | Ninguno nuevo |
| Memoria | RSS de proceso en Linux; heap JS opcional | No hay presupuesto por operación/retención | Ninguno nuevo |
| JavaScript / CSS | Tamaños Vite raw/gzip/Brotli | Pendiente de referencia estable | Informativo |
| WASM raw de entrada | Medición existente | 1,5 MiB, presupuesto heredado | Límite existente |

Las mediciones de rendimiento no sustituyen pruebas funcionales. Los benchmarks no provocan fallos de CI por variaciones de latencia.

## Limitaciones conocidas

1. El DOM se mide en Chromium; todavía no es una matriz multi-navegador ni cubre Firefox/WebKit.
2. El fixture browser actual ejercita Counter y TodoRow, listas keyed y actualizaciones síncronas; no ofrece todavía un barrido independiente y parametrizado de profundidad de componentes.
3. No hay benchmark concurrente de SSR ni benchmark de solicitudes HTTP completas.
4. Memoria asignada/retendida por operación no está disponible con una herramienta instrumentada bajo la política actual de unsafe_code = forbid; no se sustituye con estimaciones.
5. Los artefactos CI requieren descarga para comparación; no se mantiene todavía una serie histórica comprometida en Git.
6. No se ejecutan optimizaciones en esta infraestructura. Solo se justifican después de perfilar un cuello de botella y comparar el mismo caso antes/después.

El tiempo de Runtime::new también se mide como microoperación en la suite de reactividad, pero no se resta del tiempo SSR: no se hace una descomposición artificial del coste público por render.

Los puntos pendientes deben permanecer abiertos en ROADMAP.md; crear infraestructura de benchmark no basta para declarar completa Phase 8.1.
