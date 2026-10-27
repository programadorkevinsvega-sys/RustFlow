---
title: Referencia de disponibilidad de API
description: Tabla de capacidades RustFlow disponibles, limitadas o todavía planificadas.
---

Esta página es un índice de disponibilidad, no una promesa de compatibilidad estable. RustFlow es pre-1.0: valida siempre los ejemplos y la firma exacta contra la revisión del repositorio que utilices.

## Reactividad y componentes

| Capacidad | Estado | Alcance/notas |
|---|---|---|
| <code>Runtime::new</code>, señales, efectos, batching | Disponible | API de bajo nivel con ciclo de vida explícito. |
| <code>signal(value)</code>, <code>use_state(initial)</code> | Disponible | Requieren scope propietario activo; devuelven <code>Signal&lt;T&gt;</code>. |
| <code>use_effect</code>, <code>on_cleanup</code> | Disponible | Tracking síncrono de dependencias y limpieza owner-scoped. |
| Macro <code>view!</code> | Disponible | Gramática deliberadamente restringida y errores tipados. |
| Macro <code>#[rustflow::component]</code> | Disponible | Subconjunto de tipos de props y callbacks tipados. |
| Eventos DOM, reconciliación y listas keyed | Disponible en browser/WASM | Las pruebas E2E validan los casos implementados. |
| <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> | Pendiente | No inventes APIs equivalentes en ejemplos. |
| Tracking reactivo a través de <code>await</code> | No soportado | Los efectos actuales son síncronos. |

## HTTP y servidor

| Capacidad | Estado | Alcance/notas |
|---|---|---|
| Rutas, handlers async, path/query | Disponible | La aplicación registra la tabla de rutas. |
| Decodificación de JSON | Disponible | Helpers básicos; inputs inválidos generan errores HTTP. |
| Respuestas texto/HTML/JSON/redirect/download/status | Disponible | <code>Response::html</code> envía la cadena sin escape. |
| Middleware global/grupo/ruta y grupos | Disponible | Composición ordenada con posibilidad de short-circuit. |
| Request ID, eventos/spans tracing, límites y timeouts | Disponible | Subscriber/exporter configurado por la app. |
| TLS local y política de proxies confiables | Disponible | No sustituye al hardening del despliegue. |
| CLI y scaffolding | No disponible | No existen comandos propios <code>rustflow new/dev/build</code>. |
| Sesiones, auth, autorización, CSRF, server actions | No disponible | No forma parte del servidor actual. |
| Persistencia SQL/migraciones integradas | No disponible | No hay un modelo de base de datos integrado. |

## SSR, layouts e interacción cliente

| Capacidad | Estado | Alcance/notas |
|---|---|---|
| Serializar vistas a fragmentos HTML | Disponible | Escape y allowlists conservadoras de tags/URL. |
| Documentos completos <code>html_document</code> | Disponible | Genera doctype, lang, charset, title y body. |
| <code>HtmlPage</code> y <code>with_layout</code> | Disponible | Composición explícita con componentes registrados. |
| <code>SharedComponentCatalog</code> | Disponible | Compartir definiciones; runtime/estado de render por petición. |
| <code>HydrationData</code> | Disponible | Solo JSON acotado; el caller elige campos públicos. |
| Hidratación de DOM | Disponible, opt-in | Requiere HTML compatible; ante mismatch se conserva el root como estático. |
| <code>ClientNavigation</code> e History API | Disponible, opt-in | Callback de rutas de la aplicación; no es un router general. |
| Mejora progresiva de forms | Limitada | GET URL-encoded, controles escalares y opt-in explícito. |
| Uploads, mutaciones y server actions | No disponible en enhancement | Formularios incompatibles mantienen envío nativo. |
| Aplicación full-stack lista para producción | No disponible | Requiere integración, seguridad y validación adicional. |

## Dónde aprender

- [Empezar con RustFlow](/getting-started/setup/)
- [Estado y reactividad](/concepts/reactivity/)
- [Crear componentes](/guides/components/)
- [Servidor HTTP](/guides/server/)
- [SSR, documentos HTML e hidratación](/guides/ssr/)
- [Componer páginas con layouts](/guides/layouts/)
- [Navegación cliente y formularios](/guides/navigation/)
- [Tutorial de página de producto](/tutorials/product-page/)
- [Roadmap completo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md)
