---
title: Estado del proyecto
description: Qué está implementado y qué sigue pendiente en RustFlow.
---

RustFlow se encuentra en una etapa temprana de desarrollo. El [roadmap del repositorio](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) es la fuente de verdad para el progreso del proyecto.

## Disponible actualmente

### Semántica de reactividad

La especificación aceptada describe señales, efectos, dependencias dinámicas, limpieza, batching, planificación, ciclos y scopes.

### Runtime reactivo inicial

La fase de implementación del runtime reactivo está integrada en main. Incluye señales tipadas, efectos, grafo de dependencias, scheduler FIFO, batching, scopes y diagnósticos de uso inválido.

Consulta la implementación en [crates/rustflow/src/reactivity.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow/src/reactivity.rs) y la guía técnica [docs/REACTIVITY_RUNTIME.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_RUNTIME.md).

## Todavía en progreso

- La fase 1.3 tiene cobertura de regresión ampliada para dependencias, FIFO/batching, reentrancia, disposal, ciclos, pánicos, aislamiento y un modelo determinista. La CI de Rust 1.85 y stable ha pasado para el código verificado; esto no equivale a una prueba matemática de exhaustividad.
- La fase 1.4 ya cuenta con una línea base inicial medida en tres repeticiones de CI, con entorno y medianas documentados en `docs/REACTIVITY_VERIFICATION.md`. Los números sirven como referencia de humo, no como garantía de rendimiento ni sustituyen mediciones controladas en hardware dedicado.
- El modelo de vistas, el runtime de navegador/WASM, SSR, routing y las herramientas full-stack pertenecen a fases posteriores del roadmap.

No interpretes una sección futura como una API disponible. Revisa el código y el roadmap antes de depender de una función.
