---
title: Crear componentes
description: Estado actual y alcance previsto del modelo de componentes de RustFlow.
---

> **Estado: planificado.** El modelo de componentes corresponde a la fase 2 del roadmap y todavía no es una API utilizable.

RustFlow busca ofrecer componentes reutilizables escritos en Rust, con composición declarativa y estado reactivo. Antes de publicar una guía práctica, el proyecto debe completar y probar:

- Una representación tipada de nodos de texto, elementos, fragmentos y componentes.
- Atributos, propiedades, eventos e hijos dinámicos.
- Reglas de identidad para listas dinámicas.
- Ciclo de vida de montaje, actualización y desmontaje.
- Limpieza de efectos y listeners al desmontar componentes.
- Una sintaxis declarativa con errores de compilación comprensibles.

## ¿Se puede crear un componente hoy?

Todavía no con una API pública de RustFlow. La fase 1 entrega un runtime reactivo aislado, no el modelo de vistas ni la integración con el DOM. Por eso no publicamos ejemplos ficticios como #[component], view! o macros equivalentes mientras no estén implementados y verificados.

## Qué debe incluir la guía cuando esté disponible

La guía final mostrará un componente mínimo que compile, cómo declarar su estado, cómo manejar eventos, el resultado observable en el navegador y cómo se limpia al desmontarse. Los ejemplos se añadirán junto con la implementación y sus pruebas.

Consulta la fase 2 del [roadmap de RustFlow](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) para ver los criterios de aceptación.
