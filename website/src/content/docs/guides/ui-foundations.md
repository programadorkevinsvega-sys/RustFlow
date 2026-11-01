---
title: Composición de UI: contexts, listas y formularios
description: Comparte valores con contextos tipados, conserva identidad en listas y conecta controles HTML con señales.
---

RustFlow construye la UI con owners y señales. Las funciones con prefijo <code>use_</code> crean recursos dentro del owner activo durante el setup del componente. El render de <code>#[rustflow::component]</code> es repetible; no crees señales, memos, refs ni providers dentro de una expresión de render.

## 1. Contextos tipados y provider más cercano

Usa contextos para compartir configuración, tema o servicios sin propagar props por cada componente intermedio. El token tiene identidad propia: dos contextos que guardan <code>String</code> siguen siendo distintos.

Define el token en un módulo compartido:

~~~rust
use std::sync::LazyLock;
use rustflow::prelude::*;

static THEME: LazyLock<Context<String>> = LazyLock::new(Context::new);

#[rustflow::component]
fn ThemeProvider() -> impl IntoView {
    provide_context(&THEME, String::from("dark"));

    rustflow::view! {
        <main>
            <h1>"Área de la aplicación"</h1>
            <ThemeLabel />
        </main>
    }
}

#[rustflow::component]
fn ThemeLabel() -> impl IntoView {
    let theme = use_context(&THEME);

    rustflow::view! {
        <p>{theme.map(|value| value.as_str().to_owned()).unwrap_or_else(|| "Sin tema".to_owned())}</p>
    }
}
~~~

El provider registra el valor en el owner del componente. Un consumidor busca primero en su owner y continúa hacia los ancestros; un provider anidado del mismo token sombrea al externo. Si no existe provider, <code>use_context</code> devuelve <code>None</code>. <code>try_use_context</code> distingue la ausencia de errores del runtime.

El lookup no es reactivo por sí mismo. Para que un cambio actualice consumidores, proporciona un <code>Signal&lt;T&gt;</code> y lee esa señal en el consumidor. Los valores pertenecen al árbol de owners del runtime: no coloques datos por solicitud en un token global ni en un catálogo compartido.

## 2. Valores derivados con use_memo

Usa <code>use_memo</code> cuando un valor se derive de señales y quieras reutilizar el último resultado mientras sus dependencias sigan siendo válidas.

~~~rust
#[rustflow::component]
fn LineTotal(unit_price: i32, quantity: i32) -> impl IntoView {
    let price = signal(unit_price);
    let amount = signal(quantity);
    let total = use_memo(move || price.get() * amount.get());

    rustflow::view! {
        <output>"Total: " {total}</output>
    }
}
~~~

El memo es eager: ejecuta el cálculo una vez durante el setup y vuelve a ejecutarlo cuando el scheduler vacía una invalidación de las señales leídas. Las dependencias son dinámicas: si una rama deja de leer una señal, esa dependencia se elimina tras una ejecución satisfactoria. Los resultados se comparan con <code>PartialEq</code>; si el resultado nuevo es igual al anterior, el memo no notifica a sus consumidores.

Dentro de un <code>Runtime::batch</code>, el memo se recalcula en el flush del batch exterior. Antes de ese límite, las lecturas observan el último resultado confirmado. El cálculo debe ser puro. Un panic durante el cálculo inicial limpia el efecto creado y se propaga; un panic en una recomputación sigue la política del runtime y se propaga.

No pases un array de dependencias. RustFlow registra las señales leídas. No uses el memo para mutaciones, llamadas de red ni efectos secundarios.

## 3. Almacenamiento no reactivo con use_ref

Usa <code>use_ref</code> para conservar datos mutables que no deben provocar actualizaciones reactivas, por ejemplo una cadena temporal de edición o un objeto de integración local.

~~~rust
#[rustflow::component]
fn SearchBox() -> impl IntoView {
    let last_submitted = use_ref(String::new());
    let query = signal(String::new());
    let query_for_input = query.clone();
    let query_for_submit = query.clone();
    let last_for_submit = last_submitted.clone();

    let remember = move || {
        last_for_submit.set(query_for_submit.get());
    };

    rustflow::view! {
        <section>
            <label for="search-query">"Buscar"</label>
            <input
                id="search-query"
                prop:value={query.get()}
                on:input={move |event: EventPayload| {
                    if let Some(value) = event.value {
                        query_for_input.set(value);
                    }
                }}
            />
            <button type="button" on:click={move |_event| remember()}>"Recordar búsqueda"</button>
        </section>
    }
}
~~~

Las escrituras del ref no son señales y no notifican efectos. El acceso mutable puede hacerse con <code>try_with_mut</code>; la lectura con clon requiere <code>T: Clone</code>. Los conflictos de borrow y el acceso después del owner se informan explícitamente.

