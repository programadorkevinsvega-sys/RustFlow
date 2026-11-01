# Runtime reactivo de RustFlow

Este documento distingue las dos formas de utilizar el runtime implementadas en el repositorio: la API de bajo nivel basada en <code>Runtime</code> y la API de señales asociada al scope de un componente. Ambas son prototipos pre-1.0 y no tienen garantía de estabilidad.

## API de bajo nivel: Runtime explícito

Utiliza <code>Runtime</code> cuando quieras controlar explícitamente el ciclo de vida de un runtime aislado, por ejemplo, en pruebas o en un consumidor que no está montando un componente:

~~~rust
use rustflow::reactivity::{Runtime, RuntimeError};

fn main() -> Result<(), RuntimeError> {
    let runtime = Runtime::new();
    let count = runtime.signal(0_i32)?;
    let read_count = count.clone();

    let effect = runtime.effect(move || {
        println!("count = {}", read_count.get());
    })?;

    count.set(1);
    runtime.batch(|| {
        count.set(2);
        count.set(3);
    })?;

    effect.dispose()?;
    runtime.dispose()?;
    Ok(())
}
~~~

En esta API, <code>Runtime::signal</code> crea una señal tipada, <code>Runtime::effect</code> ejecuta el efecto inicialmente y registra las señales que lee, y <code>Runtime::batch</code> agrupa las invalidaciones hasta el límite exterior del batch. El ejemplo imprime el valor inicial y reacciona a los cambios; los dos writes dentro del batch se publican juntos al terminar el batch.

## API de componentes y owners

Los componentes usan una API propietaria de un scope activo. La macro <code>#[rustflow::component]</code> configura ese entorno para el setup; las llamadas a <code>signal</code>, <code>use_state</code>, <code>use_effect</code>, <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> y <code>provide_context</code> deben ocurrir como declaraciones de setup, no dentro del cuerpo de render repetible. No son utilidades globales que se puedan llamar sin owner desde cualquier <code>main</code>.

~~~rust
use rustflow::prelude::*;

#[rustflow::component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);
    let count_for_button = count.clone();

    rustflow::view! {
        <section>
            <p>"Contador: " {count}</p>
            <button on:click={move |_event| count_for_button.update(|n| *n += 1)}>
                "Sumar"
            </button>
        </section>
    }
}
~~~

Qué sucede:

1. <code>signal(initial)</code> crea una señal ligada al scope del componente.
2. La interpolación del identificador <code>{count}</code> crea una región reactiva que vuelve a evaluar el hijo cuando cambia la señal.
3. El handler clona la señal para poder actualizarla desde el evento.
4. <code>update</code> modifica el valor y notifica a los suscriptores cuando el valor cambia según la política de igualdad de la señal.
5. La reconciliación DOM actualiza la región afectada y mantiene la identidad del DOM compatible en lugar de reconstruir necesariamente toda la página.

<code>use_state(initial)</code> es un alias ergonómico que devuelve un <code>Signal&lt;T&gt;</code>; no devuelve el par de valor/función habitual de React ni utiliza hooks por posición. <code>use_effect(callback)</code> registra un efecto reactivo dentro del scope propietario. <code>on_cleanup(callback)</code> registra limpieza asociada al scope. Las funciones de la API de componentes requieren el contexto de propietario activo.

## Señales y efectos: semántica

- Las señales son tipadas. Las lecturas realizadas dentro de un efecto determinan su conjunto de dependencias activo.
- En una ejecución satisfactoria, el conjunto anterior de dependencias de un efecto se reemplaza por el conjunto leído en la nueva ejecución.
- Los efectos se ejecutan al crearse. El scheduler usa orden FIFO de invalidación, evita duplicar un efecto encolado y drena iterativamente el trabajo pendiente.
- <code>Runtime::batch</code> puede anidarse y vacía el trabajo en el límite exterior. Existe un límite de ejecuciones por vaciado para detectar ciclos.
- Los scopes poseen efectos, scopes descendientes y callbacks de limpieza. Al disponer un scope se desconectan las dependencias, se cancela trabajo pendiente y se ejecutan los callbacks de limpieza en orden LIFO.
- Los handles usados después de disponer su runtime/scope producen diagnósticos; los accesos ergonómicos pueden entrar en panic con un mensaje de diagnóstico. Prefiere las variantes <code>try_*</code> cuando debas gestionar el error explícitamente.

