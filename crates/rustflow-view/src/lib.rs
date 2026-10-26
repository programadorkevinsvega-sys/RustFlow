//! Target-independent structural view nodes for RustFlow.
//!
//! This crate models a tree of text, element, and fragment nodes. It does not
//! render HTML or DOM nodes, execute components, or connect views to reactivity.
//! Its public API is a pre-1.0 prototype and may evolve as lifecycle and
//! rendering contracts are specified.

/// A structural view tree, independent of any rendering target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// Text content, stored verbatim.
    Text(String),
    /// An element name and its ordered children.
    Element(Element),
    /// An explicit grouping of ordered children.
    Fragment(Fragment),
}

impl View {
    /// Create a text node without interpreting or transforming its content.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }

    /// Create an element node with a name and ordered children.
    ///
    /// The element name is stored as supplied; validation is not part of this
    /// initial structural model. A future renderer must define and enforce its
    /// supported element-name contract.
    pub fn element(
        tag: impl Into<String>,
        children: impl IntoIterator<Item = View>,
    ) -> Self {
        Self::Element(Element::new(tag, children))
    }

    /// Create a fragment that preserves its child grouping.
    ///
    /// Fragments are not flattened automatically.
    pub fn fragment(children: impl IntoIterator<Item = View>) -> Self {
        Self::Fragment(Fragment::new(children))
    }
}

/// A structural element node.
///
/// Attribute, property, event, and dynamic-child support are intentionally not
/// included until their contracts are specified in the roadmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    tag: String,
    children: Vec<View>,
}

impl Element {
    /// Create an element with a name and an ordered collection of children.
    pub fn new(tag: impl Into<String>, children: impl IntoIterator<Item = View>) -> Self {
        Self {
            tag: tag.into(),
            children: children.into_iter().collect(),
        }
    }

    /// Return the element name exactly as it was provided.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Return the element's children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
}

/// An explicit group of view nodes that adds no element of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    children: Vec<View>,
}

impl Fragment {
    /// Create a fragment while preserving child order and grouping.
    pub fn new(children: impl IntoIterator<Item = View>) -> Self {
        Self {
            children: children.into_iter().collect(),
        }
    }

    /// Return the fragment's children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
}

#[cfg(test)]
mod tests {
    use super::View;

    #[test]
    fn text_content_is_preserved_verbatim() {
        let content = "<b>not markup</b>\n";
        assert_eq!(View::text(content), View::Text(content.to_owned()));
    }

    #[test]
    fn elements_preserve_their_name_and_child_order() {
        let first = View::element("li", [View::text("first")]);
        let second = View::element("li", [View::text("second")]);
        let tree = View::element("ul", [first.clone(), second.clone()]);

        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };

        assert_eq!(element.tag(), "ul");
        assert_eq!(element.children(), &[first, second]);
    }

    #[test]
    fn fragments_preserve_grouping_and_order() {
        let group = View::fragment([View::text("first"), View::text("second")]);
        let tree = View::element("div", [group.clone(), View::text("third")]);

        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };

        assert_eq!(element.children(), &[group, View::text("third")]);
    }

    #[test]
    fn empty_elements_and_fragments_are_supported() {
        let element = View::element("hr", std::iter::empty::<View>());
        let fragment = View::fragment(std::iter::empty::<View>());

        let View::Element(element) = element else {
            panic!("element constructor must create an element node");
        };
        let View::Fragment(fragment) = fragment else {
            panic!("fragment constructor must create a fragment node");
        };

        assert!(element.children().is_empty());
        assert!(fragment.children().is_empty());
    }
}
