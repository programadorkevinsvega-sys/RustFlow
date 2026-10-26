---
title: Preparar el entorno
description: Ejecuta localmente el portal de documentación de RustFlow.
---

Esta guía es para contribuir al **portal de documentación**, no para instalar RustFlow como dependencia de una aplicación. El framework todavía está en desarrollo y no ofrece aún un flujo de instalación de producción.

## Requisitos

- Git.
- Node.js y npm en versiones compatibles con la versión de Astro declarada en website/package.json.

## Ejecutar el sitio

Desde la raíz del repositorio:

~~~sh
cd website
npm install
npm run dev
~~~

Abre la URL local que Astro muestre en la terminal.

## Validar una compilación estática

~~~sh
npm run build
npm run preview
~~~

Astro genera el sitio estático en website/dist/. El contenido publicado no requiere un servidor de aplicación ni una base de datos.

## Añadir una página

Crea un archivo Markdown dentro de website/src/content/docs/, por ejemplo guides/mi-guia.md:

~~~md
---
title: Mi guía
description: Qué aprenderá el desarrollador.
---

Explica el concepto, muestra un ejemplo verificable y describe las limitaciones actuales.
~~~

Añade después la página a la barra lateral en website/astro.config.mjs cuando deba aparecer en la navegación principal.
