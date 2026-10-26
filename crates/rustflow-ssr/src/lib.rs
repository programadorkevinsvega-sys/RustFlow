//! Safe-by-default HTML fragment serialization for RustFlow's structural view model.
//!
//! This crate emits HTML fragments. It is not an HTTP server, does not serialize
//! event callbacks, and does not make a View value trusted HTML.

use std::{
    collections::HashSet,
    error::Error,
    fmt,
};

use rustflow::{
    components::{ComponentError, ComponentInstance, ComponentRegistry},
    reactivity::{Runtime, RuntimeError, Scope},
};
use rustflow_view::{
    AttributeValue, ComponentNode, Element, KeyedList, Property, PropertyValue, View, ViewError,
};

const DEFAULT_MAX_DEPTH: usize = 256;

const SUPPORTED_ELEMENTS: &[&str] = &[
    "a", "abbr", "address", "article", "aside", "b", "blockquote", "body", "br",
    "button", "caption", "code", "col", "colgroup", "dd", "del", "details", "div",
    "dl", "dt", "em", "fieldset", "figcaption", "figure", "footer", "form", "h1",
    "h2", "h3", "h4", "h5", "h6", "head", "header", "hr", "html", "i", "img",
    "input", "ins", "kbd", "label", "legend", "li", "main", "mark", "nav", "ol",
    "optgroup", "option", "output", "p", "picture", "pre", "progress", "q", "s",
    "samp", "section", "select", "small", "source", "span", "strong", "sub",
    "summary", "sup", "table", "tbody", "td", "textarea", "tfoot", "th", "thead",
    "time", "title", "tr", "u", "ul", "var",
];

const VOID_ELEMENTS: &[&str] = &["br", "col", "hr", "img", "input", "source"];

const URL_ATTRIBUTES: &[&str] = &[
    "action", "formaction", "href", "poster", "src", "xlink:href",
];

const MULTI_URL_ATTRIBUTES: &[&str] = &["ping", "srcset"];

const SAFE_ATTRIBUTES: &[&str] = &[
    "accept", "accept-charset", "accesskey", "action", "alt", "autocapitalize",
    "autocomplete", "autofocus", "capture", "charset", "checked", "cite", "class",
    "cols", "colspan", "content", "contenteditable", "controls", "coords", "crossorigin",
    "datetime", "decoding", "default", "defer", "dir", "dirname", "disabled", "download",
    "draggable", "enctype", "for", "form", "formaction", "formenctype", "formmethod",
    "formnovalidate", "formtarget", "height", "hidden", "high", "hreflang", "href",
    "id", "inert", "inputmode", "integrity", "ismap", "kind", "label", "lang", "list",
    "loop", "low", "max", "maxlength", "media", "method", "min", "minlength", "multiple",
    "muted", "name", "novalidate", "open", "optimum", "pattern", "placeholder", "playsinline",
    "poster", "preload", "readonly", "referrerpolicy", "rel", "required", "reversed",
    "role", "rows", "rowspan", "sandbox", "scope", "selected", "shape", "size", "span",
    "spellcheck", "start", "step", "tabindex", "target", "title", "translate", "type",
    "usemap", "value", "width",
];

/// Errors from HTML serialization or server-side component evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsrError {
    UnsupportedElement(String),
    VoidElementHasChildren(String),
    UnsafeAttribute(String),
    UnsafeProperty(String),
    UnsafeUrl(String),
    DuplicateSerializedAttribute(String),
    InvalidPropertyValue(String),
    NestingLimitExceeded { limit: usize },
    Component(String),
    Runtime(String),
    View(ViewError),
}

