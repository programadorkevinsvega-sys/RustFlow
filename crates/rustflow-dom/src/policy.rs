use std::fmt;

/// Rejections made by the initial browser renderer's safety policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomPolicyError {
    UnsupportedElement(String),
    VoidElementHasChildren(String),
    UnsafeAttribute(String),
    UnsafeProperty(String),
    UnsafeUrl(String),
}

impl fmt::Display for DomPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedElement(name) => {
                write!(
                    f,
                    "element is not supported by the initial DOM renderer: {name}"
                )
            }
            Self::VoidElementHasChildren(name) => {
                write!(f, "void element cannot have children: {name}")
            }
            Self::UnsafeAttribute(name) => {
                write!(
                    f,
                    "attribute is not supported by the safe DOM renderer: {name}"
                )
            }
            Self::UnsafeProperty(name) => {
                write!(
                    f,
                    "property is not supported by the safe DOM renderer: {name}"
                )
            }
            Self::UnsafeUrl(_) => {
                write!(f, "URL scheme is not allowed by the safe DOM renderer")
            }
        }
    }
}

impl std::error::Error for DomPolicyError {}

const SUPPORTED_ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "blockquote",
    "br",
    "button",
    "caption",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "div",
    "dl",
    "dt",
    "em",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "i",
    "img",
    "input",
    "ins",
    "kbd",
    "label",
    "legend",
    "li",
    "main",
    "mark",
    "nav",
    "ol",
    "optgroup",
    "option",
    "output",
    "p",
    "picture",
    "pre",
    "progress",
    "q",
    "s",
    "samp",
    "section",
    "select",
    "small",
    "source",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "u",
    "ul",
    "var",
];

const VOID_ELEMENTS: &[&str] = &["br", "col", "hr", "img", "input", "source"];

const URL_ATTRIBUTES: &[&str] = &[
    "action",
    "formaction",
    "href",
    "poster",
    "src",
    "xlink:href",
];

// Multi-URL attributes require dedicated parsing; fail closed until implemented.
const MULTI_URL_ATTRIBUTES: &[&str] = &["ping", "srcset"];

/// Validate a tag against the initial renderer allowlist and void-element rules.
pub fn validate_element(name: &str, child_count: usize) -> Result<(), DomPolicyError> {
    let normalized = name.to_ascii_lowercase();
    if !SUPPORTED_ELEMENTS.contains(&normalized.as_str()) {
        return Err(DomPolicyError::UnsupportedElement(name.to_owned()));
    }
    if child_count > 0 && VOID_ELEMENTS.contains(&normalized.as_str()) {
        return Err(DomPolicyError::VoidElementHasChildren(name.to_owned()));
    }
    Ok(())
}

/// Validate a DOM property before setting it through reflection.
///
/// Event-handler properties and direct style mutation are intentionally not
/// part of this initial renderer. URL-valued properties share the attribute
/// scheme policy; non-text values are rejected for URL-bearing properties.
pub fn validate_property(
    name: &str,
    value: &rustflow_view::PropertyValue,
) -> Result<(), DomPolicyError> {
    let normalized = name.to_ascii_lowercase();
    if normalized == "style" || (normalized.starts_with("on") && normalized.len() > 2) {
        return Err(DomPolicyError::UnsafeProperty(name.to_owned()));
    }
    if URL_ATTRIBUTES.contains(&normalized.as_str()) {
        let rustflow_view::PropertyValue::Text(value) = value else {
            return Err(DomPolicyError::UnsafeProperty(name.to_owned()));
        };
        validate_attribute(name, value)?;
    }
    Ok(())
}

