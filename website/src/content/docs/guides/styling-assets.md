---
title: Estilos y assets
description: Integra CSS, clases, imágenes, fuentes y archivos estáticos con el pipeline de build real de RustFlow.
---

RustFlow separa la vista estructural del pipeline de archivos del navegador. Los componentes pueden declarar atributos como <code>class</code>, pero el framework no proporciona CSS-in-Rust, un registro de estilos, un gestor de assets ni un resolvedor de URLs públicas.

## 1. Escribir CSS normal

Usa clases CSS en los elementos y administra las hojas de estilo desde la aplicación.

~~~rust
rustflow::view! {
    <main class="page-shell">
        <section class="product-card">
            <h1>"Producto"</h1>
            <p class="product-card__description">"Descripción del producto"</p>
        </section>
    </main>
}
~~~

~~~css
.page-shell {
  max-width: 72rem;
  margin-inline: auto;
  padding: 1.5rem;
}

.product-card {
  border: 1px solid #d7dce2;
  border-radius: 0.75rem;
  padding: 1.25rem;
}

.product-card__description {
  line-height: 1.6;
}
~~~

El renderer DOM actual rechaza el atributo inline <code>style</code>. Esta decisión
está registrada en [ADR 0027](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/adr/0027-static-files-and-inline-style-policy.md).
Usa clases y hojas CSS en lugar de asumir que existe una API de estilos en línea.
La futura incorporación de archivos estáticos seguirá siendo opt-in en el adapter
HTTP y no cambiará esta política hasta que exista un contrato seguro aprobado.

## 2. Conectar CSS en el ejemplo de navegador

El ejemplo oficial de navegador utiliza Vite y wasm-pack. La configuración de Vite emplea una base relativa (<code>./</code>) para permitir despliegues bajo una ruta de aplicación. En una aplicación basada en ese ejemplo, importa un CSS desde el módulo de entrada:

~~~javascript
import "./styles.css";
import init from "../pkg/rustflow_browser.js";

init().catch((error) => {
  console.error("RustFlow failed to initialize", error);
});
~~~

Ubica <code>styles.css</code> junto al módulo de entrada y conserva la importación para que Vite lo procese durante el build. Este es un mecanismo de la herramienta de build de la aplicación, no una API runtime de RustFlow.

## 3. Imágenes y texto alternativo

Usa elementos HTML semánticos y atributos admitidos:

~~~rust
rustflow::view! {
    <figure>
        <img
            src="/images/product-main.webp"
            alt="Vista frontal del producto sobre fondo claro"
            width="960"
            height="720"
        />
        <figcaption>"Vista frontal del producto"</figcaption>
    </figure>
}
~~~

Los valores URL están sujetos a las políticas de validación del renderer. No construyas URLs de assets con entrada no confiable ni utilices un atributo de URL para incrustar contenido arbitrario. Un atributo <code>alt</code> descriptivo debe comunicar el propósito de la imagen; usa <code>alt=""</code> para imágenes puramente decorativas.

## 4. Archivos públicos y fuentes

Vite copia los archivos de su directorio <code>public/</code> al directorio de salida y conserva sus nombres. Las referencias deben coincidir con la ruta pública resultante. Los assets importados desde CSS o JavaScript pasan por el grafo de build y pueden recibir nombres/versiones de producción. Elige una estrategia y no mezcles rutas absolutas de desarrollo con URLs relativas de despliegue sin probar ambas.

Ejemplo para una fuente local en una hoja CSS:

~~~css
@font-face {
  font-family: "Application Sans";
  src: url("/fonts/application-sans.woff2") format("woff2");
  font-display: swap;
}

body {
  font-family: "Application Sans", system-ui, sans-serif;
}
~~~

Asegúrate de que la carpeta/asset exista y de que el servidor estático del despliegue publique la salida de Vite. El servidor HTTP RustFlow no incorpora un manifest de assets ni un middleware universal de archivos estáticos por esta guía.

## 5. Desarrollo y producción

- En desarrollo, Vite sirve los módulos y CSS de la aplicación.
- En producción, ejecuta el build Vite y publica el contenido de <code>dist/</code> junto al despliegue WASM esperado por el glue generado.
- Si tu aplicación se despliega en un subpath, prueba la URL final, las rutas de imágenes/fuentes y la base del bundle desde ese subpath.
- No dependas de un artefacto generado que solo exista en tu máquina; el build debe reproducir assets desde fuentes versionadas y dependencias declaradas.

RustFlow no abstrae CDN, almacenamiento remoto, transformación de imágenes, preload de fuentes ni invalidación de caché. Esas decisiones pertenecen al pipeline de build y hosting de la aplicación.

## Archivos estáticos servidos por RustFlow

El adapter HTTP puede montar un directorio bajo un prefijo público mediante una
API opt-in. La configuración declara la raíz, el tamaño máximo, la caché y el
archivo índice opcional.

Para servir archivos desde el adapter HTTP, configura una raíz canónica y
un prefijo público explícito:

~~~rust
let files = StaticFiles::new("dist")?
    .with_max_file_bytes(10 * 1024 * 1024)?
    .with_cache_control("public, max-age=3600")?
    .with_index_file("index.html")?;

let mut app = Application::new();
app.static_files("/assets", files)?;
~~~

`StaticFiles` rechaza rutas absolutas, traversal, dotfiles y symlinks. Los
archivos se sirven con GET/HEAD, MIME determinístico, límite de tamaño y caché
configurados. El SPA fallback es el archivo índice opt-in; no sustituye
autorización ni middleware de rutas protegidas.

El adapter también admite:

- Range único `bytes=start-end`, `bytes=start-` y `bytes=-suffix`.
- `Accept-Ranges: bytes`, `Content-Range`, `206` y `416` con rango disponible.
- `ETag`, `Last-Modified`, `If-None-Match` e `If-Modified-Since`.
- `If-Range` para evitar un Range cuando el cliente conserva otra versión.

No hay streaming incremental desde disco ni soporte de múltiples rangos. Los
bytes se leen dentro del límite configurado y se entregan como una respuesta
acotada. Para archivos grandes, conserva el límite explícito y usa un CDN o
servidor de despliegue que implemente streaming.

La prueba de carga reproducible vive en
`scripts/static-files.k6.js`. Con un servidor que expone
`RUSTFLOW_STATIC_PATH`:

~~~sh
RUSTFLOW_STATIC_BASE_URL=http://127.0.0.1:3001 \
RUSTFLOW_STATIC_PATH=/assets/app.css \
k6 run scripts/static-files.k6.js
~~~

El script usa 10 usuarios virtuales durante 15 segundos y exige menos de 1%
de fallos y p95 menor a 50 ms.
