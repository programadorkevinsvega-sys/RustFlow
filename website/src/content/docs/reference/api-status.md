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
| CLI <code>rustflow new/dev/build</code> y generadores | Experimental disponible | Scaffold de servidor nativo; delega a Cargo y no incluye file watcher/hot reload. Los modelos siguen siendo structs y las migraciones son plantillas; la integración SQLx de la Fase 6.5 añade pool y runner de migraciones por separado. |
| <code>rustflow_server::AppConfig</code> | Experimental disponible | Valida perfil, IP y puerto. Desarrollo/test: proceso > <code>.env.&lt;perfil&gt;</code> > <code>.env</code> > defaults; producción exige variables del proceso y no lee dotenv. No almacena secretos. |
| Extracción tipada de formularios/JSON/query y errores 422 | Experimental disponible en la Fase 6.3 | La aplicación implementa `ValidateInput`; los mensajes son salida pública. 400/415 son errores de decodificación/medio y 413 aplica al límite de cuerpo. |
| `CsrfProtection` | Experimental, opt-in | Token HMAC con expiración; debe conectarse a mutaciones autenticadas por cookies. No implementa sesiones, identidad ni autorización. |
| `Request::redirect_back` | Experimental disponible en la Fase 6.3 | Solo acepta fallback local y Referer del mismo origen; un Referer externo usa el fallback. |
| Sesiones server-side, identidad y guards de autenticación/autorización | Experimental disponible (Fase 6.4, integrada en `main`) | `SessionManager`, `SessionContext`, `AuthIdentity`, `route_protected`, `route_authorized`; sin proveedor de usuarios ni almacenamiento durable. [Guía de sesiones](/guides/sessions-and-access/). |
| Server actions | No disponible | No forma parte del servidor actual. |
| Pool SQLx para SQLite/PostgreSQL y consultas parametrizadas | Experimental en la Fase 6.5 | `DatabaseConfig`, `DatabasePool` y reexport de `rustflow_server::sqlx`; no hay ORM ni capa de repositorios. [Guía de base de datos](/guides/database/). |
| Transacciones y migraciones SQLx | Experimental en la Fase 6.5 | `begin_transaction`, `run_migrations`, `rollback_migrations_to`; rollback explícito y condicionado a archivos `.down.sql`. [Guía de base de datos](/guides/database/). |
| MySQL y MariaDB | Planificado, no disponible (Fase 6.6) | Requiere driver, configuración, compatibilidad de dialecto y CI de integración. |
| MongoDB | Planificado, no disponible (Fase 6.6) | Requiere un adaptador documental separado de SQLx `AnyPool`. |
| ORM SQL para SQLite/PostgreSQL/MySQL/MariaDB | Planificado, no disponible (Fase 6.7) | No hay modelos, derives ni constructor de consultas ORM en la API actual. |
| Mapeo documental/ODM para MongoDB | Planificado, no disponible (subfase 6.7.3) | Contrato BSON/documentos separado del ORM SQL. |

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

- [Primera aplicación con el CLI](/tutorials/first-cli-app/)
- [Guía del CLI y generadores](/guides/cli/)
- [Configuración por entornos](/guides/configuration/)
- [Base de datos, SQLx y migraciones](/guides/database/)
- [Empezar con RustFlow](/getting-started/setup/)
- [Estado y reactividad](/concepts/reactivity/)
- [Crear componentes](/guides/components/)
- [Servidor HTTP](/guides/server/)
- [Formularios y validación](/guides/forms-validation/)
- [SSR, documentos HTML e hidratación](/guides/ssr/)
- [Componer páginas con layouts](/guides/layouts/)
- [Navegación cliente y formularios](/guides/navigation/)
- [Tutorial de página de producto](/tutorials/product-page/)
- [Roadmap completo](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md)
