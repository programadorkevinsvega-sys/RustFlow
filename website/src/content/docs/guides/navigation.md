---
title: Navegación cliente y formularios
description: Mejora optativa de enlaces e inputs sin perder SSR ni el envío HTML nativo.
---

# Navegación cliente y formularios

RustFlow mantiene SSR como comportamiento base. El cliente puede mejorar solo las interacciones elegibles después de montar o hidratar correctamente la vista inicial; no reemplaza el servidor ni asume que toda URL es una ruta SPA.

## Contrato de navegación

La API experimental `rustflow_dom::ClientNavigation` se instala con el elemento raíz, un `ReactiveMountedDom` existente y un callback de rutas de la aplicación. El callback recibe ruta y query (por ejemplo, `/articles?page=2`), actualiza el estado reactivo cuando reconoce una ruta y devuelve `Ok(true)`; devuelve `Ok(false)` sin cambiar estado para rutas que el cliente no administra.

La API comprueba la respuesta GET antes de actualizar vista e historial: estado HTTP satisfactorio, `text/html`, documento completo y URL final del mismo origen. Una transición aceptada usa `history.pushState`; atrás/adelante pasa por `popstate`. Si falla la red, el estado HTTP, la compatibilidad de respuesta, el resolver, el render o la History API, RustFlow hace una navegación completa o recarga para que el servidor pueda resolver la petición y emitir su respuesta normal.

```rust
use rustflow_dom::{ClientNavigation, DomError};

let navigation = ClientNavigation::install(target, mounted, move |path| {
    match path.split('?').next().unwrap_or("/") {
        "/" | "/about" => {
            route_signal.set(path.to_owned());
            Ok(true)
        }
        _ => Ok(false),
    }
})?;
```

Intercepta únicamente clics primarios sin modificadores en enlaces HTTP(S) del mismo origen, dentro del root, sin fragmentos, descargas, `rel="external"`, opt-out o destino distinto de `_self`. Los enlaces externos, protocolos alternativos, clics modificados, fragmentos y descargas se dejan al navegador.

## Formularios y mejora progresiva

Un formulario debe optar explícitamente por la mejora mediante `data-rustflow-enhance`. El subconjunto admitido es GET, origen compatible, `application/x-www-form-urlencoded`, destino vacío/`_self`, validación HTML correcta y campos escalares compatibles. El estado pendiente bloquea envíos duplicados mientras se procesa la petición.

Los formularios sin opt-in y los formularios POST/PUT/PATCH/DELETE, con cargas de archivo, selectores múltiples, controles de envío con nombre, destinos alternativos o codificaciones no soportadas conservan su envío HTML nativo. Una respuesta no satisfactoria o un error de red no se muestra como éxito: la navegación cae al servidor.

## Límites

- El documento SSR debe contener HTML útil aunque JavaScript esté deshabilitado o RustFlow no se inicialice.
- La hidratación sigue siendo optativa y validada. No instales navegación cliente si falla la hidratación; la política de ADR 0010 conserva el root SSR como contenido estático.
- La API no es un DSL de rutas y no descarga ni inserta HTML arbitrario dentro del DOM. La aplicación mantiene un resolver explícito y actualiza la vista a través del runtime reactivo actual.
- No implementa server actions, autenticación, sesiones, CSRF ni infraestructura de carga de archivos.
