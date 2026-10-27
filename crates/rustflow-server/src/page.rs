//! Composable, server-rendered HTML pages backed by RustFlow components.
//!
//! Route handlers should project request values into an explicit list of public component
//! properties, then render the page. Each SSR render creates a fresh reactive runtime; the
//! registry stores definitions, not per-request component state.
//!
//! The local ComponentRegistry remains available for component callbacks that are not shareable.
//! HTTP routes can instead capture a SharedComponentCatalog with Send + Sync factories.
//! Each request still creates a fresh local reactive runtime, scopes, and render callbacks.

use rustflow::components::{ComponentRegistry, SharedComponentCatalog};
use rustflow_view::{Property, View};

use crate::{
    HttpError, Response, html_document, html_document_with_components,
    html_document_with_components_and_hydration_data,
    html_document_with_hydration_data, html_document_with_shared_components,
    html_document_with_shared_components_and_hydration_data,
};
use rustflow_ssr::HydrationData;

/// A composable full HTML page.
///
/// Use from_component to create a page from a route-facing component and with_layout to wrap
/// it in layout components. For concurrent HTTP routes, prefer with_shared_components with a
/// catalog built once during application initialization. Layout calls wrap the current body,
/// so add inner layouts first and outer layouts last.
#[derive(Clone)]
pub struct HtmlPage {
    title: String,
    language: String,
    body: View,
    components: Option<ComponentRegistry>,
    shared_components: Option<SharedComponentCatalog>,
    hydration_data: Option<HydrationData>,
}

impl HtmlPage {
    /// Create a page from an already-built structural view.
    pub fn new(title: impl Into<String>, body: View) -> Self {
        Self {
            title: title.into(),
            language: "en".to_owned(),
            body,
            components: None,
            shared_components: None,
            hydration_data: None,
        }
    }

    /// Create a page whose body is a registered component, passing explicit route props and
    /// optional child views to that component.
    ///
    /// Only pass fields intended to be visible in the rendered HTML. Request headers, cookies,
    /// credentials, authorization state, and private server data must not be forwarded as props.
    pub fn from_component<P, C>(
        title: impl Into<String>,
        component_name: impl Into<String>,
        properties: P,
        children: C,
    ) -> Result<Self, HttpError>
    where
        P: IntoIterator<Item = Property>,
        C: IntoIterator<Item = View>,
    {
        let body = View::component(component_name, properties, children, None)
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(Self::new(title, body))
    }

    /// Set the document language attribute. The value is escaped by the SSR renderer.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Use a local registry for callbacks that cannot satisfy `Send + Sync`.
    ///
    /// For concurrent HTTP routes, prefer `with_shared_components` with a startup-built catalog.
    pub fn with_components(mut self, registry: &ComponentRegistry) -> Self {
        self.components = Some(registry.clone());
        self.shared_components = None;
        self
    }

    /// Use the shareable immutable catalog for this page and its layout components.
    ///
    /// This is the recommended API for concurrent HTTP routes. Build the catalog during
    /// application initialization. Factories are shared; runtimes, scopes, views and render
    /// callbacks are local to each render.
    pub fn with_shared_components(mut self, catalog: &SharedComponentCatalog) -> Self {
        self.components = None;
        self.shared_components = Some(catalog.clone());
        self
    }

    /// Attach explicitly projected public JSON data and opt this page into hydration markup.
    ///
    /// This adds deterministic DOM boundary comments and an inert JSON script to the response.
    /// It does not execute browser code or automatically infer whether application data contains
    /// secrets; construct the payload from an explicit allow-list of public fields.
    pub fn with_hydration_data(mut self, data: HydrationData) -> Self {
        self.hydration_data = Some(data);
        self
    }

    /// Wrap the current body in a layout component with explicit props.
    ///
    /// Call this for the innermost layout first and the outermost layout last. The layout
    /// component receives the current body as its children; no raw HTML string is inserted.
    pub fn with_layout<P>(
        mut self,
        component_name: impl Into<String>,
        properties: P,
    ) -> Result<Self, HttpError>
    where
        P: IntoIterator<Item = Property>,
    {
        self.body = View::component(component_name, properties, [self.body], None)
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(self)
    }

