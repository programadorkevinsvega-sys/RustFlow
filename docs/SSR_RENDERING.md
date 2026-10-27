# Renderizado HTML en servidor (prototipo)

El crate `rustflow-ssr` convierte un árbol de vistas estructurales en un fragmento HTML. No sirve HTTP y no vuelve seguro el árbol para insertarlo sin más en otros contextos.

## Fragmento básico

```rust
use rustflow::view::{Element, View};
use rustflow_ssr::render_to_string;

fn example() -> Result<(), Box<dyn std::error::Error>> {
    let view = Element::new("main")?
        .with_attribute("aria-label", "Welcome")?
        .with_children([View::text("Hello & welcome")])
        .build();
    assert_eq!(
        render_to_string(&view)?,
        "<main aria-label=\"Welcome\">Hello &amp; welcome</main>"
    );
    Ok(())
}
```

## Componentes

Para callbacks locales que deliberadamente no cumplen `Send + Sync`, puedes usar `HtmlRenderer::with_components(&registry)` o `render_with_components`. En rutas HTTP concurrentes, prefiere `HtmlRenderer::with_shared_components(&catalog)` o `render_with_shared_components` con un `SharedComponentCatalog` construido al iniciar la aplicación. Cada render crea un runtime reactivo aislado. Los callbacks de limpieza del componente se ejecutan antes de retornar, incluso cuando falla la serialización de su vista.

## Política de seguridad y salida soportada

El texto y los valores de atributos se escapan. Solo se serializa un conjunto conservador de elementos comunes; etiquetas como script, iframe, object, embed y style quedan fuera de la lista permitida. Los atributos URL solo aceptan los esquemas `http`, `https`, `mailto`, `tel` o URLs relativas. `srcset` y `ping` se rechazan hasta que se implemente un parser que valide cada URL. Los atributos booleanos falsos se omiten.

Los callbacks de eventos nunca se serializan como atributos HTML `on*`. Un subconjunto de propiedades reflejables (como `value` y `checked` en formularios) se convierte a atributos; las propiedades exclusivas del cliente se omiten. Si un atributo y una propiedad producirían el mismo atributo HTML, se devuelve un error.

El renderer de `rustflow-ssr` emite fragmentos, no documentos ni respuestas HTTP. El adaptador `rustflow-server` puede envolver una vista estructural con `html_document(title, lang, &view)`, que produce doctype, atributo `lang`, metadatos UTF-8, título y cuerpo mediante nodos seguros. También puede aplicar páginas HTML configuradas para estados 4xx/5xx. El adaptador `rustflow-server` ya ofrece rutas y helpers de respuesta/documento; la composición avanzada de layouts, los datos de hidratación y la hidratación en el cliente siguen siendo trabajo posterior.


## Datos públicos previstos para hidratación (contrato inicial)

El tipo experimental `HydrationData` valida un valor JSON y devuelve JSON compacto con los caracteres sensibles para un contexto de script HTML escapados como secuencias Unicode. Rechaza entradas mayores de 1 MiB, representaciones resultantes mayores de 1 MiB y estructuras con más de 64 niveles de anidamiento.

```rust
use rustflow_ssr::HydrationData;

fn build_public_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    // Construye expresamente un DTO público; no serialices el estado completo del servidor.
    let public_data = serde_json::json!({
        "display_name": "Ada",
        "preferences": { "compact": true }
    });
    let payload = HydrationData::from_value(&public_data)?;
    assert!(!payload.as_str().contains('<'));
    Ok(())
}
```

`HydrationData::from_json` valida y normaliza una cadena JSON. `as_str` devuelve datos JSON, no HTML, no genera una etiqueta `script` y no implementa hidratación. El tipo solo admite `serde_json::Value`: closures, callbacks, nodos DOM, referencias internas y objetos Rust arbitrarios no se serializan automáticamente. El framework tampoco puede inferir si un campo representa un secreto; la aplicación debe proyectar una lista explícita de datos públicos y nunca incluir tokens de sesión, credenciales, objetos de petición ni estado privado.

El contrato de componentes compatibles con cada entorno, los límites de discrepancia, los identificadores de hidratación y la recuperación del DOM continúan en definición. No se debe interpretar esta utilidad como soporte de SSR full-stack ni como hidratación funcional.


## Capacidades de componentes y fallback SSR

Las definiciones de `rustflow::components::ComponentDefinition` son universales por compatibilidad, salvo que se indique lo contrario. Usa `.server_only()` para componentes que solo se pueden ejecutar en el servidor y `.browser_only()` para componentes que requieren APIs del navegador. Los renderizadores llaman a la variante de montaje con destino y rechazan la capacidad incompatible antes de ejecutar el setup.

