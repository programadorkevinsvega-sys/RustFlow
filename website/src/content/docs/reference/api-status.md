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
| <code>use_memo</code> | Implementado en la rama Phase 7.2; pendiente de CI/merge | Memo eager, dependencias dinámicas y comparación de salida <code>PartialEq</code>. |
| <code>use_ref</code> | Implementado en la rama Phase 7.2; pendiente de CI/merge | Storage no reactivo ligado al owner; DOM refs pendientes. |
| <code>Context&lt;T&gt;</code>, <code>provide_context</code>, <code>use_context</code> | Implementado en la rama Phase 7.2; pendiente de CI/merge | Token con identidad propia y resolución del provider más cercano. |
| Tracking reactivo a través de <code>await</code> | No soportado | Los efectos actuales son síncronos; no hay Resource/Suspense/loading boundary. |
| Referencias seguras a nodos DOM | Pendiente | <code>use_ref</code> almacena valores Rust; no expone elementos del renderer. |
| Error boundaries de componentes | No soportado | Los errores iniciales/SSR se propagan; el DOM reactivo expone <code>take_error</code> al mantener el último rango bueno. |

## HTTP y servidor

| Capacidad | Estado | Alcance/notas |
|---|---|---|
| Rutas, handlers async, path/query | Disponible | La aplicación registra la tabla de rutas. |
| Decodificación de JSON | Disponible | Helpers básicos; inputs inválidos generan errores HTTP. |
| Respuestas texto/HTML/JSON/redirect/download/status | Disponible | <code>Response::html</code> envía la cadena sin escape. |
| Middleware global/grupo/ruta y grupos | Disponible | Composición ordenada con posibilidad de short-circuit. |
| Request ID, eventos/spans tracing, límites y timeouts | Disponible | Subscriber/exporter configurado por la app. |
| `TracingConfig`, `LogLevel` y `RUSTFLOW_LOG_LEVEL` (Phase 7.1 / PR #68) | Experimental disponible, integrada en `main` | Niveles `error`, `warn`, `info`, `debug`, `trace`; resuelve la configuración mediante `AppConfig`. La aplicación sigue siendo responsable de inicializar su subscriber/exporter global. [Guía de configuración](/guides/configuration/). |
| `ErrorReporter` y `ErrorReport` (Phase 7.1 / PR #68) | Experimental disponible, integrada en `main` | Reporta respuestas HTTP 5xx con request ID, método, plantilla de ruta, status y categoría allowlisted; ejecución asíncrona aislada con timeout. No expone mensajes ni datos de petición. [Guía de errores y seguridad](/guides/errors-and-security/). |
| Health/readiness optativos (Phase 7.1 / PR #68) | Experimental disponible, opt-in | `/health/live` y `/health/ready`; la aplicación registra checks explícitos, concurrentes y con timeout individual. Fallos producen 503 genérico; sin checks registrados, readiness se considera disponible. [Guía de servidor](/guides/server/). |
| Graceful shutdown con deadline (Phase 7.1 / PR #68) | Experimental disponible, integrado en `main` | Deadline configurable (30 s por defecto); al vencer, cancela pipelines y futures de handlers activos, incluidos los que esperan concurrencia, espera que se liberen las guardas y retorna `ServerError::ShutdownTimedOut`. [Guía de servidor](/guides/server/). |
| TLS local y política de proxies confiables | Disponible | No sustituye al hardening del despliegue. |
| CLI <code>rustflow new/dev/build</code> y generadores | Experimental disponible | `new` usa la plantilla `server` por defecto o `--template crud` para generar una aplicación SQL CRUD de demostración. La plantilla CRUD utiliza ORM, formularios, transacciones, guards, CSRF y SSR; sesión volátil/login demo, no production-ready. `generate model` sigue generando structs planos. Sin file watcher/hot reload. |
| <code>rustflow_server::AppConfig</code> | Experimental disponible | Valida perfil, IP y puerto. Desarrollo/test: proceso > <code>.env.&lt;perfil&gt;</code> > <code>.env</code> > defaults; producción exige variables del proceso y no lee dotenv. No almacena secretos. |
| Extracción tipada de formularios/JSON/query y errores 422 | Experimental disponible en la Fase 6.3 | La aplicación implementa `ValidateInput`; los mensajes son salida pública. 400/415 son errores de decodificación/medio y 413 aplica al límite de cuerpo. |
| `CsrfProtection` | Experimental, opt-in | Token HMAC con expiración; debe conectarse a mutaciones autenticadas por cookies. No implementa sesiones, identidad ni autorización. |
| `Request::redirect_back` | Experimental disponible en la Fase 6.3 | Solo acepta fallback local y Referer del mismo origen; un Referer externo usa el fallback. |
| Sesiones server-side, identidad y guards de autenticación/autorización | Experimental disponible (Fase 6.4, integrada en `main`) | `SessionManager`, `SessionContext`, `AuthIdentity`, `route_protected`, `route_authorized`; sin proveedor de usuarios ni almacenamiento durable. [Guía de sesiones](/guides/sessions-and-access/). |
| Server actions | No disponible | No forma parte del servidor actual. |
| Pools SQLx tipados para SQLite/PostgreSQL/MySQL/MariaDB | Experimental disponible en Phase 6.7.2 (fusionada en main mediante PR #59) | `DatabaseConfig`, `DatabasePool`, accessors `sqlx_*_pool()` y reexport `rustflow_server::sqlx`; el handle tipado se comparte con SeaORM sin duplicar pool. [Guía](/guides/database/) · [Matriz](/guides/database/#matriz-de-versiones-verificadas). |
| Transacciones y migraciones SQLx | Experimental en las fases 6.5/6.6.1 | `begin_transaction`, `run_migrations`, `rollback_migrations_to`; rollback explícito con `.down.sql`, guardia de dialecto para MySQL/MariaDB y sin promesa de atomicidad total para DDL MySQL-family. [Guía](/guides/database/). |
| MySQL y MariaDB | Experimental integrados; targets actuales de 6.6.3 verificados | Driver SQLx 0.8.6 `mysql`, URL `mysql://`, pool, binding, transacciones y guardia de dialecto. MySQL 8.4.13 LTS y MariaDB 11.4.13 LTS pasaron las aserciones exactas e integración en Rust 1.85/stable; `DatabaseBackend::MySql` no identifica el producto servidor. [Matriz de compatibilidad](/guides/database/#matriz-de-versiones-verificadas). |
| Adaptador documental MongoDB | Experimental disponible en `main` (6.6.2); matrix consolidada de 6.6.3 verificada | `MongoConfig`, `MongoDatabase`, `DocumentBootstrap`, `DocumentIndex`, BSON helpers y acceso nativo al driver oficial 3.6.0 (MSRV 1.85). MongoDB Server 7.0.45 pasó el contrato real en Rust 1.85/stable. Solo URI `mongodb://`; no usa SQLx `AnyPool` ni migraciones SQL; el bootstrap gestiona colecciones/índices, no migraciones de esquema. [Guía](/guides/database/) · [Matriz](/guides/database/#matriz-de-versiones-verificadas). |
| <code>ModelDiagnostics</code> | Experimental disponible (Phase 6.7.1) | Valida metadatos del subconjunto acordado; complementa, no reemplaza, los derives <code>DeriveEntityModel</code>/<code>DeriveRelation</code> de SeaORM. [Guía](/guides/database/#foundation-seleccionado-para-sql-orm). |
| ORM SQL para SQLite/PostgreSQL/MySQL/MariaDB | Experimental implementado; Phase 6.7.2 integrada en main mediante PR #59 | `EntityRepository` CRUD/bulk, SeaORM query composition, loaders explícitos y transacciones mixtas sobre pools SQLx compartidos. Pruebas de cuatro motores y 8/8 checks requeridos aprobados en el HEAD verificado. Límites: codec decimal SQLite vía `f64`, `u64` solo MySQL/MariaDB y semánticas upsert/RETURNING específicas. [Guía](/guides/database/) · [Contrato](/guides/database/#roadmap-de-persistencia). |
| Extensiones ORM de relaciones/carga (Phase 6.7.4) | Experimental, integrada en `main` mediante PR #62 | `RelationLoadOptions<E>` compone filtros/orden sobre `Select<E>`; los loaders existentes aceptan `EntityOrSelect`; `load_many_projected` proyecta relaciones has-many a DTOs con extractores de clave coincidentes; `RelationGraph` compone ramas `with_many`, `with_one`, `with_many_to_many` y hojas proyectadas (profundidad 8 por defecto, máximo 16). Helpers: `attach_one_to_many`, `detach_one_to_many` y `replace_one_to_one`. Límites: limit/offset afecta al batch completo, no hay top-N por padre; las proyecciones son hojas; no hay lazy loading, deletes/cascades implícitos ni commit/rollback automático. PR #62 pasó los checks requeridos en su HEAD final `28218456db8ac323731054e291023a0765474125` y se fusionó como `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`. [Guía](/guides/database/) · [ADR 0022](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0022-advanced-sql-relation-loading.md). |
| ODM MongoDB | Experimental, integrado en `main` mediante Phase 6.7.3 / PR #60 | Trait `DocumentModel` (sin derive procedural), BSON/Serde, ObjectId y custom IDs explícitos, `DocumentRepository`, filtros/updates BSON, CRUD/bulk, paginación, DTOs de proyección, agregaciones, bootstrap/index verification, documentos embebidos y operaciones explícitas con sesiones. El HEAD final `659f1750d611e57315263216167d43375da23574` pasó Rust 1.85/stable, MongoDB 7.0.45 replica set (commit/abort), standalone (rechazo transaccional), WASM, Browser E2E, docs, governance y dependency review. Squash merge commit `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. La API continúa experimental/pre-1.0. [Guía ODM](/guides/mongodb-odm/) · [ADR 0021](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0021-mongodb-odm-architecture.md). |
| Gate global ORM/ODM de calidad y compatibilidad (Phase 6.7.6) | Completado en `main` mediante PR #66 | Pool exhaustion/recuperación y escrituras concurrentes en cuatro motores SQL, regresiones de valores SQL/BSON, migraciones PostgreSQL up/reapply/down, benchmark smoke ORM/ODM y aplicación CRUD con relación, filtros y paginación verificados. HEAD final `b049d96491e2b23bfe794ac3c0ed9abb68f7e493`; merge `6046ae7ca60843d145476177e698162b57939c68`. La fase 6.7 está completa, pero ORM/ODM siguen siendo APIs experimentales y pre-1.0. [Guía de base de datos](/guides/database/) · [Contrato ORM](/guides/database/#roadmap-de-persistencia) |


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
