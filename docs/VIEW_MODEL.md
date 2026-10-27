# Modelo de vistas y componentes de RustFlow

El modelo de vista es tipado e independiente del destino. Describe qué interfaz se quiere representar; no es HTML renderizado ni, por sí solo, una frontera de sanitización. El contrato estructural de la Fase 2 se amplió con la API de autoría, el runtime de navegador y la reconciliación de la Fase 3.

## Nodos disponibles

- **Text:** texto que un renderer escapa según su destino.
- **Element:** elemento con nombre validado, atributos, propiedades DOM, eventos, hijos ordenados y clave opcional.
- **Fragment:** agrupa varios hijos sin introducir un elemento HTML adicional.
- **Component:** descriptor con nombre de componente, propiedades tipadas, hijos y clave opcional. Una definición registrada aporta setup y render.
- **Dynamic:** evaluación diferida de un hijo. El adaptador reactivo la evalúa dentro de un efecto asociado al scope propietario.
- **KeyedList:** lista de hijos con claves estables únicas dentro de cada snapshot.

La clave identifica al hijo lógico, no su posición. Al mover una fila con la misma clave, el runtime DOM puede conservar su nodo y estado; al eliminarla o sustituirla por otra clave, se limpia la instancia anterior.

## Sintaxis declarativa: view!

La macro <code>rustflow::view!</code> acepta un subconjunto explícito de sintaxis similar a HTML:

~~~rust
use rustflow::prelude::*;

fn static_view() -> Result<View, rustflow::view::ViewError> {
    rustflow::view! {
        <main class="welcome">
            <h1>"Hola, RustFlow"</h1>
            <button disabled on:click={move |_event| {}}>
                "Continuar"
            </button>
        </main>
    }
}
~~~

Reglas importantes:

- El texto literal se escribe como literal Rust; las expresiones Rust van entre llaves.
- Los atributos normales, booleanos, <code>prop:nombre</code>, <code>on:evento</code>, <code>key={...}</code> y <code>@dynamic { expresión }</code> tienen sintaxis diferenciada.
- Los tags de componentes se escriben con nombre inicial en mayúscula y producen descriptores de componentes.
- La macro devuelve <code>Result&lt;View, ViewError&gt;</code>. Los tags mal cerrados, atributos duplicados y otras entradas inválidas producen diagnósticos localizados.
- Interpolar una señal mediante su identificador, por ejemplo <code>{count}</code>, crea una región reactiva automática. Las expresiones arbitrarias que lean señales deben delimitarse explícitamente con <code>@dynamic</code>.

## Componentes funcionales

La macro <code>#[rustflow::component]</code> genera la definición y una función de registro. El componente declara props con nombres simples y tipos explícitos:

~~~rust
use rustflow::prelude::*;

#[rustflow::component]
fn Greeting(name: String) -> impl IntoView {
    rustflow::view! {
        <article class="greeting">
            <h2>"Hola, " {name}</h2>
        </article>
    }
}
~~~

Para usar la definición en un host que requiera registro explícito:

~~~rust
let mut registry = ComponentRegistry::new();
register_greeting(&mut registry)?;
~~~

Los tipos de props soportados por esta versión incluyen <code>String</code>, <code>bool</code>, <code>i32</code>, <code>i64</code>, <code>u32</code>, <code>usize</code>, <code>f64</code> y <code>EventHandler</code> (incluida la forma cualificada documentada). La macro rechaza props faltantes, desconocidos o de tipos incompatibles. Los callbacks se pasan como props tipadas con la convención <code>on:...</code>; no se serializan como atributos HTML <code>on*</code>.

## Ciclo de vida y renderizado

El setup de un componente se ejecuta una vez por montaje y crea señales/efectos dentro de un scope propietario. La función de render debe producir una vista y no realizar efectos secundarios no gestionados. Al reemplazar o desmontar una instancia, el runtime libera el scope, los efectos y los callbacks de limpieza.

El registry resuelve descriptores, pero no hace el trabajo DOM por sí solo. El runtime de navegador se encarga del montaje y de la reconciliación: prepara la evaluación/diff y después confirma los cambios al DOM. Las actualizaciones compatibles pueden conservar el nodo DOM, el scope y el estado local de una instancia. Las ramas incompatibles se sustituyen y limpian.

## Atributos, propiedades, eventos y seguridad

Atributos HTML y propiedades DOM son categorías distintas: un atributo describe markup; una propiedad como <code>value</code> o <code>checked</code> puede representar el estado actual del elemento en el navegador. Usa <code>prop:value</code> o <code>prop:checked</code> cuando el ejemplo requiera una propiedad DOM.

Los eventos se representan como callbacks tipados con <code>EventHandler</code> y reciben un <code>EventPayload</code> normalizado. Los listeners se eliminan al limpiar la región o componente asociado.

Los nombres de tags/atributos se validan sintácticamente y el renderer DOM valida las operaciones y esquemas de URL permitidos. El SSR escapa texto/atributos y aplica una política conservadora a tags y URL. Evita crear HTML confiando únicamente en que el árbol <code>View</code> haya sido validado: no es un sanitizador universal y <code>Response::html(String)</code> emite la cadena proporcionada sin escapar.

## Disponibilidad y limitaciones

| API o capacidad | Estado |
|---|---|
| <code>view!</code>, nodos estructurales y fragments | Disponible |
| <code>#[rustflow::component]</code> y props tipadas del subconjunto | Disponible |
| Señales, <code>use_state</code>, <code>use_effect</code>, <code>on_cleanup</code> owner-scoped | Disponible dentro del setup de un componente |
| Eventos y actualizaciones DOM reactivas | Disponible en el runtime WASM |
| Reconciliación de listas keyed y limpieza de instancias | Disponible |
| <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> | Pendiente |
| Context/provider público, async effects y state tracking a través de <code>await</code> | Pendiente/no soportado |
| API estable 1.0 o compatibilidad con React | No ofrecida |

Guías de uso: [Crear componentes](../website/src/content/docs/guides/components.md), [Reactividad](../website/src/content/docs/concepts/reactivity.md), [SSR](../website/src/content/docs/guides/ssr.md) y el [ejemplo de navegador](../examples/browser/).
