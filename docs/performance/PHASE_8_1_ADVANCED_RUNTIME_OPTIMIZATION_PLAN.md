# Phase 8.1 — plan de optimización avanzada del runtime

**Estado:** optimizaciones incrementales aplicadas en la rama; la nueva reactividad granular está pendiente de CI y de evidencia comparativa. No declara completada Phase 8.1.  
**Rama:** `feat/phase-8-1-advanced-runtime-optimizations`  
**Base:** `main`, después de integrar los PR #78 y #80.  
**Referencia upstream:** `krausest/js-framework-benchmark` en `fa510a76ffe066af1d30f3e75fa51d3fcbc14009`.

## 1. Punto de partida verificado

El trabajo anterior ya integró la instrumentación y la infraestructura que deben reutilizarse:

- Benchmarks nativos de reactividad y SSR.
- Medición de memoria para ComponentOwner y ComponentInstance, incluida la verificación de outstanding después de dispose/unmount.
- Benchmarks de Chromium/WASM para listas keyed, mutaciones aisladas y componentes anidados a profundidades 5/20/80.
- Presupuestos deterministas de bundle.
- Reconciliación keyed optimizada y comparación A/B/A interna.
- Referencia upstream fijada, implementaciones keyed de React, Vue y Svelte, fixture RustFlow y workflow de comparación en un mismo job.

El CI #944 aceptó para el reconciliador keyed interno una mejora mediana del 47,85 % en inserción incremental y del 55,15 % en reverse keyed, con deriva A/A del 0,13 % y 0,82 %. Las mutaciones aisladas en listas de 100/1.000/5.000 filas tienen 20 muestras en ambas pasadas optimizadas y demuestran estabilidad A/A, pero no una comparación contra el reconciliador base. Los ocho escenarios nativos de memoria revisados tuvieron 25 muestras y outstanding solicitado cero.

Estos resultados comparan RustFlow con su propia implementación base. **No demuestran superioridad frente a React, Vue o Svelte.**

## 2. Brechas que siguen abiertas

1. Ejecutar y revisar la comparación oficial completa de los nueve IDs upstream para React, Vue, Svelte y RustFlow, recogidos secuencialmente en el mismo runner, job y navegador.
2. Ejecutar y revisar por separado las tres extensiones Phase 8.1: actualización parcial sobre 10.000 filas, append de 1.000 filas sobre 10.000 y limpieza de 10.000 filas.
3. Completar evidencia comparable para listas grandes. El montaje de 10.000 y sus mutaciones actuales son observaciones de estrés; no deben describirse como A/B/A.
4. Perfilar el runtime para localizar trabajo redundante antes de modificar algoritmos.
5. Aplicar optimizaciones incrementales y comprobar cada grupo mediante benchmarks reproducibles y pruebas de regresión.
6. Mantener diferenciadas las métricas de latencia, memoria del navegador, memoria lineal WASM, asignaciones del allocator, RSS, inicialización y bundle.

## Registro de implementación en esta rama

**Primera optimización aplicada (commit `4fcd7c4f1a2ff335b72c07219196819e99079974`):**

- Se reemplazó el matching secuencial de hijos por índices `HashMap<Key, VecDeque<usize>>` y una cola de candidatos no keyed.
- Se conserva el comportamiento anterior: primera coincidencia keyed disponible, preferencia por índice para hijos no keyed y fallback al primer hijo no keyed libre.
- Se añadieron siete pruebas nativas: 10.000 hermanos no keyed, casos mixtos/duplicados y equivalencia exhaustiva frente al algoritmo anterior en 69.905 pares de secuencias de hasta cuatro hijos.
- Complejidad esperada del matching: O(n + m) para n hijos anteriores y m hijos nuevos, con coste hash esperado O(1) por lookup; antes el fallback recorría repetidamente el vector anterior y podía ser O(n·m).

**Corrección del contrato de CI tras el fallo del HEAD `dbad09f`:**

