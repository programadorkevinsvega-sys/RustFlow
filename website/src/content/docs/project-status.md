---
title: Estado del proyecto
description: Capacidades implementadas, límites conocidos y estado verificable de RustFlow.
---

RustFlow sigue siendo un prototipo pre-1.0. El roadmap es la fuente de verdad de los criterios y estados de fase; una funcionalidad implementada no implica estabilidad de API ni preparación para producción. Este resumen es para que un usuario pueda decidir qué probar hoy sin confundir el objetivo futuro con el código disponible.

### Fase 7.1 — Capacidades operativas (completada e integrada)

Phase 7.1 se implementó y fusionó mediante el PR #68 el 1 de noviembre de 2026. El commit de integración en `main` es `c6ef2232d99233becb08008dc8b1a4a38b5bbd2f`. El SHA final de la PR (`491703f2263f20896128673aaaec66b6837711c1`) pasó los checks requeridos de Rust 1.85 y stable, formato, Clippy, pruebas de workspace y contratos de integración de bases de datos, WebAssembly, Browser E2E/bundle, build de documentación, gobernanza, revisión de dependencias y Cloudflare Pages. La prueba de apagado graceful con un handler atascado pasó en ambos toolchains.

La implementación amplía el tracing HTTP con duración por petición y configuración de nivel mediante `RUSTFLOW_LOG_LEVEL`, sin inicializar un subscriber global; incorpora `ErrorReporter` con metadatos allowlisted y ejecución aislada/bounded; añade endpoints optativos de liveness/readiness con checks explícitos y timeouts; y define un deadline para graceful shutdown que cancela los futures de handlers activos cuando vence.

Cache se evaluó y se difiere porque todavía no hay un caso de uso concreto ni contrato de invalidación/consistencia. Queue, email y storage también siguen diferidos hasta definir entrega, durabilidad, permisos y seguridad. No hay traits placeholder ni se anuncian estas integraciones como capacidades disponibles. La finalización de Phase 7.1 no implica estabilidad de API ni preparación para producción del framework pre-1.0.

### Fase 7.2 — UI completeness (completada e integrada en main)

PR #70 se fusionó en `main` el 1 de noviembre de 2026 como `506dd6f193c13a0bb941b6f75238d751a2934230`. El HEAD final de la PR (`798e0239918baf897df4fc003cc43d63081d0a30`) pasó los ocho checks: Rust stable/1.85, WebAssembly, Browser E2E/bundle, documentación, governance, dependency review y Cloudflare Pages. La implementación y la actualización documental de esta fase se entregan juntas en la PR #70. Se conservaron y auditaron las capacidades previas de componentes, listas keyed, fragments, formularios nativos, SSR e hidratación, y se completaron los siguientes contratos:

- **Memos derivados (`use_memo`):** cómputo eager asociado al owner, dependencias dinámicas y supresión de notificaciones cuando el resultado `PartialEq` no cambia. Dentro de un batch se lee el último valor confirmado hasta el flush exterior.
- **Refs Rust (`use_ref`):** almacenamiento estable, no reactivo y ligado al owner; acceso con closures sin exponer punteros/guards de préstamo duraderos; el handle se invalida al disponer el owner.
- **Contextos tipados:** `Context<T>` con identidad propia, providers por owner, búsqueda del ancestro más cercano, shadowing anidado y aislamiento entre runtimes/solicitudes SSR. El lookup no es reactivo por sí solo.
- **Referencias DOM:** `rustflow_dom::ElementRef` y `ref={...}` para elementos nativos en browser/WASM. El handle se completa tras mount o hidratación validada, sigue los reemplazos y se limpia al desmontar; no crea nodos en SSR y no es reactivo.
- **Inputs controlados:** una reconciliación evita reescribir una propiedad DOM cuando ya coincide con el valor controlado, preservando mejor la selección/caret durante actualizaciones no relacionadas.
- **Errores, estilos y accesibilidad:** se documentan la propagación de errores UI y la ausencia de ErrorBoundary; CSS/assets siguen el pipeline Vite de la aplicación, sin CSS-in-Rust ni asset registry del framework; se añade guía de accesibilidad.