## 6. Memo, refs y contextos tipados

- <code>Scope::memo</code> y <code>use_memo</code> crean un memo eager. Ejecuta el cálculo una vez al crearlo y vuelve a ejecutarlo al vaciarse el scheduler cuando cambian sus dependencias activas. Las dependencias se detectan por lecturas de señales y cambian dinámicamente entre ejecuciones.
- El memo exige <code>Clone + PartialEq</code>. Compara el resultado nuevo con el último valor confirmado; si son iguales, no notifica a lectores. Dentro de un batch, la lectura devuelve el valor anterior hasta el flush exterior. No es lazy y el cálculo debe ser puro.
- <code>use_ref</code> crea almacenamiento estable y no reactivo ligado al owner. <code>try_with</code> y <code>try_with_mut</code> permiten acceso mediante un borrow corto; <code>get</code> requiere que <code>T: Clone</code>. Escribir el ref no notifica efectos. Tras disponer el owner, los handles clonados devuelven <code>RuntimeError::RefDisposed</code>.
- <code>Context&lt;T&gt;</code> es un token de identidad propio, independiente de <code>T</code>. <code>provide_context</code> registra el valor en el owner; <code>use_context</code> busca el provider más cercano caminando por los ancestros. Un provider duplicado en el mismo owner produce error y la ausencia se representa por <code>None</code>.
- El lookup de contextos no es reactivo por sí mismo. Si el valor es una señal, las lecturas de esa señal sí se rastrean normalmente.
- <code>try_use_memo</code>, <code>try_use_ref</code>, <code>try_use_context</code> y <code>try_provide_context</code> permiten gestionar errores de owner explícitamente. Los helpers ergonómicos hacen panic con diagnóstico ante un owner/runtime inválido.
- <code>use_ref</code> es almacenamiento Rust, no una referencia a un nodo DOM. No existe aún un contrato de refs DOM.

## 7. Límites y reglas de uso

- El runtime reactivo utiliza <code>Rc</code> y <code>RefCell</code>: no es <code>Send</code> ni <code>Sync</code>. No compartas las señales locales de un runtime entre threads.
- Crea un runtime y scopes locales por render de servidor; no guardes estado reactivo ni valores de provider de una petición en un catálogo compartido.
- Los efectos y la limpieza son síncronos. No se garantiza el seguimiento de dependencias a través de <code>await</code>; no existe API de recursos async ni loading boundary.
- No hay ErrorBoundary de componentes. Los errores iniciales y SSR se propagan; el renderer reactivo del navegador conserva el último rango bueno y expone el último error mediante <code>take_error</code>. Un panic de handler no tiene aislamiento UI garantizado.
- La interpolación automática se aplica al identificador de señal reconocido; expresiones arbitrarias que lean señales requieren una región dinámica explícita con <code>@dynamic</code>.
- La actualización de la UI está cubierta por el runtime de navegador del proyecto. El runtime de bajo nivel, por sí solo, no muta el DOM.

## Pruebas y referencia

La implementación está en <code>crates/rustflow/src/reactivity.rs</code>; la semántica detallada y los casos borde están en [REACTIVITY_SEMANTICS.md](REACTIVITY_SEMANTICS.md) y la verificación de regresión/benchmarks en [REACTIVITY_VERIFICATION.md](REACTIVITY_VERIFICATION.md). La guía para programadores está en [Reactividad](../website/src/content/docs/concepts/reactivity.md), y el ejemplo funcional del navegador en [examples/browser](../examples/browser/).

No interpretes microbenchmarks o pruebas de regresión como prueba matemática de exhaustividad ni como garantía de rendimiento en todos los entornos.
