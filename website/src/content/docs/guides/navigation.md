---
title: Navegación cliente y formularios
description: Añade navegación optativa y formularios progresivos sin perder el comportamiento HTML nativo.
---

RustFlow mantiene SSR como comportamiento base. <code>rustflow_dom::ClientNavigation</code> es un adaptador opt-in que mejora ciertos enlaces y formularios después de que la aplicación haya montado el runtime y, cuando proceda, completado la hidratación. No es un router general ni una navegación automática para todas las URLs.

## 1. Qué problema resuelve

Una navegación HTML convencional pide una nueva página al servidor y reemplaza el documento. Con la navegación cliente optativa, los clics elegibles pueden actualizar la ruta reactiva y el historial sin una navegación de documento completa. Si el enlace no es elegible, falla la petición o la aplicación no reconoce la ruta, se conserva el fallback nativo o se recarga para que el servidor atienda la URL.

Este modelo es apropiado cuando la aplicación controla un conjunto pequeño de rutas y puede mantener el estado de ruta explícitamente. No sustituye un router con reconocimiento automático de rutas, loaders, layouts ni server actions.

## 2. Instalar la navegación

La instalación recibe el elemento raíz, una instancia de <code>ReactiveMountedDom</code> ya montada y un callback que resuelve la ruta. La aplicación conserva la definición de rutas y actualiza su propia señal cuando acepta una ruta:

~~~rust
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
~~~

Este fragmento muestra el contrato del callback; <code>target</code>, <code>mounted</code> y <code>route_signal</code> deben existir en la aplicación real. Devuelve <code>Ok(true)</code> solo después de actualizar el estado de una ruta soportada. Devuelve <code>Ok(false)</code> para que la navegación convencional resuelva lo que el cliente no administra. Mantén viva la instancia mientras quieras que los listeners sigan activos y deja que se cancele al salir de su ciclo de vida.

## 3. Qué enlaces puede interceptar

La API solo intercepta clics primarios sin modificadores en enlaces HTTP(S) del mismo origen, dentro del root de navegación y con destino compatible. No intercepta, entre otros:

- enlaces externos o protocolos que no sean HTTP(S);
- clics con Ctrl/Meta/Shift/Alt o botón distinto del primario;
- fragmentos de URL;
- descargas;
- <code>rel="external"</code>, una exclusión explícita o targets diferentes de <code>_self</code>.

Antes de cambiar el historial, valida una respuesta GET satisfactoria con tipo de contenido HTML, estructura de documento completo y URL final del mismo origen. La respuesta se valida; no se inserta su HTML como una cadena arbitraria en el DOM vivo. Las transiciones aceptadas usan History API; atrás/adelante pasa por <code>popstate</code>. Los errores de red, HTTP, ruta, render o History API producen fallback a navegación de documento o recarga.

## 4. Mejorar un formulario GET

El formulario debe solicitarlo de forma explícita con <code>data-rustflow-enhance</code>. Por ejemplo:

~~~html
<form method="get" action="/search" data-rustflow-enhance>
  <label for="q">Buscar productos</label>
  <input id="q" name="q" type="search">
  <button type="submit">Buscar</button>
</form>
~~~

La mejora requiere origen compatible, método GET, codificación <code>application/x-www-form-urlencoded</code>, target vacío o <code>_self</code>, validación HTML correcta y controles escalares admitidos. El estado pendiente evita envíos duplicados durante la petición.

## 5. Qué conserva el envío nativo

Los formularios sin el atributo de opt-in, los métodos POST/PUT/PATCH/DELETE, los uploads, los selectores múltiples, controles de envío con nombre, targets alternativos, codificaciones no admitidas y otras combinaciones incompatibles mantienen el envío normal del navegador. No hay procesamiento de server actions ni cargas de archivos dentro de esta mejora.

El HTML del servidor debe seguir siendo útil con JavaScript desactivado o si la inicialización de RustFlow falla. No construyas una página que solo funciona después de la mejora cliente.

## 6. Relación con hidratación

La hidratación de DOM es también optativa. Debe completarse con una vista compatible antes de instalar navegación cliente. Si hay una discrepancia, la política conservadora conserva el root SSR completo como HTML estático y dispone los recursos candidatos; no intenta reparar automáticamente subregiones. No instales navegación sobre una aplicación cuyo montaje/hidratación haya fallado.

Consulta [SSR y documentos HTML](/guides/ssr/) para el ciclo de vida de página y [el ejemplo del navegador](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/examples/browser) para las pruebas E2E de historial, formularios nativos, opt-in y fallbacks.

La demo interactiva y sus tests están explicados paso a paso en el [tutorial de interfaz reactiva de tareas](/tutorials/reactive-todo/). Para verificar historial, formularios compatibles y fallbacks, sigue [Probar y depurar aplicaciones](/guides/testing/).

## Límites

La aplicación tiene que resolver rutas y actualizar la vista. Esta API de navegación cliente no descarga e inserta HTML arbitrario como mecanismo de routing, no genera rutas a partir del filesystem, no aporta autenticación/sesiones ni procesa mutaciones de formularios, y no reemplaza el routing del servidor. Para la extracción y validación tipada del lado servidor, los errores 422 y el middleware CSRF optativo en mutaciones autenticadas mediante cookies, consulta la [guía de formularios y validación](/guides/forms-validation/). La API es pre-1.0 y puede cambiar.
