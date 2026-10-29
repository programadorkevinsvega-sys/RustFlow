---
title: Estado del proyecto
description: Capacidades implementadas, límites conocidos y estado verificable de RustFlow.
---

RustFlow sigue siendo un prototipo pre-1.0. El roadmap es la fuente de verdad de los criterios y estados de fase; una funcionalidad implementada no implica estabilidad de API ni preparación para producción. Este resumen es para que un usuario pueda decidir qué probar hoy sin confundir el objetivo futuro con el código disponible.

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
- Los modelos son structs Rust y las migraciones plantillas SQL; no existe ORM, conexión o runner de migraciones integrado.

Sigue la [guía del CLI](/guides/cli/) y el [tutorial completo](/tutorials/first-cli-app/) para comandos y recuperación ante errores.

### Fase 6.2 — Configuración por entornos: implementación experimental

La configuración pública rustflow_server::AppConfig valida el perfil, la IP de bind y el puerto de la aplicación generada. En desarrollo/test aplica la precedencia variable del proceso > .env.<perfil> > .env > defaults. Producción no lee archivos dotenv y exige RUSTFLOW_ENV=production, RUSTFLOW_HOST y RUSTFLOW_PORT desde el entorno del proceso.

.env.example no contiene secretos reales y el scaffold ignora los archivos de entorno privados. Esta API no es un sistema de configuración general ni un gestor de secretos; tampoco configura sesiones, base de datos o autenticación. La implementación de la Fase 6.2 está en la PR #46, fusionada mediante squash en el commit 34110b77afad1ade5827979825e98ecb4767a5dd; la verificación post-merge debe consultarse en CI.

### Fase 6.3 — Validación y formularios del servidor: implementada e integrada en main

El trabajo de esta fase añade extracción URL-encoded tipada con Request::form, validación de formularios/JSON/query mediante ValidateInput, y respuestas HTTP 422 con errores separados por campo y formulario. Los errores de decodificación y medio no compatible siguen siendo genéricos (400/415), mientras que el límite de cuerpo del servidor rechaza payloads demasiado grandes con 413.

CsrfProtection ofrece middleware optativo basado en tokens firmados y expiran en ocho horas. La aplicación debe proveer una clave aleatoria de al menos 32 bytes fuera del repositorio y conectar el middleware a las rutas mutables que dependen de cookies. Request::redirect_back acepta solo un fallback local y usa un Referer cuando el origen coincide. Estas herramientas de validación/CSRF no implementan por sí solas login ni un proveedor de usuarios. La Fase 6.4 añade primitivas experimentales de sesión e identidad, mientras que la aplicación sigue siendo responsable de verificar credenciales, asignar roles desde datos confiables y comprobar permisos según cada caso de uso.

La PR #47 se fusionó en `main` mediante squash en el commit `0a307e84bafabbc7dfaa5d4e361723b82fce042e`. Los checks requeridos pasaron en el HEAD final de la PR antes de la fusión, incluidos Rust 1.85/stable, WASM, E2E de navegador y presupuesto del bundle, build del sitio, gobernanza y revisión de dependencias. Esto confirma la verificación del HEAD de la PR; la consulta de estados combinados del commit de merge no devolvió estados individuales, así que no se afirma una ejecución post-merge verificada. Consulta la [guía de formularios y validación](/guides/forms-validation/).

## Matriz rápida: ¿qué puedes hacer?

| Tarea | ¿Disponible hoy? | Qué leer |
|---|---|---|
| Crear estado local en un componente | Sí, con owner activo | [Reactividad](/concepts/reactivity/) |
| Crear componentes con props y eventos tipados | Sí, dentro del subconjunto descrito | [Componentes](/guides/components/) |
| Actualizar DOM con señales/listas keyed | Sí, en el runtime WASM | [README del navegador](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) |
| Registrar rutas HTTP y handlers async | Sí | [Servidor HTTP](/guides/server/) |
| Renderizar documentos HTML seguros desde vistas | Sí, APIs experimentales | [SSR](/guides/ssr/) |
| Componer páginas con layouts y props de ruta | Sí, con componentes registrados/catálogo | [Layouts](/guides/layouts/) |
| Hidratar HTML SSR existente | Sí, opt-in y con una vista cliente compatible | [SSR e hidratación](/guides/ssr/) |
| Navegación cliente e historial | Sí, opt-in con callback de ruta de aplicación | [Navegación](/guides/navigation/) |
| Extracción tipada y errores 422 | Experimental, integrada en main (Fase 6.3) | [Formularios y validación](/guides/forms-validation/) |
| Forms mejorados | Solo GET elegibles con opt-in | [Navegación](/guides/navigation/) |
| CLI y scaffolding | Experimental, integrado en `main` (Fase 6.1) | [CLI y generadores](/guides/cli/) |
| <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> | No | [Roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) |
| Protección CSRF en mutaciones mediante cookies | Experimental, opt-in; requiere conectar middleware y proveer clave | [Formularios y validación](/guides/forms-validation/) |
| Sesiones server-side y guards de acceso | Experimental, integrado en `main` (Fase 6.4); sin proveedor de usuarios ni persistencia durable | [Guía de sesiones](/guides/sessions-and-access/) |
| Pool SQLx para SQLite/PostgreSQL | Experimental, disponible (Fase 6.5) | [Base de datos](/guides/database/); no incluye ORM |
| MySQL y MariaDB | No disponibles; planificados para la Fase 6.6 | [Roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) |
| MongoDB | No disponible; adaptador documental planificado para la Fase 6.6 | [Roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) |
| ORM SQL / mapeo documental MongoDB | No disponible; planificado para la Fase 6.7 | [Roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) |