    /// Render a complete safe HTML document as an HTTP response.
    ///
    /// Component-backed bodies/layouts require a local registry or shared catalog.
    pub fn render(&self) -> Result<Response, HttpError> {
        if let Some(data) = self.hydration_data.as_ref() {
            return match (&self.components, &self.shared_components) {
                (Some(registry), _) => html_document_with_components_and_hydration_data(
                    &self.title,
                    &self.language,
                    &self.body,
                    registry,
                    data,
                ),
                (_, Some(catalog)) => html_document_with_shared_components_and_hydration_data(
                    &self.title,
                    &self.language,
                    &self.body,
                    catalog,
                    data,
                ),
                (None, None) => {
                    html_document_with_hydration_data(&self.title, &self.language, &self.body, data)
                }
            };
        }

        match (&self.components, &self.shared_components) {
            (Some(registry), _) => {
                html_document_with_components(&self.title, &self.language, &self.body, registry)
            }
            (_, Some(catalog)) => html_document_with_shared_components(
                &self.title,
                &self.language,
                &self.body,
                catalog,
            ),
            (None, None) => html_document(&self.title, &self.language, &self.body),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HtmlPage;
    use rustflow::components::{ComponentDefinition, ComponentRegistry, ComponentRender};
    use rustflow_view::{Property, PropertyValue, View};
    use std::rc::Rc;

    fn wrapper(name: &str, tag: &str) -> ComponentDefinition {
        let tag = tag.to_owned();
        ComponentDefinition::new(name, move |_context, _properties| {
            let tag = tag.clone();
            let render: ComponentRender = Rc::new(move |_properties, children| {
                Ok(View::element(tag.clone(), children.to_vec())?)
            });
            Ok(render)
        })
        .expect("wrapper definition")
    }

    #[test]
    fn hydratable_page_embeds_script_safe_public_data_and_boundary_markers() {
        use rustflow_ssr::HydrationData;

        let body = View::element("main", [View::text("Hello")]).expect("body");
        let data = HydrationData::from_json(r#"{"public":"</script><svg>&"}"#)
            .expect("valid public JSON");
        let response = HtmlPage::new("Hydration", body)
            .with_hydration_data(data)
            .render()
            .expect("render hydratable page");
        let html = std::str::from_utf8(response.body())
            .expect("HTML response is UTF-8");

        assert!(html.contains(
            "<!--rustflow:root:start--><main>Hello</main><!--rustflow:root:end-->"
        ));
        assert!(html.contains(
            "<script type=\"application/json\" id=\"rustflow-hydration-data\">"
        ));
        assert!(html.contains(r#""public":"\u003c/script\u003e\u003csvg\u003e\u0026""#));
        assert!(!html.contains("</script><svg>"));
    }

    #[test]
    fn route_component_props_flow_through_nested_layouts_into_a_safe_document() {
        let article = ComponentDefinition::new("ArticlePage", |_context, _properties| {
            let render: ComponentRender = Rc::new(|properties, _children| {
                let slug = properties
                    .iter()
                    .find(|property| property.name() == "slug")
                    .and_then(|property| match property.value() {
                        PropertyValue::Text(value) => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "missing".to_owned());
                Ok(View::element(
                    "article",
                    [View::element("h1", [View::text(slug)])?],
                )?)
            });
            Ok(render)
        })
        .expect("article definition");

        let mut registry = ComponentRegistry::new();
        registry.register(article).expect("register article");
        registry
            .register(wrapper("SectionLayout", "main"))
            .expect("register section layout");
        registry
            .register(wrapper("AppLayout", "div"))
            .expect("register app layout");

        // A route handler can map a validated path parameter into this public prop.
        let page = HtmlPage::from_component(
            "Article & details",
            "ArticlePage",
            [Property::new("slug", "first-post").expect("slug property")],
            [],
        )
        .expect("page")
        .language("es")
        .with_components(&registry)
        .with_layout("SectionLayout", [])
        .expect("section layout")
        .with_layout("AppLayout", [])
        .expect("app layout");

        let response = page.render().expect("render page");
        let html = std::str::from_utf8(response.body.as_ref()).expect("UTF-8 HTML");
        assert!(html.starts_with("<!doctype html><html lang=\"es\">"));
        assert!(html.contains("<title>Article &amp; details</title>"));
        assert!(html.contains("<div><main><article><h1>first-post</h1></article></main></div>"));
    }
}
