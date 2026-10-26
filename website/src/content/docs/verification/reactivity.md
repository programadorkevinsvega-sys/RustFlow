---
title: Verificación del runtime reactivo
description: Cobertura de pruebas, criterios de cierre y protocolo de benchmarks.
---

Esta página distingue la implementación existente de las garantías verificadas. El [roadmap oficial](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el estado de cada fase.

## Fase 1.3 — Corrección

La suite de regresión cubre dependencias, reemplazo de dependencias dinámicas, batching, efectos anidados, disposal, limpieza LIFO, ciclos, escrituras reentrantes, efectos encolados que se eliminan, pánicos y aislamiento entre runtimes. La cobertura ampliada pasó CI en Rust 1.85 y stable para el commit verificado del [PR de verificación](https://github.com/programadorkevinsvega-sys/RustFlow/pull/9). La CI debe volver a pasar para cualquier cambio posterior antes de considerar el estado cerrado.

Una fase no se cierra por añadir tests: deben ejecutarse correctamente en las versiones de Rust soportadas y los defectos encontrados deben corregirse. La secuencia determinista de operaciones no sustituye las pruebas basadas en propiedades ni demuestra exhaustividad matemática.

## Fase 1.4 — Rendimiento

El repositorio incorpora microbenchmarks para lecturas, escrituras, scheduling de un efecto y fan-out de 256 efectos. La CI ejecuta tres repeticiones con el perfil optimizado y ya se registró una línea base inicial con medianas y detalles del entorno en [docs/REACTIVITY_VERIFICATION.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_VERIFICATION.md). Los tiempos de un runner compartido son una referencia inicial, no una garantía de rendimiento.

Para establecer una línea base reproducible, registra el commit, las versiones de Rust y Cargo, sistema operativo/kernel, CPU, número de iteraciones y todas las salidas. Ejecuta varias repeticiones en una máquina inactiva y compara medianas obtenidas en entornos equivalentes.

## Protocolo detallado

Consulta [docs/REACTIVITY_VERIFICATION.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_VERIFICATION.md) para comandos de validación, cargas medidas y metodología reproducible.
