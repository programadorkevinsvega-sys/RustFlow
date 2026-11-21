---
title: Errores en UI, SSR y operaciones reactivas
description: Comprende qué errores propaga RustFlow, qué puede recuperar el renderer y qué límites aún no ofrece.
---

RustFlow distingue errores de construcción/render del árbol, errores del runtime reactivo, errores de actualización del DOM y errores HTTP. La API experimental <code>View::error_boundary(child, fallback)</code> permite aislar fallos locales de componentes/UI; no reemplaza a los errores HTTP ni captura infraestructura del servidor.

## 1. Errores de construcción y render inicial

Las APIs que construyen vistas pueden devolver <code>ViewError</code>. Un componente puede devolver <code>ComponentError</code>; el host/renderer propaga el fallo de setup o render. El montaje inicial del navegador y el render SSR devuelven un error al llamador en vez de sustituir silenciosamente el componente por una vista vacía.

Valida datos y tipos de props antes de construir la vista. No captures un error y lo conviertas en una lista vacía si el resultado altera el significado de la pantalla.

## 2. Errores en actualizaciones reactivas del DOM

<code>DomRenderer::mount_reactive</code> mantiene una región con su scope. Si una actualización de vista falla, el renderer conserva el último rango DOM correcto y almacena el último <code>DomError</code> en <code>ReactiveMountedDom</code>. El consumidor puede consultar ese diagnóstico mediante <code>take_error()</code>.

Esto conserva el último rango montado incluso fuera de una boundary. Dentro de un <code>ErrorBoundary</code>, un fallo del protected subtree usa el fallback declarado y la próxima actualización exitosa puede volver a montar el contenido protegido.

## 3. Panics de efectos y limpieza

El scheduler restaura sus campos activos y las invariantes del grafo cuando un efecto hace panic. Se desconectan las dependencias del run fallido, se ejecutan los cleanups registrados y el panic se propaga bajo una estrategia de unwind. Un panic durante la primera ejecución de un efecto nuevo dispone ese efecto antes de reanudar el panic.

No uses panics como control de flujo normal. Prefiere errores de dominio tipados como valores de señales/memos o propaga un error del componente al host. El comportamiento de panic de WebAssembly y de builds con panic-abort no es equivalente a una excepción recuperable del navegador.

## 4. Event handlers y SSR

Los callbacks de eventos pertenecen al runtime browser. RustFlow no promete que un panic arbitrario de handler sea capturado y convertido en un mensaje accesible. Valida entradas del usuario dentro del callback y evita operaciones que puedan hacer panic por datos no confiables.

En SSR, un error de setup, render o serialización se propaga a la ruta/llamador que realiza el render. Aísla cada solicitud con su propio runtime y convierte errores inesperados en la respuesta HTTP genérica correspondiente; no expongas detalles internos al cliente.

## 5. No confundir ErrorReporter HTTP con error boundaries

El <code>ErrorReporter</code> de Phase 7.1 pertenece al servidor HTTP y recibe metadatos allowlisted para resultados 5xx. No intercepta panics de un componente de navegador, no crea un fallback de UI y no enlaza errores de SSR a una boundary visual. Mantén esos contratos separados.

## 6. ErrorBoundary de componentes

~~~rust
let view = View::error_boundary(
    View::component("RiskyPanel", [], [], None)?,
    View::element("section", [View::text("We couldn't render this panel.")])?,
)?;
~~~

La API requiere exactamente dos hijos: el contenido protegido y el fallback. El fallback es texto/vistas escapadas; no acepta HTML crudo ni datos sensibles. En SSR se representa el fallback si el subtree protegido falla; en browser se ejecuta la misma separación y se limpia el scope protegido al entrar en fallback.

Una boundary captura fallos de setup/render locales y panics del subtree, pero no captura respuestas HTTP, autenticación, autorización, CSRF, server actions ni Resource::Error. Esos errores deben seguir su contrato server/resource y aparecer mediante estados seguros.

Las boundaries pueden anidar: la boundary más cercana captura el fallo. Si la actualización posterior ya puede construir el contenido protegido, la boundary reemplaza el fallback y vuelve a crear los scopes/efectos necesarios. Si el fallback mismo falla, el error se propaga; no existe un fallback invisible ni una política global de recuperación.