- Los toolchains Rust 1.85 y stable fallaban en `cargo fmt --all -- --check` por formato de `sequence.rs`; el formato se alineó con rustfmt.
- El gate keyed previo medía `keyed_list_incremental_insert_100` y `keyed_list_reverse_100`, workloads de KeyedList y no el camino de secuencias mixtas que este PR optimiza. En el HEAD que falló reportó 2,3 % y 4,3 % contra un gate del 20 %. No se redujeron umbrales ni se marcaron esos números como aprobados.
- El A/B/A keyed existente queda limitado al PR que modifica el reconciliador keyed. Este PR incorpora un A/B/A separado, en el mismo job/runner/Chromium, para desplazar un nodo keyed a través de 5.000 hermanos no keyed.
- La comprobación de identidad se limita al ancla keyed: los hermanos no keyed conservan identidad posicional y pueden ser reutilizados/reemplazados conforme al matching no keyed existente al desplazar el ancla. La fixture valida explícitamente el orden y el contenido de los 5.000 hermanos en cada fase y compara la misma referencia DOM del ancla keyed antes/después.
- El nuevo gate compara 12 muestras por variante, valida 2 warmups, versiones de Chromium/Node, orden y contenido de los 5.000 hermanos y la misma referencia DOM para el ancla keyed; no exige identidad estable de los nodos no keyed, que siguen el contrato posicional. Exige 20 % de mejora tanto antes como después y deriva A/A máxima del 15 %. Las tres variantes usan el mismo fixture y solo alternan `browser.rs` y el módulo DOM entre HEAD y base.
- La prueba no puede aprobar si faltan muestras, no se conserva identidad, los metadatos difieren, la mejora es insuficiente o la deriva es excesiva.

### Evidencia A/B/A aceptada — secuencias mixtas (4 de noviembre de 2026)

