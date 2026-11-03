---
title: Verificación del runtime reactivo
description: Cobertura de pruebas, criterios de cierre y protocolo de benchmarks.
---

Esta página distingue la implementación existente de las garantías verificadas. El [roadmap oficial](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el estado de cada fase.

## Fase 1.3 — Corrección

La suite de regresión cubre dependencias, reemplazo de dependencias dinámicas, batching, efectos anidados, disposal, limpieza LIFO, ciclos, escrituras reentrantes, efectos encolados que se eliminan, pánicos y aislamiento entre runtimes. La cobertura ampliada pasó CI en Rust 1.85 y stable para el commit verificado del [PR de verificación](https://github.com/programadorkevinsvega-sys/RustFlow/pull/9). La CI debe volver a pasar para cualquier cambio posterior antes de considerar el estado cerrado.

Una fase no se cierra por añadir tests: deben ejecutarse correctamente en las versiones de Rust soportadas y los defectos encontrados deben corregirse. La secuencia determinista de operaciones no sustituye las pruebas basadas en propiedades ni demuestra exhaustividad matemática.

## Fase 1.4 — Rendimiento

La referencia de esta fase conserva el microbenchmark original de Phase 1.4, que incluía lecturas, escrituras, un efecto y un lote de 256 efectos dependientes. Esa tabla y sus tres repeticiones son un **registro histórico**, no describen toda la suite actual. El benchmark de reactividad se amplió después: ahora mide creación/destrucción del runtime, señales, efectos y dependencias dinámicas, memos, batching, fan-out de 1 a 1.024 suscriptores, una dependencia de 1.000 señales y ciclos de vida de owners. La guía actualizada de [rendimiento](https://rustflow.pages.dev/guides/performance/) y el [contrato Phase 8.1](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/PERFORMANCE.md) documentan la suite completa, incluida la medición separada de SSR, DOM, memoria, inicialización y bundle.

Para establecer una línea base reproducible, registra el commit, las versiones de Rust y Cargo, sistema operativo/kernel, CPU, número de iteraciones y todas las salidas. Ejecuta varias repeticiones en una máquina inactiva y compara medianas obtenidas en entornos equivalentes. Los tiempos de un runner compartido son una referencia inicial, no una garantía de rendimiento.

## Protocolo detallado

Consulta [docs/REACTIVITY_VERIFICATION.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_VERIFICATION.md) para el registro histórico y protocolo de verificación de reactividad, y [docs/PERFORMANCE.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/PERFORMANCE.md) para la metodología completa vigente de Phase 8.1.
