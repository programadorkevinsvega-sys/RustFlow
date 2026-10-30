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
| Pool SQLx para SQLite/PostgreSQL y consultas parametrizadas | Experimental disponible (Fase 6.5); matrix de 6.6.3 verificada | `DatabaseConfig`, `DatabasePool` y reexport de `rustflow_server::sqlx`; sin ORM ni repositorios. El contrato probado cubre PostgreSQL 16.15 y SQLite 3.46.0 bundled en Rust 1.85/stable; la suite completa pasó en ambos toolchains. [Guía](/guides/database/) · [Matriz](/guides/database/#matriz-de-versiones-verificadas). |
| Transacciones y migraciones SQLx | Experimental en las fases 6.5/6.6.1 | `begin_transaction`, `run_migrations`, `rollback_migrations_to`; rollback explícito con `.down.sql`, guardia de dialecto para MySQL/MariaDB y sin promesa de atomicidad total para DDL MySQL-family. [Guía](/guides/database/). |
| MySQL y MariaDB | Experimental integrados; targets actuales de 6.6.3 verificados | Driver SQLx 0.8.6 `mysql`, URL `mysql://`, pool, binding, transacciones y guardia de dialecto. MySQL 8.4.13 LTS y MariaDB 11.4.13 LTS pasaron las aserciones exactas e integración en Rust 1.85/stable; `DatabaseBackend::MySql` no identifica el producto servidor. [Matriz de compatibilidad](/guides/database/#matriz-de-versiones-verificadas). |
| Adaptador documental MongoDB | Experimental disponible en `main` (6.6.2); matrix consolidada de 6.6.3 verificada | `MongoConfig`, `MongoDatabase`, `DocumentBootstrap`, `DocumentIndex`, BSON helpers y acceso nativo al driver oficial 3.6.0 (MSRV 1.85). MongoDB Server 7.0.45 pasó el contrato real en Rust 1.85/stable. Solo URI `mongodb://`; no usa SQLx `AnyPool` ni migraciones SQL; el bootstrap gestiona colecciones/índices, no migraciones de esquema. [Guía](/guides/database/) · [Matriz](/guides/database/#matriz-de-versiones-verificadas). |
| <code>ModelDiagnostics</code> + SeaORM <code>DeriveEntityModel</code> | Experimental disponible (Fase 6.7.1, validation/facade) | SeaORM genera Entity/Model/ActiveModel/Column/PrimaryKey y RustFlow valida atributos/tipos. CRUD no está conectado todavía al pool AnyPool existente; falta migrar a un pool SQLx tipado único en 6.7.2. [Contrato](/guides/database/#foundation-del-sql-orm-de-phase-671). |
| ORM SQL para SQLite/PostgreSQL/MySQL/MariaDB | Planificado, no disponible (Fase 6.7.2+) | CRUD, mapeo de tipos, query builder ejecutable, transacciones de modelo y relaciones/carga siguen pendientes. SeaORM 1.1.20 fue seleccionado como foundation; la integración del pool tipado y los codecs se verifica en 6.7.2. |
| Mapeo documental/ODM para MongoDB | Planificado, no disponible (subfase 6.7.3) | Contrato BSON/documentos separado del ORM SQL; el objetivo de Fase 6.7 es maduro, no CRUD mínimo. |

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
