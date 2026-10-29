# RustFlow Roadmap

This roadmap defines a path from an empty repository to a production-grade full-stack Rust web framework. Every phase starts as planned. Mark an item complete only when its acceptance criteria are met and supported by code, tests, documentation, or release evidence. Dates are intentionally omitted.

## Vision

RustFlow aims to combine React-like reactive UI components, a Rust-owned reactivity engine, server rendering and hydration, and a Laravel-inspired full-stack developer experience: routing, handlers/controllers, middleware, validation, sessions, authentication, database integration, migrations, generators, and CLI tooling.

Reuse mature infrastructure for HTTP, async execution, database drivers, serialization, and cryptography. Do not reinvent an async runtime, database drivers, or cryptographic primitives.

## Product-level acceptance gate: React-like authoring

RustFlow is **not** intended to be a drop-in React-compatible replacement. However, its ordinary component-authoring experience must be recognizably declarative, typed, reactive, and convenient, using familiar React-inspired concepts and Rust-style names such as `use_state` and `use_effect`. This is a product requirement, not merely a future polish item. API names do not imply React's hook-cell ordering or exact signatures.

The following is a **design target, not code that compiles against today's API**:

```rust
#[component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);

    view! {
        <button on:click={move |_| count.update(|n| *n += 1)}>
            "Count: " {count}
        </button>
    }
}
```

Before Phase 3 can pass its exit gate, the public API and browser runtime must demonstrate all of the following:

- An ergonomic `signal(value)` API with read, write, and update operations inside a component, while ownership remains tied to an explicit component/runtime scope. Do not introduce a process-global mutable runtime singleton as a shortcut.
- Ordinary signal interpolation in `view!` tracks dependencies and updates automatically; users do not need to wrap every normal signal child in `@dynamic`.
- A `#[component]`-style function-component API with typed props, useful compile-time diagnostics, and automatic integration into component descriptors/registration for normal use.
- Typed callback props between components, in addition to typed DOM event handlers.
- DOM updates that patch only the affected nodes/regions, preserving mounted DOM/component identity when keys and component identity indicate continuity.
- Deterministic cleanup of effects, subscriptions, event listeners, and child scopes on unmount.
- A browser-loadable example and automated browser E2E coverage for a counter and a todo list using the intended public API.

The exact public syntax and runtime ownership mechanism must be recorded in an ADR and verified with compile tests before implementation is treated as complete. Any syntax change from this illustrative target must preserve the stated developer experience or be explicitly reviewed and documented. SSR/hydration are separate milestones: completion of these client-side criteria does not imply that the SSR/hydration criteria are complete.

## Status policy

- [ ] Planned
- [x] Complete and verified

All tasks begin unchecked. Discussion, scaffolding, or an unverified implementation does not count as completion.

## Phase 0 — Project foundation

### 0.1 Governance
- [x] Select and add a dual MIT/Apache-2.0 license.
- [x] Define contribution guidelines, code of conduct, maintainers, and decision process.
- [x] Define supported Rust toolchain and MSRV policy (stable toolchain; MSRV 1.85).
- [x] Define semantic versioning, pre-1.0 API stability, and deprecation policy.
- [x] Configure required status checks in the active `Protect main — RustFlow` ruleset. Verified through the GitHub API on 2026-10-27: the active ruleset targets the default branch and lists `Rust checks (1.85)`, `Rust checks (stable)`, `Build documentation`, `Governance files and whitespace`, `WebAssembly target check`, `Browser E2E and bundle size`, and `Review dependency changes`. The `Browser E2E and bundle size` context is duplicated in the ruleset (one entry has no integration ID); remove that duplicate administratively and re-verify the ruleset.

### 0.2 Product scope
- [x] Define target users, product principles, supported platforms, and explicit non-goals in `docs/PRODUCT_SCOPE.md`.
- [x] Define MVP user stories and acceptance criteria in `docs/PRODUCT_SCOPE.md`.
- [x] Establish a risk register for reactivity, macros, WASM, hydration, security, and maintenance in `docs/PRODUCT_SCOPE.md`.
- [x] Create ADRs for crate boundaries, client runtime, and server integration (`docs/adr/`).

### 0.3 Engineering baseline
- [x] Establish workspace naming and dependency conventions in `docs/ENGINEERING_BASELINE.md` and the root Cargo manifest.
- [x] Verify CI for formatting, Clippy, unit tests, documentation tests, and dependency review (successful PR runs: CI, governance, and dependency review on the Phase 0.3 baseline).
- [x] Add issue/PR templates and review checklist.
- [x] Align README and contributor instructions with the initial workspace and CI baseline; re-check policy consistency during review.

**Exit gate:** project scope and policies are explicit; CI runs on pull requests; major architectural decisions have recorded context and trade-offs.

## Phase 1 — Reactivity engine prototype

