---
title: SSR, documentos HTML e hidratación
description: Aprende a renderizar vistas de forma segura, crear documentos completos y preparar hidratación optativa.
---

El renderizado del lado del servidor (SSR) transforma una vista tipada en HTML que el navegador puede mostrar antes de ejecutar código de cliente. En RustFlow conviene distinguir tres responsabilidades: el serializador produce un fragmento HTML, el adapter HTTP puede convertir una vista en un documento completo y la hidratación del navegador intenta conectar una vista cliente compatible al DOM SSR existente.

Todo este conjunto está en etapa de prototipo. Usa los ejemplos para aprender el contrato actual, no como una afirmación de que el framework ya ofrece un despliegue de producción completo.

## 1. Renderizar una vista a un fragmento

~~~rust
use rustflow::view::{Element, View};
use rustflow_ssr::render_to_string;

fn example() -> Result<(), Box<dyn std::error::Error>> {
    let view = Element::new("main")?
        .with_attribute("aria-label", "Welcome")?
        .with_children([View::text("Hello & welcome")])
        .build();

    let html = render_to_string(&view)?;
    assert_eq!(
        html,
        "<main aria-label=\"Welcome\">Hello &amp; welcome</main>"
    );
    Ok(())
}
~~~

Este nivel es útil cuando quieres convertir una vista estructural a un fragmento, por ejemplo para un renderizador superior o una prueba. El resultado es un fragmento, no una página HTML completa ni una respuesta HTTP. El texto y los valores de los atributos se escapan.

El SSR solo serializa un conjunto conservador de elementos/atributos. Rechaza, entre otros, tags ejecutables o de contenido activo no admitidos, atributos URL no aceptados y atributos multi-URL sin parser validado. Los callbacks de evento nunca se convierten en atributos HTML <code>on*</code>.

## 2. Generar un documento HTML desde una ruta

Para una respuesta HTTP completa usa <code>html_document(title, lang, &view)</code>:

~~~rust
use rustflow::view::View;
use rustflow_server::{Application, HttpError, Request, html_document};

let mut app = Application::new();

app.get("/", |_request: Request| async {
    let heading = View::element("h1", [View::text("Mi sitio")])
        .map_err(|_| HttpError::internal_server_error())?;
    let body = View::element("main", [heading])
        .map_err(|_| HttpError::internal_server_error())?;

    html_document("Mi sitio", "es", &body)
})?;
~~~

El helper produce un documento con doctype, idioma, charset, título y cuerpo. El texto del título y el cuerpo siguen la política de escape de SSR. Así puedes escribir handlers que devuelven una página sin construir manualmente un string HTML.

## 3. Renderizar componentes registrados

Una vista puede contener descriptores de componentes. Para renderizarlos, el renderer necesita encontrar su definición. Un registry local puede servir para callbacks de una sola ejecución; para rutas HTTP concurrentes, la vía recomendada es construir un <code>SharedComponentCatalog</code> inmutable al arrancar y compartirlo con <code>Arc</code>.

Las definiciones compartidas no deben guardar datos de una petición en estado mutable. Cada render crea su runtime, scopes, vistas y callbacks locales. El catálogo contiene fábricas compartibles, no el estado de usuario ni una instancia montada.

El ejemplo [basic.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs) registra un componente de artículo y un layout en un catálogo compartido, expone una ruta de artículo y renderiza la página con <code>HtmlPage</code>.

## 4. Componer una página con HtmlPage

<code>HtmlPage</code> organiza un documento SSR con título, idioma, cuerpo y, de manera opcional, componentes, datos de hidratación y layouts.

~~~rust
use rustflow::view::{Property, View};
use rustflow_server::HtmlPage;

let body = View::element("article", [
    View::element("h1", [View::text("Artículo")])?,
])?;

let response = HtmlPage::new("Artículo", body)
    .language("es")
    .render()?;
~~~