Las APIs nuevas siguen siendo experimentales y pre-1.0. No se ofrecen Resources/Suspense/loading boundaries ni ErrorBoundary de componentes; el rendering asíncrono requiere contratos explícitos de cancelación, resultados obsoletos, SSR, hidratación y errores. Las referencias DOM no sustituyen `use_ref` y solo existen en el adaptador browser/WASM. Consulta [Reactividad](/concepts/reactivity/), [Composición de UI](/guides/ui-foundations/), [Errores en UI y SSR](/guides/ui-errors/), [Estilos y assets](/guides/styling-assets/) y la [referencia de disponibilidad de API](/reference/api-status/).
### Fase 7.3 — Testing y documentación (completada e integrada en main)

La Phase 7.3 se completó mediante la PR #71, fusionada por squash el 1 de noviembre de 2026 como `ebc671b4c5dc7688813158f663a3dc560c34edb7`. El HEAD final de la PR (`f3c7e5451095759428745f7760f3c531c91c89bc`) pasó los checks requeridos de Rust stable/1.85, formato, Clippy, tests del workspace y del ejemplo autenticado, contratos de bases de datos, WebAssembly, Browser E2E/bundle, build del sitio, gobernanza, dependency review y Cloudflare Pages.

La entrega incluye `rustflow-server/testing` opt-in con `TestAppBuilder` y request/response helpers que usan el router real in-process; fixtures SQL SQLite aisladas y namespaces para tablas en bases externas dedicadas; fixtures MongoDB con base de datos única y cleanup explícito; y helpers Playwright para aislamiento de páginas y captura de errores. El ejemplo autenticado demuestra password verification, sesiones server-side, identidad/roles, rutas protegidas, autorización, CSRF y logout. El ejemplo CRUD completo es la plantilla explícita `rustflow new <nombre> --template crud`, ya ejercitada por la prueba de integración del CLI; no es una plantilla de producción.

Las APIs de testing siguen siendo experimentales y opt-in. Las pruebas in-process no cubren TLS, dirección peer, trusted proxies, desconexiones ni graceful shutdown; las fixtures externas requieren bases de test dedicadas y cada fixture solo limpia sus recursos namespaced. Consulta [Probar y depurar aplicaciones](/guides/testing/), [Referencia de API y feature flags](/reference/api-status/), [Catálogo de ejemplos](/reference/examples/) y el [contrato técnico](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/TESTING.md).

## Estado por fases

### Fase 0 — Fundación del proyecto: completada

Licencia MIT/Apache-2.0, gobernanza, reglas de contribución, alcance del producto, límites de arquitectura, MSRV Rust 1.85 y CI. Esto establece las reglas de desarrollo; no es un hito de funcionalidad de usuario.

### Fase 1 — Runtime reactivo: completada

Incluye señales tipadas, seguimiento de dependencias, efectos, scheduler, batching, scopes propietarios, limpieza y diagnósticos. Hay una API de bajo nivel con <code>Runtime</code> y una API owner-scoped de componentes con <code>signal</code>, <code>use_state</code>, <code>use_effect</code> y <code>on_cleanup</code>. El runtime es de un hilo (<code>Rc</code>/<code>RefCell</code>) y los efectos son síncronos.

### Fase 2 — Modelo de vista y componentes estructurales: completada dentro de su alcance

Define nodos de texto/elemento/fragmento/componente/dinámicos, atributos, propiedades, callbacks, errores tipados, claves y la macro declarativa <code>view!</code>. La API funcional <code>#[rustflow::component]</code> y la integración de señales se completaron después como parte de la Fase 3. El modelo no es HTML ya sanitizado.

### Fase 3 — Runtime DOM/WASM: completada y verificada

El renderer monta y reconcilia texto, elementos, fragments, componentes, regiones dinámicas y listas keyed. Conserva la identidad DOM/estado de instancias compatibles, actualiza props y limpia recursos de ramas reemplazadas. El ejemplo del navegador incluye Counter y lista de tareas; los E2E Chromium verifican identidad, eventos, listas, montaje/desmontaje y el presupuesto de WASM.

### Fase 4 — Servidor HTTP: completada y verificada como prototipo

Incluye rutas, handlers asíncronos, parámetros de path/query, decodificación básica de JSON, respuestas, grupos, middleware global/de grupo/de ruta, rutas nombradas, request IDs, trazas estructuradas, límites de body, timeouts, concurrencia acotada, apagado graceful, TLS local con rustls y política de proxies confiables. Las pruebas incluyen peticiones TCP locales.

