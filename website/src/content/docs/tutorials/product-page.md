---
title: Tutorial: página de producto SSR
description: Construye una página de producto a partir de una ruta HTTP, props públicas, un componente registrado y un layout.
---

Este tutorial reúne el modelo de páginas SSR actual de RustFlow. El objetivo es terminar con una ruta como <code>/products/keyboard</code> que devuelva un documento HTML completo, un título, detalles del producto y una envoltura de tienda compartida.

**Alcance del ejemplo:** los productos viven en una tabla en memoria para enseñar la arquitectura. No incluye base de datos, carrito persistente, checkout, pagos, cuentas, autorización ni servidor de acciones. No representa una tienda lista para producción.

## Qué vas a construir

- Una ruta HTTP que lee el ID de producto.
- Una definición de componente SSR para el contenido específico del producto.
- Un layout reutilizable con una cabecera y un área de contenido.
- Un catálogo inmutable compartido por handlers.
- Una página HTML con props de producto explícitas.

Para preparar el proyecto, clona RustFlow y ejecuta el ejemplo HTTP siguiendo [Empezar con RustFlow](/getting-started/setup/). Los fragmentos siguientes usan APIs del repositorio; puedes tomar como estructura runnable el ejemplo [basic.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs).

## 1. Definir el componente de producto

Una definición compartida se crea con un setup factory y un callback de render. El callback extrae las props tipadas que el handler le pasó y produce un árbol estructural:

~~~rust
use std::rc::Rc;
use rustflow::components::{ComponentRender, SharedComponentDefinition};
use rustflow::view::{PropertyValue, View};

let product_page = SharedComponentDefinition::new(
    "ProductPage",
    |_context, _initial_properties| {
        let render: ComponentRender = Rc::new(|properties, _children| {
            let text_prop = |name: &str| {
                properties
                    .iter()
                    .find(|property| property.name() == name)
                    .and_then(|property| match property.value() {
                        PropertyValue::Text(value) => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_default()
            };

            let name = text_prop("name");
            let price = text_prop("price");
            let description = text_prop("description");

            Ok(View::element("article", [
                View::element("h1", [View::text(name)])?,
                View::element("p", [View::text(price)])?,
                View::element("p", [View::text(description)])?,
            ])?)
        });
        Ok(render)
    },
)?;
~~~

Este componente no consulta una base de datos ni crea HTML mediante interpolación de strings. Recibe props de la aplicación y construye elementos/texto. SSR se encarga de escapar el texto al serializarlo.

## 2. Crear el layout de tienda

El layout se comparte entre páginas y coloca el contenido recibido como children:

~~~rust
let store_layout = SharedComponentDefinition::new(
    "StoreLayout",
    |_context, _initial_properties| {
        let render: ComponentRender = Rc::new(|_properties, children| {
            Ok(View::element("div", [
                View::element("header", [
                    View::element("a", [
                        View::text("Mi tienda")
                    ])?
                ])?,
                View::element("main", children.to_vec())?,
                View::element("footer", [
                    View::text("Catálogo de demostración")
                ])?,
            ])?)
        });
        Ok(render)
    },
)?;
~~~

El layout no sabe qué producto se está mostrando. Esa separación permite reutilizar la misma estructura con distintos productos.

## 3. Construir un catálogo compartido

~~~rust
use std::sync::Arc;
use rustflow::components::SharedComponentCatalogBuilder;

let mut builder = SharedComponentCatalogBuilder::new();
builder.register(product_page)?;
builder.register(store_layout)?;
let catalog = Arc::new(builder.build());
~~~

Hazlo una vez al iniciar el servidor, no en cada petición. El catálogo conserva las fábricas de las definiciones, pero cada render crea sus vistas y scopes locales. Esto es lo que permite compartir el catálogo sin compartir estado mutable de una petición.

## 4. Mapear una ruta a datos públicos

En una aplicación real, los datos podrían venir de un servicio o base de datos. Para un tutorial inicial basta una tabla fija:

~~~rust
struct Product {
    name: &'static str,
    price: &'static str,
    description: &'static str,
}

fn demo_product(id: &str) -> Option<Product> {
    match id {
        "keyboard" => Some(Product {
            name: "Teclado mecánico",
            price: "$89.00",
            description: "Teclado compacto para trabajo y programación.",
        }),
        "mouse" => Some(Product {
            name: "Ratón ergonómico",
            price: "$49.00",
            description: "Ratón de demostración con diseño ergonómico.",
        }),
        _ => None,
    }
}
~~~

Los campos usados aquí son públicos y están destinados a mostrarse en la página. No pases tokens, información privada de cuenta ni credenciales como props.

## 5. Conectar la ruta, el componente y el layout

Este handler captura el catálogo construido durante el startup, obtiene el ID de ruta, convierte solo los campos públicos en props y renderiza un documento completo:

~~~rust
use std::sync::Arc;
use rustflow::view::Property;
use rustflow_server::{Application, HtmlPage, HttpError, Request, Response};

let mut app = Application::new();
let route_catalog = Arc::clone(&catalog);

app.get("/products/{id}", move |request: Request| {
    let catalog = Arc::clone(&route_catalog);
    async move {
        let id = request.path_param("id").unwrap_or_default().to_owned();
        let Some(product) = demo_product(&id) else {
            return Ok(Response::not_found("No se encontró el producto"));
        };

        let props = [
            Property::new("name", product.name)
                .map_err(|_| HttpError::internal_server_error())?,
            Property::new("price", product.price)
                .map_err(|_| HttpError::internal_server_error())?,
            Property::new("description", product.description)
                .map_err(|_| HttpError::internal_server_error())?,
        ];

        HtmlPage::from_component("Producto", "ProductPage", props, [])?
            .language("es")
            .with_shared_components(&catalog)
            .with_layout("StoreLayout", [])?
            .render()
    }
})?;
~~~

El código es un fragmento para integrar en el binario de servidor, junto con los imports y los pasos anteriores. El tipo de retorno y los errores están alineados con las APIs usadas por el ejemplo del workspace.

## 6. Probar el resultado

Ejecuta el servidor que contiene el handler y abre:

- <code>/products/keyboard</code>: página de detalle del teclado.
- <code>/products/mouse</code>: misma estructura, distinto contenido.
- <code>/products/unknown</code>: respuesta 404 del handler.

Puedes comprobar el HTML generado con la pestaña de código fuente o con una petición HTTP. El resultado debe incluir un documento completo, el encabezado de tienda, el título y la descripción. Los valores se deben mostrar como texto escapado.

## 7. Qué falta para una tienda real

Este ejemplo enseña composición SSR, no todo el dominio de e-commerce. Para convertirlo en una tienda real todavía necesitarías diseñar y probar almacenamiento, inventario, precios autoritativos, validación, carrito, autenticación/autorización, protección CSRF, checkout y pagos. Esas piezas no están integradas por RustFlow actualmente.

Si quieres introducir interactividad de navegador, el catálogo/producto puede inspirar componentes de una UI WASM, pero el demo de página SSR de este tutorial no se hidrata automáticamente. La hidratación y la navegación son opt-in y necesitan su propio contrato y configuración. Consulta [SSR e hidratación](/guides/ssr/) y [Navegación](/guides/navigation/).

## Conceptos utilizados

- [Servidor HTTP](/guides/server/) explica rutas, handlers y errores.
- [Crear componentes](/guides/components/) explica props, eventos y vistas.
- [Composición de layouts](/guides/layouts/) explica los children y el catálogo compartido.
- [Disponibilidad de API](/reference/api-status/) distingue capacidades actuales y pendientes.
