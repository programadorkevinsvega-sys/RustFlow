---
title: Crear componentes
description: Estado del modelo de vistas y del sistema de componentes de RustFlow.
---

> **Estado: parcial.** RustFlow ya incorpora un modelo estructural tipado para texto, elementos y fragmentos. El sistema de componentes, el ciclo de vida y el renderizado continúan pendientes.

## Modelo de vistas disponible

El modelo está disponible desde `rustflow::view`. Permite construir un árbol en memoria sin depender de un navegador:

```rust
use rustflow::view::View;

fn main() {
    let _page = View::element(
        "main",
        [
            View::element("h1", [View::text("Hola, RustFlow")]),
            View::fragment([
                View::text("Este árbol es "),
                View::text("estructural."),
            ]),
        ],
    );
}
```

Los nodos admitidos por esta primera porción son:

- **Texto:** conserva el contenido como una cadena; aún no se transforma en HTML.
- **Elemento:** conserva el nombre de etiqueta y sus hijos en orden.
- **Fragmento:** agrupa hijos sin introducir una regla de aplanamiento.

El árbol es una representación de datos, no un renderer. No crea nodos del DOM, no aplica actualizaciones reactivas y no representa todavía componentes ejecutables. Los nombres de etiqueta tampoco se validan en esta versión inicial.

## Lo que sigue pendiente

Antes de ofrecer una API de componentes utilizable, el roadmap exige definir y probar:

- Nodos de componente y contratos de composición.
- Atributos, propiedades, atributos booleanos, eventos e hijos dinámicos.
- Claves e identidad para listas dinámicas.
- Errores de construcción y contratos de renderizado.
- Ciclo de vida de creación, montaje, actualización y desmontaje.
- Limpieza de efectos y listeners al desmontar.
- Una futura sintaxis declarativa con diagnósticos comprobables.

Todavía no existe una macro `view!`, un renderer de navegador ni un flujo completo para crear y ejecutar una aplicación web. La implementación de este árbol nativo es el primer paso de la fase 2, no una fase completada.

Consulta el [roadmap de RustFlow](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) para conocer los criterios de aceptación.