### 1.1 Semantics
- [x] Specify signal reads/writes and dependency tracking (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Specify effects, nested effects, dynamic dependency replacement, and cleanup (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Define scheduling order, batching, re-entrancy, and cycle behavior (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Define ownership scopes and deterministic disposal (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).
- [x] Define single-threaded browser behavior and server-side isolation/thread-safety boundaries (accepted contract: `docs/REACTIVITY_SEMANTICS.md`).

### 1.2 Implementation
- [x] Implement typed signals (`crates/rustflow/src/reactivity.rs`).
- [x] Implement effects and dependency graph (`crates/rustflow/src/reactivity.rs`).
- [x] Implement scheduler and batching (`crates/rustflow/src/reactivity.rs`).
- [x] Implement owner scopes and disposal (`crates/rustflow/src/reactivity.rs`).
- [x] Add diagnostics for invalid/disposed runtime use (`RuntimeError` in `crates/rustflow/src/reactivity.rs`).

### 1.3 Correctness
- [x] Test that only dependent effects rerun.
- [x] Test dynamic dependencies and stale dependency removal.
- [x] Test nested effects, disposal, batching, cycles, re-entrant writes, and panic/error paths.
- [x] Add property/model-based tests where useful (dependency-free deterministic operation model; randomized property testing remains a future enhancement if justified).
- [x] Document runtime limitations and guarantees (`docs/REACTIVITY_RUNTIME.md`, `docs/REACTIVITY_VERIFICATION.md`, and the public website).

### 1.4 Baseline performance
- [x] Benchmark reads, writes, effect scheduling, and high fan-out (`crates/rustflow/benches/reactivity.rs`).
- [x] Record reproducible methodology and baseline numbers (`docs/REACTIVITY_VERIFICATION.md`; three CI repetitions with environment details).

**Exit gate:** standalone crate, documented semantics, deterministic tests, explicit cleanup behavior, and reproducible benchmarks.

## Phase 2 — View and component model

### 2.1 View representation
- [x] Define typed nodes for text, elements, fragments, and components in `crates/rustflow-view/src/lib.rs`.
- [x] Define attributes, DOM properties, boolean attributes, typed event callbacks, and deferred children.
- [x] Define key values and keyed-list identity rules: keys must be unique within a list snapshot and represent the same logical child across reorders; renderer reconciliation is implemented in Phase 3.
- [x] Define typed construction errors and the target-independent `Renderer` mount/update/unmount contract.

### 2.2 Lifecycle
- [x] Specify create, mount, update, and unmount behavior through `ComponentDefinition`, `ComponentRegistry`, and `ComponentInstance`.
- [x] Connect each component instance to an owned reactive scope; unmount disposes owned effects, child scopes, and cleanup callbacks.
- [x] Specify event-listener ownership and cleanup: platform adapters register removal callbacks with `ComponentContext::on_cleanup`; actual DOM registration/removal belongs to Phase 3.
- [x] Evaluate context/provider semantics: defer a public provider API until a concrete use case justifies it; it is not required for this initial component contract.
- [x] Document render-time side-effect constraints. Setup owns effects/resources; the repeatable render callback must produce a view and leave side effects to owned runtime scopes.

### 2.3 Declarative syntax
- [x] Prototype the dependency-free `rustflow::view!` procedural macro in `crates/rustflow-macros`.
- [x] Parse the documented constrained HTML-like grammar with Rust expressions in braces.
- [x] Support text, attributes, boolean attributes, DOM properties, events, nested elements, component descriptors, keys, and deferred children.
- [x] Emit actionable parser diagnostics using source spans.
- [x] Add compile-fail tests and document the supported syntax in rustdoc and the public component guide.

### 2.4 Tests
- [x] Test text, nested elements/components, attributes, properties, fragments, event callbacks, dynamic children, and keyed-list duplicate detection.
- [x] Test macro expansion and mismatched-closing-tag diagnostics.

**Exit gate: complete for the native structural view/component model only.** Typed view construction, reusable component definitions and registry resolution, explicit lifecycle/cleanup, and macro diagnostics are covered by tests. This Phase 2 closure was limited to the native structural model. At that time the ergonomic authoring contract—`#[component]`, typed props/callback props, automatic reactive interpolation, browser DOM reconciliation, and E2E tests—was assigned to Phase 3 and was implemented in the Phase 3 closure recorded below. Phase 2 itself does not close the separate SSR, hydration, server, or CLI phases.

## Phase 3 — Browser runtime and interactive MVP

**Estado: CERRADA en `main` el 2026-10-26.** Los criterios de aceptación de la Fase 3 están implementados y verificados. La PR #26 se integró primero en la rama de la PR #25; después, la PR #25 se fusionó en `main`. La CI posterior al merge pasó los siete checks en el commit `43f5fb9c12996ea600b47930553a9f91dc6055e9`, incluidos Rust estable/1.85, compilación WASM, E2E de navegador, presupuesto del bundle, documentación y gobernanza. Este cierre cubre la Fase 3; no declara completas las fases posteriores ni las APIs que siguen marcadas como pendientes.

### 3.1 API pública de autoría

- [x] Definir e implementar la API signal(value) con get, set y update, ligada a un scope de propietario activo; comprobar el uso fuera de scope y la disposición.
- [x] Exponer use_state(initial) como comodidad sobre señales ligadas al propietario; devuelve Signal<T> y no almacena estado en celdas indexadas por el orden de hooks.
- [x] Exponer use_effect(...) como efecto ligado al scope; comprobar cleanup antes de cada rerun y al disponer el componente.
- [x] Documentar qué nombres RustFlow existen hoy y cuáles siguen planeados: use_state y use_effect están disponibles; use_memo, use_ref y use_context siguen pendientes.
- [x] Añadir #[component] con adaptador/registro generado, props tipados, rechazo de props faltantes/desconocidos/tipos incompatibles y callbacks de componente tipados. El subconjunto actual requiere parámetros con nombres simples y tipos explícitos; los tipos admitidos se documentan en la guía.
- [x] Admitir callback props a través de EventHandler, sin convertir callbacks en atributos HTML.
- [x] Hacer que interpolar un identificador de señal como {count} produzca un hijo dinámico reactivo sin requerir @dynamic; expresiones arbitrarias que lean señales todavía requieren una región dinámica explícita.
- [x] Compilar y probar un componente Counter real en el ejemplo de navegador.

### 3.2 Pipeline de compilación

- [x] Configurar el target wasm32-unknown-unknown y el check de compilación dedicado en CI.
- [x] Integrar wasm-bindgen y web-sys en el adaptador específico de navegador.
- [x] Separar las dependencias del navegador del modelo de vistas y del runtime reactivo multiplataforma.
- [x] Construir un bundle de navegador de producción con WASM y assets estáticos mediante wasm-pack y Vite.

### 3.3 Runtime DOM

- [x] Montar texto, elementos y fragmentos mediante rangos delimitados por comentarios.
- [x] Aplicar atributos y propiedades mediante APIs DOM tipadas con validación de elementos, atributos y esquemas URL; no tratar el árbol como HTML seguro.
- [x] Registrar y retirar listeners; delegar los eventos que burbujean en el límite de montaje y usar listeners directos para los eventos que necesitan ese tratamiento.
- [x] Completar las actualizaciones DOM dirigidas para todo el modelo: el plan diff/evaluación valida los subárboles y prepara fuera del DOM los rangos nuevos o incompatibles; el commit aplica parches compatibles y modifica sólo los rangos afectados. Se reconcilian texto, elementos, fragmentos, listas keyed, componentes y regiones dinámicas.
- [x] Reconciliar listas dinámicas con claves únicas y conservar la identidad DOM de filas compatibles al reordenarlas.
- [x] Separar explícitamente diff/evaluación y commit DOM mediante `ReconciliationPlan` y `commit_plan`. Las props nuevas generan una vista previsualizada sin repetir setup; los componentes de identidad compatible mantienen su scope/estado, `KeyedChild` propaga su clave al descriptor y los rangos/scope/efectos/listeners de instancias sustituidas se limpian.
- [x] Añadir errores recuperables a ReactiveMountedDom::take_error y diagnóstico de renderizados fallidos en la consola del navegador.

### 3.4 MVP interactivo y verificación

- [x] Añadir un Counter con estado reactivo y actualización de un nodo existente.
- [x] Añadir una lista de tareas con alta, marcado, eliminación y reordenación keyed.
- [x] Cubrir inputs, formularios básicos, HTML semántico y etiquetas accesibles.
- [x] Ejecutar E2E en Chromium sobre el bundle de producción: Counter, propagación desde un nodo hijo, CRUD/reordenación de tareas y montaje/desmontaje condicional.
- [x] Medir WASM raw y gzip y aplicar un presupuesto raw de 1.5 MiB.
- [x] Documentar configuración local, compilación release/debug, source maps de los assets JS y troubleshooting básico.

**Cierre verificado:** la Fase 3 está cerrada en `main` desde el 2026-10-26. La PR #26 (`feat/phase-3: preserve component identity`) se fusionó en la rama `feat/phase-3-browser-runtime`, y luego la PR #25 se fusionó en `main`. Los siete checks post-merge pasaron en `43f5fb9c12996ea600b47930553a9f91dc6055e9`. La cobertura incluye actualización de props sin reiniciar el estado local, preservación de identidad DOM, listas keyed, sustitución y limpieza de instancias, cambios estructurales, Counter/todo, E2E Chromium y presupuesto WASM. `use_memo`, `use_ref` y `use_context` siguen pendientes; SSR/hidratación y otros requisitos de fases posteriores permanecen fuera del alcance de este cierre.

## Phase 4 — HTTP server and routing

**Estado: Fase 4 cerrada y verificada en main el 2026-10-26.** La PR #30 integró los criterios del servidor HTTP. La auditoría de cierre se completó en la PR #31, fusionada en el commit `6f4123b94f4273701f0336c4c982973af7bf7562`. Todos los checks post-merge pasaron en ese commit: Rust 1.85 y stable, WASM, E2E Chromium y presupuesto del bundle, Rustdoc, build del website, gobernanza, revisión de dependencias y Cloudflare Pages. Este cierre no declara listo el despliegue de producción ni las fases 5–11.

### 4.1 HTTP foundation
- [x] Integrate Axum, Tokio, and Tower behind the RustFlow-owned `rustflow-server` adapter.
- [x] Define RustFlow-owned request, response, HTTP error, method, server configuration, and application types. State is captured explicitly by route closures; a separate state container remains optional future work.
- [x] Implement route matching, path parameters, query strings, and conventional HTTP methods.
- [x] Provide text, HTML, JSON, redirect, byte-download, and status helpers.
- [x] Set request-body limits, request timeouts, and a global concurrency cap with configurable defaults.

### 4.2 Handlers and errors
- [x] Implement async route registration and reject duplicate method/path registrations.
- [x] Implement basic path-parameter and typed query/JSON decoding helpers.
- [x] Define an async closure-handler convention with explicit captured state.
- [x] Map handler errors safely and provide 404/405 routing behavior.
- [x] Add in-memory dispatch tests, unique named routes, and deterministic route introspection.
- [x] Add representative real-TCP integration tests for HTML/JSON routes, path/query decoding, 400/404/405/413/504 responses, request-ID correlation, custom error pages, body limits/timeouts, and graceful shutdown (`crates/rustflow-server/tests/http_integration.rs`).
- [x] Add full route-group support (`Application::group`, nested prefixes, group name prefixes, and inherited group middleware).
- [x] Expand network/deployment coverage for TLS/reverse-proxy trust, client aborts, malformed transport input, and concurrent/load behavior (`crates/rustflow-server/tests/http_integration.rs`).

### 4.3 Middleware and lifecycle
- [x] Implement global ordered async middleware composition with a continuation that can wrap or short-circuit handlers. Route-scoped middleware is registered with `route_middleware` and group middleware is inherited by `Application::group`.
- [x] Validate or generate a request ID, expose it to handlers, echo it in `x-request-id` on success and error responses, and emit structured `tracing` spans/events with method, route template, request ID, and status. Subscriber/export configuration stays application-owned.
- [x] Provide graceful-shutdown APIs driven by an injected future at the composition root. Wiring OS signals into that future remains the caller's responsibility.
- [x] Validate route templates, reject equivalent parameter-only patterns, and clamp body/timeout/concurrency configuration to safe minimums.
- [x] Document explicit closure-captured application state. `ServerConfig::production`, trusted-proxy policy, `require_https`, and `validate`/`validate_tls` reject plaintext production binds that have no TLS listener and no trusted proxy.

### 4.4 Safe server rendering
- [x] Add rustflow-ssr to serialize supported view nodes into HTML fragments independently of an HTTP framework.
- [x] Escape text and attribute values by default; apply a scheme allowlist to URL attributes and reject multi-URL attributes until parsed.
- [x] Add safe full-document responses from structural views (`html_document`) and configurable escaped HTML error pages for selected 4xx/5xx statuses. Layout composition remains an application-level concern.
- [x] Test escaping, rejected/void elements, URL policy, component cleanup, keyed order, nesting limits, full-document charset/title/body escaping, configurable 404/405 pages, route dispatch, body limits, handler timeouts, middleware, named-route introspection, and representative TCP requests covering status/content-type/request-ID headers, error cases, body limits, timeouts, and graceful shutdown.
Transport coverage in Phase 4.2 includes local TLS, trusted-proxy header reconstruction, client abort, malformed input, and bounded concurrency. It does not replace a production threat model, reverse-proxy deployment recipe, or Phase 9 security review.

**Exit gate:** an executable example server handles multiple routes, renders safe HTML, and returns JSON. Automated coverage includes in-memory dispatch, route groups, route-scoped middleware, local TLS, trusted-proxy policy, client abort, malformed input, and bounded concurrency. At the time Phase 4 closed, sessions, auth, CSRF, rate limiting, and full-stack SSR/hydration were later phases. Phases 5.2–5.4 have since added SSR page/layout composition, opt-in hydration, and opt-in navigation/forms, but this server prototype is still not production-ready.

### 4.5 Subfase A — Resiliencia del handshake TLS

- [x] Separar la aceptación TCP de los handshakes TLS para que una conexión lenta no bloquee la aceptación de otras.
- [x] Limitar handshakes concurrentes a 64 y aplicar un timeout de 10 segundos por handshake.
- [x] Cancelar el listener y los handshakes pendientes al finalizar el servidor.
- [x] Añadir una regresión de TCP real que mantiene un cliente sin ClientHello y comprueba que otra conexión TLS válida recibe respuesta.
- [x] Verificar en CI la prueba de regresión, Clippy, Rust 1.85 y Rust stable en la PR #31; todos pasaron también post-merge en `main` (`6f4123b94f4273701f0336c4c982973af7bf7562`).

### 4.6 Subfase B — Consistencia de documentación

- [x] Actualizar el README raíz para describir la reconciliación DOM y los E2E actuales.
- [x] Corregir estados obsoletos de la guía pública de componentes y explicar los límites del modelo de la Fase 2.
- [x] Alinear el estado del alcance de producto con el cierre de la Fase 0.2 sin declarar el producto estable.
- [x] Sincronizar la página de estado y el README del website con las fases y el hosting actual.
- [x] Documentar el timeout y el límite de concurrencia TLS en las guías aplicables.
- [x] Confirmar build de Astro y Rustdoc en la PR #31 y post-merge en `main`; ambos gates pasaron.

### 4.7 Subfase C — Gate final y registro del cierre

- [x] Pasar todos los checks de la PR #31 en el HEAD `8e92f31c2442988b4d7c5bdd3fc3f9f9066946f0`: Rust 1.85, Rust stable, WASM, E2E Chromium/presupuesto del bundle, Build documentation, gobernanza, revisión de dependencias y Cloudflare Pages.
- [x] Revisar los resultados y confirmar que SSR/hidratación, autenticación y preparación para producción siguen correctamente fuera del alcance de esta fase.
- [x] Tras el merge, comprobar todos los checks post-merge en `main` y registrar el SHA `6f4123b94f4273701f0336c4c982973af7bf7562`; todos terminaron correctamente.
- [x] Auditar el ruleset activo mediante la API de GitHub el 2026-10-27: el ruleset `Protect main — RustFlow` está activo y apunta a la rama predeterminada; requiere los siete contextos distintos esperados: Rust checks (1.85), Rust checks (stable), Build documentation, Governance files and whitespace, WebAssembly target check, Browser E2E and bundle size y Review dependency changes. La entrada `Browser E2E and bundle size` está duplicada (una de las entradas no tiene `integration_id`). La limpieza de esa entrada sigue pendiente como ajuste administrativo, fuera del código de esta PR.

## Phase 5 — SSR, hydration, and full-stack data flow

### 5.1 Rendering contract
- [x] Define server-capable and browser-only components (`ComponentCapability`, target-aware mounting, browser-only SSR fallback/error, and regression tests).
- [x] Define safe serialization for hydration data (`rustflow_ssr::HydrationData`; bounded JSON, nesting validation, HTML-script-safe escaping, and regression tests).
- [x] Define the server/client data boundary: known server crates/dependencies are denied in the WASM graph; `HydrationData` accepts only an explicitly supplied JSON value and does not provide blanket serialization of arbitrary Rust structs. Applications must project an allow-list of public fields and must never pass sessions, credentials, authorization decisions, or private state. Semantic detection of secrets in arbitrary values is not a framework capability and remains an application-level security responsibility.
- [x] Define deterministic rendering and non-hydratable fallbacks (repeatable static-tree test; browser-only components require an explicit static SSR fallback or return an error; hydration mismatch recovery remains future work).

**Estado de 5.1 (completada el 2026-10-27):** `HydrationData` implementa y prueba JSON acotado; `ComponentCapability` declara componentes `Universal`, `ServerOnly` y `BrowserOnly`; SSR/browser validan el destino antes de ejecutar setup; SSR exige fallback estático explícito para componentes browser-only, o devuelve un error; y CI bloquea crates de servidor conocidos en el grafo WASM. El límite de secretos se define mediante proyección explícita de campos públicos por la aplicación: RustFlow no serializa automáticamente structs arbitrarios ni puede deducir el significado secreto de una cadena. La hidratación DOM, los identificadores de límite, la detección/recuperación de discrepancias siguen siendo trabajo de 5.3, no están implícitos en el cierre de este contrato.

### 5.2 SSR
- [x] Make the startup-built shared catalog the canonical API in the runnable server example, SSR route integration test, and developer guidance. Verified by [PR #37](https://github.com/programadorkevinsvega-sys/RustFlow/pull/37), merged in commit `5e905d98d858dfed7aa868afe29bc8920dda37aa`; required PR checks and post-merge `main` checks passed.
- [x] Build an immutable `SharedComponentCatalog` with `Send + Sync` setup/fallback factories and capture it in HTTP route closures.
- [x] Prove concurrent request isolation, local runtime/scope cleanup, browser-only fallback behavior, and all required CI checks before closing this architectural follow-up.

- [x] Render structural view trees into HTML fragments using rustflow-ssr.
- [x] Expose complete HTML document responses and configurable escaped error pages through `rustflow-server` (Phase 4.4); route handlers can call these helpers.
- [x] Add advanced layout composition and explicit route-to-component data integration (rustflow-server::HtmlPage, nested layout components, and allow-listed route props). At the time of this Phase 5.2 closure, hydration/navigation were still incomplete; they are tracked and closed independently in 5.3/5.4.
- [x] Test escaping and configured document/error-page responses.
- [x] Test request-isolated SSR/component state and deterministic boundaries: repeated renders with the same route props produce identical HTML, each render creates/disposes its own component scope, and browser-only/server-only policies remain covered by SSR target tests.

### 5.3 Hydration
- [x] Add minimal, safely serialized hydration data through `HydrationData` and the opt-in HTML page helpers.
- [x] Map rendered nodes to deterministic root, component, dynamic, and static SSR boundaries.
- [x] Attach client events to existing compatible DOM without rebuilding nodes or duplicating handlers.
- [x] Detect and report incompatible DOM/state as bounded hydration mismatch diagnostics.
- [x] Define conservative mismatch recovery: preserve the entire SSR root as static HTML and dispose candidate resources rather than destructively replacing DOM.
- [x] Test form input, conditional regions, keyed-list reorder, ordinary link navigation, repeated event handling, mismatch preservation, and duplicate-hydration rejection.

**Cierre de 5.3 (2026-10-27):** PR #39 se fusionó en `main` mediante squash en el commit `d866298989821eaee64a4ea00466012f36db3ebf`. La CI posterior al merge pasó en Rust 1.85, Rust stable, WASM y Browser E2E/bundle size; en el PR también pasaron la construcción de documentación, gobernanza y revisión de dependencias. La hidratación es opt-in y exige que la vista inicial del cliente sea compatible con el HTML SSR. La recuperación implementada conserva estático el root completo si hay discrepancia; la reparación granular de subregiones queda fuera de alcance. El router cliente, el historial, la interceptación de enlaces y la mejora progresiva de envíos de formulario se incorporaron después en la Fase 5.4.

### 5.4 Navigation and forms
- [x] Add client navigation after route semantics stabilize.
- [x] Implement browser history and link interception.
- [x] Define progressive-enhancement behavior for forms.
- [x] Test server/client boundaries and reload fallbacks.

**Implementation verified in PR #41 (2026-10-27; merged):** the route-callback client adapter, History API/popstate handling, conservative link interception, opt-in GET form enhancement, and native/server fallbacks are implemented and backed by browser tests. The PR-head CI passed Rust stable, Rust 1.85, the WebAssembly target check, browser production build, raw WASM bundle-size budget, Playwright E2E (18 tests), documentation build, governance/whitespace checks, and dependency review.

**Limitations:** the application owns its client route callback; this is not a general-purpose router or server-action framework. Form enhancement is limited to supported same-origin GET submissions with URL-encoded scalar controls. Unsupported methods/controls, uploads, alternate targets, and other ineligible interactions keep native browser behavior. SSR remains authoritative for direct requests and fallbacks.

**Merge closure:** PR #41 se fusionó en `main` mediante squash en `1da8b110ed3e57abccb2c14b6236fcbbc0fbf849`. La CI posterior al merge en el commit `cc26782ef28cf14c5ad039bdcbdded93a5972a34` terminó correctamente en los checks visibles de Rust 1.85, Rust stable, WebAssembly target, Browser E2E/bundle size, Cloudflare Pages y gobernanza. Los checks de documentación y revisión de dependencias también habían pasado en el PR. La fase está implementada y verificada dentro de este alcance; los límites descritos arriba permanecen.

## Phase 6 — Laravel-inspired developer experience

### 6.1 CLI
- [x] Implement rustflow new.
- [x] Implement rustflow dev and rustflow build.
- [x] Generate components, pages, controllers, models, and migrations.
- [x] Prevent generators from silently overwriting user files.
- [x] Add deterministic CLI integration tests.

**Verification gate (met):** PR #45 se fusionó en `main` mediante squash en `6fb26c310359fd0e186e92696f09edcca24ed2eb`. En el HEAD final de la PR pasaron Rust 1.85/stable, CI, WASM, Browser E2E/bundle size, documentación, gobernanza y revisión de dependencias. El cierre se limita al CLI, scaffold y generadores descritos arriba.

### 6.2 Configuration
- [x] Define configuration precedence and validation.
- [x] Add a safe .env.example without real secrets.
- [x] Separate development, test, and production settings.
- [x] Fail fast on invalid production configuration.
- [x] Document secret management.

**Cierre verificado:** la PR #46 ([configuración por entornos](https://github.com/programadorkevinsvega-sys/RustFlow/pull/46)) se integró mediante squash en `main` en el commit `34110b77afad1ade5827979825e98ecb4767a5dd`. Los ocho checks requeridos pasaron en el HEAD final de la PR `24fb875e83a6f7b324a281b5f065c43ab8a2e158`; la comprobación post-merge también terminó en verde para Rust 1.85/stable, WASM, E2E y presupuesto del bundle, documentación, gobernanza y Cloudflare Pages.

### 6.3 Validation and forms
- [x] Implement typed input extraction and validation.
- [x] Return structured field/form errors.
- [x] Add CSRF protection for cookie-authenticated mutations.
- [x] Add safe redirect-back behavior.
- [x] Test oversized payloads and unexpected input.

**Closure evidence:** PR #47 ([Validation and forms](https://github.com/programadorkevinsvega-sys/RustFlow/pull/47)) was squash-merged into `main` as commit `0a307e84bafabbc7dfaa5d4e361723b82fce042e`. All required checks passed on the final PR HEAD `222f1b381aeffd82127c7fbe688b75f0d6e4007a` before merge: Rust 1.85/stable, WASM target, browser E2E and bundle budget (run `40667608192`); website build (run `40667608245`); governance/whitespace (run `40667608233`); and dependency review (run `40667608162`). The implementation, HTTP integration tests, technical contract, ADR and public guide are included in the merged change. The combined-status query for the merge commit returned no status entries while this closure was recorded, so this evidence confirms the green PR-head checks and does not claim a separately verified post-merge run.

### 6.4 Sessions and access control
- [x] Define session abstraction and secure cookie defaults.
- [x] Implement expiry, rotation, invalidation, and storage adapters.
- [x] Implement authentication guards and identity extraction.
- [x] Implement authorization policies with deny-by-default behavior.
- [x] Use maintained password hashing and cryptographic libraries.
- [x] Test fixation, CSRF, privilege escalation, and access boundaries.

**Cierre verificado:** la PR #49 ([sesiones y control de acceso](https://github.com/programadorkevinsvega-sys/RustFlow/pull/49)) se integró mediante squash en `main` en el commit `d53036410fc0d51900acf0c59b52c85f64cd1640`. Los checks requeridos pasaron en el HEAD final de la PR `f23fba7fcbba6d7da50d959d2fa74fcd5e60dcab`; la CI post-merge terminó en verde en [CI](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/40841892564), [documentación](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/40841892518), [gobernanza](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/40841892473) y Cloudflare Pages. Las pruebas TCP cubren acceso anónimo, rotación anti-fijación, login/logout protegidos por CSRF, denegación por rol, revocación/replay de cookies y preservación de múltiples cabeceras Set-Cookie. `InMemorySessionStore` sigue siendo volátil y local al proceso; sesiones durables, SQLx, transacciones y migraciones corresponden a la Fase 6.5.

### 6.5 Database and migrations
- [x] Integrate SQLx and configurable connection pools.
- [x] Add transaction helpers and error mapping.
- [x] Implement versioned migrations and rollback policy.
- [x] Defer model/query abstractions until real use cases justify them; no ORM or generic repository was introduced.
- [x] Use parameterized SQL and test both supported database engines in CI.
- [x] Document pool sizing and deployment migration practices.

**Implementation in PR #51:** SQLx `AnyPool` supports SQLite/PostgreSQL, pool settings resolve through `AppConfig::setting`, transactions expose explicit commit/rollback, HTTP error mapping avoids leaking driver messages, and SQLx applies versioned migrations with opt-in rollback. The generated CLI integration test builds a generated application and verifies its database health route, validation, login/password verification, session/auth guards, CSRF-protected CRUD, persisted results, SSR output, and HTML escaping. SQLite migrations/transactions/CRUD and a PostgreSQL service smoke test also run in CI. The API remains experimental; no ORM, generic repository abstraction, MySQL support, or database-backed sessions are included.

**Exit gate:** covered by the generated CLI integration test, with repeatable CI coverage on Rust stable and Rust 1.85. The Phase 6.5 implementation criteria are satisfied for SQLite and PostgreSQL. Additional engines (MySQL, MariaDB, and MongoDB) are planned for Phase 6.6; the SQL ORM and MongoDB document-mapping layer are planned for Phase 6.7. These are roadmap targets only and are not implemented by the Phase 6.5 closure. Production deployment/security hardening and durable database-backed sessions remain separate concerns.

## Phase 6.6 — Additional database engines (in progress)

**Status: in progress pending merge of PR #57.** Phase 6.6.1 was merged into `main` as commit `48a491d660c71005b58ed88a4f6a7e8ba3390c9d` (PR #54); its original MySQL 8.0 and MariaDB 11.4 contracts passed under Rust 1.85 and stable. Phase 6.6.2 was merged into `main` as commit `e3ceb86d757bb7aa1a07e16a1e21b462adb69841` (PR #55), with MongoDB 7.0.45 integration and all required checks passing on the final PR head. Phase 6.6.3's verified matrix is documented in `docs/DATABASE_SUPPORT_MATRIX.md`: bundled SQLite 3.46.0, PostgreSQL 16.15, MySQL 8.4.13 LTS, MariaDB 11.4.13 LTS, and MongoDB 7.0.45, with runtime assertions and driver/MSRV details. The implementation head `51ac4c212b3d6bf68483ee2ef8e28b5c3a75f421` passed all required workflows on Rust stable and Rust 1.85, including database integration, generated CLI smoke tests, docs, governance, dependency review, WebAssembly and browser E2E/bundle checks. Phase 6.6 remains open until PR #57 is integrated into `main`.

### 6.6.1 Relational engines: MySQL and MariaDB
- [x] Select and document the SQLx driver/configuration strategy compatible with RustFlow's Rust 1.85 MSRV and dependency policy.
- [x] Validate `mysql://` URL configuration for both products while preserving SQLite/PostgreSQL contracts and credential redaction.
- [x] Verify pool lifecycle, parameter binding, transactions, unique-constraint mapping, invalid credentials, and connection failure behavior against each server.
- [x] Define and test migration directory/dialect expectations without assuming DDL portability or full migration atomicity.
- [x] Run disposable MySQL 8.0 and MariaDB 11.4 integration services on Rust 1.85 and stable.

**Implementation and limitations:** SQLx 0.8.6's `mysql` feature supplies one MySQL-protocol driver for both products. `DatabaseBackend::MySql` identifies that driver family, not the server product. Phase 6.6.1 originally tested MySQL 8.0 and MariaDB 11.4; the Phase 6.6.3 public matrix supersedes those targets with maintained MySQL 8.4.13 LTS and MariaDB 11.4.13 LTS, alongside PostgreSQL 16.15, bundled SQLite 3.46.0, and MongoDB 7.0.45. These current targets are accepted only after exact-version assertions and all final-head checks pass. MySQL-family DDL may implicitly commit.

### 6.6.2 Document database: MongoDB
- [x] Add a maintained MongoDB Rust-driver adapter separately from SQLx `AnyPool`; MongoDB must not be described as a SQL engine.
- [x] Define MongoDB connection/client configuration, timeouts, BSON serialization/deserialization, and safe error mapping.
- [x] Define the migration/bootstrap contract for collections and indexes; do not reuse SQL migrations as if they applied to MongoDB.
- [x] Add repeatable integration tests against a disposable MongoDB service for connect, CRUD, indexes, and failure paths.

### 6.6.3 Public support contract
- [x] Publish the engine/version matrix and Rust/MSRV compatibility details in `docs/DATABASE_SUPPORT_MATRIX.md`.
- [x] Document shared relational APIs versus engine-specific SQL/migration/DDL behavior; do not promise identical SQL semantics across engines.
- [x] Update configuration examples, technical/public docs, generated-app tests, and exact-version CI assertions; the final passing evidence is recorded above.

**Exit gate:** SQLite, PostgreSQL, MySQL, MariaDB, and MongoDB each have an explicit verified support contract and reproducible integration coverage. SQL engines have correct engine-specific tests/migration expectations; MongoDB has its own document and index lifecycle contract. Merely adding a dependency does not close this phase.

## Phase 6.7 — ORM and model persistence (planned)

**Status: planned.** This phase targets a **mature, cohesive ORM from the outset**, not a minimal ORM/MVP that is declared complete and incrementally grows afterward. The term **SQL ORM** applies only to SQLite, PostgreSQL, MySQL, and MariaDB. MongoDB is document-oriented and belongs to the separate document-mapping/ODM subphase below; it must not be forced into a SQL query or relational model.

**Mandatory maturity rule:** the subphases below organize architecture and implementation work; they are not permission to ship or close Phase 6.7 as a CRUD-only ORM. Phase 6.7 cannot be marked complete until the complete public contract, query builder, relationships/loading, transaction semantics, type mapping, diagnostics, cross-engine behavior, documentation, and quality gates in this phase are implemented and verified together. Design the API and dependency boundaries for this full target before coding begins. Reuse proven, maintained foundations where appropriate rather than reinventing SQL dialect generation or database drivers. An ADR must select the ORM pattern and dependency strategy before the public API is fixed. The goal is a maintainable, well-tested ORM, not a feature-for-feature clone of another framework; any excluded capability must be explicitly listed in the accepted contract instead of left ambiguous.

### 6.7.1 Model contract and architecture
- [ ] Record an ADR that compares maintained foundations and selects the architectural pattern (for example, Active Record or Data Mapper), dependency boundaries, and trade-offs before fixing the public API.
- [ ] Define model/table/column metadata, naming rules, primary keys (including the supported key shapes), nullable/default/generated fields, indexes/constraints, and type conversion.
- [ ] Publish a type-support matrix for integers, booleans, text, bytes, decimals/numerics, UUIDs, timestamps/time zones, enums, JSON, nullable values, and engine-specific types where supported.
- [ ] Define a typed model contract and derive/proc-macro diagnostics with compile-fail tests for invalid attributes, unsupported fields, and conflicting declarations.
- [ ] Specify insert/update/delete semantics, returned/generated values, dirty-state or unit-of-work behavior appropriate to the chosen pattern, model lifecycle hooks if included, and safe error behavior.
- [ ] Preserve an explicit escape hatch for raw, parameterized SQL and engine-specific capabilities; never interpolate user values into SQL or rely on a hidden global database singleton.
- [ ] Define how transactions and model operations compose, including rollback behavior and how partial failures are represented.
- [ ] Establish the mandatory versus explicitly excluded feature matrix in the ADR; do not leave the maturity target open-ended.

### 6.7.2 SQL ORM core
- [ ] Implement typed model mapping/derives and row decoding for SQLite, PostgreSQL, MySQL, and MariaDB against the Phase 6.6 verified adapters.
- [ ] Implement complete core CRUD plus bulk insert/update/delete where semantically supported, with explicit handling of generated keys, defaults, affected-row counts, missing records, and unique/foreign-key conflicts.
- [ ] Provide a composable, typed query builder for projections, aliases, predicates, nested AND/OR expressions, comparisons, NULL checks, IN, LIKE, ordering, distinct, limits/offset pagination, and documented support for joins, grouping/aggregates, HAVING and subqueries.
- [ ] Define eager/relation loading as part of the complete ORM contract, with explicit load strategies and query-count tests; do not introduce silent lazy I/O that hides database work.
- [ ] Support parameterized execution throughout. Identifiers and query fragments must be validated/constructed through safe typed APIs; values from callers must always be bound parameters.
- [ ] Provide explicit APIs for dialect-specific operations (such as upsert/RETURNING) where support differs, and document capability detection rather than pretending all engines behave identically.
- [ ] Integrate model operations with explicit transactions, including rollback on errors, and test bulk operations, conflict paths, and partial failure semantics.
- [ ] Maintain a tested type-mapping matrix per supported SQL engine, including NULLs, JSON, timestamps, numeric/decimal, UUID and generated/default values where supported.
- [ ] Integrate with the existing migration strategy without implying that migrations are portable across dialects; document and test per-engine migration directories when needed.

### 6.7.3 MongoDB document mapper / ODM
- [ ] Define and document a separate typed document model/derive contract based on BSON, collections, `_id`, serializers/deserializers, and MongoDB's own query/update operators.
- [ ] Implement CRUD, bulk operations where supported, projections, sorting, pagination, and aggregation capabilities supported by the selected driver, without presenting SQL constructs as portable to MongoDB.
- [ ] Define index creation/verification, unique indexes, embedded documents versus references, and explicit handling of missing/null/default fields.
- [ ] Specify session/transaction behavior and server/replica-set constraints from verified driver/server capabilities; do not claim parity with SQL transactions without evidence.
- [ ] Provide safe filters and typed APIs while preserving a documented escape hatch for driver-native operations.
- [ ] Add compile-time and integration tests for BSON type mapping, IDs, missing fields, nested documents, updates, indexes, duplicate keys, and failure paths.

### 6.7.4 Relations and loading behavior
- [ ] Implement and test SQL `belongs_to`/many-to-one, one-to-one, one-to-many, and many-to-many relationships, including join-table metadata and foreign-key behavior.
- [ ] Support explicit eager/batch loading for SQL relations, nested loads within the documented scope, and predictable relation ordering; add query-count tests to guard against N+1 regressions.
- [ ] Define relation mutation semantics, transaction requirements, and behavior when related records are missing or constraints fail.
- [ ] Keep MongoDB embedding/referencing and document population semantics in the ODM contract; do not emulate relational joins where MongoDB does not provide them.

### 6.7.5 CLI, generated applications, and documentation
- [ ] Evolve `rustflow generate model` only after the model contract is accepted; keep the current generator's plain-struct behavior until then.
- [ ] Add a generated CRUD application that proves validation, persistence, transactions, authorization/CSRF integration, and rendered output using the supported ORM surface.
- [ ] Document real examples and limits in `docs/`, README, and `website/src/content/docs`; keep examples matched to the public API.

### 6.7.6 Quality and compatibility gate
- [ ] Run unit, compile-fail, property/model-based where useful, integration, and query-safety tests across all supported SQL engines and MongoDB.
- [ ] Exercise migrations, connection loss/timeouts, pool exhaustion, transaction rollback, constraint violations, concurrent writes, and engine-version differences in repeatable CI.
- [ ] Measure representative CRUD, relationship-loading, bulk-operation and query-builder overhead; document reproducible methodology and avoid unsupported performance claims.
- [ ] Review dependency maintenance, Rust 1.85/stable MSRV policy, security advisories, feature combinations, API diagnostics, and failure behavior before claiming maturity.
- [ ] Keep database engine behavior and unsupported operations explicit in docs; verify every documented example against the actual public API.
- [ ] Add a complete generated application demonstrating models, relationships, validation, transaction composition, filtering/pagination, and secure HTTP integration.

**Maturity gate (all mandatory):** Phase 6.7 cannot close at a CRUD-only or prototype state. A generated application must prove the supported SQL ORM contract on SQLite, PostgreSQL, MySQL, and MariaDB and the separate mature document API on MongoDB. Model mapping, query building, relationships/loading, transaction/error semantics, integration tests, compile-fail diagnostics, documentation and CI must all pass together. Any feature intentionally excluded from the mature target must be named in the accepted ADR and shown as an explicit limitation; subphase completion alone does not mean the ORM/ODM is ready or complete.

## Phase 7 — Core framework completeness

### 7.1 Operational capabilities
- [ ] Add structured tracing and configurable log levels.
- [ ] Add safe error-reporting hooks.
- [ ] Add health/readiness endpoints.
- [ ] Define cache interfaces if justified.
- [ ] Define queue, email, and storage integrations only when delivery/security contracts are clear.
- [ ] Ensure graceful shutdown and timeouts are tested.

### 7.2 UI completeness
- [ ] Improve contexts, dynamic lists, fragments, forms, and controlled inputs.
- [ ] Add `use_memo` for derived computations with documented dependency tracking and caching semantics.
- [ ] Add `use_ref` for stable non-reactive component-scoped storage and DOM handles.
- [ ] Add typed provider/context support and `use_context`, including nearest-provider lookup and missing-context behavior.
- [ ] Keep all `use_*` APIs owner-scoped and signal-model-native; do not introduce positional hook cells unless a separate ADR explicitly changes the architecture.
- [ ] Define error isolation behavior.
- [ ] Add loading/async rendering semantics only when runtime design supports them.
- [ ] Document styling and asset integration.
- [ ] Include accessibility guidance and examples.

### 7.3 Testing and docs
- [ ] Provide test app builders and request helpers.
- [ ] Add isolated database and browser test utilities.
- [ ] Add a full CRUD example and an authenticated example.
- [ ] Document every stable public API and feature flag.
- [ ] Verify examples in CI where practical.

**Exit gate:** common app tasks need no undocumented workaround; APIs and examples match actual behavior.

## Phase 8 — Performance, accessibility, and DX

### 8.1 Performance
- [ ] Benchmark reactivity, DOM updates, SSR throughput, latency, and memory.
- [ ] Track client bundle size and startup cost.
- [ ] Profile large lists, nested components, and frequent updates.
- [ ] Define measurable performance budgets.
- [ ] Optimize only from reproducible measurements.

### 8.2 DX
- [ ] Improve macro diagnostics.
- [ ] Add hot reload only with documented state-preservation semantics.
- [ ] Improve incremental build behavior.
- [ ] Document editor support and formatting.
- [ ] Test dev/production behavior differences.

### 8.3 Accessibility and compatibility
- [ ] Test keyboard interaction and focus management.
- [ ] Define supported browser policy.
- [ ] Test Chromium, Firefox, and WebKit where CI permits.
- [ ] Document unsupported browser APIs and progressive enhancement.

**Exit gate:** benchmarks are reproducible, accessibility is part of review, and supported targets are explicit.

## Phase 9 — Security and supply-chain hardening

### 9.1 Threat model
- [ ] Threat-model SSR, hydration, sessions, cookies, server actions, uploads, and database access.
- [ ] Document trust boundaries and untrusted input.
- [ ] Review XSS, CSRF, SSRF, path traversal, request smuggling, and denial of service where applicable.
- [ ] Require a security checklist for sensitive features.

### 9.2 Automation
- [ ] Run dependency advisory checks and secret scanning.
- [ ] Inventory and review unsafe Rust.
- [ ] Fuzz parsers, routing, deserialization, and exposed input surfaces.
- [ ] Add regression tests for escaping and authorization.
- [ ] Audit CI actions and release dependencies.

### 9.3 Operations
- [ ] Document TLS, secure headers, cookies, proxy trust, and rate limiting.
- [ ] Ensure production errors do not leak secrets or internals.
- [ ] Document backups, restores, incident response, and key rotation where relevant.
- [ ] Verify private vulnerability reporting instructions.

**Exit gate:** release-scope threat model and security review are complete; unresolved high-severity findings block release.

## Phase 10 — Release candidate and production readiness

### 10.1 API and compatibility
- [ ] Review public APIs and experimental feature labels.
- [ ] Define 1.0 API and deprecation policy.
- [ ] Test feature combinations and supported targets.
- [ ] Audit panic paths and error propagation.

### 10.2 Reliability
- [ ] Run unit, integration, browser E2E, and migration suites.
- [ ] Run load and soak tests on representative applications.
- [ ] Test shutdown, dependency outages, timeouts, and pool exhaustion.
- [ ] Test repeated navigation and memory/lifecycle stability.
- [ ] Verify rollback and recovery procedures.

### 10.3 Deployment
- [ ] Publish production build and deployment guides.
- [ ] Document TLS/reverse proxy, environment, health checks, and asset caching.
- [ ] Document migration rollout and rollback.
- [ ] Document logs, metrics, tracing, alerting, backup, and restore.
- [ ] Produce a dependency inventory/SBOM where feasible.

### 10.4 Release
- [ ] Verify license and third-party notices.
- [ ] Publish release notes and known limitations.
- [ ] Tag a release candidate and attach reproducible test evidence.
- [ ] Obtain independent review and explicit maintainer approval.
- [ ] Publish 1.0 only after all mandatory gates pass.

**Exit gate:** security issues are resolved, compatibility is documented, deployment is tested, and maintainers approve the release.

## Phase 11 — Ecosystem and maintenance

### 11.1 Ecosystem
- [ ] Add extension points only for validated use cases.
- [ ] Publish component library and design-system guidance.
- [ ] Maintain official integrations and starter templates.
- [ ] Consider internationalization and advanced accessibility tooling.
- [ ] Establish community support pathways.

### 11.2 Maintenance
- [ ] Maintain Rust/dependency support policy.
- [ ] Publish security advisories and patch releases.
- [ ] Track browser/database compatibility and regressions.
- [ ] Maintain migration tools and upgrade guides.
- [ ] Revisit ADRs and retire obsolete abstractions.

**Exit gate:** repeatable release, support, security, and compatibility processes are established.

## MVP definition

The MVP must demonstrate:
1. A typed Rust function component with local reactive signal state and typed props, using the intended ergonomic public API.
2. Signal-backed view interpolation that updates automatically and patches the relevant DOM region.
3. Typed DOM event handlers and component callback props, with cleanup on component unmount.
4. A todo example using the same public API, with conditional rendering and keyed-list reconciliation that preserves identity.
5. A Rust server serving routes and safe HTML/JSON responses.
6. Unit, browser, and server integration tests in CI.
7. A clean-checkout developer guide.

SSR/hydration, full authentication, a custom ORM, and production deployment are later milestones. Do not imply they are part of the MVP.

## Definition of done

- [ ] Acceptance criteria pass.
- [ ] Normal, edge, and failure paths have tests.
- [ ] Public APIs and limitations are documented.
- [ ] Security, accessibility, and performance impact is considered.
- [ ] Examples and CI are updated.
- [ ] Breaking changes are recorded.
- [ ] No secrets, build artifacts, or unrelated changes are included.