El cierre se limita a la base HTTP: no incorpora sesiones, autenticación, CSRF, rate limiting, almacenamiento ni hardening de producción. Consulta [Servidor HTTP](/guides/server/).

### Fase 5.1 — Contrato de renderizado y límites de destino: completada

Componentes pueden declarar capacidad Universal, ServerOnly o BrowserOnly; se valida el destino antes de ejecutar setup y los componentes browser-only requieren un fallback SSR estático explícito si se quieren renderizar en servidor. <code>HydrationData</code> ofrece un payload JSON acotado y escapado para contexto de script, pero no inspecciona el significado de los campos ni detecta secretos.

### Fase 5.2 — SSR y composición de página: completada

RustFlow puede renderizar vistas estructurales a fragmentos HTML escapados, generar documentos completos con <code>html_document</code>, configurar páginas de error 4xx/5xx y componer páginas con <code>HtmlPage</code>. Soporta catálogos compartidos inmutables de componentes para rutas HTTP concurrentes, con runtime/scope y datos de render locales a cada solicitud. <code>with_layout</code> anida componentes de layout y la aplicación pasa props de ruta explícitas.

### Fase 5.3 — Hidratación DOM: completada

La hidratación es optativa y añade límites deterministas a la salida SSR; el cliente adjunta eventos a un DOM existente si la vista es compatible. Las pruebas cubren formularios/inputs, regiones condicionales, listas keyed, navegación de enlaces ordinaria, repetición de eventos y rechazo de una hidratación duplicada. Ante una discrepancia incompatible, la recuperación conserva el root SSR completo como contenido estático y dispone los recursos candidatos; no hace reparación granular automática.

### Fase 5.4 — Navegación cliente y formularios: implementada

<code>ClientNavigation</code> ofrece interceptación conservadora de enlaces same-origin, integración con History API y manejo de atrás/adelante. La mejora progresiva de formulario requiere opt-in y se limita a GET URL-encoded con controles escalares compatibles; el resto conserva el envío nativo. La aplicación debe definir las rutas del cliente; esto no es un router general ni incluye server actions. El PR #41 se fusionó y los checks de CI post-merge sobre el commit documentado en el roadmap se ejecutaron con éxito; los checks propios del PR también cubrieron documentación, gobernanza y dependencias. El ADR 0011 sigue marcado como “Proposed for review”: ese estado indica que falta la aprobación arquitectónica formal, no que la implementación o sus pruebas estén ausentes.

### Fase 6.1 — CLI y generadores: implementación experimental

La Fase 6.1 se fusionó a `main` mediante la PR #45 (commit `6fb26c310359fd0e186e92696f09edcca24ed2eb`). Incluye `rustflow new`, `rustflow dev`, `rustflow build` y los generadores de componentes, páginas, controladores, modelos y migraciones. Los checks de la PR pasaron en el HEAD final antes de la fusión.

- `new` crea un proyecto de servidor y fija dependencias Git a la revisión fuente del CLI.
- `dev` y `build` delegan a Cargo; `dev` no incluye watcher ni hot reload.
- Los generadores rechazan colisiones y editan solo una sección delimitada en los índices de módulos.
- El generador `model` crea un struct de dominio ordinario, no una entidad SeaORM ni un modelo `DocumentModel`. El scaffold de servidor sí incluye conexión SQLx y runner de migraciones SQLite-first; las APIs ORM/ODM experimentales se usan explícitamente desde `rustflow-server` y no se generan automáticamente.

Sigue la [guía del CLI](/guides/cli/) y el [tutorial completo](/tutorials/first-cli-app/) para comandos y recuperación ante errores.

### Fase 6.2 — Configuración por entornos: implementación experimental

La configuración pública rustflow_server::AppConfig valida el perfil, la IP de bind y el puerto de la aplicación generada. En desarrollo/test aplica la precedencia variable del proceso > .env.<perfil> > .env > defaults. Producción no lee archivos dotenv y exige RUSTFLOW_ENV=production, RUSTFLOW_HOST y RUSTFLOW_PORT desde el entorno del proceso.

.env.example no contiene secretos reales y el scaffold ignora los archivos de entorno privados. Esta API no es un sistema de configuración general ni un gestor de secretos; tampoco configura sesiones, base de datos o autenticación. La implementación de la Fase 6.2 está en la PR #46, fusionada mediante squash en el commit 34110b77afad1ade5827979825e98ecb4767a5dd; la verificación post-merge debe consultarse en CI.

