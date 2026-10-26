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

La fase de corrección cuenta con pruebas de regresión ampliadas que pasaron CI en Rust 1.85 y stable. La fase de rendimiento dispone de una línea base inicial medida en tres repeticiones; los números y las limitaciones del entorno están documentados en `docs/REACTIVITY_VERIFICATION.md`. Esto mejora la evidencia disponible, pero no afirma que se hayan probado todas las secuencias posibles ni establece una garantía de rendimiento.

## Fuente técnica

Consulta la [especificación de semántica](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_SEMANTICS.md), la [guía del runtime](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_RUNTIME.md) y los tests junto a la implementación para ver la API exacta y los ejemplos verificados.
