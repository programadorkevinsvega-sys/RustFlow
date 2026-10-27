---
title: Crear componentes
description: Aprende a construir componentes, pasar props, manejar eventos y actualizar la interfaz.
---

Un componente encapsula una parte reutilizable de una interfaz: por ejemplo, un contador, una fila de una lista o la tarjeta de un producto. En RustFlow, el componente combina una función de render, props tipadas y, cuando corresponde, señales y efectos ligados a un scope propietario.

El modelo es **signal-first**, no una copia de React. Los helpers con nombre <code>use_</code> no dependen del orden de llamada y devuelven los tipos de RustFlow.

## 1. Crear un componente sencillo

~~~rust
use rustflow::prelude::*;

#[rustflow::component]
fn Greeting(name: String) -> impl IntoView {
    rustflow::view! {
        <article class="greeting">
            <h2>"Hola, " {name}</h2>
            <p>"Este bloque se puede reutilizar."</p>
        </article>
    }
}
~~~

Qué significa cada parte:

- <code>#[rustflow::component]</code> genera una definición de componente y su función de registro.
- <code>name: String</code> es una prop obligatoria y tipada.
- <code>impl IntoView</code> indica que el resultado se convierte a la representación estructural de vista.
- <code>view!</code> describe elementos, atributos, expresiones y callbacks mediante la sintaxis declarativa de RustFlow.

El valor de <code>name</code> se convierte en texto de la vista; al renderizarlo, el renderer aplica su política de escape. La vista no es una cadena HTML sin procesar.

## 2. Crear estado interactivo

~~~rust
use rustflow::prelude::*;

#[rustflow::component]
fn Counter(initial: i32) -> impl IntoView {
    let count = use_state(initial);
    let count_for_button = count.clone();

    rustflow::view! {
        <section class="counter">
            <p>"Contador: " {count}</p>
            <button on:click={move |_event| {
                count_for_button.update(|value| *value += 1);
            }}>
                "Sumar"
            </button>
        </section>
    }
}
~~~

El estado vive en una señal asociada al scope del componente. La interpolación del identificador de señal es reactiva; el evento actualiza la misma señal. El runtime DOM vuelve a calcular la región dependiente y reconcilia los nodos afectados.

<code>use_state(initial)</code> devuelve un <code>Signal&lt;T&gt;</code>, no una tupla valor/setter. <code>signal(initial)</code> es otro helper disponible para crear una señal dentro del owner activo. No llames a estas funciones desde un contexto sin owner: están diseñadas para el setup de un componente.

## 3. Props y callbacks tipados

Un componente puede recibir un callback como prop. El callback no se convierte en un atributo HTML con nombre <code>on*</code>; se mantiene como valor de callback tipado y se invoca desde el componente.

~~~rust
use rustflow::prelude::*;

#[rustflow::component]
fn IncrementButton(on_increment: EventHandler) -> impl IntoView {
    rustflow::view! {
        <button on:click={move |event| on_increment.invoke(event)}>
            "Incrementar"
        </button>
    }
}
~~~

En este ejemplo, <code>on_increment</code> es la prop del componente y <code>on:click</code> es el evento DOM. Son dos niveles distintos: el evento llama al callback que la aplicación pasó al componente.

La API de <code>#[rustflow::component]</code> es un subconjunto experimental. Los parámetros deben tener nombres simples y tipos explícitos; se admiten <code>String</code>, <code>bool</code>, <code>i32</code>, <code>i64</code>, <code>u32</code>, <code>usize</code>, <code>f64</code> y <code>EventHandler</code>. La macro genera errores para props faltantes, desconocidas o incompatibles.

## 4. Sintaxis de vista: texto, props, eventos y propiedades

~~~rust
rustflow::view! {
    <label for="query">"Buscar"</label>
    <input
        id="query"
        prop:value={current_query}
        aria-label="Buscar productos"
        on:input={move |event: EventPayload| {
            query_signal.set(event.value.unwrap_or_default());
        }}
    />
    <button disabled={is_saving} on:click={save}>
        "Guardar"
    </button>
}
~~~

Convenciones:

- Texto literal entre comillas: <code>"Buscar"</code>.
- Expresión Rust entre llaves: <code>{current_query}</code>.
- Atributos normales: <code>class="..."</code>, <code>aria-label="..."</code>.
- Propiedad DOM: <code>prop:value={...}</code> o <code>prop:checked={...}</code>.
- Evento DOM: <code>on:input={...}</code>, <code>on:click={...}</code>.
- Atributo booleano: <code>disabled</code> o su expresión booleana.
- Clave de lista: <code>key={id}</code> en el nodo al que se aplique la clave.
- Región dinámica explícita: <code>@dynamic { expresión }</code> cuando la expresión completa no es un identificador de señal que la macro pueda reconocer.

El ejemplo es un patrón sintáctico: los nombres de señales, el callback <code>save</code> y los tipos de propiedades tienen que existir en la función real donde se coloca.

## 5. Listas, claves y estado local

Las claves identifican los elementos lógicos de una lista. Usa una clave estable, como el ID de una tarea o producto, no el índice visual de la fila si los elementos se pueden mover.

En el ejemplo de navegador, la lista de tareas permite agregar, completar, eliminar y reordenar tareas. Las pruebas E2E comprueban que una fila compatible conserva identidad DOM y estado local cuando cambia de posición. Cuando una fila se elimina o cambia a una clave incompatible, se desmonta y ejecuta la limpieza asociada.

Este patrón es adecuado para filas de tabla, tareas, artículos, tarjetas de producto o controles repetidos que mantienen estado propio.

## 6. Registro de componentes

La macro genera una función de registro a partir del nombre del componente. Por ejemplo, el ejemplo del navegador registra las definiciones antes de montar:

~~~rust
let mut registry = ComponentRegistry::new();
register_counter(&mut registry)?;
~~~

Cuando un descriptor de componente se procesa, el host debe poder resolver la definición. La registry define el catálogo de componentes, pero no sustituye al renderer DOM ni al renderer SSR. En handlers HTTP concurrentes, usa el catálogo compartido de servidor de la guía [SSR y documentos HTML](/guides/ssr/).

## 7. Efectos y limpieza

Usa <code>use_effect</code> para trabajo reactivo con dependencias rastreadas por lecturas de señal. Usa <code>on_cleanup</code> para registrar la limpieza de recursos ligados al scope. No pongas efectos secundarios no gestionados dentro de una función de render que puede ejecutarse repetidamente.

La limpieza es síncrona y ocurre al volver a evaluar/limpiar efectos según su contrato y al disponer el scope. No se ofrece cancelación automática de cualquier tarea asíncrona externa.

## 8. Casos de uso

- **Controles de formulario:** estado de una entrada y un evento que actualiza la señal.
- **Tarjetas reutilizables:** props de título, descripción, imagen o precio; el componente produce una vista, mientras los datos siguen siendo responsabilidad de la aplicación.
- **Listas interactivas:** claves estables para preservar identidad de filas entre reordenaciones.
- **Layouts y piezas compartidas:** encabezados, navegación y envolturas de página se pueden modelar como componentes; consulta [Layouts](/guides/layouts/).
- **Interfaz browser-only:** componentes que necesitan APIs del navegador deben declarar su capacidad y proporcionar un fallback SSR cuando deban aparecer en una página SSR; consulta [SSR](/guides/ssr/).

## Límites actuales

- <code>use_memo</code>, <code>use_ref</code> y <code>use_context</code> todavía no existen.
- No se usan reglas de hooks por orden de llamada y no se promete compatibilidad con React.
- La macro acepta un subconjunto intencional de tipos de props y de expresiones de vista.
- Expresiones arbitrarias que lean señales necesitan una región dinámica explícita.
- El render SSR no conecta automáticamente listeners de navegador. La hidratación es optativa y debe configurarse explícitamente.
- El runtime es pre-1.0 y no debe considerarse API estable.

Para recorrer el caso de uso de UI completo desde el navegador, sigue el [tutorial de interfaz reactiva de tareas](/tutorials/reactive-todo/). El [catálogo de ejemplos](/reference/examples/) relaciona cada guía con el ejecutable correspondiente.

## Ejemplo y pruebas de referencia

El ejemplo ejecutable está en [examples/browser/src/app.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/src/app.rs), y su [README](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) explica build y E2E. La documentación técnica de nodos, validación y ciclo de vida está en [VIEW_MODEL.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/VIEW_MODEL.md).
