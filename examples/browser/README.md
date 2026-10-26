# RustFlow browser MVP

This package produces a real browser-loadable WebAssembly app using the Phase 3 DOM adapter. It includes a signal-backed counter component, a conditionally mounted help message, and a keyed todo list with add, complete, remove, and reorder interactions.

## Build and run

From this directory:

```sh
cargo install wasm-pack --locked --version 0.13.1
npm install
npm run build:wasm
npm run check:bundle
npm run dev
```

Open the Vite URL printed in the terminal. The Rust entry point is annotated with `#[wasm_bindgen(start)]`; Vite loads the generated `pkg/rustflow_browser.js` glue and the `rustflow_browser_bg.wasm` module.

## Browser E2E

```sh
npx playwright install chromium
npm run test:e2e
```

The browser tests verify that counter updates retain the relevant DOM element, conditional regions mount and unmount, and keyed todo nodes keep their identity when their order changes. CI also reports raw and gzip WASM sizes and enforces a 1.5 MiB raw WASM budget.

This is an early-stage framework example, not a production-ready app. The server-side rendering, hydration, and deployment phases are deliberately outside this package.
