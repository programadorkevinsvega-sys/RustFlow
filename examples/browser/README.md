# RustFlow browser MVP

This package builds a real browser-loadable WebAssembly app using the Phase 3 DOM adapter. It includes a signal-backed Counter component, a conditionally mounted help message, and a keyed todo list with add, complete, remove, and reorder interactions.

The bundle is an implementation milestone, **not proof that Phase 3 is closed**. Component descriptors can still use safe range replacement when their props/trees change; complete component identity-preserving reconciliation remains an explicit blocker.

## Clean Ubuntu setup

Prerequisites: Ubuntu 22.04/24.04, Git, a Rust toolchain installed with rustup (1.85 or newer), Node.js 22 with npm, and network access to crates.io/npm/Playwright browser downloads.

Install native build tools and the WebAssembly target:

    sudo apt-get update
    sudo apt-get install -y build-essential curl pkg-config libssl-dev
    rustup toolchain install stable --component rustfmt clippy
    rustup default stable
    rustup target add wasm32-unknown-unknown

## Build and run

From this directory:

    cargo install wasm-pack --locked --version 0.13.1
    npm install
    npm run build
    npm run check:bundle
    npm run dev

Open the Vite URL printed in the terminal. The Rust entry point is annotated with #[wasm_bindgen(start)]; Vite loads the generated JavaScript glue and rustflow_browser_bg.wasm. npm run build produces static files in dist/ and Vite JavaScript source maps.

For a debug-oriented WASM artifact, run npm run build:wasm:debug before npm run dev. This command replaces the generated glue/WASM under pkg/ with the debug build; rerun npm run build to restore the release build.

## Browser E2E

    npx playwright install --with-deps chromium
    npm run test:e2e

The browser tests verify that clicking a nested Counter child reaches the delegated handler and retains the existing DOM element; the todo test covers add, complete, remove, and reordering while preserving keyed row identity; a conditional help region mounts and unmounts without replacing its parent heading. CI reports raw and gzip WASM sizes and enforces a 1.5 MiB raw WASM budget.

## Troubleshooting

- **wasm32-unknown-unknown is not installed:** run rustup target add wasm32-unknown-unknown.
- **Cargo cannot resolve web-sys features:** update the branch, then run cargo check -p rustflow-dom --target wasm32-unknown-unknown; feature names must match the published web-sys API.
- **The bundle-size script says the WASM file is missing:** run npm run build first.
- **Playwright cannot launch Chromium:** run npx playwright install --with-deps chromium again and inspect the failed Playwright trace.
- **A later reactive update fails:** inspect the browser console; the retained ReactiveMountedDom also exposes take_error() so the application can examine the most recent render error.

This is an early-stage framework example, not a production-ready application. Server-side rendering, hydration and deployment work remain outside the current browser MVP.