impl fmt::Display for SsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedElement(name) => {
                write!(f, "element is not supported by the SSR renderer: {name}")
            }
            Self::VoidElementHasChildren(name) => {
                write!(f, "void element cannot have children: {name}")
            }
            Self::UnsafeAttribute(name) => {
                write!(f, "attribute is rejected by the SSR safety policy: {name}")
            }
            Self::UnsafeProperty(name) => {
                write!(f, "property is not serializable by the SSR safety policy: {name}")
            }
            Self::UnsafeUrl(_) => write!(f, "URL scheme is not allowed by the SSR safety policy"),
            Self::DuplicateSerializedAttribute(name) => {
                write!(f, "multiple inputs serialize to the same HTML attribute: {name}")
            }
            Self::InvalidPropertyValue(name) => {
                write!(f, "property has the wrong value type for HTML serialization: {name}")
            }
            Self::NestingLimitExceeded { limit } => {
                write!(f, "view nesting exceeds the configured limit of {limit}")
            }
            Self::Component(message) => write!(f, "component rendering failed: {message}"),
            Self::Runtime(message) => write!(f, "SSR reactive runtime failed: {message}"),
            Self::View(error) => write!(f, "view construction failed: {error}"),
        }
    }
}

impl Error for SsrError {}

impl From<ViewError> for SsrError {
    fn from(error: ViewError) -> Self {
        Self::View(error)
    }
}

impl From<RuntimeError> for SsrError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error.to_string())
    }
}

impl From<ComponentError> for SsrError {
    fn from(error: ComponentError) -> Self {
        Self::Component(error.to_string())
    }
}

/// A reusable renderer for safe HTML fragments.
///
/// Component definitions are optional. When supplied, component descriptors are
/// resolved with a new reactive runtime for every render request.
#[derive(Clone)]
pub struct HtmlRenderer {
    registry: Option<ComponentRegistry>,
    max_depth: usize,
}

