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

El renderer de `rustflow-ssr` emite fragmentos, no documentos ni respuestas HTTP. El adaptador `rustflow-server` puede envolver una vista estructural con `html_document(title, lang, &view)`, que produce doctype, atributo `lang`, metadatos UTF-8, título y cuerpo mediante nodos seguros. También puede aplicar páginas HTML configuradas para estados 4xx/5xx. El adaptador `rustflow-server` ofrece composición de páginas y datos de hidratación; la hidratación DOM opt-in de la Fase 5.3 está implementada. La navegación cliente y la mejora progresiva de formularios se implementaron como APIs optativas en la Fase 5.4; consulta el contrato detallado a continuación y la guía pública de navegación.


## Datos públicos para hidratación (contrato actual)

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

La API `HydrationData` sigue siendo solo el formato del payload; la integración SSR y la hidratación DOM se documentan por separado a continuación. No convierte por sí misma una página en interactiva: el servidor debe emitir límites compatibles y el cliente debe invocar explícitamente el hidratador.


## Hidratación DOM (Fase 5.3, implementada)

La hidratación está opt-in. El renderizado SSR normal conserva su HTML existente; el renderizador solo emite comentarios estructurales cuando se solicita `HtmlRenderer::with_hydration_boundaries()`. Los marcadores versionan la posición del root y delimitan componentes, regiones dinámicas y subárboles estáticos. Sus identificadores son deterministas en orden de recorrido para la misma vista y el mismo catálogo.

Para páginas de servidor, adjunta únicamente datos explícitamente públicos:

```rust
use rustflow_server::HtmlPage;
use rustflow_ssr::HydrationData;

let data = HydrationData::from_json(r#"{"display_name":"Ada"}"#)?;
let response = HtmlPage::new("Inicio", body)
    .with_hydration_data(data)
    .render()?;
```

La página añade el fragmento SSR con límites y un elemento inerte `script[type="application/json"]` con id `rustflow-hydration-data`. La serialización existente escapa los caracteres peligrosos para el contexto HTML. Esto no serializa estado privado ni deduce automáticamente las propiedades iniciales de los componentes: la aplicación debe derivar la vista y su estado inicial a partir de una lista permitida de datos públicos.

En el navegador, `DomRenderer::read_hydration_data(&document)` analiza el payload opcional como valor JavaScript y `DomRenderer::hydrate_into(&target, &view)` o `hydrate_reactive(&target, &owner_scope, render)` valida primero el DOM existente. Compara marcadores, etiquetas, atributos, texto y la estructura inicial, y solo después activa manejadores y efectos. Los nodos compatibles no se reconstruyen. Los eventos se registran a partir de las vinculaciones de la vista del cliente; los callbacks nunca vienen serializados desde el servidor.

Si faltan o se repiten límites, el orden de nodos difiere, o el estado/atributos iniciales no coinciden, la hidratación devuelve un diagnóstico `DomError::HydrationMismatch`. No intenta reparar con un reemplazo destructivo ni activa parcialmente la interactividad del root: el HTML de servidor permanece como contenido estático. La misma raíz no puede hidratarse dos veces con éxito. Esta política conservadora evita perder contenido válido, aunque la recuperación granular de la subregión más pequeña aún no está implementada.

Las listas deben presentar el mismo orden inicial y las mismas claves/vistas estructurales; las claves no se serializan como estado ejecutable. Los formularios conservan los valores ya introducidos en el DOM durante la validación inicial; los atributos/propiedades SSR representables se cotejan antes de adjuntar eventos. El contenido estático o los fallbacks de componentes no compatibles con el entorno siguen siendo estáticos. La navegación cliente, la intercepción de enlaces y el historial se implementan por separado mediante ClientNavigation; no se habilitan automáticamente al hidratar.

El PR #39 se fusionó en `main` y los checks requeridos de Rust 1.85/stable, WASM y Browser E2E/bundle size pasaron en la CI posterior al merge; el PR también pasó documentación, gobernanza y revisión de dependencias. La hidratación sigue siendo opt-in y sus límites (en particular el fallback estático del root completo y la necesidad de configurar navegación cliente por separado) están documentados en el ADR 0010. La navegación se implementó después como alcance independiente de la Fase 5.4.

## Navegación cliente y formularios (Fase 5.4, mejora optativa)

El adaptador de navegador expone `rustflow_dom::ClientNavigation` para mejorar un montaje reactivo ya creado. Instálalo solo después de `DomRenderer::mount_reactive` o de una hidratación exitosa con `DomRenderer::hydrate_reactive`. La API no incorpora un DSL de rutas: la aplicación conserva el resolver de rutas y actualiza su señal/estado de vista en el callback.

```rust
use rustflow_dom::{ClientNavigation, DomError};

// target es el elemento raíz y mounted es el ReactiveMountedDom creado previamente.
// route_signal es una señal propiedad del runtime de la aplicación.
let navigation = ClientNavigation::install(target, mounted, move |path| {
    match path.split('?').next().unwrap_or("/") {
        "/" | "/about" => {
            route_signal.set(path.to_owned());
            Ok(true)
        }
        _ => Ok(false), // La ruta no se modifica; el servidor seguirá siendo la autoridad.
    }
})?;
```

Solo se interceptan clics primarios sin modificadores en enlaces HTTP(S) del mismo origen, dentro del montaje, sin fragmento, descarga, `rel="external"`, opt-out ni destino distinto de `_self`. Los enlaces externos, protocolos no HTTP, fragmentos, descargas, destinos alternativos y clics modificados conservan la navegación convencional.

Antes de cambiar la vista o la URL, RustFlow comprueba que la petición GET devuelve un estado satisfactorio, `text/html`, un documento HTML completo y una URL final del mismo origen. El callback debe aceptar la ruta y el montaje reactivo no debe informar un error. Después se usa la History API: las navegaciones aceptadas crean entradas nuevas; atrás/adelante procesa `popstate` con los mismos controles. Errores HTTP/red, contenido incompatible, una ruta no soportada, errores de render o fallos de History provocan navegación completa o recarga. El contenido HTML descargado no se inserta mediante `innerHTML`; el callback reconstruye la vista con el runtime existente.

La mejora de formularios es deliberadamente más estrecha que HTML: añade `data-rustflow-enhance` solo a los formularios que deban probar la mejora cliente y usar GET, `application/x-www-form-urlencoded`, destino vacío/`_self`, origen compatible y controles escalares admitidos. POST, métodos de mutación, archivos, selectores múltiples, controles submit con nombre, codificaciones o destinos no soportados, formularios inválidos y formularios sin opt-in conservan el envío nativo. La mejora bloquea envíos duplicados durante la petición y, si la petición falla, vuelve al servidor con una navegación completa.

El SSR sigue siendo la base funcional: antes de JavaScript, antes de inicializar RustFlow y cuando la mejora no sea elegible, los enlaces y formularios HTML siguen funcionando de forma convencional. Una discrepancia de hidratación conserva estático todo el root según ADR 0010; no instales navegación cliente después de una hidratación fallida. La API actual requiere que la aplicación mantenga sus rutas cliente alineadas con las rutas HTTP del servidor y no incluye server actions, sesiones, autenticación, CSRF ni cargas de archivos.

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

Los datos de ruta deben extraerse y validarse de forma explícita. Pasa como `Property` solo los campos destinados a ser públicos en el HTML; no pases cookies, credenciales, tokens, autorización ni estado privado del servidor. La composición de páginas y layouts es independiente de la hidratación o la navegación. La hidratación DOM y ClientNavigation ya están implementadas como APIs optativas en fases posteriores; HtmlPage no las instala automáticamente.