Este ejemplo usa una vista estructural sin componentes registrados, por eso no necesita un catálogo. Si el cuerpo o los layouts usan componentes, añade <code>with_shared_components(&catalog)</code> para el catálogo compartido o <code>with_components(&registry)</code> para un registry local. Para rutas de servidor concurrentes, prefiere el catálogo compartido. Consulta [Composición de layouts](/guides/layouts/).

## 5. Datos para hidratación: permitir solo campos públicos

La hidratación es opt-in. <code>HydrationData</code> valida JSON acotado y escapa caracteres sensibles al contexto de script; no convierte por sí solo la página en interactiva. No acepta de forma automática cualquier struct Rust ni puede inferir si una cadena es un secreto.

~~~rust
use rustflow_ssr::HydrationData;

let public_json = r#"{"display_name":"Ada","compact":true}"#;
let data = HydrationData::from_json(public_json)?;

let response = HtmlPage::new("Perfil", body)
    .with_hydration_data(data)
    .render()?;
~~~

Proyecta expresamente los datos que el navegador puede ver. Nunca incluyas tokens de sesión, credenciales, cookies, decisiones de autorización, objetos de petición o estado privado. El límite validado de payload y profundidad está definido por la implementación y debe consultarse en el código de la revisión actual.

Al habilitar este mecanismo, el HTML incluye marcadores de límites deterministas y un payload JSON inerte. El cliente todavía necesita invocar explícitamente el hidratador con una vista compatible.

## 6. Cómo funciona la hidratación cliente

La hidratación intenta enlazar eventos del runtime del navegador a nodos SSR existentes, sin reconstruirlos indiscriminadamente. En el protocolo actual:

- La hidratación se activa de forma explícita; SSR normal mantiene su salida.
- Los nodos de la vista cliente deben coincidir con la estructura emitida por el servidor.
- La API detecta algunos tipos de discrepancia y rechaza una hidratación duplicada.
- Si hay una discrepancia incompatible, la recuperación conservadora conserva el root SSR completo como HTML estático y dispone los recursos candidatos.
- No existe reparación granular automática de cualquier subregión, ni serialización automática de modelos de aplicación.

Esto es especialmente relevante en formularios: no destruyas el valor que un usuario ya escribió al reemplazar la página con una vista que no coincide. Cuando el markup inicial no puede hidratarse con seguridad, el fallback conserva el contenido SSR estático.

## 7. Cómo se conecta con la navegación

La navegación cliente debe instalarse solo después de que el montaje/hidratación aplicable haya tenido éxito. El documento del servidor sigue siendo la fuente de verdad para peticiones directas y fallbacks. La API de navegación no inserta arbitrariamente el HTML descargado; la aplicación mantiene el resolver de rutas y el estado que actualiza la vista.

Consulta [Navegación cliente y formularios](/guides/navigation/) para los criterios de interceptación, History API, formularios GET opt-in y casos que mantienen el envío HTML nativo.

## Errores frecuentes

- **Usar un fragmento como si fuera un documento:** utiliza <code>render_to_string</code> para fragmentos y <code>html_document</code>/<code>HtmlPage</code> para una respuesta completa.
- **Enviar HTML construido desde entrada de usuario:** usa nodos estructurales. <code>Response::html(String)</code> no escapa su argumento.
- **Compartir estado de una petición en el catálogo:** comparte definiciones inmutables, no señales ni props con datos específicos de usuarios.
- **Suponer que HydrationData hidrata:** el tipo solo valida/formatea JSON y las APIs de página emiten los límites; el cliente debe ejecutar el hidratador explícitamente.
- **Hidratar una vista distinta:** conserva markup compatible y trata la discrepancia de forma conservadora.
- **Pasar secretos al navegador:** proyecta solo una allow-list de campos públicos.

## Verificación

La implementación y tests viven en los crates <code>rustflow-ssr</code>, <code>rustflow-server</code> y <code>rustflow-dom</code>. Ejecuta la suite del workspace y los tests E2E del ejemplo de navegador para la revisión exacta que estás usando. Para el contrato técnico, consulta [SSR_RENDERING.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/SSR_RENDERING.md).