impl HtmlRenderer {
    /// Create a renderer for trees without component descriptors.
    pub fn new() -> Self {
        Self {
            registry: None,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    /// Create a renderer that resolves component descriptors using this registry.
    pub fn with_components(registry: &ComponentRegistry) -> Self {
        Self {
            registry: Some(registry.clone()),
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    /// Set the maximum nested-view depth. A minimum of one is enforced.
    pub fn with_max_depth(mut self, limit: usize) -> Self {
        self.max_depth = limit.max(1);
        self
    }

    /// Render a structural view to an HTML fragment.
    ///
    /// Text and attribute values are escaped. Unsupported elements, event-handler
    /// attributes, raw-HTML sinks, unsafe URL schemes, and unsupported multi-URL
    /// attributes are rejected.
    pub fn render(&self, view: &View) -> Result<String, SsrError> {
        let runtime = Runtime::new();
        let root_scope = runtime.root_scope();
        let mut context = RenderContext {
            registry: self.registry.clone(),
            runtime: runtime.clone(),
            output: String::new(),
            max_depth: self.max_depth,
        };

        let render_result = context.write_view(view, &root_scope, 0);
        let dispose_result = runtime.dispose();
        render_result?;
        dispose_result?;
        Ok(context.output)
    }
}

impl Default for HtmlRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a tree that does not contain component descriptors.
pub fn render_to_string(view: &View) -> Result<String, SsrError> {
    HtmlRenderer::new().render(view)
}

/// Render a tree with component descriptors resolved through a registry.
pub fn render_with_components(
    view: &View,
    registry: &ComponentRegistry,
) -> Result<String, SsrError> {
    HtmlRenderer::with_components(registry).render(view)
}

struct RenderContext {
    registry: Option<ComponentRegistry>,
    runtime: Runtime,
    output: String,
    max_depth: usize,
}

impl RenderContext {
    fn write_view(
        &mut self,
        view: &View,
        owner_scope: &Scope,
        depth: usize,
    ) -> Result<(), SsrError> {
        if depth > self.max_depth {
            return Err(SsrError::NestingLimitExceeded {
                limit: self.max_depth,
            });
        }

        match view {
            View::Text(text) => {
                escape_text(text, &mut self.output);
                Ok(())
            }
            View::Element(element) => self.write_element(element, owner_scope, depth),
            View::Fragment(fragment) => {
                for child in fragment.children() {
                    self.write_view(child, owner_scope, depth + 1)?;
                }
                Ok(())
            }
            View::Component(component) => self.write_component(component, owner_scope, depth),
            View::Dynamic(dynamic) => {
                let child = dynamic.evaluate()?;
                self.write_view(&child, owner_scope, depth + 1)
            }
            View::KeyedList(list) => self.write_keyed_list(list, owner_scope, depth),
        }
    }

    fn write_element(
        &mut self,
        element: &Element,
        owner_scope: &Scope,
        depth: usize,
    ) -> Result<(), SsrError> {
        let tag = element.tag().to_ascii_lowercase();
        if !SUPPORTED_ELEMENTS.contains(&tag.as_str()) {
            return Err(SsrError::UnsupportedElement(tag));
        }
        if !element.children().is_empty() && VOID_ELEMENTS.contains(&tag.as_str()) {
            return Err(SsrError::VoidElementHasChildren(tag));
        }

        self.output.push('<');
        self.output.push_str(&tag);
        let mut emitted = HashSet::new();

        for attribute in element.attributes() {
            self.write_attribute(attribute.name(), attribute.value(), &mut emitted)?;
        }
        for property in element.properties() {
            self.write_property(&tag, property, &mut emitted)?;
        }
        self.output.push('>');

        if VOID_ELEMENTS.contains(&tag.as_str()) {
            return Ok(());
        }

        // A textarea's initial value is represented by its text content, not a value attribute.
        if tag == "textarea" {
            if let Some(property) = element.properties().iter().find(|p| p.name().eq_ignore_ascii_case("value")) {
                let PropertyValue::Text(value) = property.value() else {
                    return Err(SsrError::InvalidPropertyValue(property.name().to_owned()));
                };
                if !element.children().is_empty() {
                    return Err(SsrError::DuplicateSerializedAttribute("textarea value/children".into()));
                }
                escape_text(value, &mut self.output);
            } else {
                for child in element.children() {
                    self.write_view(child, owner_scope, depth + 1)?;
                }
            }
        } else {
            for child in element.children() {
                self.write_view(child, owner_scope, depth + 1)?;
            }
        }

        self.output.push_str("</");
        self.output.push_str(&tag);
        self.output.push('>');
        Ok(())
    }

    fn write_attribute(
        &mut self,
        name: &str,
        value: &AttributeValue,
        emitted: &mut HashSet<String>,
    ) -> Result<(), SsrError> {
        let normalized = name.to_ascii_lowercase();
        if normalized == "style"
            || matches!(normalized.as_str(), "srcdoc" | "innerhtml" | "outerhtml")
            || (normalized.starts_with("on") && normalized.len() > 2)
            || MULTI_URL_ATTRIBUTES.contains(&normalized.as_str())
            || !is_allowed_attribute(&normalized)
        {
            return Err(SsrError::UnsafeAttribute(name.to_owned()));
        }

        if !emitted.insert(normalized.clone()) {
            return Err(SsrError::DuplicateSerializedAttribute(normalized));
        }

        let attribute_value = match value {
            AttributeValue::Boolean(false) => return Ok(()),
            AttributeValue::Boolean(true) => "",
            AttributeValue::Text(value) => value.as_str(),
        };

        if URL_ATTRIBUTES.contains(&normalized.as_str()) {
            if matches!(value, AttributeValue::Boolean(_)) {
                return Err(SsrError::UnsafeAttribute(name.to_owned()));
            }
            validate_url(attribute_value)?;
        }

        self.output.push(' ');
        self.output.push_str(&normalized);
        self.output.push_str("=\"");
        escape_attribute(attribute_value, &mut self.output);
        self.output.push('"');
        Ok(())
    }

    fn write_property(
        &mut self,
        tag: &str,
        property: &Property,
        emitted: &mut HashSet<String>,
    ) -> Result<(), SsrError> {
        let name = property.name().to_ascii_lowercase();
        if name == "style" || (name.starts_with("on") && name.len() > 2) {
            return Err(SsrError::UnsafeProperty(property.name().to_owned()));
        }

        if name == "value" && tag == "textarea" {
            return Ok(());
        }

        if let Some(attribute_name) = boolean_property_attribute(&name) {
            let PropertyValue::Boolean(value) = property.value() else {
                return Err(SsrError::InvalidPropertyValue(property.name().to_owned()));
            };
            return self.write_attribute(
                attribute_name,
                &AttributeValue::Boolean(*value),
                emitted,
            );
        }

        let Some(attribute_name) = text_property_attribute(tag, &name) else {
            // Properties with no server-side HTML representation remain client-only.
            return Ok(());
        };
        let value = match property.value() {
            PropertyValue::Text(value) => value.clone(),
            PropertyValue::Boolean(_) if matches!(name.as_str(), "contenteditable" | "draggable" | "spellcheck") => {
                property_value_as_text(property.value())
            }
            PropertyValue::Integer(_) | PropertyValue::Float(_) => property_value_as_text(property.value()),
            PropertyValue::Boolean(_) => {
                return Err(SsrError::InvalidPropertyValue(property.name().to_owned()));
            }
        };
        self.write_attribute(
            attribute_name,
            &AttributeValue::Text(value),
            emitted,
        )
    }

    fn write_keyed_list(
        &mut self,
        list: &KeyedList,
        owner_scope: &Scope,
        depth: usize,
    ) -> Result<(), SsrError> {
        for child in list.children() {
            self.write_view(child.view(), owner_scope, depth + 1)?;
        }
        Ok(())
    }

    fn write_component(
        &mut self,
        component: &ComponentNode,
        parent_scope: &Scope,
        depth: usize,
    ) -> Result<(), SsrError> {
        let registry = self.registry.clone().ok_or_else(|| {
            SsrError::Component(format!(
                "component {} requires a ComponentRegistry",
                component.name()
            ))
        })?;

        let mut instance = registry.mount(component, self.runtime.clone(), parent_scope)?;
        let view = instance.view().clone();
        let render_result = self.write_view(&view, instance.scope(), depth + 1);
        let cleanup_result = instance.unmount().map_err(SsrError::from);

        match render_result {
            Err(error) => {
                let _ = cleanup_result;
                Err(error)
            }
            Ok(()) => cleanup_result,
        }
    }
}

fn is_allowed_attribute(name: &str) -> bool {
    SAFE_ATTRIBUTES.contains(&name)
        || name
            .strip_prefix("aria-")
            .is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
            })
        || name
            .strip_prefix("data-")
            .is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
            })
}

