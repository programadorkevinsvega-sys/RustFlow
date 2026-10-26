---
title: Verificación del runtime reactivo
description: Cobertura de pruebas, criterios de cierre y protocolo de benchmarks.
---

Esta página distingue la implementación existente de las garantías verificadas. El [roadmap oficial](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el estado de cada fase.

## Fase 1.3 — Corrección

La suite de regresión cubre dependencias, reemplazo de dependencias dinámicas, batching, efectos anidados, disposal, limpieza LIFO, ciclos, escrituras reentrantes, efectos encolados que se eliminan, pánicos y aislamiento entre runtimes. Los cambios recientes amplían esa cobertura; consulta el estado de CI del [PR de verificación](https://github.com/programadorkevinsvega-sys/RustFlow/pull/9) antes de considerar esos cambios validados.

Una fase no se cierra por añadir tests: deben ejecutarse correctamente en las versiones de Rust soportadas y los defectos encontrados deben corregirse. La secuencia determinista de operaciones no sustituye las pruebas basadas en propiedades ni demuestra exhaustividad matemática.

## Fase 1.4 — Rendimiento

El repositorio incorpora microbenchmarks para lecturas, escrituras, scheduling de un efecto y fan-out de 256 efectos. La CI ejecuta una medición de humo con el perfil optimizado. Esa ejecución ayuda a comprobar que los benchmarks funcionan, pero los tiempos de un runner compartido no son por sí solos una línea base estable.

Para establecer una línea base reproducible, registra el commit, las versiones de Rust y Cargo, sistema operativo/kernel, CPU, número de iteraciones y todas las salidas. Ejecuta varias repeticiones en una máquina inactiva y compara medianas obtenidas en entornos equivalentes.

## Protocolo detallado

Consulta [docs/REACTIVITY_VERIFICATION.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_VERIFICATION.md) para comandos de validación, cargas medidas y metodología reproducible.
