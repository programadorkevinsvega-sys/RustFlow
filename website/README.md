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

The site is configured for a GitHub Pages project site at https://programadorkevinsvega-sys.github.io/RustFlow/. The CI workflow validates builds. Before enabling automatic publishing, configure the repository's Pages source to **GitHub Actions** and add the deployment workflow/permissions appropriate to the repository settings.