<badge>Aceptada</badge> **Run CI:** [#41761877066](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/41761877066) · **Job:** Browser E2E and bundle size · **SHA medido:** `e10323f425f48075dcc7ec2583586e1c8f4f22e9` · **Base:** `7c82040ff7b1813c626ff1a3bc3cb00879abbba1`.

El job midió, en el mismo runner y Chromium, mover un único ancla keyed desde el primer lugar hasta el final de 5.000 hermanos no keyed. Ejecutó 12 muestras por variante y 2 warmups; comprobó el orden/contenido de los 5.000 hermanos y la identidad DOM del ancla keyed. Los nodos no keyed mantienen identidad posicional, no keyed estable.

| Variante | Muestras | Mediana |
|---|---:|---:|
| Optimizado antes | 12 | 20,0 ms |
| Base | 12 | 30,7 ms |
| Optimizado después | 12 | 20,6 ms |

- Mejora optimizado antes vs. base: **34,85 %**.
- Mejora optimizado después vs. base: **32,90 %**.
- Deriva A/A optimizado: **3,00 %**.
- Entorno reportado: Chromium `156.0.8078.4`, Node.js `v22.23.4`.
- Gate: mejora mínima del 20 % en ambas pasadas y deriva A/A máxima del 15 %. Resultado: `accepted: true`, cero violaciones.

**Artefacto crudo:** [phase-8-1-browser-performance](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/41761877066/artifacts/13291261632) (artifact ID `13291261632`, incluye los informes y las muestras raw).

Esta aceptación solo demuestra la mejora en este workload de secuencias mixtas y en este entorno. **No es** la comparación competitiva oficial contra React, Vue y Svelte ni evidencia para las extensiones de 10.000 filas. El SHA medido antecede al ajuste de `cfg` de `sequence` en `46e76b3`; ese ajuste solo cambia dónde se compila el módulo (WASM y tests native), no el algoritmo WASM medido.

**Tercera optimización aplicada en esta rama (pendiente de CI):**

- `commit_sequence` ya no crea objetos JavaScript `Map` cuando el montaje no contiene componentes ni regiones dinámicas.
- Los índices por identidad DOM se construyen solo si el vector correspondiente de regiones no está vacío; las búsquedas siguen siendo obligatorias para los nodos de componente/dinámicos presentes.
- Esto evita dos asignaciones de objetos JS y sus inserciones por cada secuencia simple reconciliada (por ejemplo, cada elemento de una lista DOM ordinaria). La ganancia debe comprobarse con el benchmark de listas y el perfil WASM/JS; todavía no se publica como mejora cuantificada.

**Segunda optimización aplicada en esta rama (pendiente de CI):**

- Se elimina la validación recursiva redundante por cada nodo del plan de reconciliación.
- El árbol de entrada sigue validándose en `Renderer::update`; los subárboles producidos por componentes y dinámicos se validan en su punto de entrada.
- El commit ya no vuelve a recorrer el mismo árbol completo para repetir validaciones que se ejecutaron antes de preparar el plan.
- La validación de propiedades, atributos, refs y la construcción segura de nodos no se desactiva; solo se evita repetir el recorrido para cada descendiente.

Los dos cambios requieren la ejecución de los checks Rust/WASM/Browser E2E del HEAD actual y la revisión de artefactos antes de aceptarse como mejoras cuantificadas. Todavía no se atribuye una ganancia de latencia medida a ninguno de ellos.

## 3. Orden de implementación

### DiffContext — índice directo para identidades únicas

`DiffContext` ahora conserva un índice directo para componentes keyed y regiones dinámicas que aparecen una sola vez. Solo crea un vector secundario cuando detecta una identidad repetida dentro del mismo contexto. Se mantienen los arrays de uso para no reutilizar dos veces una misma región.

Esto elimina la asignación de un `Vec<usize>` por cada identidad única —el caso habitual en filas con IDs estables—. Las rutas de identidad duplicada continúan soportadas; las pruebas de listas, movimientos y cleanup deben permanecer verdes.

### Reutilización de subárbol keyed sin cambios

Cuando un hijo conserva el mismo descriptor de componente (mismo nombre/clave y props/hijos iguales), el plan marca directamente su región montada como reutilizada. Cuando conserva el mismo `DynamicChild` (misma identidad de evaluator), el renderer mueve su rango DOM y conserva su scope, estado y efecto. No vuelve a ejecutar el preview ni el evaluator solo por un cambio de posición en la lista.

Si cambian props/hijos o cambia la identidad del evaluator, se conserva el camino completo de validación, evaluación y diff. Las regiones reactivas descendientes continúan reaccionando mediante sus propios efectos. E2E y los benchmarks keyed deben confirmar identidad, contenido, orden, listeners y cleanup antes de aceptar la optimización.

### Commit keyed — iterar vistas anteriores por referencia

`commit_sequence` ahora recibe un iterador de tamaño exacto de referencias `&View`. En la rama keyed ya no clona cada `child.view()` ni crea un `Vec<View>` temporal para calcular rangos DOM; element, fragment y root también pasan iteradores directos. Se conserva el conteo de nodos, el orden, las verificaciones de rangos y la colocación DOM de derecha a izquierda.

La aceptación requiere E2E y comparación de swap/append/delete; la reducción de clonaciones es una hipótesis estructural hasta medirla.

### Reconciliación keyed — conservar índice y evitar boxes por hijo

- `KeyedList` ahora conserva el índice `Key -> posición` calculado al validar claves; `plan_keyed_children` consulta directamente el índice del snapshot anterior.
- `PlannedChild` almacena `ReconciliationPlan` inline dentro de su `Vec`; la indirección del vector basta para mantener un tamaño finito del tipo. Se mantienen los `Box` en los límites recursivos de componentes y regiones dinámicas.
- El cambio evita reconstruir el mapa anterior por diff y elimina una asignación heap por fila planificada. Las claves siguen siendo únicas, estables y validadas.

La medición debe confirmar tiempo de swap/append/delete, asignaciones y uso de memoria; no declarar ganancia antes de los artefactos del mismo runner.

### Build release — LTO entre crates (pendiente de medir)

Se habilitó `lto = "fat"` y `codegen-units = 1` en el perfil release del workspace. La intención es permitir eliminación/inlining entre `rustflow-browser`, `rustflow`, `rustflow-view` y `rustflow-dom`, reduciendo código duplicado sin quitar validaciones ni cambiar APIs.

La aceptación requiere medir el WASM raw/gzip/Brotli y volver a comparar P50/P95. Un bundle menor no prueba por sí mismo una mejora de latencia; tampoco se ampliará el presupuesto ni se aceptará una regresión de rendimiento para satisfacer el tamaño.

### Optimización aplicada — estado granular por fila (pendiente de evidencia)

En el fixture keyed del benchmark oficial se separó el estado estructural del estado de cada fila:

- `row_order: Signal<Vec<i64>>` contiene únicamente el orden e identidades visibles.
- Cada fila tiene un child scope y una señal `Signal<Todo>` propios, creados a partir de `Signal::child_scope()`.
- El parent dynamic lee el orden; cada fila usa una vista dinámica cacheada que lee solo su señal. Cambios de título/selección no invalidan el parent ni reconstruyen los otros descriptores de fila.
- La actualización de cada décima fila y la selección agrupan los writes con `Signal::batch()`, de forma que el scheduler vacía una vez y cada región afectada se ejecuta como máximo una vez por flush pendiente.
- El cache de vista vive junto al estado/scope de la fila; los callbacks mantienen referencias débiles al mapa de filas para evitar un ciclo de retención. Al retirar una fila se dispone su owner y se elimina su identidad/entrada de la lista.
- La implementación conserva claves estables, contenido/orden del DOM y el modelo de eliminación explícita de owners. El patrón no cambia la semántica general de `Signal<Vec<T>>::update`; es una arquitectura granular usada por el benchmark y disponible para aplicaciones que modelen items independientes.

**Cadena de implementación:** `0c8639a` (estado granular y batch por señal), `2e0b0e4` / `bff34fd` (correcciones de tipos/estado), `a3533da` (reutilización del índice de estados), `d546da6` (cache de vista junto al estado) y `723facd` (Weak para evitar ciclos). La rama actual continúa este trabajo.

**Incidencias de validación observadas antes del último ajuste:** CI #41946501068 sobre `ee119999` compiló el bundle, pero el WASM raw fue 624.953 bytes (límite activo 614.400) y el total gzip 226.089 bytes (límite 225.280). WebKit pasó 20/21 E2E; falló la navegación hash `#hydration-details`, sin causalidad demostrada con el cambio de filas. Después se eliminaron el conjunto de IDs redundante y el segundo mapa de vistas, y se cambió la captura del mapa a `Weak`. Esos ajustes aún requieren un CI completo sobre el HEAD final.

**No existe todavía una mejora comparativa aceptada para estas operaciones.** Hasta disponer de artefactos oficiales válidos, las mediciones anteriores siguen siendo la línea de base; no se declara superioridad frente a React, Vue o Svelte.

### Etapa A — reproducibilidad y evidencia competitiva

- Revisar el workflow `Official framework benchmarks` ya integrado y su configuración efectiva.
- Ejecutar el workflow desde el HEAD de esta rama una vez que la implementación de la rama sea apta para medir.
- Verificar la versión real de Chromium en runtime; no etiquetar Playwright Chromium como Google Chrome.
- Comprobar que cada uno de los nueve IDs tiene muestras válidas de los cuatro frameworks y que los metadatos corresponden al mismo job/runner.
- Conservar artefactos originales, muestras crudas, orden, rondas, versiones, p50/p90/p95 y agregados.
- Reportar por separado el agregado upstream de nueve operaciones, las tres extensiones y el agregado combinado.
- No mezclar memoria, bundle ni startup con el índice geométrico de latencia CPU.

### Etapa B — perfilado del runtime

Perfilar, con fixtures representativos y sin concurrencia de benchmarks, al menos:

- `plan_diff_with_context`, `ComponentInstance::preview_update` y `DiffContext`.
- `KeyedList`, indexación y colocación de nodos.
- `Signal<T>`, colecciones `Signal<Vec<T>>`, notificaciones y scheduling.
- Montaje/actualización de componentes y listas grandes.
- Mutaciones DOM y cruces Rust/WASM/JavaScript.
- Owners, scopes, callbacks, listeners y cleanup.

Cada informe de perfil debe identificar el commit, el escenario, el entorno, la metodología, el coste observado y la hipótesis de optimización. No se aceptan cambios de arquitectura motivados únicamente por intuición.

### Etapa C — optimización incremental

Priorizar solo los cuellos de botella demostrados. Líneas candidatas sujetas a perfilado:

- Evitar recorridos, comparaciones, búsquedas y estructuras temporales redundantes.
- Evitar `preview_update` o planificación cuando los cambios relevantes no lo requieran, sin omitir actualizaciones válidas.
- Reducir copias completas de colecciones y fan-out innecesario en actualizaciones de filas.
- Separar cambios estructurales de cambios de valores por elemento cuando el contrato reactivo lo permita.
- Evitar operaciones DOM redundantes y cruces WASM/JS innecesarios.
- Reducir costes de montaje y desmontaje en listas y árboles anidados.

Cada grupo de cambios debe tener una línea base reproducible, pruebas focalizadas, medición posterior y comparación de regresiones. Si una hipótesis no mejora el escenario objetivo o perjudica otros escenarios, se descarta o se documenta.

### Etapa D — seguridad y compatibilidad

Conservar, como mínimo:

- Identidad keyed y orden exacto del DOM.
- Estado local y referencias de nodos que deben sobrevivir.
- Suscripciones, efectos y dependencias dinámicas.
- `use_memo`, contextos, `use_ref`, callbacks y listeners.
- Owners/scopes y cleanup determinista.
- SSR, hidratación y aislamiento entre renders.
- APIs públicas y compatibilidad de semántica.

No introducir `unsafe`, punteros crudos, virtualización exclusiva para RustFlow ni cambios que desactiven validaciones/tests como atajo de rendimiento. Cualquier excepción de seguridad debe justificarse con invariantes, pruebas y herramientas apropiadas.

### Etapa E — documentación y cierre

Actualizar la documentación técnica y pública, los artefactos de historial y `ROADMAP.md` después de verificar los resultados. Diferenciar explícitamente:

- Mejora frente al reconciliador base de RustFlow.
- Comparación oficial frente a React.
- Comparación oficial frente a Vue.
- Comparación oficial frente a Svelte.

No afirmar una victoria agregada si hay escenarios ausentes, muestras inválidas, condiciones no equivalentes o regresiones relevantes.

## 4. Contrato de benchmark

- Usar builds de producción y versiones fijadas.
- Ejecutar los cuatro frameworks secuencialmente en el mismo job y runner; no comparar runners distintos.
- Mantener los nueve IDs upstream y las tres extensiones Phase 8.1 como conjuntos identificables.
- Conservar muestras crudas y metadatos de cada ejecución.
- Informar p50, p90 y p95 por operación/framework y la media geométrica normalizada correspondiente.
- Usar la razón `median(RustFlow) / median(framework)`; menor que 1 significa RustFlow más rápido.
- Mantener pruebas de memoria separadas de las muestras de latencia.
- No usar resultados públicos de la tabla de clasificación como si procedieran del runner de este repositorio.
- No convertir los objetivos aspiracionales de 2× en presupuestos artificiales ni en afirmaciones antes de medir.

## 5. Gates de aceptación de esta línea de trabajo

- [ ] Comparación oficial completa de nueve operaciones para los cuatro frameworks con artefactos válidos.
- [ ] Tres operaciones extendidas de 10.000 filas completadas y reportadas separadamente.
- [ ] Condiciones, versiones, navegador y runner verificados desde los artefactos.
- [ ] A/B/A de escenarios grandes únicamente cuando el protocolo sea realmente ejecutable y comparable; toda limitación queda explícita.
- [ ] Perfiles de los cuellos de botella principales guardados con identificación de commit y entorno.
- [ ] Cada optimización aceptada tiene pruebas de corrección y evidencia antes/después.
- [ ] Sin regresiones inaceptables de DOM, owners, cleanup, SSR/hidratación o APIs.
- [ ] Presupuestos de memoria y bundle revisados y conservados.
- [ ] Documentación técnica, website y roadmap sincronizados con el estado real.
- [ ] Todos los checks requeridos de GitHub Actions verdes sobre el HEAD final.
- [ ] Revisión humana de artefactos, resultados por operación y limitaciones antes de cerrar Phase 8.1.

## 6. Estado del documento

Este archivo registra tanto las optimizaciones de la rama como la evidencia todavía pendiente. La implementación granular por fila no está aceptada hasta que el HEAD final pase CI y produzca muestras comparativas reproducibles. No declarar superioridad ni cerrar Phase 8.1 antes de revisar los artefactos oficiales y las regresiones.
