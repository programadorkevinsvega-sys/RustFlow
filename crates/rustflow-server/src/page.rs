//! Composable, server-rendered HTML pages backed by RustFlow components.
//!
//! Route handlers should project request values into an explicit list of public component
//! properties, then render the page. Each SSR render creates a fresh reactive runtime; the
//! registry stores definitions, not per-request component state.

use rustflow::components::ComponentRegistry;
use rustflow_view::{Property, View};

use crate::{HttpError, Response, html_document, html_document_with_components};

/// A composable full HTML page.
///
/// Use from_component to create a page from a route-facing component, with_layout to wrap it
/// in layout components, and with_components to supply the registry that resolves those
/// components. Layout calls wrap the current body, so add inner layouts first and outer
/// layouts last.
#[derive(Clone)]
pub struct HtmlPage {
    title: String,
    language: String,
    body: View,
    components: Option<ComponentRegistry>,
}

impl HtmlPage {
    /// Create a page from an already-built structural view.
    pub fn new(title: impl Into<String>, body: View) -> Self {
        Self {
            title: title.into(),
            language: "en".to_owned(),
            body,
            components: None,
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

    /// Use a registry to resolve the page component and any layout components.
    pub fn with_components(mut self, registry: &ComponentRegistry) -> Self {
        self.components = Some(registry.clone());
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
    /// A component-backed body or layout requires a registry supplied by with_components.
    pub fn render(&self) -> Result<Response, HttpError> {
        match &self.components {
            Some(registry) => {
                html_document_with_components(&self.title, &self.language, &self.body, registry)
            }
            None => html_document(&self.title, &self.language, &self.body),
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
        assert!(html.contains(
            "<div><main><article><h1>first-post</h1></article></main></div>"
        ));
    }
}