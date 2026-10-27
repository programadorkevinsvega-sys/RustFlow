---
title: Estado y reactividad
description: Aprende señales, efectos, dependencias y limpieza con la API disponible de RustFlow.
---

La reactividad permite que un cambio de datos actualice el trabajo que depende de él. RustFlow utiliza **señales** y seguimiento de dependencias; no almacena el estado en celdas asociadas al orden de llamadas de hooks como hace React.

Esta API es un prototipo y requiere un scope propietario activo para las funciones de componentes.

## 1. Crear y actualizar estado dentro de un componente

~~~rust
use rustflow::prelude::*;

#[rustflow::component]
fn Counter(initial: i32) -> impl IntoView {
    let count = use_state(initial);
    let count_for_button = count.clone();

    rustflow::view! {
        <section>
            <p>"Total: " {count}</p>
            <button on:click={move |_event| {
                count_for_button.update(|value| *value += 1);
            }}>
                "Añadir uno"
            </button>
        </section>
    }
}
~~~

En este ejemplo:

- <code>use_state(initial)</code> crea y devuelve un <code>Signal&lt;i32&gt;</code> ligado al scope del componente.
- <code>{count}</code> interpola la señal como hijo reactivo porque es un identificador de señal reconocido.
- El botón tiene un callback tipado. El clon de la señal permite mover un handle al callback sin perder el handle utilizado por el render.
- <code>update</code> modifica el valor y notifica a los suscriptores cuando corresponde; el runtime DOM actualiza la región afectada.
- Cuando el componente se reemplaza o desmonta, su scope y sus recursos ligados se limpian.

También existe <code>signal(initial)</code>, que crea la misma clase de señal owner-scoped. Usa el nombre que haga más claro si estás describiendo estado de componente o una señal general. Ambos requieren un owner activo.

## 2. Ejecutar trabajo reactivo con use_effect

<code>use_effect</code> registra un callback que vuelve a ejecutarse cuando cambian las señales leídas durante una ejecución. Las dependencias se deducen de las lecturas; no se proporciona un array de dependencias al estilo React.

~~~rust
#[rustflow::component]
fn LoggedCounter(initial: i32) -> impl IntoView {
    let count = use_state(initial);
    let observed = count.clone();

    use_effect(move || {
        // Se vuelve a ejecutar cuando cambia la señal observada.
        let current = observed.get();
        web_sys::console::log_1(&format!("count = {current}").into());
    });

    rustflow::view! {
        <p>"Valor actual: " {count}</p>
    }
}
~~~

Este ejemplo está pensado para el adaptador WebAssembly porque utiliza <code>web_sys</code>. En código multiplataforma, sustituye el efecto externo por una acción adecuada al target. Los efectos se ejecutan inicialmente y después según sus dependencias activas.

No pongas lógica con efectos secundarios directamente en una función de render que se repite. Usa un efecto owner-scoped o un callback de evento según corresponda.

## 3. Batching y runtime explícito

Para controlar un runtime fuera de un componente, la API de bajo nivel permite crear el runtime y manejar su ciclo de vida:

~~~rust
use rustflow::reactivity::{Runtime, RuntimeError};

fn example() -> Result<(), RuntimeError> {
    let runtime = Runtime::new();
    let count = runtime.signal(0_i32)?;
    let observed = count.clone();
    let effect = runtime.effect(move || println!("count = {}", observed.get()))?;

    runtime.batch(|| {
        count.set(1);
        count.set(2);
        count.set(3);
    })?;

    effect.dispose()?;
    runtime.dispose()?;
    Ok(())
}
~~~

El batch agrupa las invalidaciones hasta que termina el bloque exterior. El ejemplo de bajo nivel sirve para entender el scheduler, escribir pruebas y construir integraciones que necesiten ciclo de vida explícito; no es la forma de montar una interfaz web completa.

## 4. Limpieza y scopes

<code>on_cleanup(callback)</code> registra trabajo que debe ejecutarse al disponer el scope propietario. En un componente, úsalo para liberar recursos que tú mismo creaste, por ejemplo una suscripción gestionada por la aplicación. Los efectos tienen limpieza de scope y se vuelven a evaluar según las dependencias.

La limpieza es síncrona. No asumas que el runtime cancela automáticamente tareas arbitrarias externas ni que mantiene seguimiento reactivo a través de <code>await</code>.

## 5. Cómo se calculan las dependencias

En cada ejecución correcta de un efecto, las lecturas de señales determinan las dependencias activas. Si una rama deja de leer una señal, esa dependencia anterior se elimina. Las actualizaciones se programan en orden FIFO y los efectos pendientes se deduplican durante el vaciado. Un límite de ejecuciones ayuda a detectar ciclos que no convergen.

## APIs disponibles y pendientes

| API | Estado y alcance |
|---|---|
| <code>signal(value)</code>, <code>use_state(initial)</code> | Disponibles durante el setup de un componente con owner activo; devuelven <code>Signal&lt;T&gt;</code>. |
| <code>Signal::get/set/update</code> | Disponible; las operaciones ergonómicas pueden hacer panic ante un handle inválido. |
| <code>use_effect(callback)</code> | Disponible con seguimiento de dependencias y propiedad del scope. |
| <code>on_cleanup(callback)</code> | Disponible para limpieza del scope propietario. |
| <code>Runtime::new/signal/effect/batch</code> | API de bajo nivel con ciclo de vida explícito. |
| <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> | Todavía no implementados. |
| Efectos asíncronos / tracking a través de <code>await</code> | No soportados por el runtime actual. |

## Casos de uso y cuándo no usarlo

**Úsalo** para estado local de un widget, valores que determinan una región DOM, efectos síncronos asociados al estado, listas que cambian y limpieza de recursos ligados al componente.

**No asumas** que proporciona store global concurrente, persistencia, fetching de datos, cancelación automática de tareas async ni contexto/provider. La UI reactiva funciona dentro del runtime del navegador; el runtime aislado no modifica el DOM por sí solo.

## Siguiente lectura

- [Crear componentes](/guides/components/)
- [Modelo de vistas y componentes](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/VIEW_MODEL.md)
- [Semántica del runtime](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_SEMANTICS.md)
- [Verificación y benchmarks](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/REACTIVITY_VERIFICATION.md)