Un componente browser-only debe definir un fallback estructural para SSR o el renderizado devuelve `SsrError::NonHydratableComponent`:

```rust
use rustflow::{components::ComponentDefinition, view::View};
use std::rc::Rc;

let client_widget = ComponentDefinition::new("ClientWidget", |_context, _props| {
    let render: rustflow::components::ComponentRender =
        Rc::new(|_props, _children| Ok(View::text("interactive in browser")));
    Ok(render)
})?
.browser_only()
.with_ssr_fallback(View::text("Static placeholder"));
```

El fallback se serializa mediante la misma política de seguridad de SSR y es estático: no se convierte en un límite hidratable, no adjunta eventos y no afirma que haya interactividad cliente. Las capacidades de ejecución no sustituyen la separación de dependencias por crate; el CI de WASM bloquea las dependencias de servidor conocidas. La aplicación sigue teniendo que proyectar expresamente campos públicos antes de crear `HydrationData`; RustFlow no puede saber si un valor de texto es un secreto.

## Catálogo compartido y páginas SSR (Phase 5.2)

Para componentes que se usan desde handlers HTTP, utiliza `SharedComponentCatalog`. Construye el catálogo una sola vez al arrancar la aplicación y captura un `Arc<SharedComponentCatalog>` desde las rutas. El ejemplo ejecutable `crates/rustflow-server/examples/basic.rs` muestra el flujo completo.

```rust
use std::{rc::Rc, sync::Arc};
use rustflow::components::{
    ComponentRender, SharedComponentCatalogBuilder, SharedComponentDefinition,
};
use rustflow::view::{Property, PropertyValue, View};
use rustflow_server::{Application, HtmlPage, HttpError, Request};

let mut builder = SharedComponentCatalogBuilder::new();
builder.register(
    SharedComponentDefinition::new("ArticlePage", |_context, _initial| {
        let render: ComponentRender = Rc::new(|properties, _children| {
            let id = properties
                .iter()
                .find(|property| property.name() == "id")
                .and_then(|property| match property.value() {
                    PropertyValue::Text(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "missing".to_owned());
            Ok(View::element("article", [View::element("h1", [View::text(id)])?])?)
        });
        Ok(render)
    })?
    .server_only(),
)?;
let catalog = Arc::new(builder.build());

let mut app = Application::new();
let route_catalog = Arc::clone(&catalog);
app.get("/articles/{id}", move |request: Request| {
    let catalog = Arc::clone(&route_catalog);
    async move {
        let id = request.path_param("id").unwrap_or("missing").to_owned();
        let property = Property::new("id", id)
            .map_err(|_| HttpError::internal_server_error())?;
        HtmlPage::from_component("Article", "ArticlePage", [property], [])?
            .with_shared_components(&catalog)
            .render()
    }
})?;
```

El setup compartido y la factoría de fallback deben cumplir `Send + Sync`. Pueden producir un `ComponentRender` basado en `Rc`, señales y callbacks locales porque ese render se crea para la petición actual. El runtime, las señales, los scopes, las instancias y las vistas de trabajo no se comparten entre peticiones.

Para componentes browser-only, define `with_ssr_fallback_factory(|| ...)`. El catálogo conserva la factoría compartible y construye una `View` local por render, serializada con las mismas reglas seguras de SSR.

### Composición de layouts

Los layouts son componentes registrados en el mismo catálogo compartido. Las llamadas `with_layout` envuelven el contenido actual: añade primero el layout interior y luego el exterior.

```rust
HtmlPage::from_component("Article", "ArticlePage", [property], [])?
    .language("es")
    .with_shared_components(&catalog)
    .with_layout("ContentLayout", [])?
    .with_layout("AppLayout", [])?
    .render()
```

### ¿Cuándo usar ComponentRegistry?

El `ComponentRegistry` local se conserva para callbacks que deliberadamente no cumplen `Send + Sync`, especialmente en contextos locales o de navegador. No lo construyas en cada petición como patrón de servidor. Para handlers concurrentes, usa `SharedComponentDefinition`, `SharedComponentCatalogBuilder` y `HtmlPage::with_shared_components`.

Los datos de ruta deben extraerse y validarse de forma explícita. Pasa como `Property` solo los campos destinados a ser públicos en el HTML; no pases cookies, credenciales, tokens, autorización ni estado privado del servidor. Esta API no implementa todavía hidratación DOM, identificadores de límites ni navegación cliente: esos requisitos pertenecen a las fases 5.3 y 5.4.
