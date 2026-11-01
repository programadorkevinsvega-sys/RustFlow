# Phase 7.2 — UI completeness: audit and implementation record

- Base <code>main</code> commit audited: <code>207c61c6fa2751a4d7bbd4b5c914a2cbb8588401</code>
- Implementation branch: <code>feat/ui-phase-7-2-completeness</code>
- Status: implementation in progress; pull-request checks and maintainer review are required before acceptance.

## 1. Architectural audit

The framework is split across <code>rustflow</code>, <code>rustflow-view</code>, <code>rustflow-dom</code>, <code>rustflow-ssr</code>, and the component macro crate. Reactivity is a synchronous, single-threaded graph built from <code>Rc&lt;RefCell&lt;_&gt;&gt;</code>, owner scopes, signals, eager effects, dynamic dependency collection, FIFO scheduling, batching, and deterministic disposal. Component setup receives an owner context; the render function is repeated and must not create owner-scoped resources.

The browser adapter owns DOM nodes and event listeners. The view model is target-independent. SSR builds isolated render state and does not manufacture browser nodes. These boundaries are preserved.

## 2. Capabilities already present at the base commit

- Signal-based reactive values, eager effects, dynamic dependency tracking, batching, cleanup, and owner scopes.
- A component setup/render split and component-owned child scopes.
- Target-independent fragments that preserve sibling order without adding an element wrapper.
- Keyed lists with duplicate-key rejection and key-based reconciliation. Existing browser E2E covers reorder, preservation of component-local state, insertion/removal, and cleanup.
- Browser renderer support for DOM properties and typed events; SSR structural rendering and opt-in hydration with conservative mismatch handling.
- Native HTML forms and input/select/textarea elements through the view model. The framework does not provide a general client-side form controller abstraction.
- HTTP-side typed request-form extraction, validation, CSRF integration, and progressive form behavior are separate server capabilities; they are not the same API as controlled browser inputs.

These existing capabilities were audited rather than reimplemented.

## 3. Phase 7.2 changes on this branch

- Adds owner-scoped eager memos that use the existing effect dependency collector and an output signal. Dependencies are dynamic; equality of the derived result suppresses output notifications.
- Adds owner-scoped non-reactive references with short-lived borrow closures and deterministic invalidation on owner disposal.
- Adds unique typed context identities, per-scope provider values, nearest-ancestor lookup, missing-provider results, and component-context convenience methods.
- Extends the public prelude and component macro's setup/render classification for the new owner-scoped APIs.
- Avoids rewriting an equal DOM property during reconciliation, which prevents unrelated updates from resetting a controlled input's live value/caret unnecessarily.
- Adds unit and browser regression scenarios for the new runtime contracts and controlled-input selection preservation.

The final status of these changes depends on compilation and CI for the final PR SHA; source additions are not represented as passing tests until those checks complete.

## 4. Public contracts and limitations

### Memo

<code>use_memo</code> computes immediately, is eager, and returns a read-only cloneable memo that can be interpolated as a dynamic view child. It tracks only signals read by its latest successful computation. Recompute occurs at the runtime's next scheduler flush; within an open batch, reads return the previous committed value. <code>PartialEq</code> equality of results suppresses downstream notifications. A failed initial computation cleans up its newly created effect and resumes the panic. A later panic follows the runtime's existing effect panic policy: the failed run detaches its dependencies and the panic propagates. Use a typed result as the memo value for recoverable domain errors.

### Ref

<code>use_ref</code> owns non-reactive storage. It does not notify effects. Access uses closures or cloned reads where <code>T: Clone</code>; it does not expose raw pointers or borrow guards. A cloned handle becomes invalid when the owner disposes. DOM element references are not included yet; do not claim that a <code>use_ref</code> contains a mounted browser element.

### Context

A context token has identity independent of <code>T</code>. The nearest provider in the owner ancestry wins. A missing provider is <code>None</code>; runtime errors are available through <code>try_use_context</code>. Lookup is not reactive by itself. Store a signal as the value if a consumer should react to changes. Provider state is local to the runtime/owner tree, so a shared token does not share values across SSR requests.

### Lists and fragments

Keyed lists reject duplicate keys and preserve compatible keyed regions across reordering. Fragments preserve sibling grouping and do not add an HTML element wrapper. Hydration remains opt-in and requires the supported boundary protocol; mismatches are surfaced rather than silently repaired.

### Forms and controlled inputs

Controlled values use DOM properties such as <code>value</code> and <code>checked</code> plus input/change handlers that update the source signal. Uncontrolled values use initial HTML attributes/properties and then remain under browser control. Equal controlled property values are not written again during reconciliation. RustFlow does not promise cursor transformation when the model value itself changes, nor a dedicated IME/composition abstraction. The supported native controls must be verified against the renderer's element/property allowlists.

### Error behavior

There is no component-level error-boundary/fallback API. Initial component/mount and SSR errors propagate as render errors. Reactive DOM update failures are retained by <code>ReactiveMountedDom::take_error</code>; the last successful range is preserved on failed updates. Runtime effect panics restore internal graph invariants, detach failed-run dependencies, and propagate; they are not converted into a fallback view. Browser event-handler panic isolation is not promised. The HTTP <code>ErrorReporter</code> added in Phase 7.1 reports selected HTTP failures and is not a UI error boundary.

### Asynchronous rendering

Effects and cleanup are synchronous. There is no supported owner-bound resource, cancellation, stale-result guard, Suspense, or loading boundary contract. Do not add an ad-hoc async UI primitive until cancellation, SSR, hydration, errors, and scheduling are designed together.

### Styling and assets

The view layer supports regular attributes, including <code>class</code>, but rejects inline <code>style</code> in the browser renderer. There is no CSS-in-JS, style registry, asset manifest, or server asset URL resolver in the framework core. Browser example builds use Vite; CSS and static assets belong to the consuming application's Vite/public build pipeline.

### Accessibility

Use semantic HTML controls and relationships such as <code>label</code>/<code>for</code>, native button types, headings, lists, image alt text, visible focus, and keyboard-operable controls. ARIA supplements missing semantics; it does not replace native elements.

## 5. Verification inventory

- Reactivity unit tests in <code>crates/rustflow/src/reactivity.rs</code> exercise memo execution counts, cache equality, dynamic dependencies, batching, disposal, refs, contexts, and panic cleanup.
- View-model unit tests already cover fragment structure, empty fragments, keyed ordering, and duplicate keys.
- Browser E2E already covers keyed list reorder/state/cleanup and hydration; this branch adds a controlled-textbox caret regression.
- SSR isolation, fragments, inputs, and hydration compatibility must continue to pass the existing server-side tests.
- Required GitHub checks must be read from the current ruleset and reported individually against the final PR SHA. No result is inferred from a started workflow.