## Cómo interpretar las pruebas

Las pruebas unitarias, de integración HTTP/TCP y E2E Chromium demuestran los casos que ejercitan. No son una garantía de que se hayan probado todas las combinaciones ni sustituyen un modelo de amenazas, la operación real de producción o una promesa de compatibilidad. Antes de tomar decisiones de adopción, consulta el HEAD actual de <code>main</code>, la CI correspondiente y las limitaciones de cada guía.


## Fase 6.4 — Sesiones y control de acceso (integrada en main)

La PR #49 se fusionó mediante squash en el commit `d53036410fc0d51900acf0c59b52c85f64cd1640`. Los checks requeridos pasaron en el HEAD final de la PR y la CI post-merge terminó en verde para Rust, WASM, Browser E2E/presupuesto de bundle, documentación, gobernanza y Cloudflare Pages. La implementación aporta sesiones server-side, cookies host-only seguras, rotación e invalidación, identidad extraída desde la sesión, guards previos al handler, políticas deny-by-default y hashing de contraseñas con `ring`. El adaptador incluido es volátil y local al proceso; no hay proveedor de usuarios ni persistencia durable. Consulta la [guía de sesiones](/guides/sessions-and-access/) y el [ADR 0015](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0015-sessions-and-access-control.md).

## Fase 6.5 — Base de datos y migraciones (implementación verificada)

La implementación de la Fase 6.5 añade SQLx con pools configurables para SQLite/PostgreSQL, transacciones explícitas, mapeo seguro de errores HTTP, migraciones versionadas y rollback explícito cuando existan archivos `.down.sql`. El scaffold inicializa una pool, comprueba `/health/database`, aplica migraciones en desarrollo/test y ofrece `cargo run --bin migrate` como paso único de despliegue en producción. La guía técnica y la guía pública documentan pool sizing, consultas parametrizadas y responsabilidades de despliegue.

La prueba de integración del CLI genera una aplicación y verifica validación, persistencia, verificación de contraseña, sesión/autenticación, CSRF, SSR y escape HTML. CI cubre Rust 1.85/stable, formato, Clippy, pruebas, documentación, WASM, Browser E2E, gobernanza, dependencias y build del sitio. Las APIs son experimentales: no hay ORM, capa genérica de repositorios, sesiones SQL durables ni proveedor de usuarios. La revisión de despliegue y seguridad de producción continúa siendo una responsabilidad de la aplicación.

## Fase 6.6 — Motores de base de datos adicionales (planificada)

Esta fase tiene como objetivo añadir MySQL y MariaDB como motores relacionales y MongoDB mediante un adaptador documental independiente. Requiere seleccionar drivers compatibles con la política de MSRV, definir configuración y mapeo de errores, probar transacciones/migraciones en los motores SQL, definir el ciclo de índices/colecciones para MongoDB y ejecutar pruebas de integración reproducibles. **Nada de esto está implementado actualmente**; SQLite y PostgreSQL siguen siendo los únicos motores disponibles en la Fase 6.5.

## Fase 6.7 — ORM SQL y mapeo documental (planificada)

La fase se divide en subfases para mantener contratos separados, pero **el objetivo obligatorio es una capa madura desde el principio, no un ORM mínimo que se declare terminado tras CRUD**. Las subfases son orden de ejecución; el cierre de la Fase 6.7 requiere que el contrato completo de modelos, consultas, relaciones/carga, transacciones, diagnósticos, compatibilidad y CI pase conjuntamente. Un ADR previo a la implementación debe fijar arquitectura, fundamentos mantenidos y capacidades obligatorias/excluidas.

- **6.7.1 — Arquitectura y contrato de modelos:** metadatos tipados, claves, conversión de tipos, errores y un ADR antes de fijar la API pública.
- **6.7.2 — ORM SQL:** operaciones CRUD, filtros y consultas parametrizadas para SQLite, PostgreSQL, MySQL y MariaDB, con diferencias de dialecto explícitas.
- **6.7.3 — Mapeador documental/ODM:** modelos BSON, colecciones, IDs y operaciones de documentos para MongoDB; no reutiliza semántica SQL.
- **6.7.4 — Relaciones y carga:** relaciones SQL y estrategias de carga sin N+1, con referencias/embebidos de MongoDB definidos por separado.
- **6.7.5 — CLI, ejemplos y documentación:** evolucionar el generador `model` tras aceptar el contrato y demostrar CRUD de extremo a extremo.
- **6.7.6 — Calidad y compatibilidad:** pruebas por motor, compile-fail, seguridad de consultas, MSRV, rendimiento medido y límites documentados.

Estas subfases son objetivos pendientes, no funcionalidades publicadas. La Fase 6.7 solo podrá cerrarse con una aplicación CRUD generada, ejemplos reproducibles y CI para los motores declarados compatibles.
