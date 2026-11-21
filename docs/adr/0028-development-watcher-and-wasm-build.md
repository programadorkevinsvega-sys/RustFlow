# ADR 0028: Development watcher and WASM build contract

- **Status:** Accepted - implementation for Phase 2.3
- **Date:** 2026-11-21
- **Phase:** 2.3 - Development watcher and WASM build

## Context

`rustflow dev` previously delegated once to `cargo run` and explicitly did not
watch files or rebuild browser/WASM assets. This leaves no restart lifecycle,
no build-failure recovery, and no deterministic handling of terminated child
processes.

The implementation must preserve one-shot Cargo behavior and add a bounded
development supervisor at the CLI composition boundary.

## Decision

`rustflow dev` watches by default in the nearest RustFlow project.

- `--once` performs one native build and one application run.
- `--browser-dir <dir>` validates an existing browser Cargo project and rebuilds
  its WASM package with `wasm-pack` after relevant native/browser changes.
- `--release`, `--offline`, and `--locked` are forwarded to Cargo.
- Application arguments after `--` are forwarded only to the application.
- Native builds use Cargo and spawn the resolved application binary directly;
  this avoids leaving a `cargo run` wrapper between the CLI and the process.
- Watch events are debounced and filtered. Generated `target`, `node_modules`,
  `pkg`, `dist`, and `.git` paths do not trigger rebuilds.
- A failed build leaves the watcher alive and does not start stale application
  output. A later successful build restarts the application.
- SIGINT/SIGTERM handling stops the current application child and exits the
  supervisor without starting another build.
- The watcher does not preserve application state; restart is explicit process
  recreation. Browser state preservation and hot-module state transfer remain
  outside this contract.

## Browser/WASM scope

The browser adapter remains the existing Cargo/wasm-pack/Vite flow. The watcher
does not invent a second bundler or a hidden browser runtime.

With `--browser-dir`, RustFlow rebuilds `pkg/rustflow_browser.js` and the WASM
artifact on relevant source changes. Starting or supervising Vite remains a
separate opt-in developer action. The browser directory must contain
`Cargo.toml`; wasm-pack and the `wasm32-unknown-unknown` target remain external
tools and failures are reported by stable public diagnostics.

## Alternatives considered

1. **Keep `cargo run` as the watcher process:** rejected because killing Cargo
   does not reliably terminate the application process or prevent process
   orphans.
2. **Preserve application state during restart:** rejected because the repository
   has no reviewed state-transfer or hydration-preservation protocol.
3. **Add a Rust-owned browser bundler:** rejected because Vite/wasm-pack remain
   maintained tools and the phase requires orchestration, not bundler ownership.
4. **Watch every file:** rejected because Cargo targets, dependencies, generated
   WASM output and directory noise create loops and false rebuilds.

## Consequences

- Native development gains bounded rebuild/restart behavior.
- Build and process failures are visible and do not emit stale success defaults.
- Users can opt into browser WASM rebuilds with an explicit directory.
- Incremental build caching is delegated to Cargo/wasm-pack.
- Browser state, hot reload, and E2E server proxying are not implied by
  `rustflow dev`.

## Verification

Required evidence includes:

- unit tests for relevant/deleted/ignored watch events;
- CLI integration tests for one-shot execution and invalid projects;
- a real watch integration test covering initial start, source edit, rebuild,
  restart and SIGINT cleanup;
- build-failure recovery after correcting the source;
- WASM tool/target missing diagnostics;
- browser E2E after rebuilding when browser output is part of a changed PR.
