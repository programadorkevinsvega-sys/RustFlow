# ADR 0006: Safe HTML serialization for structural views

- **Status:** Proposed for formal review; safe structural HTML serialization is implemented in the current `rustflow-ssr` runtime. This status tracks formal ADR approval, not whether the serializer exists.
- **Date:** 2026-10-26
- **Phase:** 4.4 — Safe server rendering foundation

## Context

The structural view model is data, not HTML. Server-side rendering needs a dependency-light serializer that escapes output, enforces a conservative tag and URL policy, and resolves reusable components without sharing runtime state across renders.

## Decision

Add the \`rustflow-ssr\` crate. It depends on \`rustflow\` and \`rustflow-view\`, but not a specific HTTP server or browser API. \`HtmlRenderer\` creates an isolated reactive runtime for every render, optionally resolves component descriptors through a cloned \`ComponentRegistry\`, and disposes runtime-owned scopes before returning. The API exposes \`render_to_string\` for trees without component descriptors and \`render_with_components\` for registered component trees.

The serializer:

- Escapes text and attribute values, including ampersands, angle brackets, quotes, and apostrophes.
- Allows only a conservative list of common HTML elements and rejects void elements with children.
- Rejects event-handler attributes, raw-HTML sinks, inline \`style\`, \`srcset\`/\`ping\` until a multi-URL parser exists, and URL schemes other than \`http\`, \`https\`, \`mailto\`, \`tel\`, or relative URLs.
- Omits event callbacks from HTML. Event wiring and hydration are later work.
- Serializes a small documented subset of reflected DOM properties to initial HTML attributes, such as input \`value\` and \`checked\`; non-reflectable client properties remain client-only.
- Enforces a configurable maximum nested-view depth.
- Can resolve nested component descriptors through the same registry and invokes instance cleanup after rendering each component.

## Alternatives considered

- **Put HTML serialization in rustflow-view:** would mix a renderer policy with target-independent structural types.
- **Put web-sys in the server renderer:** would incorrectly couple HTML serialization to a browser.
- **Add SSR to the HTTP server immediately:** would make this safety-critical transformation dependent on unfinished routing and request/response APIs.
- **Use a separate rustflow-ssr crate:** allows native deterministic tests, a clear safety boundary, and later integration with any selected server adapter.

## Consequences and limitations

- Safe HTML fragments can be rendered without bringing Axum/Tokio into the view model.
- This is not an HTTP server, full-document/layout system, hydration implementation, or production security certification.
- The renderer is intentionally stricter than general HTML and may reject tags/attributes that applications eventually need. Policy expansions require tests.
