# ADR 0008: Safe HTML document shells and configurable error pages

- **Status:** Proposed for review
- **Date:** 2026-10-26
- **Phase:** 4.4 — Safe server rendering

## Context

`rustflow-ssr` intentionally emits safe HTML fragments and does not own HTTP policy. The HTTP adapter needs a safe way to wrap a structural view in a complete document and to customize 4xx/5xx bodies without inserting raw strings into HTML.

## Decision

Expose `html_document(title, lang, view)` and a component-aware counterpart from `rustflow-server`. The document shell (doctype, html/head/meta/title/body) is built from structural view nodes and serialized by `rustflow-ssr`; the serializer supports the void `meta` element and its `charset` attribute. User-provided title/message/body text is never emitted as raw HTML.

Add `ErrorPages` as a server configuration value keyed by 4xx/5xx `StatusCode`. When configured, the outer HTTP adapter replaces that status body's content with a safe HTML document while preserving status and correlation headers. Unconfigured statuses retain their normal response. The application still owns layouts, localization decisions, and broader error reporting.

## Consequences

- A full HTML document can be returned without bypassing the SSR safety policy.
- Router-generated 404/405 responses can use custom safe pages and still echo `x-request-id`.
- `Response::html(String)` remains an explicit raw-string escape hatch for trusted content only; it is not an HTML sanitizer.
- Hydration, layout composition, and broader HTTP E2E coverage remain separate work.
