---
title: Reactividad
description: Modelo y límites del runtime reactivo inicial de RustFlow.
---

El runtime reactivo de RustFlow se implementa en crates/rustflow/src/reactivity.rs. La API exacta debe consultarse en el código y en las pruebas de la versión actual; no asumas que coincide con APIs de otros frameworks.

## Conceptos implementados

- **Signals:** almacenan valores tipados y permiten observar cambios.
- **Effects:** ejecutan trabajo reactivo y registran las dependencias que leen.
- **Dependencias dinámicas:** las lecturas realizadas durante la ejecución determinan las dependencias activas.
- **Scheduler y batching:** organizan la ejecución de efectos y agrupan actualizaciones.
- **Owner scopes:** permiten asociar efectos y callbacks de limpieza a un ciclo de vida.
- **Diagnósticos:** informan sobre algunos usos inválidos del runtime.

## Restricciones importantes

El runtime inicial utiliza Rc y RefCell y es de un solo hilo: no es Send ni Sync. No lo trates como un runtime concurrente.

La implementación ya existe, pero la fase de corrección y los benchmarks siguen pendientes en el roadmap. Por ello, esta guía describe el comportamiento actual sin afirmar que todos los casos límite estén validados.

## Fuente técnica

Consulta la [especificación de semántica](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_SEMANTICS.md), la [guía del runtime](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_RUNTIME.md) y los tests junto a la implementación para ver la API exacta y los ejemplos verificados.
