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

Usa `HtmlRenderer::with_components(&registry)` o `render_with_components` para resolver descriptores. Cada llamada crea un runtime reactivo aislado. Los callbacks de limpieza del componente se ejecutan antes de retornar, incluso cuando falla la serialización de su vista.

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

## Composición de páginas SSR y layouts (Phase 5.2)

El adaptador HTTP expone `rustflow_server::HtmlPage` para construir una página HTML completa desde una vista o un componente de ruta registrado. Los layouts también son componentes: cada uno recibe como hijos el contenido ya compuesto, y la serialización final pasa por las mismas reglas seguras de `rustflow-ssr`.

```rust
use rustflow_server::{HtmlPage, HttpError, Request, Response};
use rustflow::components::ComponentRegistry;
use rustflow_view::Property;

// Dentro de un handler GET /articles/{slug}. Construye el registro dentro
// de esta invocación: ComponentRegistry aún no es Send + Sync.
let mut registry = ComponentRegistry::new();
register_article_and_layouts(&mut registry)
    .map_err(|_| HttpError::internal_server_error())?;

let slug = request.path_param("slug").unwrap_or("missing").to_owned();
let slug_prop = Property::new("slug", slug)
    .map_err(|_| HttpError::internal_server_error())?;

HtmlPage::from_component("Article", "ArticlePage", [slug_prop], [])?
    .language("es")
    .with_components(&registry)
    .with_layout("ContentLayout", [])? // layout interior
    .with_layout("AppLayout", [])?     // layout exterior
    .render()
```

**Limitación actual del prototipo:** `ComponentRegistry` y el runtime reactivo usan estado basado en `Rc`, por lo que no son `Send + Sync`. Los handlers HTTP de `Application` sí exigen closures/futuros compatibles con esos límites. Por eso no captures un registro preconstruido en la closure de ruta: constrúyelo dentro de la invocación del handler (como en la prueba `route_renders_registered_component_with_request_props_and_layout`) y úsalo de forma síncrona para renderizar la respuesta. Esta solución funcional reconstruye las definiciones por petición; compartir un catálogo de componentes entre workers de servidor de forma segura es una mejora arquitectónica pendiente, no algo que deba asumirse resuelto.

El orden importa: `with_layout` envuelve el cuerpo construido hasta ese momento, por lo que se añade primero el layout interior y al final el exterior. Los componentes de layout y de contenido deben registrarse en el mismo `ComponentRegistry`.

Los datos de ruta no se inyectan de forma implícita. El handler extrae y valida parámetros/query, decide qué campos son públicos y los pasa como `Property` explícitas al componente. No pases cookies, credenciales, tokens, objetos completos de petición, decisiones de autorización ni estado privado. Esto reduce la superficie de exposición, pero no sustituye el juicio de la aplicación sobre qué valores son secretos.

`HtmlPage::new(title, view)` sirve para vistas estructurales sin descriptores de componente; `from_component` crea un descriptor de componente de página. Si el cuerpo o los layouts contienen componentes, llama a `with_components`. El atributo `lang`, el título y el cuerpo se serializan mediante el renderer seguro; esta API no acepta una cadena de HTML arbitrario ni implementa todavía hidratación DOM.

Cada render SSR crea un runtime reactivo y scopes de componentes nuevos; el registro no almacena estado reactivo por petición. En un handler HTTP, el registro se reconstruye por la limitación `Send + Sync` descrita arriba. Las pruebas comprueban que una secuencia de props de ruta `7, 12, 7` produce HTML `7, 12, 7`, que el resultado para props idénticas es determinista, que cada render dispone su scope y que el router HTTP puede obtener un parámetro de ruta, pasarlo como prop y devolver una página compuesta con layout. La detección/recuperación de discrepancias, los identificadores de hidratación y la navegación cliente siguen fuera de la Fase 5.2.