### Fase 6.3 — Validación y formularios del servidor: implementada e integrada en main

El trabajo de esta fase añade extracción URL-encoded tipada con Request::form, validación de formularios/JSON/query mediante ValidateInput, y respuestas HTTP 422 con errores separados por campo y formulario. Los errores de decodificación y medio no compatible siguen siendo genéricos (400/415), mientras que el límite de cuerpo del servidor rechaza payloads demasiado grandes con 413.

CsrfProtection ofrece middleware optativo basado en tokens firmados y expiran en ocho horas. La aplicación debe proveer una clave aleatoria de al menos 32 bytes fuera del repositorio y conectar el middleware a las rutas mutables que dependen de cookies. Request::redirect_back acepta solo un fallback local y usa un Referer cuando el origen coincide. Estas herramientas de validación/CSRF no implementan por sí solas login ni un proveedor de usuarios. La Fase 6.4 añade primitivas experimentales de sesión e identidad, mientras que la aplicación sigue siendo responsable de verificar credenciales, asignar roles desde datos confiables y comprobar permisos según cada caso de uso.

La PR #47 se fusionó en `main` mediante squash en el commit `0a307e84bafabbc7dfaa5d4e361723b82fce042e`. Los checks requeridos pasaron en el HEAD final de la PR antes de la fusión, incluidos Rust 1.85/stable, WASM, E2E de navegador y presupuesto del bundle, build del sitio, gobernanza y revisión de dependencias. Esto confirma la verificación del HEAD de la PR; la consulta de estados combinados del commit de merge no devolvió estados individuales, así que no se afirma una ejecución post-merge verificada. Consulta la [guía de formularios y validación](/guides/forms-validation/).

### Fase 6.4 — Sesiones y control de acceso (integrada en main)

La PR #49 se fusionó mediante squash en el commit `d53036410fc0d51900acf0c59b52c85f64cd1640`. Los checks requeridos pasaron en el HEAD final de la PR y la CI post-merge terminó en verde para Rust, WASM, Browser E2E/presupuesto de bundle, documentación, gobernanza y Cloudflare Pages. La implementación aporta sesiones server-side, cookies host-only seguras, rotación e invalidación, identidad extraída desde la sesión, guards previos al handler, políticas deny-by-default y hashing de contraseñas con `ring`. El adaptador incluido es volátil y local al proceso; no hay proveedor de usuarios ni persistencia durable. Consulta la [guía de sesiones](/guides/sessions-and-access/) y el [ADR 0015](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0015-sessions-and-access-control.md).

### Fase 6.5 — Base de datos y migraciones (implementación verificada)

La implementación de la Fase 6.5 añade SQLx con pools configurables para SQLite/PostgreSQL, transacciones explícitas, mapeo seguro de errores HTTP, migraciones versionadas y rollback explícito cuando existan archivos `.down.sql`. El scaffold inicializa una pool, comprueba `/health/database`, aplica migraciones en desarrollo/test y ofrece `cargo run --bin migrate` como paso único de despliegue en producción. La guía técnica y la guía pública documentan pool sizing, consultas parametrizadas y responsabilidades de despliegue.

La prueba de integración del CLI genera una aplicación y verifica validación, persistencia, verificación de contraseña, sesión/autenticación, CSRF, SSR y escape HTML. RustFlow ya incluye APIs ORM SQL y ODM MongoDB experimentales, pero el scaffold predeterminado no genera automáticamente entidades, repositorios ni una aplicación CRUD completa. Siguen pendientes sesiones SQL durables y proveedor de usuarios. La revisión de despliegue y seguridad de producción continúa siendo responsabilidad de la aplicación.

### Fase 6.6 — Motores de base de datos adicionales (completada e integrada en main)

La subfase 6.6.1 quedó integrada en `main` mediante PR #54 y el commit `48a491d660c71005b58ed88a4f6a7e8ba3390c9d`; sus pruebas originales cubrieron MySQL 8.0 y MariaDB 11.4. En aquel hito, el driver SQLx `mysql` se usaba mediante `AnyPool`; la integración ORM posterior de la Fase 6.7.2 adoptó pools SQLx tipados por backend y comparte el mismo pool con SeaORM. `DatabaseBackend::MySql` representa el driver compartido, no identifica el producto servidor. Las URLs usan `mysql://` y las migraciones de esa familia requieren `.rustflow-dialect` con `mysql`. En la subfase 6.6.3 se actualiza el target público a MySQL 8.4.13 LTS y MariaDB 11.4.13 LTS.

