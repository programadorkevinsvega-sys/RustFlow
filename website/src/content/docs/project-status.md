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

CsrfProtection ofrece middleware optativo basado en tokens firmados y expiran en ocho horas. La aplicación debe proveer una clave aleatoria de al menos 32 bytes fuera del repositorio y conectar el middleware a las rutas mutables que dependen de cookies. Request::redirect_back acepta solo un fallback local y usa un Referer cuando el origen coincide. Estas herramientas no implementan sesiones, login, autenticación ni autorización; las comprobaciones de permisos siguen siendo responsabilidad del caso de uso.

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
| CLI y scaffolding | Experimental, en revisión en la Fase 6.1 | [CLI y generadores](/guides/cli/) |
| <code>use_memo</code>, <code>use_ref</code>, <code>use_context</code> | No | [Roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) |
| Protección CSRF en mutaciones mediante cookies | Experimental, opt-in; requiere conectar middleware y proveer clave | [Formularios y validación](/guides/forms-validation/) |
| Sesiones, login/auth, autorización integrada, server actions y persistencia | No | [Product scope](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/PRODUCT_SCOPE.md) |

## Cómo interpretar las pruebas

Las pruebas unitarias, de integración HTTP/TCP y E2E Chromium demuestran los casos que ejercitan. No son una garantía de que se hayan probado todas las combinaciones ni sustituyen un modelo de amenazas, la operación real de producción o una promesa de compatibilidad. Antes de tomar decisiones de adopción, consulta el HEAD actual de <code>main</code>, la CI correspondiente y las limitaciones de cada guía.


## Fase 6.4 — Sesiones y control de acceso (en revisión)

La rama dedicada incorpora sesiones server-side, cookies host-only seguras, rotación e invalidación, extracción de identidad, guards antes de ejecutar el handler, política deny-by-default y hashing de contraseñas con ring. El adaptador incluido es volátil y local al proceso; el proveedor de usuarios y la persistencia de base de datos siguen fuera del alcance. La fase permanece abierta hasta que pasen todos los checks requeridos del HEAD final de la PR. Consulta la [guía de sesiones](/guides/sessions-and-access/).
