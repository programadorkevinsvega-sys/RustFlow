---
title: Crear componentes
description: Modelo tipado de vistas, atributos, eventos y ciclo de vida de componentes.
---

> **Estado: prototipo en desarrollo.** RustFlow tiene un modelo estructural tipado, ciclo de vida nativo, un renderer inicial WASM y un serializador de fragmentos HTML. No se considera una app full-stack lista para producción.

## Construir un árbol de vistas

El modelo está disponible desde rustflow::view. Los constructores de elementos validan la sintaxis del nombre y devuelven un resultado:

```rust
use rustflow::view::View;

fn page() -> Result<View, rustflow::view::ViewError> {
    let input = rustflow::view::Element::new("input")?
        .with_attribute("aria-label", "Buscar")?
        .with_boolean_attribute("disabled", false)?
        .with_property("value", "consulta")?
        .build();
    View::element("main", [View::text("Hola, RustFlow"), input])
}
```

## Atributos, propiedades y eventos

Los atributos HTML, las propiedades del DOM y los eventos son tipos diferentes. Los manejadores de eventos se almacenan como callbacks tipados, no como atributos HTML on*. Los nombres de etiquetas, atributos, propiedades y eventos pasan validación sintáctica; duplicados y nombres no permitidos producen errores tipados.

Esta comprobación no sanitiza URL ni vuelve seguro el árbol como HTML. No insertes valores mediante HTML sin procesar. Cualquier renderer debe escapar texto y atributos, validar URL y retirar sus listeners al desmontar.

## Componentes y ciclo de vida

Una ComponentDefinition ejecuta setup una vez por instancia y devuelve una función pura de renderizado. ComponentRegistry resuelve los descriptores de componente a definiciones registradas y rechaza nombres duplicados o desconocidos. La macro rustflow::view! construye árboles a partir de una sintaxis HTML-like restringida. ComponentInstance conserva un scope reactivo, permite recalcular la vista mediante update y libera los efectos y callbacks de limpieza con unmount. Los errores de actualización conservan la vista anterior. Las callbacks de renderizado deben limitarse a producir una nueva vista; los efectos y recursos se registran durante setup mediante ComponentContext.

La API de componentes genera una vista de reemplazo. El crate rustflow-dom ya monta esa vista en el DOM y ofrece mount_reactive; las actualizaciones reemplazan el rango completo, por lo que keyed reconciliation y actualizaciones dirigidas a subárboles siguen pendientes. Aún falta el bundle de navegador y los tests E2E.

## Claves e hijos dinámicos

View::keyed_list conserva el orden e impide claves duplicadas. View::dynamic difiere la evaluación del hijo hasta que un adaptador lo solicita; un adaptador reactivo debe ejecutar esa evaluación dentro de un efecto que pertenezca al scope de la instancia.

## Sintaxis declarativa disponible

El macro acepta etiquetas anidadas, cadenas/números o expresiones entre llaves, atributos booleanos, valores de atributos, propiedades con prop:nombre={valor}, eventos con on:evento={closure}, hijos diferidos con @dynamic { expresión } y descriptores de componente con nombres en PascalCase. Cierra cada etiqueta con su nombre correspondiente. El macro devuelve Result<View, ViewError>.

## Renderizado HTML en servidor

El crate rustflow-ssr serializa vistas estructurales a fragmentos HTML con escape de texto/atributos y política de URL restrictiva. Puede resolver componentes registrados y ejecutar sus callbacks de limpieza. El crate rustflow-server integra ese resultado con respuestas HTTP y agrega rutas, JSON, query/path parameters, límites de cuerpo, timeout, middleware global ordenado, rutas nombradas e introspección. También permite apagado graceful mediante un future que inyecta el composition root. Los callbacks de eventos no se incluyen en el HTML; la hidratación sigue pendiente.

## Lo que sigue pendiente

La reconciliación keyed, el bundle del navegador, los tests E2E, middleware por ruta, tracing/logging, pruebas de red más amplias, wiring automático de señales del sistema operativo, la hidratación y la CLI corresponden a entregables posteriores. La API sigue siendo pre-1.0 y no está lista para producción.