La subfase 6.6.2 quedó integrada en `main` mediante PR #55, con el commit `e3ceb86d757bb7aa1a07e16a1e21b462adb69841`. Añade un adaptador MongoDB independiente de SQLx, configuración y timeout, BSON, acceso CRUD a colecciones nativas, bootstrap de colecciones/índices y mapeo seguro de errores. La CI de su HEAD final pasó con integración MongoDB 7.0.45 en Rust estable/1.85. La subfase 6.6.3 verificó el contrato público completo: SQLite 3.46.0 bundled, PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS y MongoDB 7.0.45, con MSRV, configuraciones y pruebas del scaffold documentados. El code head `51ac4c212b3d6bf68483ee2ef8e28b5c3a75f421` pasó CI en Rust estable/1.85 y los gates de documentación, gobernanza, dependencias, WebAssembly y Browser E2E/bundle. La Phase 6.6 queda completa con este contrato. En ese momento el ORM SQL y el ODM MongoDB pasaron al trabajo de Phase 6.7.2/6.7.3; posteriormente, la fase global 6.7 se completó mediante PR #66, como se registra más adelante.

### Fase 6.7.1 — Contrato de modelos y arquitectura

PR #58 integró ADR 0020, el patrón Data Mapper sobre SeaORM 1.1.20 y el derive de validación <code>ModelDiagnostics</code>. Esta subfase definió el modelo/metadata, los límites de tipos, los estados ActiveModel, hooks, errores y transacciones; no añadía todavía persistencia ejecutable.

### Fase 6.7.2 — ORM SQL (integrada en main)

La PR #59 se fusionó mediante Squash Merge el 2026-10-31 en el commit `8d387943d5f6ccb4d3f33ba3708311442e238c5f`. La implementación integra SeaORM 1.1.20 en <code>rustflow-server</code> sobre un único pool SQLx tipado compartido por backend. <code>DatabasePool::entity</code> crea repositorios enlazados a la conexión o transacción existente; la API incluye CRUD/bulk, composición de consultas tipadas, loaders explícitos, parámetros SQL seguros, transacciones mixtas ORM/SQL y clasificación de errores. El contrato de integración está preparado para SQLite, PostgreSQL 16.15, MySQL 8.4.13 y MariaDB 11.4.13.

La aceptación de 6.7.2 quedó cerrada sobre el HEAD `2cadce73c19c37c2f088f572ea37a83c65c72380`: los tests de límites numéricos, unsigned soportados por dialecto, conteos de consultas en dos saltos relacionales, mutaciones attach/update/detach/cascade y terminación real del backend PostgreSQL durante `COMMIT` pasaron. También pasaron los 8 checks requeridos (Rust estable/1.85, WASM, Browser E2E/bundle, documentación, gobernanza, dependencias y Cloudflare Pages). La PR #59 está cerrada y fusionada. La aceptación de 6.7.2 no cerró por sí sola la Phase 6.7 global; el ODM de MongoDB se registra como subfase independiente a continuación. La fase global se completó posteriormente mediante PR #66.

### Fase 6.7.3 — ODM documental MongoDB (integrada en main)

La arquitectura queda registrada en ADR 0021: Data Mapper documental basado en `DocumentModel`, BSON/Serde y el único cliente oficial `MongoDatabase` existente. No incorpora otro driver/cliente, SeaORM, relaciones SQL ni carga perezosa. La implementación aporta `DocumentRepository`, `DocumentFilter`, `DocumentField<T>`, `DocumentQuery`, `DocumentUpdate`, CRUD/bulk acotado, proyecciones, agregaciones, operaciones explícitas con sesiones, verificación de índices y cobertura de documentos embebidos. PR #60 pasó CI en el HEAD final `659f1750d611e57315263216167d43375da23574` y se fusionó mediante squash en `main` el 2026-10-31, commit `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. La API sigue experimental. La subfase 6.7.3 no cerró por sí sola la fase global; esta se completó posteriormente mediante PR #66, integrada en `main` como `6046ae7ca60843d145476177e698162b57939c68`.