fn boolean_property_attribute(name: &str) -> Option<&'static str> {
    match name {
        "autofocus" => Some("autofocus"),
        "allowfullscreen" => Some("allowfullscreen"),
        "checked" => Some("checked"),
        "controls" => Some("controls"),
        "disabled" => Some("disabled"),
        "formnovalidate" => Some("formnovalidate"),
        "hidden" => Some("hidden"),
        "inert" => Some("inert"),
        "loop" => Some("loop"),
        "multiple" => Some("multiple"),
        "muted" => Some("muted"),
        "novalidate" => Some("novalidate"),
        "open" => Some("open"),
        "readonly" => Some("readonly"),
        "required" => Some("required"),
        "selected" => Some("selected"),
        _ => None,
    }
}

fn text_property_attribute(tag: &str, name: &str) -> Option<&'static str> {
    match name {
        "accept" => Some("accept"),
        "action" => Some("action"),
        "alt" => Some("alt"),
        "autocomplete" => Some("autocomplete"),
        "class" | "classname" => Some("class"),
        "cols" => Some("cols"),
        "colspan" => Some("colspan"),
        "contenteditable" => Some("contenteditable"),
        "dir" => Some("dir"),
        "draggable" => Some("draggable"),
        "enctype" => Some("enctype"),
        "formaction" => Some("formaction"),
        "for" | "htmlfor" => Some("for"),
        "height" => Some("height"),
        "href" => Some("href"),
        "id" => Some("id"),
        "lang" => Some("lang"),
        "maxlength" => Some("maxlength"),
        "method" => Some("method"),
        "min" => Some("min"),
        "minlength" => Some("minlength"),
        "max" => Some("max"),
        "name" => Some("name"),
        "pattern" => Some("pattern"),
        "placeholder" => Some("placeholder"),
        "poster" => Some("poster"),
        "rel" => Some("rel"),
        "role" => Some("role"),
        "rowspan" => Some("rowspan"),
        "rows" => Some("rows"),
        "size" => Some("size"),
        "spellcheck" => Some("spellcheck"),
        "src" => Some("src"),
        "step" => Some("step"),
        "tabindex" => Some("tabindex"),
        "target" => Some("target"),
        "title" => Some("title"),
        "type" => Some("type"),
        "value" if tag != "textarea" => Some("value"),
        "width" => Some("width"),
        _ => None,
    }
}