/// Reject style sinks and active URL schemes before applying an attribute.
pub fn validate_attribute(name: &str, value: &str) -> Result<(), DomPolicyError> {
    let name = name.to_ascii_lowercase();
    if name == "style" || MULTI_URL_ATTRIBUTES.contains(&name.as_str()) {
        return Err(DomPolicyError::UnsafeAttribute(name));
    }
    if URL_ATTRIBUTES.contains(&name.as_str()) {
        let normalized = value
            .chars()
            .filter(|character| !character.is_ascii_control())
            .collect::<String>()
            .trim()
            .to_ascii_lowercase();
        if let Some(scheme) = url_scheme(&normalized) {
            if !matches!(scheme, "http" | "https" | "mailto" | "tel") {
                return Err(DomPolicyError::UnsafeUrl(value.to_owned()));
            }
        }
    }
    Ok(())
}

fn url_scheme(value: &str) -> Option<&str> {
    let colon = value.find(':')?;
    let delimiter = value.find(|character: char| matches!(character, '/' | '?' | '#'));
    if delimiter.is_some_and(|delimiter| delimiter < colon) {
        return None;
    }
    let candidate = &value[..colon];
    let mut characters = candidate.chars();
    if !characters.next().is_some_and(|character| character.is_ascii_alphabetic()) {
        return None;
    }
    if !characters.all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-')
    }) {
        return None;
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::{DomPolicyError, validate_attribute, validate_element, validate_property};
    use rustflow_view::PropertyValue;

    #[test]
    fn allows_common_semantic_elements_and_rejects_active_elements() {
        assert!(validate_element("button", 1).is_ok());
        assert!(validate_element("custom-widget", 0).is_err());
        assert!(matches!(
            validate_element("script", 0),
            Err(DomPolicyError::UnsupportedElement(_))
        ));
        assert!(matches!(
            validate_element("iframe", 0),
            Err(DomPolicyError::UnsupportedElement(_))
        ));
    }

    #[test]
    fn rejects_children_on_void_elements() {
        assert!(validate_element("img", 0).is_ok());
        assert!(matches!(
            validate_element("input", 1),
            Err(DomPolicyError::VoidElementHasChildren(_))
        ));
    }

    #[test]
    fn rejects_dangerous_dom_properties_and_validates_url_properties() {
        assert!(matches!(
            validate_property("onclick", &PropertyValue::Text("alert(1)".into())),
            Err(DomPolicyError::UnsafeProperty(_))
        ));
        assert!(matches!(
            validate_property("style", &PropertyValue::Text("display:none".into())),
            Err(DomPolicyError::UnsafeProperty(_))
        ));
        assert!(matches!(
            validate_property("href", &PropertyValue::Text("javascript:alert(1)".into())),
            Err(DomPolicyError::UnsafeUrl(_))
        ));
        assert!(validate_property("href", &PropertyValue::Text("/safe/path".into())).is_ok());
        assert!(matches!(
            validate_property("src", &PropertyValue::Boolean(true)),
            Err(DomPolicyError::UnsafeProperty(_))
        ));
    }

    #[test]
    fn rejects_inline_style_and_active_url_schemes() {
        assert!(matches!(
            validate_attribute("style", "color:red"),
            Err(DomPolicyError::UnsafeAttribute(_))
        ));
        assert!(matches!(
            validate_attribute("href", "javascript:alert(1)"),
            Err(DomPolicyError::UnsafeUrl(_))
        ));
        assert!(matches!(
            validate_attribute("src", "java\nscript:alert(1)"),
            Err(DomPolicyError::UnsafeUrl(_))
        ));
        assert!(validate_attribute("href", "/docs/start").is_ok());
        assert!(validate_attribute("href", "#section").is_ok());
        assert!(validate_attribute("href", "https://example.com").is_ok());
        assert!(validate_attribute("href", "mailto:hello@example.com").is_ok());
        assert!(matches!(
            validate_attribute("href", "file:///etc/passwd"),
            Err(DomPolicyError::UnsafeUrl(_))
        ));
        assert!(matches!(
            validate_attribute("href", "custom-scheme:payload"),
            Err(DomPolicyError::UnsafeUrl(_))
        ));
        assert!(matches!(
            validate_attribute("srcset", "a.png 1x, b.png 2x"),
            Err(DomPolicyError::UnsafeAttribute(_))
        ));
        assert!(validate_attribute("aria-label", "Search").is_ok());
    }
}