### Fase 6.7.4 — Relaciones y carga SQL avanzada (integrada en main)

PR #62 se fusionó mediante squash en `main` el 2026-10-31, commit `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7` (HEAD final `28218456db8ac323731054e291023a0765474125`). Añade `RelationLoadOptions<E>` para filtros/orden por relación, `load_many_projected` para DTOs de relaciones has-many con claves de agrupación explícitas y `RelationGraph` para ramas múltiples y carga anidada. El grafo admite `with_many`, `with_one`, `with_many_to_many` y ramas proyectadas hoja; la profundidad predeterminada es 8 y el máximo configurable es 16. Los helpers de mutación son `attach_one_to_many`, `detach_one_to_many` y `replace_one_to_one`.

Los loaders siguen siendo explícitos y por lotes, sin carga perezosa. Limit/offset aplica al conjunto relacionado completo, no a cada padre; no se promete top-N por padre. Los DTOs proyectados no sirven como nodos padre de relaciones anidadas. Attach/detach actualizan la FK de una fila existente y nunca borran; replacement one-to-one usa una transacción proporcionada por el caller, que también controla commit/rollback. MongoDB mantiene su semántica ODM independiente. El HEAD final pasó CI, build del sitio, gobernanza y revisión de dependencias antes de la fusión. La plantilla CRUD de Phase 6.7.5 está aceptada e integrada; el gate global de madurez 6.7.6 quedó completado posteriormente mediante PR #66.

### Fase 6.7.5 — CLI y aplicación CRUD generada (integrada en main)

PR #64 se fusionó mediante squash en `main` el 2026-10-31 como `edd665ad452c769c78ba8190c233e349a44b054a` (HEAD verificado del PR: `596366af0eccfb93fbbeaafb6df80a708cce6a1f`). Añade `rustflow new <nombre> --template crud` sin modificar la plantilla `server` predeterminada ni el comportamiento plano de `rustflow generate model`. La plantilla incorpora una entidad SeaORM explícita, migraciones por dialecto, operaciones CRUD con `EntityRepository`, validación, transacciones con rollback, guards de sesión/autorización, CSRF opt-in aplicado a mutaciones y HTML estructural SSR. La integración generada y los contratos de los cuatro motores SQL pasaron CI con Rust 1.85 y stable. La demo utiliza `InMemorySessionStore`, credenciales demo y cookies de desarrollo; rechaza `RUSTFLOW_ENV=production` y no es un ejemplo de operación en producción.

### Fase 6.7.6 — Calidad y compatibilidad (integrada en main)

PR #66 cerró el gate global y se fusionó mediante squash el 2026-11-01 como `6046ae7ca60843d145476177e698162b57939c68` (HEAD final verificado: `b049d96491e2b23bfe794ac3c0ed9abb68f7e493`). La aceptación incluye pruebas de saturación/recuperación del pool y 32 escrituras concurrentes en cada motor SQL; regresiones para valores que parecen SQL/BSON operators; migraciones PostgreSQL up/reapply/down aisladas; benchmark smoke de CRUD, bulk, ejecución de consultas, batch loading y MongoDB ODM; y aplicación CRUD con relación Category/Product, `load_one`, filtros tipados y paginación acotada. Todos los checks requeridos pasaron en Rust stable y 1.85, integraciones SQL/MongoDB, PostgreSQL migrations, benchmark smoke, WASM, Browser E2E/bundle, documentación, governance, dependency review y Cloudflare Pages. La API ORM/ODM continúa experimental y pre-1.0; la demo generada no es un starter de producción. La excepción `RUSTSEC-2023-0071` permanece documentada y sin resolver.

### Matriz rápida: ¿qué puedes hacer?