fn property_value_as_text(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Text(value) => value.clone(),
        PropertyValue::Boolean(value) => value.to_string(),
        PropertyValue::Integer(value) => value.to_string(),
        PropertyValue::Float(value) => value.to_string(),
    }
}

fn validate_url(value: &str) -> Result<(), SsrError> {
    let normalized = value
        .chars()
        .filter(|character| !character.is_ascii_control())
        .collect::<String>()
        .trim()
        .to_ascii_lowercase();
    if let Some(scheme) = url_scheme(&normalized) {
        if !matches!(scheme, "http" | "https" | "mailto" | "tel") {
            return Err(SsrError::UnsafeUrl(value.to_owned()));
        }
    }
    Ok(())
}

fn url_scheme(value: &str) -> Option<&str> {
    let colon = value.find(':')?;
    let delimiter = value.find(['/', '?', '#']);
    if delimiter.is_some_and(|delimiter| delimiter < colon) {
        return None;
    }
    let candidate = &value[..colon];
    let mut characters = candidate.chars();
    if !characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return None;
    }
    if !characters.all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-')
    }) {
        return None;
    }
    Some(candidate)
}

fn escape_text(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            other => output.push(other),
        }
    }
}

fn escape_attribute(value: &str, output: &mut String) {
    escape_text(value, output);
}

