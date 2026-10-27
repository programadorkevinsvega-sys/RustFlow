use std::{error::Error, net::SocketAddr, rc::Rc, sync::Arc};

use rustflow::components::{
    ComponentRender, SharedComponentCatalogBuilder, SharedComponentDefinition,
};
use rustflow::view::{Property, PropertyValue, View};
use rustflow_server::{Application, HtmlPage, HttpError, Request, Response, html_document};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 1. Declare the server-rendered product page as a reusable component definition.
    let product_page =
        SharedComponentDefinition::new("ProductPage", |_context, _initial_properties| {
            let render: ComponentRender = Rc::new(|properties, _children| {
                let text_property = |name: &str| {
                    properties
                        .iter()
                        .find(|property| property.name() == name)
                        .and_then(|property| match property.value() {
                            PropertyValue::Text(value) => Some(value.clone()),
                            _ => None,
                        })
                        .unwrap_or_default()
                };

                let name = text_property("name");
                let price = text_property("price");
                let description = text_property("description");

                Ok(View::element(
                    "article",
                    [
                        View::element("h1", [View::text(name)])?,
                        View::element("p", [View::text(price)])?,
                        View::element("p", [View::text(description)])?,
                    ],
                )?)
            });
            Ok(render)
        })?
        .server_only();

    // 2. Create a reusable layout. Children contain the page-specific component.
    let store_layout =
        SharedComponentDefinition::new("StoreLayout", |_context, _initial_properties| {
            let render: ComponentRender = Rc::new(|_properties, children| {
                Ok(View::element(
                    "div",
                    [
                        View::element(
                            "header",
                            [View::element("strong", [View::text("Mi tienda")])?],
                        )?,
                        View::element("main", children.to_vec())?,
                        View::element("footer", [View::text("Catálogo de demostración")])?,
                    ],
                )?)
            });
            Ok(render)
        })?
        .server_only();

    // 3. Build the immutable catalog once. Do not store per-request state in it.
    let mut builder = SharedComponentCatalogBuilder::new();
    builder.register(product_page)?;
    builder.register(store_layout)?;
    let catalog = Arc::new(builder.build());

    let mut app = Application::new();

    app.get("/", |_request: Request| async {
        let title = View::element("h1", [View::text("Catálogo de demostración")])
            .map_err(|_| HttpError::internal_server_error())?;
        let keyboard = View::element(
            "p",
            [View::text(
                "Abre /products/keyboard para ver el teclado mecánico.",
            )],
        )
        .map_err(|_| HttpError::internal_server_error())?;
        let mouse = View::element(
            "p",
            [View::text(
                "Abre /products/mouse para ver el ratón ergonómico.",
            )],
        )
        .map_err(|_| HttpError::internal_server_error())?;
        let body = View::element("main", [title, keyboard, mouse])
            .map_err(|_| HttpError::internal_server_error())?;
        html_document("Catálogo", "es", &body)
    })?;

    // 4. Resolve a route, expose only public fields as component props, then render the page.
    let route_catalog = Arc::clone(&catalog);
    app.get("/products/{id}", move |request: Request| {
        let catalog = Arc::clone(&route_catalog);
        async move {
            let id = request.path_param("id").unwrap_or_default().to_owned();
            let Some(product) = demo_product(&id) else {
                return Err(HttpError::not_found("No se encontró el producto"));
            };

            let name = Property::new("name", product.name.to_owned())
                .map_err(|_| HttpError::internal_server_error())?;
            let price = Property::new("price", product.price.to_owned())
                .map_err(|_| HttpError::internal_server_error())?;
            let description = Property::new("description", product.description.to_owned())
                .map_err(|_| HttpError::internal_server_error())?;

            HtmlPage::from_component("Producto", "ProductPage", [name, price, description], [])?
                .language("es")
                .with_shared_components(&catalog)
                .with_layout("StoreLayout", [])?
                .render()
        }
    })?;

    let address = SocketAddr::from(([127, 0, 0, 1], 3000));
    let shutdown = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("No se pudo instalar el handler de Ctrl+C: {error}");
        }
    };
    app.serve_with_shutdown(address, shutdown).await?;
    Ok(())
}