<code>use_ref</code> aún no proporciona una referencia a un elemento DOM. No supongas que guarda un <code>web_sys::Element</code> ni que existe un nodo durante SSR. Las refs DOM requieren una API separada que cubra montaje, hidratación, reemplazo y desmontaje.

## 4. Listas dinámicas: usa claves estables

Representa cada elemento con una clave que identifique la entidad, no su posición visual. Las claves deben ser únicas dentro de la lista.

~~~rust
let rows = View::keyed_list([
    KeyedChild::new("product-17", product_view(product_17)?),
    KeyedChild::new("product-42", product_view(product_42)?),
])?;
~~~

Cuando la lista cambia de orden, la reconciliación por clave puede conservar el owner, el estado local, los listeners y el nodo DOM de un elemento cuya clave continúa presente. Insertar o borrar una clave crea o dispone su región correspondiente. Las claves duplicadas se rechazan al construir la lista con <code>ViewError::DuplicateKey</code>; no dependas de una selección silenciosa del primer o último duplicado.

No uses el índice como clave cuando los elementos puedan reordenarse, insertarse o eliminarse. Un índice representa una posición, no la identidad de la entidad.

## 5. Fragments

<code>View::fragment</code> agrupa varios nodos hermanos en el modelo de vista sin crear un elemento HTML envolvente. En el navegador, el renderer mantiene los rangos con comentarios internos para actualizar o desmontar la región; esos comentarios no son wrappers visibles ni cambian la semántica de los elementos.

Los fragments conservan el orden de hijos, pueden anidarse y admiten cero, uno o varios nodos. SSR serializa los hijos en orden. La hidratación solo funciona cuando el HTML y las fronteras coinciden con el protocolo soportado.

## 6. Formularios y controlled inputs

RustFlow utiliza controles HTML nativos. Para un control controlado, el estado de la aplicación es la fuente de verdad: enlaza la propiedad viva <code>value</code> o <code>checked</code> a la señal y actualiza esa señal con el evento de entrada/cambio.

~~~rust
#[rustflow::component]
fn ProfileForm() -> impl IntoView {
    let display_name = signal(String::from("Ada"));
    let display_for_input = display_name.clone();

    rustflow::view! {
        <form action="/profile" method="post">
            <label for="display-name">"Nombre visible"</label>
            <input
                id="display-name"
                name="display_name"
                type="text"
                required
                autocomplete="name"
                prop:value={display_name.get()}
                on:input={move |event: EventPayload| {
                    if let Some(value) = event.value {
                        display_for_input.set(value);
                    }
                }}
            />
            <button type="submit">"Guardar"</button>
        </form>
    }
}
~~~

Para checkbox/radio utiliza <code>prop:checked</code> y lee <code>EventPayload.checked</code> en <code>change</code>. Para texto, textarea y select usa la propiedad live y el evento compatible. Los controles sin binding quedan bajo el comportamiento HTML nativo.

- **Controlled:** la UI se deriva de una señal, por ejemplo <code>prop:value={state.get()}</code>, y los eventos escriben el nuevo valor en esa señal.
- **Uncontrolled:** el navegador conserva el valor editado; un valor inicial no sincroniza automáticamente el estado de Rust.
- **Initial value:** SSR y el montaje inicial preparan el valor HTML/DOM. Esto no sustituye un handler de entrada.
- **DOM property:** <code>value</code> y <code>checked</code> son propiedades vivas, diferentes de los atributos HTML del mismo nombre.

Durante una reconciliación, RustFlow evita volver a escribir una propiedad si el valor DOM ya coincide con el valor controlado. Esto ayuda a evitar que una actualización ajena reinicie la selección/caret. No existe una promesa de transformar la posición del cursor cuando cambia realmente el valor del modelo ni una API especial de composición IME. Prueba los patrones de edición y validación en los navegadores que tu aplicación soporte.

Los formularios estándar continúan funcionando con envío HTML normal. Las APIs de validación, CSRF y extracción tipada de peticiones del servidor se describen por separado en [Formularios y validación](/guides/forms-validation/); no convierten automáticamente un input del navegador en un input controlado.

## 7. SSR y ciclo de vida

Los memos, refs y providers se asocian al owner del render. El render SSR debe crear runtimes/scopes separados por solicitud. El valor de un provider no debe colocarse en estado global compartido, y el ref no representa un nodo DOM en el servidor. Tras el desmontaje, los recursos de owner se limpian y las señales/memos quedan inválidos.

Consulta [SSR e hidratación](/guides/ssr/) para conocer el alcance de hidratación y sus límites.