#[cfg(test)]
mod tests {
    use super::{HtmlRenderer, SsrError, render_to_string, render_with_components};
    use rustflow::{
        components::{ComponentDefinition, ComponentRegistry, ComponentRender},
        view::{Element, Property, View},
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn escapes_text_and_attribute_values() {
        let view = Element::new("div")
            .expect("element")
            .with_attribute("title", "\"<hello & goodbye>'")
            .expect("attribute")
            .with_children([View::text("<script> & \"quoted\"")])
            .build();
        assert_eq!(
            render_to_string(&view).expect("render"),
            "<div title=\"&quot;&lt;hello &amp; goodbye&gt;&#39;&quot;\">&lt;script&gt; &amp; &quot;quoted&quot;</div>"
        );
    }

    #[test]
    fn emits_boolean_attributes_only_when_true() {
        let view = Element::new("input")
            .expect("element")
            .with_boolean_attribute("disabled", true)
            .expect("disabled")
            .with_boolean_attribute("required", false)
            .expect("required")
            .build();
        assert_eq!(
            render_to_string(&view).expect("render"),
            "<input disabled=\"\">"
        );
    }

    #[test]
    fn rejects_active_elements_and_void_elements_with_children() {
        let script = View::element("script", [View::text("alert(1)")])
            .expect("syntactically valid node");
        assert!(matches!(
            render_to_string(&script),
            Err(SsrError::UnsupportedElement(_))
        ));
        let input = View::element("input", [View::text("unexpected")]).expect("node");
        assert!(matches!(
            render_to_string(&input),
            Err(SsrError::VoidElementHasChildren(_))
        ));
    }

    #[test]
    fn rejects_unsafe_urls_style_and_multi_url_attributes() {
        let url = Element::new("a")
            .expect("element")
            .with_attribute("href", "javascript:alert(1)")
            .expect("attribute")
            .build();
        assert!(matches!(
            render_to_string(&url),
            Err(SsrError::UnsafeUrl(_))
        ));

        let style = Element::new("div")
            .expect("element")
            .with_attribute("style", "display:none")
            .expect("syntactically valid attribute")
            .build();
        assert!(matches!(
            render_to_string(&style),
            Err(SsrError::UnsafeAttribute(_))
        ));

        let srcset = Element::new("img")
            .expect("element")
            .with_attribute("srcset", "a.png 1x, b.png 2x")
            .expect("syntactically valid attribute")
            .build();
        assert!(matches!(
            render_to_string(&srcset),
            Err(SsrError::UnsafeAttribute(_))
        ));
    }

    #[test]
    fn serializes_form_properties_but_not_event_callbacks() {
        let view = Element::new("input")
            .expect("element")
            .with_property("value", "query")
            .expect("value")
            .with_property("checked", true)
            .expect("checked")
            .with_event("click", |_event| {})
            .expect("event")
            .build();
        assert_eq!(
            render_to_string(&view).expect("render"),
            "<input value=\"query\" checked=\"\">"
        );
    }

    #[test]
    fn rejects_conflicting_attribute_and_property_output() {
        let view = Element::new("input")
            .expect("element")
            .with_attribute("value", "attribute")
            .expect("attribute")
            .with_property("value", "property")
            .expect("property")
            .build();
        assert!(matches!(
            render_to_string(&view),
            Err(SsrError::DuplicateSerializedAttribute(_))
        ));
    }

    #[test]
    fn renders_dynamic_and_keyed_children_in_order() {
        let view = View::fragment([
            View::keyed_list([
                rustflow_view::KeyedChild::new("one", View::text("first")),
                rustflow_view::KeyedChild::new("two", View::text("second")),
            ])
            .expect("keyed list"),
            View::dynamic(|| Ok(View::text("dynamic"))),
        ]);
        assert_eq!(render_to_string(&view).expect("render"), "firstseconddynamic");
    }

    #[test]
    fn renders_registered_components_and_runs_cleanup() {
        let cleanups = Rc::new(Cell::new(0));
        let setup_cleanups = cleanups.clone();
        let definition = ComponentDefinition::new("Card", move |context, _properties| {
            let cleanup_count = setup_cleanups.clone();
            context.on_cleanup(move || cleanup_count.set(cleanup_count.get() + 1))?;
            let render: ComponentRender = Rc::new(|_properties, children| {
                Ok(View::element("section", children.to_vec())?)
            });
            Ok(render)
        }).expect("definition");
        let mut registry = ComponentRegistry::new();
        registry.register(definition).expect("registry");
        let view = View::component("Card", [], [View::text("body")], None).expect("component");
        assert_eq!(
            render_with_components(&view, &registry).expect("render"),
            "<section>body</section>"
        );
        assert_eq!(cleanups.get(), 1);
    }

    #[test]
    fn rejects_unregistered_components_and_excessive_nesting() {
        let view = View::component("Missing", [], [], None).expect("component");
        assert!(matches!(render_to_string(&view), Err(SsrError::Component(_))));
        let nested = View::element(
            "div",
            [View::element("span", [View::text("deep")]).expect("span")],
        )
        .expect("div");
        assert!(matches!(
            HtmlRenderer::new().with_max_depth(1).render(&nested),
            Err(SsrError::NestingLimitExceeded { limit: 1 })
        ));
    }
}
