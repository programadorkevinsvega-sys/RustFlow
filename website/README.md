# RustFlow documentation website

Public developer documentation built with Astro and Starlight. Content is primarily Markdown/MDX and is generated as a static site; the published site does not need a dynamic application backend.

## Local development

~~~sh
npm install
npm run dev
~~~

## Build and preview

~~~sh
npm run build
npm run preview
~~~

## Structure

- src/content/docs/: public documentation pages.
- astro.config.mjs: site metadata and sidebar navigation.
- dist/: generated static output (do not commit build artifacts).

## Publishing

The canonical site is https://rustflow.pages.dev (Cloudflare Pages), as configured in astro.config.mjs. GitHub Actions validates the static-site build on pull requests and main; the Cloudflare Pages status check records deployment for the relevant commit. Do not configure GitHub Pages unless the hosting strategy is deliberately changed.
