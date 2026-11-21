# ADR 0027: Static files and inline style policy

- **Status:** Accepted
- **Date:** 2026-11-20
- **Phase:** 2.1 - Architecture decision for styles and static assets

## Context

RustFlow currently rejects the `style` attribute and property in its shared DOM
policy. The same policy is used for browser mutation and hydration validation,
and the server-side structural serializer already rejects inline styles.
Removing that restriction would introduce two coupled problems:

- arbitrary CSS is a broad parsing and security surface;
- style values can carry browser-specific URL or behavioral features that are
  difficult to classify consistently across browsers.

The HTTP adapter also does not currently provide a static-file service.
Applications can deploy Vite assets outside RustFlow or serve files through
their own web server, but the public API does not define a RustFlow-owned
contract for filesystem serving.

This decision separates presentation policy from file serving. It does not
implement either capability beyond the current rejection of inline styles.

## Decision

### Inline style policy

Keep inline `style` rejected by RustFlow.

- Reject `style` as both an attribute and a reflected/property sink.
- Reject it in SSR, browser mounting, reconciliation and hydration checks.
- Do not add CSS parsing or sanitization in this phase.
- Direct applications to external CSS, classes and application-owned build
  pipelines such as Vite.
- Permit style implementation later only through a separate, reviewed contract
  that defines a typed property allowlist, browser/security behavior, SSR
  serialization and compatibility tests.

This preserves a fail-closed default. CSS sanitization libraries can remove or
rewrite declarations, but they do not remove the need to define URL handling,
custom properties, viewport behavior, CSS injection boundaries and browser
differences. Adding arbitrary inline CSS now would widen the renderer's
security and maintenance surface without a demonstrated framework requirement.

### Static files

Define static files as an opt-in server-adapter capability. It belongs in
`rustflow-server`, not in the view model or domain/application crates.

A future `StaticFiles` API must accept:

- an absolute or application-relative source directory;
- an explicit public URL prefix;
- a maximum file size;
- cache policy;
- a directory-listing policy that defaults to disabled;
- dot-file policy that defaults to disabled;
- an explicit file-symlink policy.

The initial implementation contract must:

- resolve each request against one canonical root;
- reject missing roots and ambiguous public prefixes;
- reject absolute request paths and normalized `..` traversal;
- reject symlinks or require canonical containment for every served file;
- serve only regular files;
- support GET and HEAD only;
- return generic 404/405 responses without exposing filesystem paths;
- set MIME type from the approved content policy;
- apply cache headers and `Cache-Control` explicitly;
- avoid directory listings and hidden-file serving;
- keep SPA fallback opt-in and route it through an explicit HTML file;
- compose with existing global and route middleware;
- preserve the existing request timeout and concurrency limit.

Filesystem failures must be classified without leaking absolute paths or
platform-specific details. Permission errors, missing files and unsupported
media types must use stable public outcomes.

## Alternatives considered

1. **Allow arbitrary inline style:** rejected because arbitrary CSS expands
   the browser/security surface and is not necessary for external CSS workflows.
2. **Sanitize inline CSS now:** rejected because there is no current CSS
   contract or demonstrated need sufficient to justify parser dependencies and
   browser-specific policy.
3. **Implement static serving as a core/framework port:** rejected because
   serving files is an HTTP/filesystem adapter concern. Public configuration
   can be application-owned while concrete serving stays in `rustflow-server`.
4. **Auto-mount static files from a conventional directory:** rejected because
   implicit routes and filesystem roots can expose unintended files and conflict
   with application routes.
5. **Serve files through direct string concatenation:** rejected because it
   permits traversal, symlink escape and platform-dependent path ambiguity.
6. **Implement a custom static server from scratch:** rejected unless mature
   Axum/Tower integrations prove insufficient. The implementation phase should
   prefer a maintained, reviewed adapter and focused containment tests.

## Consequences

- Inline styles remain unavailable, with external CSS as the documented
  alternative.
- Static serving is not an implemented capability until the 2.2 subphase.
- A future implementation will be opt-in and explicit.
- Applications remain responsible for deployment-time publishing of generated
  assets, cache/CDN policy and secret exclusion.
- Static serving must not weaken existing XSS escaping, URL validation,
  authorization or route middleware.

## Security and operational risks

- Traversal and percent-encoded path ambiguity.
- Symlink escape from a published directory.
- Accidental publication of dotfiles, secrets or deployment files.
- MIME sniffing or wrong MIME headers.
- Cache disclosure or stale application content.
- Large files consuming memory or saturation budgets.
- Files changing after validation and before opening.
- SPA fallback masking missing application routes or authorization boundaries.
- Symlink/canonicalization behavior differing across platforms.

## Verification required for acceptance

This ADR requires no server implementation or k6 run.

- Confirm that `style` remains rejected for attributes and properties.
- Confirm that DOM and SSR policy documentation use the same decision.
- Confirm that static files are listed as unavailable until implemented.
- Add implementation tests in subphase 2.2 for:
  - root resolution and invalid configuration;
  - `..`, absolute paths and encoded traversal;
  - symlinks and canonical containment;
  - dotfiles and directory listing;
  - GET/HEAD and unsupported methods;
  - MIME and cache headers;
  - missing, unreadable and oversized files;
  - middleware and authorization composition;
  - HTTP timeout/concurrency behavior;
  - k6 load and latency thresholds only if 2.2 changes server capacity.
