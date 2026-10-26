---
title: Crear componentes
description: Modelo tipado de vistas, atributos, eventos y ciclo de vida de componentes.
---

> **Estado: prototipo en desarrollo.** RustFlow tiene un modelo estructural tipado, ciclo de vida nativo, un renderer inicial WASM y un serializador de fragmentos HTML. No se considera una app full-stack lista para producción.

## Objetivo de ergonomía React-like (planeado, no disponible todavía)

RustFlow apunta a una experiencia de autoría declarativa y reactiva parecida en conceptos a React, pero no a una API compatible con React. El objetivo de diseño es escribir componentes tipados con estado local, interpolación reactiva automática, callbacks tipados y actualizaciones DOM eficientes.

El siguiente código es una **ilustración del objetivo futuro; no compila con la API actual**:

```rust
#[component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);

    view! {
        <div class="counter">
            <p>"Contador: " {count}</p>
            <button on:click={move |_| count.update(|n| *n += 1)}>
                "Sumar"
            </button>
        </div>
    }
}
```

Para llegar a esa experiencia aún faltan, entre otras piezas, la API ergonómica de `signal(value)` con `update`, la macro `#[component]`, props y callbacks tipados, la interpolación reactiva automática y el parcheo DOM fino.

### Vocabulario familiar con semántica RustFlow

La meta es que los nombres resulten reconocibles para quienes conocen React, escritos con la convención de Rust. **Estos nombres son objetivos de diseño; no están exportados por la API actual.**

| Concepto conocido | Nombre RustFlow propuesto | Intención |
| --- | --- | --- |
| `useState` | `use_state` | Estado local ligado al scope del componente y respaldado por señales. |
| `useEffect` | `use_effect` | Efecto ligado al scope, con dependencias detectadas al leer señales. |
| `useMemo` | `use_memo` | Cálculo derivado con memoización y seguimiento reactivo. |
| `useRef` | `use_ref` | Almacenamiento estable no reactivo o referencia a un nodo. |
| `useContext` | `use_context` | Acceso tipado al contexto/provider más cercano. |
| `onClick` / `className` | `on:click` / `class` | La sintaxis actual de `view!` ya usa convenciones HTML/RustFlow. |
| `key` | `key` | Ya existe en el modelo de vistas; la reconciliación permanece pendiente. |

RustFlow seguirá el modelo de señales: no requiere identificar estado por el orden de llamada de hooks. Por eso, nombres parecidos no garantizan las mismas firmas, arrays de dependencias ni reglas que React. Véase [ADR 0004](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0004-react-like-component-authoring.md) y [ROADMAP.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md).


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

El crate `rustflow-dom` monta vistas en el DOM y ofrece `mount_reactive`. El renderer está evolucionando desde el reemplazo de rango completo hacia la reconciliación: la rama de implementación parchea texto y elementos compatibles, y reutiliza nodos compatibles con la misma clave en listas, cuando el árbol no contiene componentes ni hijos dinámicos. Los subárboles con componentes o evaluadores dinámicos aún siguen el camino conservador de reemplazo; por tanto, la reconciliación completa y la preservación de identidad de componentes siguen pendientes. Aún falta el bundle de navegador y los tests E2E.

## Claves e hijos dinámicos

View::keyed_list conserva el orden e impide claves duplicadas. View::dynamic difiere la evaluación del hijo hasta que un adaptador lo solicita; un adaptador reactivo debe ejecutar esa evaluación dentro de un efecto que pertenezca al scope de la instancia.

## Sintaxis declarativa disponible

El macro acepta etiquetas anidadas, cadenas/números o expresiones entre llaves, atributos booleanos, valores de atributos, propiedades con prop:nombre={valor}, eventos con on:evento={closure}, hijos diferidos con @dynamic { expresión } y descriptores de componente con nombres en PascalCase. Cierra cada etiqueta con su nombre correspondiente. El macro devuelve Result<View, ViewError>.

## Renderizado HTML en servidor

El crate rustflow-ssr serializa vistas estructurales a fragmentos HTML con escape de texto/atributos y política de URL restrictiva. Puede resolver componentes registrados y ejecutar sus callbacks de limpieza. El crate rustflow-server ofrece `html_document` para envolver una vista en un documento completo y `ErrorPages` para personalizar respuestas 4xx/5xx; los títulos y mensajes se renderizan como texto seguro. También integra rutas HTTP, JSON, query/path parameters, límites de cuerpo, timeout, middleware global ordenado, rutas nombradas, introspección y apagado graceful mediante un future del composition root. Los callbacks de eventos no se incluyen en el HTML; la hidratación sigue pendiente.

En cada petición, `Request::request_id()` expone el identificador de correlación. `x-request-id` solo se acepta si cumple el formato permitido; de lo contrario se genera un identificador nuevo. El servidor crea spans y eventos `tracing`, pero el binario de la aplicación debe configurar su propio subscriber/exporter. Este ID solo correlaciona telemetría y nunca debe usarse como credencial.

## Lo que sigue pendiente

La reconciliación completa (incluyendo subárboles dinámicos/componentes), el bundle del navegador, los tests E2E, middleware por ruta y grupos de rutas, el wiring automático de señales del sistema operativo, la hidratación y la CLI siguen pendientes. Ya existe una suite representativa de integración TCP para rutas HTML/JSON, parámetros, errores, límite de body, timeout, request ID y apagado graceful; aún faltan pruebas de TLS/reverse proxy, abortos de cliente, carga y transporte malformado. La aplicación debe configurar su propio subscriber/exporter de `tracing`. La API sigue siendo pre-1.0 y no está lista para producción.
