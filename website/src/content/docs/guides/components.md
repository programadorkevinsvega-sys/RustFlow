---
title: Crear componentes
description: API actual de componentes RustFlow, vistas tipadas y runtime de navegador.
---

> **Estado: prototipo en desarrollo.** RustFlow ya tiene un subconjunto ejecutable de la API React-like, un runtime WASM con regiones dinámicas y un ejemplo de navegador. La Fase 3 permanece abierta porque la reconciliación completa de componentes y la separación diff/commit siguen pendientes.

## API disponible hoy

La reactividad sigue basada en señales ligadas a scopes, no en celdas indexadas por el orden de llamada de hooks. signal, use_state y use_effect requieren un scope propietario activo durante el setup de un componente. Signal proporciona get(), set(value) y update(|value| ...); al disponer el scope, la señal deja de ser válida. Los efectos se limpian antes de cada rerun y al disponer su propietario.

Un subconjunto funcional de la API es:

    use rustflow::prelude::*;

    #[rustflow::component]
    fn Counter(initial: i32) -> impl IntoView {
        let count = signal(initial);
        let increment = count.clone();

        rustflow::view! {
            <div class="counter">
                <p>
                    "Contador: "
                    <span data-testid="counter-value">{count}</span>
                </p>
                <button on:click={move |_| increment.update(|n| *n += 1)}>
                    "Sumar"
                </button>
            </div>
        }
    }

La macro genera la definición del componente y una función de registro. Para registrarlo explícitamente:

    let mut registry = rustflow::components::ComponentRegistry::new();
    register_counter(&mut registry)?;

El ejemplo de referencia completo —incluido el montaje DOM, el bundle WASM y la lista de tareas— está en [examples/browser](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/examples/browser).

La interpolación de un identificador que contiene una señal crea una región dinámica reactiva automáticamente. Las expresiones arbitrarias que lean señales no se convierten automáticamente en efectos; usa la sintaxis explícita de región dinámica con @dynamic para esas expresiones mientras el compilador no pueda inferirlas con seguridad.

### Límites actuales de #[component]

La macro admite parámetros con nombres simples y tipos explícitos. El conjunto implementado es String, bool, i32, i64, u32, usize, f64 y EventHandler (incluido el tipo cualificado rustflow_view::EventHandler). Rechaza props faltantes, desconocidos o de tipo incorrecto con errores durante la construcción/montaje. Genera register_nombre(&mut registry) para registro explícito; no es una macro de derivación genérica de structs.

Los callbacks de componentes se expresan como props tipados usando la convención on:increment; se almacenan como un callback tipado y no como un atributo HTML on*.

## Vocabulario actual y planeado

| Concepto | API RustFlow | Estado |
| --- | --- | --- |
| Señal reactiva | signal(value), Signal::get/set/update | Disponible dentro de un scope propietario |
| Estado local familiar | use_state(initial) | Disponible; devuelve Signal |
| Efecto | use_effect(...) | Disponible; dependencias detectadas leyendo señales |
| Cálculo memoizado | use_memo | Planeado |
| Referencia estable | use_ref | Planeado |
| Contexto tipado | use_context | Planeado |
| Evento de DOM | on:click, on:input, etc. | Disponible con callbacks tipados |
| Clave de lista | key / View::keyed_list | Disponible; la reconciliación DOM compatible conserva identidad |

Los nombres son inspiración de vocabulario, no promesa de compatibilidad semántica con React. RustFlow no adopta el modelo de hooks por orden de llamada. Véase [ADR 0004](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0004-react-like-component-authoring.md) y el [roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md).

## DOM y seguridad

El adaptador aplica atributos y propiedades mediante APIs DOM, valida nombres y restringe esquemas URL; no inserta el árbol como HTML crudo. Los eventos que burbujean usan delegación en el límite del montaje y otros eventos conservan listeners directos. La limpieza está ligada a los rangos, scopes y ciclo de vida.

El renderer parchea texto y elementos compatibles, tiene scopes/effects por hijo dinámico y conserva nodos compatibles de una lista al cambiar su orden por clave. **Sigue incompleto** el diff/commit explícito y la preservación de identidad de componentes al cambiar descriptores/props: algunos árboles con componentes aún usan reemplazo de rango completo. Por eso no se declara cerrada la Fase 3 aunque el bundle y los E2E actuales pasen.

## Compilar y ejecutar el ejemplo de navegador

En examples/browser:

    npm install
    npm run build
    npm run check:bundle
    npm run dev

Para las pruebas E2E, instala Chromium con npx playwright install --with-deps chromium y ejecuta npm run test:e2e. La CI realiza la compilación WASM, construye los assets de producción, mide WASM y ejecuta las pruebas de Chromium. Consulta el [README del ejemplo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) para el setup y troubleshooting.
