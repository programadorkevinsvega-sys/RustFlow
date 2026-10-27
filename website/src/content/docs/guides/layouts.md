---
title: Componer páginas con layouts
description: Construye una página SSR con componentes de layout, props de ruta y un catálogo compartido.
---

Un layout es un componente que envuelve el contenido de una página con estructura compartida: navegación, una cabecera, un área de contenido o un pie de página. En RustFlow, la composición SSR actual se expresa con componentes registrados y <code>HtmlPage::with_layout</code>. No hay un sistema automático de layouts basado en el filesystem. Los bloques de código siguientes son extractos conectados entre sí; la [página de producto ejecutable](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs) ofrece el programa completo.

## 1. Entender la composición

Una página suele tener tres niveles:

1. **Datos de ruta:** el handler obtiene un identificador o query string.
2. **Componente de página:** recibe solo los datos públicos que necesita y construye el contenido específico.
3. **Layouts:** envuelven el contenido con estructura compartida sin concatenar strings HTML.

Usar componentes de layout ayuda a mantener el encabezado y la estructura visual en un lugar común. La aplicación sigue siendo responsable de decidir qué página corresponde a cada ruta.

## 2. Crear un layout que reciba children

El callback de render de un componente recibe las props actuales y la lista de hijos. Un layout puede reutilizar esos hijos dentro de un elemento estructural:

~~~rust
use std::rc::Rc;
use rustflow::components::{ComponentRender, SharedComponentDefinition};
use rustflow::view::{View, Property};

let app_layout = SharedComponentDefinition::new(
    "AppLayout",
    |_context, _initial_properties| {
        let render: ComponentRender = Rc::new(|_properties, children| {
            Ok(View::element("div", [
                View::element("header", [View::text("Mi sitio")])?,
                View::element("main", children.to_vec())?,
                View::element("footer", [View::text("Pie del sitio")])?,
            ])?)
        });
        Ok(render)
    },
)?;
~~~

El layout recibe el cuerpo como children y lo coloca dentro del área principal. La construcción usa vistas estructurales; no se inserta HTML crudo. En este ejemplo el layout no tiene props propias. Puedes aceptar props tipadas mediante las definiciones de componente cuando sean necesarias.

## 3. Registrar definiciones en un catálogo compartido

Para usarlos desde handlers HTTP concurrentes, construye un catálogo una sola vez al inicio del servidor:

~~~rust
use std::sync::Arc;
use rustflow::components::SharedComponentCatalogBuilder;

let mut builder = SharedComponentCatalogBuilder::new();
builder.register(app_layout)?;
let catalog = Arc::new(builder.build());
~~~

Las fábricas de las definiciones compartidas tienen que ser <code>Send + Sync</code>; el callback de render que se devuelve puede utilizar estado reactivo local porque se crea para cada render. No guardes señales, props de usuario ni estado de petición dentro del catálogo compartido.

## 4. Crear el cuerpo y envolverlo con layouts

La API más directa para el handler de una ruta es <code>HtmlPage</code>:

~~~rust
use rustflow::view::{Property, View};
use rustflow_server::{HtmlPage, HttpError, Request};

app.get("/articles/{id}", move |request: Request| {
    let catalog = Arc::clone(&catalog);
    async move {
        let id = request.path_param("id").unwrap_or("missing").to_owned();
        let id_prop = Property::new("id", id)
            .map_err(|_| HttpError::internal_server_error())?;

        HtmlPage::from_component("Artículo", "ArticlePage", [id_prop], [])?
            .language("es")
            .with_shared_components(&catalog)
            .with_layout("SectionLayout", [])?
            .with_layout("AppLayout", [])?
            .render()
    }
})?;
~~~

Este fragmento presupone que el catálogo contiene las definiciones <code>ArticlePage</code>, <code>SectionLayout</code> y <code>AppLayout</code>, y que <code>catalog</code> se construyó antes de registrar la ruta.

El orden de las llamadas importa: cada <code>with_layout</code> envuelve el cuerpo actual. Añade primero el layout más interno y termina con el más externo. Por tanto, el resultado conceptual es <code>AppLayout(SectionLayout(ArticlePage(id)))</code>.

## 5. Pasar datos de ruta de forma explícita

<code>HtmlPage::from_component</code> recibe el nombre del componente, un conjunto de props y una lista de hijos. El handler debe proyectar solo los campos que el navegador puede ver; no pases el objeto Request, cabeceras, cookies, credenciales ni datos privados. El escape de SSR protege la representación HTML, pero no convierte en público un dato que debía permanecer secreto.

Para una ruta de artículo puedes pasar un slug o título público. Para una página de producto, pasa identificador, nombre, precio y descripción que estén destinados a ser visibles. No incluyas el carrito privado del usuario ni la información de autenticación.

## 6. Cuándo usar layouts

- **Sitio de contenido:** cabecera, navegación y pie de página compartidos en cada artículo.
- **Catálogo:** una envoltura de tienda compartida alrededor de cada producto.
- **Panel administrativo:** una estructura común de página; la autenticación/autorización debe implementarse de manera explícita fuera de las capacidades actuales de RustFlow.
- **Secciones anidadas:** un layout del sitio y otro de sección alrededor del contenido.

## Límites actuales

- La resolución de rutas es responsabilidad de la aplicación y del handler HTTP.
- La composición es por componentes explícitos; no hay convención automática de archivos, loaders ni router unificado.
- Para peticiones concurrentes usa <code>SharedComponentCatalog</code>. El registry local está disponible para callbacks que no sean compartibles.
- La aplicación debe mantener los datos privados fuera de props y de <code>HydrationData</code>.
- Layouts, SSR e hidratación están en APIs experimentales pre-1.0.

La ruta de ejemplo del repositorio está en [crates/rustflow-server/examples/basic.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs). Continúa con [SSR e hidratación](/guides/ssr/) o el [tutorial de página de producto](/tutorials/product-page/).
