---
title: Crear componentes
description: API actual de componentes RustFlow, vistas tipadas y runtime de navegador.
---

> **Estado: prototipo en desarrollo; integración de Fase 3 pendiente.** La rama apilada #26 sobre la PR #25 ya implementa reconciliación explícita diff/commit, actualización de props sin reinicializar el setup, preservación de estado/identidad cuando componente y clave son compatibles, y cleanup de rangos sustituidos. La Fase 3 permanece abierta en `main` hasta que ambas PR se revisen e integren.

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

El renderer calcula un `ReconciliationPlan` antes del commit DOM; valida las vistas candidatas y prepara fuera del DOM las ramas nuevas o incompatibles. Durante el commit conserva nodos compatibles, actualiza props a través del render del componente sin recrear su scope, sigue la clave del `KeyedChild` para componentes de lista y limpia las instancias/scope/efectos/listeners salientes. Las pruebas del navegador cubren cambios de props, identidad DOM, reordenación keyed, sustitución por cambio de clave y cleanup. El gate funcional de la rama requiere CI verde en el HEAD exacto; la Fase 3 sigue abierta en `main` hasta que se integren las PR apiladas #26 y #25.

## Compilar y ejecutar el ejemplo de navegador

En examples/browser:

    npm install
    npm run build
    npm run check:bundle
    npm run dev

Para las pruebas E2E, instala Chromium con npx playwright install --with-deps chromium y ejecuta npm run test:e2e. La CI realiza la compilación WASM, construye los assets de producción, mide WASM y ejecuta las pruebas de Chromium. Consulta el [README del ejemplo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) para el setup y troubleshooting.
