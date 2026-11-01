# Phase 7.2 — UI completeness: audit and implementation record

- Base <code>main</code> commit audited: <code>207c61c6fa2751a4d7bbd4b5c914a2cbb8588401</code>
- Implementation branch: <code>feat/ui-phase-7-2-completeness</code>
- Status: Phase 7.2 implementation, regression tests, technical contracts/ADRs, and public website documentation are delivered together in PR #70. The public APIs remain experimental and pre-1.0.

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

The regression tests listed in Section 5 are included in this change. Verification results belong to the exact final PR SHA and must be read from GitHub checks; source additions alone are not evidence that tests passed.

## 4. Public contracts and limitations

### Memo

<code>use_memo</code> computes immediately, is eager, and returns a read-only cloneable memo that can be interpolated as a dynamic view child. It tracks only signals read by its latest successful computation. Recompute occurs at the runtime's next scheduler flush; within an open batch, reads return the previous committed value. <code>PartialEq</code> equality of results suppresses downstream notifications. A failed initial computation cleans up its newly created effect and resumes the panic. A later panic follows the runtime's existing effect panic policy: the failed run detaches its dependencies and the panic propagates. Use a typed result as the memo value for recoverable domain errors.

### Ref

<code>use_ref</code> owns non-reactive Rust storage. It does not notify effects. Access uses closures or cloned reads where <code>T: Clone</code>; it does not expose raw pointers or borrow guards. A cloned handle becomes invalid when the owner disposes. It is intentionally distinct from browser element references.

### DOM element references

The target-independent view crate provides <code>NodeRef&lt;T&gt;</code> and <code>Element::with_node_ref</code>. The browser adapter exports <code>ElementRef</code> as a handle to <code>web_sys::Element</code>. Native elements can use the declarative <code>ref={element_ref}</code> attribute in <code>view!</code> or the builder method. Component descriptors cannot accept refs because a component may render zero, one, or multiple DOM elements.

A new handle is empty during view construction and on the server. The browser renderer attaches it only after a successful mount or after the existing SSR range has passed hydration validation. Reconciliation keeps the reference on a compatible element, changes it to a replacement element, and clears it when its node is removed or the owning mounted range is unmounted. Detached prepared mounts do not expose nodes. Cleanup compares DOM node identity before clearing so an old mount cannot erase a newer assignment. One handle may identify only one native element at a time; duplicate use or conflicting mounts are rejected with a DOM error.

<code>ElementRef::get()</code> returns a cloned browser element handle or <code>None</code> before mount/after unmount. Reading or changing a node through this handle is not reactive and does not schedule effects. It does not create a DOM node, force a render, or make browser globals available during SSR.

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
- View-model unit tests cover fragment structure, empty fragments, keyed ordering, duplicate keys, typed node-reference identity, and conditional clearing.
- Browser E2E covers keyed list reorder/state/cleanup, controlled-textbox caret preservation, DOM reference mount/replacement/unmount, and reference attachment during successful SSR hydration.
- SSR isolation, fragments, inputs, and hydration compatibility must continue to pass the existing server-side tests.
- Required GitHub checks must be read from the current ruleset and reported individually against the final PR SHA. No result is inferred from a started workflow.