| Tarea | ¿Disponible hoy? | Qué leer |
|---|---|---|
| Crear estado local en un componente | Sí, con owner activo | [Reactividad](/concepts/reactivity/) |
| Crear componentes con props y eventos tipados | Sí, dentro del subconjunto descrito | [Componentes](/guides/components/) |
| Referenciar elementos DOM nativos de forma segura | Sí, experimental y solo browser/WASM; mount, reemplazo, hidratación validada y unmount | [Composición de UI](/guides/ui-foundations/) |
| Actualizar DOM con señales/listas keyed | Sí, en el runtime WASM | [README del navegador](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) |
| Configurar nivel de tracing | Sí, mediante `TracingConfig`; subscriber propiedad de la aplicación | [Servidor HTTP](/guides/server/) · [Configuración](/guides/configuration/) |
| Reportar errores internos sin datos de petición | Sí, mediante `ErrorReporter` con metadatos allowlisted | [Seguridad HTTP](/guides/errors-and-security/) |
| Health/readiness del framework | Sí, opt-in y con checks explícitos bounded | [Servidor HTTP](/guides/server/) |
| Graceful shutdown con deadline | Sí, con cancelación de handlers activos al vencer el deadline | [Servidor HTTP](/guides/server/) |
| Registrar rutas HTTP y handlers async | Sí | [Servidor HTTP](/guides/server/) |
| Renderizar documentos HTML seguros desde vistas | Sí, APIs experimentales | [SSR](/guides/ssr/) |
| Componer páginas con layouts y props de ruta | Sí, con componentes registrados/catálogo | [Layouts](/guides/layouts/) |
| Hidratar HTML SSR existente | Sí, opt-in y con una vista cliente compatible | [SSR e hidratación](/guides/ssr/) |
| Navegación cliente e historial | Sí, opt-in con callback de ruta de aplicación | [Navegación](/guides/navigation/) |
| Extracción tipada y errores 422 | Experimental, integrada en main (Fase 6.3) | [Formularios y validación](/guides/forms-validation/) |
| Forms mejorados | Solo GET elegibles con opt-in | [Navegación](/guides/navigation/) |
| CLI y scaffolding | Experimental e integrado en `main` (Fase 6.1 y Phase 6.7.5/6.7.6) | `rustflow new` conserva `server` como plantilla predeterminada; `rustflow new --template crud` genera la aplicación ORM educativa con relación Category/Product, carga explícita, filtros y paginación demostrados en los cuatro motores SQL. No es un starter de producción. [CLI y generadores](/guides/cli/) |
| <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> | Sí, de forma experimental (Phase 7.2) | [Reactividad](/concepts/reactivity/) · [Composición de UI](/guides/ui-foundations/) |
| Protección CSRF en mutaciones mediante cookies | Experimental, opt-in; requiere conectar middleware y proveer clave | [Formularios y validación](/guides/forms-validation/) |
| Sesiones server-side y guards de acceso | Experimental, integrado en `main` (Fase 6.4); sin proveedor de usuarios ni persistencia durable | [Guía de sesiones](/guides/sessions-and-access/) |
| Pools SQLx tipados para SQLite/PostgreSQL/MySQL/MariaDB | Experimental, integrados en main mediante PR #59 | Un pool compartido con SeaORM por backend; CRUD/bulk, queries tipadas y transacciones mixtas. Los 8 checks requeridos pasaron en el HEAD verificado. [Base de datos](/guides/database/) |
| MySQL y MariaDB | Experimental integrados (6.6.1); target contract actual verificado en 6.6.3 | SQLx 0.8.6 driver `mysql`; MySQL 8.4.13 LTS/MariaDB 11.4.13 LTS se validaron con aserciones de versión en Rust 1.85/stable. [Guía](/guides/database/) · [Matriz](/guides/database/#matriz-de-versiones-verificadas) |
| MongoDB | Adaptador documental experimental integrado en `main` (subfase 6.6.2, PR #55) | `MongoConfig`, `MongoDatabase`, colecciones nativas, BSON, bootstrap e índices; independiente de SQLx. CI final aprobada con Rust estable/1.85 e integración MongoDB 7.0.45. [Guía](/guides/database/) |
| ORM SQL para SQLite/PostgreSQL/MySQL/MariaDB | Experimental, 6.7.2 integrada en main mediante PR #59 | SeaORM 1.1.20 sobre un único pool SQLx tipado por backend; `EntityRepository` CRUD/bulk, queries tipadas, loaders explícitos y transacciones mixtas. Los cuatro motores y checks de aceptación se verificaron en el HEAD de la PR. No se anuncia como estable. [Guía](/guides/database/) · [Contrato](/guides/database/#roadmap-de-persistencia). |
| Extensiones ORM de relaciones/carga | Experimental, 6.7.4 integrada en `main` mediante PR #62 | Filtros/orden por edge, proyección DTO has-many, grafos explícitos/nested (profundidad 8 por defecto, máximo 16), helpers attach/detach/reemplazo uno-a-uno. Sin lazy loading, top-N por padre ni cascadas implícitas; proyección como hoja. [Guía](/guides/database/) · [ADR 0022](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0022-advanced-sql-relation-loading.md) |
| ODM MongoDB | Experimental, integrada en `main` (Phase 6.7.3 / PR #60) | `DocumentModel`, `DocumentRepository`, filtros y updates BSON, paginación/proyecciones, agregación, sesiones, IDs y documentos embebidos. PR #60 pasó los checks requeridos en el HEAD final `659f1750d611e57315263216167d43375da23574` y se fusionó como `7ffb89b5ca36017a428cf3f828b68f590f8d404b`. La API sigue pre-1.0. [Guía ODM](/guides/mongodb-odm/) · [ADR 0021](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0021-mongodb-odm-architecture.md) |

### Cómo interpretar las pruebas

Las pruebas unitarias, de integración HTTP/TCP y E2E Chromium demuestran los casos que ejercitan. No son una garantía de que se hayan probado todas las combinaciones ni sustituyen un modelo de amenazas, la operación real de producción o una promesa de compatibilidad. Antes de tomar decisiones de adopción, consulta el HEAD actual de <code>main</code>, la CI correspondiente y las limitaciones de cada guía.


### Fase 6.7 — ORM SQL y mapeo documental (completada)

La fase se dividió en subfases para mantener contratos separados, pero **el objetivo obligatorio fue una capa madura desde el principio, no un ORM mínimo que se declare terminado tras CRUD**. El contrato conjunto de modelos, consultas, relaciones/carga, transacciones, diagnósticos, compatibilidad, integración, documentación y CI fue aceptado mediante PR #66. La implementación permanece experimental y pre-1.0.

- **6.7.1 — Arquitectura y contrato de modelos:** cerrada e integrada mediante PR #58; ADR 0020 selecciona Data Mapper/SeaORM 1.1.20 y define el contrato `ModelDiagnostics`.
- **6.7.2 — ORM SQL:** criterios de aceptación cerrados e integrados en `main` mediante PR #59 (Squash Merge, commit `8d387943d5f6ccb4d3f33ba3708311442e238c5f`); pools SQLx tipados compartidos con SeaORM, EntityRepository, CRUD/bulk, query composition, loaders explícitos y transacciones mixtas para SQLite/PostgreSQL/MySQL/MariaDB.
- **6.7.3 — ODM MongoDB:** integrado en `main` mediante PR #60, fusionada el 2026-10-31 (merge commit `7ffb89b5ca36017a428cf3f828b68f590f8d404b`). La implementación incluye Data Mapper documental independiente, CRUD/bulk, filtros/updates, proyecciones, agregaciones, IDs, documentos embebidos y operaciones explícitas con sesiones. Consulta el [ADR 0021](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0021-mongodb-odm-architecture.md) y la [guía ODM](/guides/mongodb-odm/).
- **6.7.4 — Relaciones y carga avanzada:** integrada en `main` mediante PR #62, squash merge `f7ae9fa35afd2a4021a7e6fb56e6d728445d20d7`; opciones de filtro/orden, has-many projections, grafos anidados y helpers explícitos de asociación, desasociación y reemplazo one-to-one. Véase la [guía de base de datos](/guides/database/) y [ADR 0022](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0022-advanced-sql-relation-loading.md).
- **6.7.5 — CLI, ejemplos y documentación:** integrada mediante PR #64, squash merge `edd665ad452c769c78ba8190c233e349a44b054a`; CI y checks requeridos pasaron en el HEAD final del PR.
- **6.7.6 — Calidad y compatibilidad:** completada y fusionada mediante PR #66 (HEAD final `b049d96491e2b23bfe794ac3c0ed9abb68f7e493`; merge `6046ae7ca60843d145476177e698162b57939c68`). Se aceptaron las pruebas de pool exhaustion/recuperación, concurrencia, query safety, migraciones reversibles, benchmark smoke y aplicación CRUD relacional con filtros/paginación; todos los checks requeridos pasaron.

La Fase 6.7 queda completada en `main` mediante PR #66. Esto cierra la implementación y el gate de compatibilidad de esta fase, no la preparación general para producción: las APIs siguen siendo experimentales; durable sessions, proveedor de usuarios, hardening de seguridad y despliegue validado pertenecen a trabajo posterior.
