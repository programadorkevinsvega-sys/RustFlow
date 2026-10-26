//! Target-independent structural view nodes for RustFlow.
//!
//! This model is data, not rendered HTML. A renderer must escape text and
//! attribute values, validate URL-bearing values, and implement the documented
//! lifecycle contract before emitting markup or mutating a DOM.

use std::{
    collections::HashSet,
    fmt,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Errors found while building a structural view tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    InvalidElementName(String),
    InvalidAttributeName(String),
    DangerousAttributeName(String),
    InvalidPropertyName(String),
    DangerousPropertyName(String),
    InvalidEventName(String),
    InvalidComponentName(String),
    DuplicateAttribute(String),
    DuplicateProperty(String),
    DuplicateEvent(String),
    DuplicateKey(Key),
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidElementName(name) => write!(f, "invalid element name: {name}"),
            Self::InvalidAttributeName(name) => write!(f, "invalid attribute name: {name}"),
            Self::DangerousAttributeName(name) => write!(
                f,
                "attribute {name} is not accepted; event handlers and raw HTML sinks are not attributes"
            ),
            Self::InvalidPropertyName(name) => write!(f, "invalid property name: {name}"),
            Self::DangerousPropertyName(name) => {
                write!(f, "property {name} is not accepted by the safe view model")
            }
            Self::InvalidEventName(name) => write!(f, "invalid event name: {name}"),
            Self::InvalidComponentName(name) => write!(f, "invalid component name: {name}"),
            Self::DuplicateAttribute(name) => write!(f, "duplicate attribute: {name}"),
            Self::DuplicateProperty(name) => write!(f, "duplicate property: {name}"),
            Self::DuplicateEvent(name) => write!(f, "duplicate event handler: {name}"),
            Self::DuplicateKey(key) => write!(f, "duplicate key in list: {key:?}"),
        }
    }
}
impl std::error::Error for ViewError {}

/// A stable identity for a keyed child.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    String(String),
    Integer(i64),
}
impl From<String> for Key {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<&str> for Key {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
impl From<i64> for Key {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<i32> for Key {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}

/// A typed element name. Names are syntax-validated, not allowlisted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementName(String);
impl ElementName {
    /// Validate a plain HTML/custom-element-style name.
    ///
    /// This rejects whitespace, markup delimiters, quotes, and attribute-like
    /// injection. It does not decide which tags are appropriate for an app.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic());
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-');
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidElementName(name));
        }
        Ok(Self(name))
    }
    /// Return the name exactly as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A typed HTML attribute name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributeName(String);
impl AttributeName {
    /// Validate an attribute name and reject inline event/raw-HTML sinks.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == ':');
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'));
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidAttributeName(name));
        }
        let lowercase = name.to_ascii_lowercase();
        if (lowercase.starts_with("on") && lowercase.len() > 2)
            || matches!(lowercase.as_str(), "srcdoc" | "innerhtml" | "outerhtml")
        {
            return Err(ViewError::DangerousAttributeName(name));
        }
        Ok(Self(name))
    }
    /// Return the attribute name exactly as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A typed DOM property name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyName(String);
impl PropertyName {
    /// Validate a property name and reject direct raw-HTML sinks.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$');
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidPropertyName(name));
        }
        let lowercase = name.to_ascii_lowercase();
        if matches!(lowercase.as_str(), "innerhtml" | "outerhtml" | "srcdoc") {
            return Err(ViewError::DangerousPropertyName(name));
        }
        Ok(Self(name))
    }
    /// Return the property name exactly as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A lowercase event name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventName(String);
impl EventName {
    /// Validate a browser-style event name, such as click or key-down.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
            && name
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        if !valid {
            return Err(ViewError::InvalidEventName(name));
        }
        Ok(Self(name))
    }
    /// Return the event name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A component type name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentName(String);
impl ComponentName {
    /// Validate a Rust-style component identifier.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidComponentName(name));
        }
        Ok(Self(name))
    }
    /// Return the component name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A declarative attribute value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// A textual attribute value.
    Text(String),
    /// A boolean attribute: false should be omitted by a renderer, true emitted as present.
    Boolean(bool),
}
impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for AttributeValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for AttributeValue {
    fn from(value: i32) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<i64> for AttributeValue {
    fn from(value: i64) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<f64> for AttributeValue {
    fn from(value: f64) -> Self {
        Self::Text(value.to_string())
    }
}

/// A DOM property value, kept separate from HTML attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// A textual property.
    Text(String),
    /// A boolean property.
    Boolean(bool),
    /// An integer property.
    Integer(i64),
    /// A floating-point property.
    Float(f64),
}
impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for PropertyValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for PropertyValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}
impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<f64> for PropertyValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// A named attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    name: AttributeName,
    value: AttributeValue,
}
impl Attribute {
    /// Create a validated attribute.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: AttributeName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the attribute name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the attribute value.
    pub fn value(&self) -> &AttributeValue {
        &self.value
    }
}

/// A named DOM property or component prop.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    name: PropertyName,
    value: PropertyValue,
}
impl Property {
    /// Create a validated property.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: PropertyName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the property name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the property value.
    pub fn value(&self) -> &PropertyValue {
        &self.value
    }
}

/// The subset of event data shared by platform adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPayload {
    /// Current textual value, when the target exposes one.
    pub value: Option<String>,
    /// Current checked state, when the target exposes one.
    pub checked: Option<bool>,
}

/// An owned event callback.
#[derive(Clone)]
pub struct EventHandler {
    id: u64,
    callback: Rc<dyn Fn(EventPayload)>,
}
impl EventHandler {
    /// Create an event callback with a stable identity.
    pub fn new(callback: impl Fn(EventPayload) + 'static) -> Self {
        Self {
            id: next_id(),
            callback: Rc::new(callback),
        }
    }
    /// Invoke the callback with normalized event data.
    pub fn invoke(&self, payload: EventPayload) {
        (self.callback)(payload);
    }
    /// Return the handler identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventHandler")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for EventHandler {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// An event name and its callback.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    name: EventName,
    handler: EventHandler,
}
impl EventBinding {
    /// Return the event name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return a clone of the callback handle.
    pub fn handler(&self) -> EventHandler {
        self.handler.clone()
    }
}

/// A structural view tree, independent of any rendering target.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Text content, stored verbatim.
    Text(String),
    /// An element with attributes, properties, events, and ordered children.
    Element(Element),
    /// An explicit grouping of ordered children.
    Fragment(Fragment),
    /// A component descriptor resolved by a component host.
    Component(ComponentNode),
    /// A deferred child evaluator.
    Dynamic(DynamicChild),
    /// An ordered list with unique stable keys.
    KeyedList(KeyedList),
}
impl View {
    /// Create a text node without interpreting or transforming its content.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }
    /// Create an element with a validated name and ordered children.
    pub fn element(
        tag: impl Into<String>,
        children: impl IntoIterator<Item = View>,
    ) -> Result<Self, ViewError> {
        Ok(Element::new(tag)?.with_children(children).build())
    }
    /// Create a fragment preserving child grouping.
    pub fn fragment(children: impl IntoIterator<Item = View>) -> Self {
        Self::Fragment(Fragment::new(children))
    }
    /// Create a component descriptor. Executing it requires a component host.
    pub fn component(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        Ok(Self::Component(ComponentNode::new(
            name, properties, children, key,
        )?))
    }
    /// Defer evaluation of a child until an adapter requests it.
    pub fn dynamic(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self::Dynamic(DynamicChild::new(evaluator))
    }
    /// Create a keyed list, rejecting duplicate keys.
    pub fn keyed_list(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        Ok(Self::KeyedList(KeyedList::new(children)?))
    }
}

/// Convert a supported value into a view tree for declarative syntax.
pub trait IntoView {
    /// Convert the value into a view or return a construction error.
    fn into_view(self) -> Result<View, ViewError>;
}
impl IntoView for View {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(self)
    }
}
impl IntoView for String {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}
impl IntoView for &str {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}
impl IntoView for bool {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for i32 {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for i64 {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for f64 {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for char {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}

/// A validated element node.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    tag: ElementName,
    attributes: Vec<Attribute>,
    properties: Vec<Property>,
    events: Vec<EventBinding>,
    children: Vec<View>,
    key: Option<Key>,
}
impl Element {
    /// Create an element after validating its name.
    pub fn new(tag: impl Into<String>) -> Result<Self, ViewError> {
        Ok(Self {
            tag: ElementName::new(tag)?,
            attributes: Vec::new(),
            properties: Vec::new(),
            events: Vec::new(),
            children: Vec::new(),
            key: None,
        })
    }
    /// Set ordered children for this element.
    pub fn with_children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
    /// Add a validated attribute. Duplicate names are rejected.
    pub fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        let attribute = Attribute::new(name, value)?;
        if self
            .attributes
            .iter()
            .any(|existing| existing.name == attribute.name)
        {
            return Err(ViewError::DuplicateAttribute(attribute.name().to_owned()));
        }
        self.attributes.push(attribute);
        Ok(self)
    }
    /// Add a boolean HTML attribute.
    pub fn with_boolean_attribute(
        self,
        name: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ViewError> {
        self.with_attribute(name, AttributeValue::Boolean(enabled))
    }
    /// Add a DOM property. Duplicate names are rejected.
    pub fn with_property(
        mut self,
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        let property = Property::new(name, value)?;
        if self
            .properties
            .iter()
            .any(|existing| existing.name == property.name)
        {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
        self.properties.push(property);
        Ok(self)
    }
    /// Add an event callback. Events are not encoded as inline HTML attributes.
    pub fn with_event(
        mut self,
        name: impl Into<String>,
        callback: impl Fn(EventPayload) + 'static,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler: EventHandler::new(callback),
        });
        Ok(self)
    }
    /// Add a preconstructed event handler.
    pub fn with_event_handler(
        mut self,
        name: impl Into<String>,
        handler: EventHandler,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler,
        });
        Ok(self)
    }
    /// Attach a stable identity to this element.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    /// Return the validated element name.
    pub fn tag(&self) -> &str {
        self.tag.as_str()
    }
    /// Return attributes in declaration order.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// Return DOM properties in declaration order.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return event bindings in declaration order.
    pub fn events(&self) -> &[EventBinding] {
        &self.events
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
    /// Finish the builder and produce a view node.
    pub fn build(self) -> View {
        View::Element(self)
    }
}

/// An explicit group of view nodes that adds no element of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    children: Vec<View>,
}
impl Fragment {
    /// Create a fragment preserving child order and grouping.
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

/// A component descriptor passed to the component host.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentNode {
    name: ComponentName,
    properties: Vec<Property>,
    children: Vec<View>,
    key: Option<Key>,
}
impl ComponentNode {
    /// Create a component descriptor and reject duplicate properties.
    pub fn new(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        let name = ComponentName::new(name)?;
        let properties = properties.into_iter().collect::<Vec<_>>();
        ensure_unique_properties(&properties)?;
        Ok(Self {
            name,
            properties,
            children: children.into_iter().collect(),
            key,
        })
    }
    /// Return the component type name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return component properties.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
}
fn ensure_unique_properties(properties: &[Property]) -> Result<(), ViewError> {
    let mut names = HashSet::new();
    for property in properties {
        if !names.insert(property.name().to_owned()) {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
    }
    Ok(())
}

/// A child whose view tree is evaluated on demand.
#[derive(Clone)]
pub struct DynamicChild {
    id: u64,
    evaluator: Rc<dyn Fn() -> Result<View, ViewError>>,
}
impl DynamicChild {
    /// Create a deferred evaluator.
    pub fn new(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self {
            id: next_id(),
            evaluator: Rc::new(evaluator),
        }
    }
    /// Evaluate the child. A reactive renderer should call it within its owned effect.
    pub fn evaluate(&self) -> Result<View, ViewError> {
        (self.evaluator)()
    }
    /// Return this evaluator's identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for DynamicChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicChild")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for DynamicChild {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A keyed child, useful for identity-preserving list updates.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedChild {
    key: Key,
    view: View,
}
impl KeyedChild {
    /// Create a keyed child.
    pub fn new(key: impl Into<Key>, view: View) -> Self {
        Self {
            key: key.into(),
            view,
        }
    }
    /// Return this child's key.
    pub fn key(&self) -> &Key {
        &self.key
    }
    /// Return this child's view.
    pub fn view(&self) -> &View {
        &self.view
    }
}

/// An ordered keyed collection. Duplicate identities are rejected.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedList {
    children: Vec<KeyedChild>,
}
impl KeyedList {
    /// Create a keyed list while preserving order and checking key uniqueness.
    pub fn new(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        let children = children.into_iter().collect::<Vec<_>>();
        let mut keys = HashSet::new();
        for child in &children {
            if !keys.insert(child.key.clone()) {
                return Err(ViewError::DuplicateKey(child.key.clone()));
            }
        }
        Ok(Self { children })
    }
    /// Return keyed items in declaration order.
    pub fn children(&self) -> &[KeyedChild] {
        &self.children
    }
}

/// Contract implemented by a platform-specific renderer.
///
/// Implementations must mount each node once, update without leaving a
/// partially-mutated tree on recoverable errors, and release event handlers and
/// owned resources on unmount. Text and attribute values must be escaped;
/// URL-bearing values must be checked by a platform-appropriate policy.
pub trait Renderer {
    /// Handle representing the mounted root.
    type Mounted;
    /// Rendering failure.
    type Error;
    /// Construct and mount a view tree.
    fn mount(&mut self, view: &View) -> Result<Self::Mounted, Self::Error>;
    /// Reconcile a mounted root with a new view tree.
    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        previous: &View,
        next: &View,
    ) -> Result<(), Self::Error>;
    /// Unmount a tree and release listeners and resources it owns.
    fn unmount(&mut self, mounted: Self::Mounted) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{
        AttributeValue, EventPayload, Key, KeyedChild, KeyedList, Property, PropertyValue, View,
        ViewError,
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn text_content_is_preserved_verbatim() {
        let content = "<b>not markup</b>\n";
        assert_eq!(View::text(content), View::Text(content.to_owned()));
    }
    #[test]
    fn elements_preserve_their_name_and_child_order() {
        let first = View::element("li", [View::text("first")]).expect("valid element");
        let second = View::element("li", [View::text("second")]).expect("valid element");
        let tree = View::element("ul", [first.clone(), second.clone()]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.tag(), "ul");
        assert_eq!(element.children(), &[first, second]);
    }
    #[test]
    fn fragments_preserve_grouping_and_order() {
        let group = View::fragment([View::text("first"), View::text("second")]);
        let tree =
            View::element("div", [group.clone(), View::text("third")]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.children(), &[group, View::text("third")]);
    }
    #[test]
    fn empty_elements_and_fragments_are_supported() {
        let element = View::element("hr", std::iter::empty::<View>()).expect("valid element");
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
    #[test]
    fn malformed_element_names_are_rejected() {
        assert!(matches!(
            View::element("div onclick=evil", []),
            Err(ViewError::InvalidElementName(_))
        ));
        assert!(matches!(
            View::element("img/>", []),
            Err(ViewError::InvalidElementName(_))
        ));
    }
    #[test]
    fn inline_event_and_raw_html_attributes_are_rejected() {
        assert!(matches!(
            super::Element::new("button")
                .expect("valid name")
                .with_attribute("onclick", "alert(1)"),
            Err(ViewError::DangerousAttributeName(_))
        ));
        assert!(matches!(
            super::Element::new("iframe")
                .expect("valid name")
                .with_attribute("srcdoc", "<script>"),
            Err(ViewError::DangerousAttributeName(_))
        ));
    }
    #[test]
    fn attributes_properties_and_boolean_attributes_remain_distinct() {
        let node = super::Element::new("input")
            .expect("valid element")
            .with_attribute("aria-label", "Search")
            .expect("valid attribute")
            .with_boolean_attribute("disabled", true)
            .expect("valid boolean attribute")
            .with_property("value", "query")
            .expect("valid property")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        assert_eq!(element.attributes().len(), 2);
        assert_eq!(
            element.attributes()[1].value(),
            &AttributeValue::Boolean(true)
        );
        assert_eq!(element.properties().len(), 1);
        assert_eq!(
            element.properties()[0].value(),
            &PropertyValue::Text("query".into())
        );
    }
    #[test]
    fn duplicate_attributes_and_properties_are_rejected() {
        let duplicate_attribute = super::Element::new("div")
            .expect("valid element")
            .with_attribute("class", "one")
            .expect("first attribute")
            .with_attribute("class", "two");
        assert!(matches!(
            duplicate_attribute,
            Err(ViewError::DuplicateAttribute(_))
        ));
        let duplicate_property = super::Element::new("input")
            .expect("valid element")
            .with_property("value", "one")
            .expect("first property")
            .with_property("value", "two");
        assert!(matches!(
            duplicate_property,
            Err(ViewError::DuplicateProperty(_))
        ));
    }
    #[test]
    fn event_handler_receives_payload() {
        let observed = Rc::new(Cell::new(false));
        let callback_observed = observed.clone();
        let node = super::Element::new("button")
            .expect("valid element")
            .with_event("click", move |payload| {
                callback_observed.set(payload.checked == Some(true))
            })
            .expect("valid event")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        element.events()[0].handler().invoke(EventPayload {
            value: None,
            checked: Some(true),
        });
        assert!(observed.get());
    }
    #[test]
    fn dynamic_children_are_evaluated_on_demand() {
        let calls = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let dynamic = View::dynamic(move || {
            callback_calls.set(callback_calls.get() + 1);
            Ok(View::text("current"))
        });
        let View::Dynamic(child) = dynamic else {
            panic!("dynamic constructor must create a dynamic node");
        };
        assert_eq!(calls.get(), 0);
        assert_eq!(child.evaluate().expect("evaluation"), View::text("current"));
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn keyed_list_preserves_order_and_rejects_duplicate_keys() {
        let list = KeyedList::new([
            KeyedChild::new("first", View::text("a")),
            KeyedChild::new("second", View::text("b")),
        ])
        .expect("unique keys");
        assert_eq!(list.children()[0].key(), &Key::from("first"));
        assert_eq!(list.children()[1].view(), &View::text("b"));
        let duplicate = KeyedList::new([
            KeyedChild::new("same", View::text("a")),
            KeyedChild::new("same", View::text("b")),
        ]);
        assert!(matches!(duplicate, Err(ViewError::DuplicateKey(_))));
    }
    #[test]
    fn component_descriptors_validate_names_and_duplicate_properties() {
        let properties = [
            Property::new("title", "Hello").expect("valid property"),
            Property::new("disabled", true).expect("valid property"),
        ];
        let component = View::component(
            "Greeting",
            properties,
            [View::text("body")],
            Some(Key::from("greeting")),
        )
        .expect("valid component");
        let View::Component(component) = component else {
            panic!("component constructor must create a component descriptor");
        };
        assert_eq!(component.name(), "Greeting");
        assert_eq!(component.properties().len(), 2);
        assert_eq!(component.children(), &[View::text("body")]);
        assert!(matches!(
            View::component(
                "Greeting",
                [
                    Property::new("title", "one").expect("valid property"),
                    Property::new("title", "two").expect("valid property")
                ],
                [],
                None
            ),
            Err(ViewError::DuplicateProperty(_))
        ));
        assert!(matches!(
            View::component("bad name", [], [], None),
            Err(ViewError::InvalidComponentName(_))
        ));
    }
}
 | '-'));
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidPropertyName(name));
        }
        let lowercase = name.to_ascii_lowercase();
        if matches!(lowercase.as_str(), "innerhtml" | "outerhtml" | "srcdoc") {
            return Err(ViewError::DangerousPropertyName(name));
        }
        Ok(Self(name))
    }
    /// Return the property name exactly as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A lowercase event name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventName(String);
impl EventName {
    /// Validate a browser-style event name, such as click or key-down.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
            && name
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        if !valid {
            return Err(ViewError::InvalidEventName(name));
        }
        Ok(Self(name))
    }
    /// Return the event name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A component type name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentName(String);
impl ComponentName {
    /// Validate a Rust-style component identifier.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidComponentName(name));
        }
        Ok(Self(name))
    }
    /// Return the component name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A declarative attribute value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// A textual attribute value.
    Text(String),
    /// A boolean attribute: false should be omitted by a renderer, true emitted as present.
    Boolean(bool),
}
impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for AttributeValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for AttributeValue {
    fn from(value: i32) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<i64> for AttributeValue {
    fn from(value: i64) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<f64> for AttributeValue {
    fn from(value: f64) -> Self {
        Self::Text(value.to_string())
    }
}

/// A DOM property value, kept separate from HTML attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// A textual property.
    Text(String),
    /// A boolean property.
    Boolean(bool),
    /// An integer property.
    Integer(i64),
    /// A floating-point property.
    Float(f64),
}
impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for PropertyValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for PropertyValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}
impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<f64> for PropertyValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// A named attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    name: AttributeName,
    value: AttributeValue,
}
impl Attribute {
    /// Create a validated attribute.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: AttributeName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the attribute name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the attribute value.
    pub fn value(&self) -> &AttributeValue {
        &self.value
    }
}

/// A named DOM property or component prop.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    name: PropertyName,
    value: PropertyValue,
}
impl Property {
    /// Create a validated property.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: PropertyName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the property name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the property value.
    pub fn value(&self) -> &PropertyValue {
        &self.value
    }
}

/// The subset of event data shared by platform adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPayload {
    /// Current textual value, when the target exposes one.
    pub value: Option<String>,
    /// Current checked state, when the target exposes one.
    pub checked: Option<bool>,
}

/// An owned event callback.
#[derive(Clone)]
pub struct EventHandler {
    id: u64,
    callback: Rc<dyn Fn(EventPayload)>,
}
impl EventHandler {
    /// Create an event callback with a stable identity.
    pub fn new(callback: impl Fn(EventPayload) + 'static) -> Self {
        Self {
            id: next_id(),
            callback: Rc::new(callback),
        }
    }
    /// Invoke the callback with normalized event data.
    pub fn invoke(&self, payload: EventPayload) {
        (self.callback)(payload);
    }
    /// Return the handler identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventHandler")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for EventHandler {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// An event name and its callback.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    name: EventName,
    handler: EventHandler,
}
impl EventBinding {
    /// Return the event name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return a clone of the callback handle.
    pub fn handler(&self) -> EventHandler {
        self.handler.clone()
    }
}

/// A structural view tree, independent of any rendering target.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Text content, stored verbatim.
    Text(String),
    /// An element with attributes, properties, events, and ordered children.
    Element(Element),
    /// An explicit grouping of ordered children.
    Fragment(Fragment),
    /// A component descriptor resolved by a component host.
    Component(ComponentNode),
    /// A deferred child evaluator.
    Dynamic(DynamicChild),
    /// An ordered list with unique stable keys.
    KeyedList(KeyedList),
}
impl View {
    /// Create a text node without interpreting or transforming its content.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }
    /// Create an element with a validated name and ordered children.
    pub fn element(
        tag: impl Into<String>,
        children: impl IntoIterator<Item = View>,
    ) -> Result<Self, ViewError> {
        Ok(Element::new(tag)?.with_children(children).build())
    }
    /// Create a fragment preserving child grouping.
    pub fn fragment(children: impl IntoIterator<Item = View>) -> Self {
        Self::Fragment(Fragment::new(children))
    }
    /// Create a component descriptor. Executing it requires a component host.
    pub fn component(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        Ok(Self::Component(ComponentNode::new(
            name, properties, children, key,
        )?))
    }
    /// Defer evaluation of a child until an adapter requests it.
    pub fn dynamic(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self::Dynamic(DynamicChild::new(evaluator))
    }
    /// Create a keyed list, rejecting duplicate keys.
    pub fn keyed_list(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        Ok(Self::KeyedList(KeyedList::new(children)?))
    }
}

/// Convert a supported value into a view tree for declarative syntax.
pub trait IntoView {
    /// Convert the value into a view or return a construction error.
    fn into_view(self) -> Result<View, ViewError>;
}
impl IntoView for View {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(self)
    }
}
impl IntoView for String {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}
impl IntoView for &str {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}

/// A validated element node.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    tag: ElementName,
    attributes: Vec<Attribute>,
    properties: Vec<Property>,
    events: Vec<EventBinding>,
    children: Vec<View>,
    key: Option<Key>,
}
impl Element {
    /// Create an element after validating its name.
    pub fn new(tag: impl Into<String>) -> Result<Self, ViewError> {
        Ok(Self {
            tag: ElementName::new(tag)?,
            attributes: Vec::new(),
            properties: Vec::new(),
            events: Vec::new(),
            children: Vec::new(),
            key: None,
        })
    }
    /// Set ordered children for this element.
    pub fn with_children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
    /// Add a validated attribute. Duplicate names are rejected.
    pub fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        let attribute = Attribute::new(name, value)?;
        if self
            .attributes
            .iter()
            .any(|existing| existing.name == attribute.name)
        {
            return Err(ViewError::DuplicateAttribute(attribute.name().to_owned()));
        }
        self.attributes.push(attribute);
        Ok(self)
    }
    /// Add a boolean HTML attribute.
    pub fn with_boolean_attribute(
        self,
        name: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ViewError> {
        self.with_attribute(name, AttributeValue::Boolean(enabled))
    }
    /// Add a DOM property. Duplicate names are rejected.
    pub fn with_property(
        mut self,
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        let property = Property::new(name, value)?;
        if self
            .properties
            .iter()
            .any(|existing| existing.name == property.name)
        {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
        self.properties.push(property);
        Ok(self)
    }
    /// Add an event callback. Events are not encoded as inline HTML attributes.
    pub fn with_event(
        mut self,
        name: impl Into<String>,
        callback: impl Fn(EventPayload) + 'static,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler: EventHandler::new(callback),
        });
        Ok(self)
    }
    /// Add a preconstructed event handler.
    pub fn with_event_handler(
        mut self,
        name: impl Into<String>,
        handler: EventHandler,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler,
        });
        Ok(self)
    }
    /// Attach a stable identity to this element.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    /// Return the validated element name.
    pub fn tag(&self) -> &str {
        self.tag.as_str()
    }
    /// Return attributes in declaration order.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// Return DOM properties in declaration order.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return event bindings in declaration order.
    pub fn events(&self) -> &[EventBinding] {
        &self.events
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
    /// Finish the builder and produce a view node.
    pub fn build(self) -> View {
        View::Element(self)
    }
}

/// An explicit group of view nodes that adds no element of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    children: Vec<View>,
}
impl Fragment {
    /// Create a fragment preserving child order and grouping.
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

/// A component descriptor passed to the component host.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentNode {
    name: ComponentName,
    properties: Vec<Property>,
    children: Vec<View>,
    key: Option<Key>,
}
impl ComponentNode {
    /// Create a component descriptor and reject duplicate properties.
    pub fn new(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        let name = ComponentName::new(name)?;
        let properties = properties.into_iter().collect::<Vec<_>>();
        ensure_unique_properties(&properties)?;
        Ok(Self {
            name,
            properties,
            children: children.into_iter().collect(),
            key,
        })
    }
    /// Return the component type name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return component properties.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
}
fn ensure_unique_properties(properties: &[Property]) -> Result<(), ViewError> {
    let mut names = HashSet::new();
    for property in properties {
        if !names.insert(property.name().to_owned()) {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
    }
    Ok(())
}

/// A child whose view tree is evaluated on demand.
#[derive(Clone)]
pub struct DynamicChild {
    id: u64,
    evaluator: Rc<dyn Fn() -> Result<View, ViewError>>,
}
impl DynamicChild {
    /// Create a deferred evaluator.
    pub fn new(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self {
            id: next_id(),
            evaluator: Rc::new(evaluator),
        }
    }
    /// Evaluate the child. A reactive renderer should call it within its owned effect.
    pub fn evaluate(&self) -> Result<View, ViewError> {
        (self.evaluator)()
    }
    /// Return this evaluator's identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for DynamicChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicChild")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for DynamicChild {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A keyed child, useful for identity-preserving list updates.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedChild {
    key: Key,
    view: View,
}
impl KeyedChild {
    /// Create a keyed child.
    pub fn new(key: impl Into<Key>, view: View) -> Self {
        Self {
            key: key.into(),
            view,
        }
    }
    /// Return this child's key.
    pub fn key(&self) -> &Key {
        &self.key
    }
    /// Return this child's view.
    pub fn view(&self) -> &View {
        &self.view
    }
}

/// An ordered keyed collection. Duplicate identities are rejected.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedList {
    children: Vec<KeyedChild>,
}
impl KeyedList {
    /// Create a keyed list while preserving order and checking key uniqueness.
    pub fn new(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        let children = children.into_iter().collect::<Vec<_>>();
        let mut keys = HashSet::new();
        for child in &children {
            if !keys.insert(child.key.clone()) {
                return Err(ViewError::DuplicateKey(child.key.clone()));
            }
        }
        Ok(Self { children })
    }
    /// Return keyed items in declaration order.
    pub fn children(&self) -> &[KeyedChild] {
        &self.children
    }
}

/// Contract implemented by a platform-specific renderer.
///
/// Implementations must mount each node once, update without leaving a
/// partially-mutated tree on recoverable errors, and release event handlers and
/// owned resources on unmount. Text and attribute values must be escaped;
/// URL-bearing values must be checked by a platform-appropriate policy.
pub trait Renderer {
    /// Handle representing the mounted root.
    type Mounted;
    /// Rendering failure.
    type Error;
    /// Construct and mount a view tree.
    fn mount(&mut self, view: &View) -> Result<Self::Mounted, Self::Error>;
    /// Reconcile a mounted root with a new view tree.
    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        previous: &View,
        next: &View,
    ) -> Result<(), Self::Error>;
    /// Unmount a tree and release listeners and resources it owns.
    fn unmount(&mut self, mounted: Self::Mounted) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{
        AttributeValue, EventPayload, Key, KeyedChild, KeyedList, Property, PropertyValue, View,
        ViewError,
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn text_content_is_preserved_verbatim() {
        let content = "<b>not markup</b>\n";
        assert_eq!(View::text(content), View::Text(content.to_owned()));
    }
    #[test]
    fn elements_preserve_their_name_and_child_order() {
        let first = View::element("li", [View::text("first")]).expect("valid element");
        let second = View::element("li", [View::text("second")]).expect("valid element");
        let tree = View::element("ul", [first.clone(), second.clone()]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.tag(), "ul");
        assert_eq!(element.children(), &[first, second]);
    }
    #[test]
    fn fragments_preserve_grouping_and_order() {
        let group = View::fragment([View::text("first"), View::text("second")]);
        let tree =
            View::element("div", [group.clone(), View::text("third")]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.children(), &[group, View::text("third")]);
    }
    #[test]
    fn empty_elements_and_fragments_are_supported() {
        let element = View::element("hr", std::iter::empty::<View>()).expect("valid element");
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
    #[test]
    fn malformed_element_names_are_rejected() {
        assert!(matches!(
            View::element("div onclick=evil", []),
            Err(ViewError::InvalidElementName(_))
        ));
        assert!(matches!(
            View::element("img/>", []),
            Err(ViewError::InvalidElementName(_))
        ));
    }
    #[test]
    fn inline_event_and_raw_html_attributes_are_rejected() {
        assert!(matches!(
            super::Element::new("button")
                .expect("valid name")
                .with_attribute("onclick", "alert(1)"),
            Err(ViewError::DangerousAttributeName(_))
        ));
        assert!(matches!(
            super::Element::new("iframe")
                .expect("valid name")
                .with_attribute("srcdoc", "<script>"),
            Err(ViewError::DangerousAttributeName(_))
        ));
    }
    #[test]
    fn attributes_properties_and_boolean_attributes_remain_distinct() {
        let node = super::Element::new("input")
            .expect("valid element")
            .with_attribute("aria-label", "Search")
            .expect("valid attribute")
            .with_boolean_attribute("disabled", true)
            .expect("valid boolean attribute")
            .with_property("value", "query")
            .expect("valid property")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        assert_eq!(element.attributes().len(), 2);
        assert_eq!(
            element.attributes()[1].value(),
            &AttributeValue::Boolean(true)
        );
        assert_eq!(element.properties().len(), 1);
        assert_eq!(
            element.properties()[0].value(),
            &PropertyValue::Text("query".into())
        );
    }
    #[test]
    fn duplicate_attributes_and_properties_are_rejected() {
        let duplicate_attribute = super::Element::new("div")
            .expect("valid element")
            .with_attribute("class", "one")
            .expect("first attribute")
            .with_attribute("class", "two");
        assert!(matches!(
            duplicate_attribute,
            Err(ViewError::DuplicateAttribute(_))
        ));
        let duplicate_property = super::Element::new("input")
            .expect("valid element")
            .with_property("value", "one")
            .expect("first property")
            .with_property("value", "two");
        assert!(matches!(
            duplicate_property,
            Err(ViewError::DuplicateProperty(_))
        ));
    }
    #[test]
    fn event_handler_receives_payload() {
        let observed = Rc::new(Cell::new(false));
        let callback_observed = observed.clone();
        let node = super::Element::new("button")
            .expect("valid element")
            .with_event("click", move |payload| {
                callback_observed.set(payload.checked == Some(true))
            })
            .expect("valid event")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        element.events()[0].handler().invoke(EventPayload {
            value: None,
            checked: Some(true),
        });
        assert!(observed.get());
    }
    #[test]
    fn dynamic_children_are_evaluated_on_demand() {
        let calls = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let dynamic = View::dynamic(move || {
            callback_calls.set(callback_calls.get() + 1);
            Ok(View::text("current"))
        });
        let View::Dynamic(child) = dynamic else {
            panic!("dynamic constructor must create a dynamic node");
        };
        assert_eq!(calls.get(), 0);
        assert_eq!(child.evaluate().expect("evaluation"), View::text("current"));
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn keyed_list_preserves_order_and_rejects_duplicate_keys() {
        let list = KeyedList::new([
            KeyedChild::new("first", View::text("a")),
            KeyedChild::new("second", View::text("b")),
        ])
        .expect("unique keys");
        assert_eq!(list.children()[0].key(), &Key::from("first"));
        assert_eq!(list.children()[1].view(), &View::text("b"));
        let duplicate = KeyedList::new([
            KeyedChild::new("same", View::text("a")),
            KeyedChild::new("same", View::text("b")),
        ]);
        assert!(matches!(duplicate, Err(ViewError::DuplicateKey(_))));
    }
    #[test]
    fn component_descriptors_validate_names_and_duplicate_properties() {
        let properties = [
            Property::new("title", "Hello").expect("valid property"),
            Property::new("disabled", true).expect("valid property"),
        ];
        let component = View::component(
            "Greeting",
            properties,
            [View::text("body")],
            Some(Key::from("greeting")),
        )
        .expect("valid component");
        let View::Component(component) = component else {
            panic!("component constructor must create a component descriptor");
        };
        assert_eq!(component.name(), "Greeting");
        assert_eq!(component.properties().len(), 2);
        assert_eq!(component.children(), &[View::text("body")]);
        assert!(matches!(
            View::component(
                "Greeting",
                [
                    Property::new("title", "one").expect("valid property"),
                    Property::new("title", "two").expect("valid property")
                ],
                [],
                None
            ),
            Err(ViewError::DuplicateProperty(_))
        ));
        assert!(matches!(
            View::component("bad name", [], [], None),
            Err(ViewError::InvalidComponentName(_))
        ));
    }
}
));
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidPropertyName(name));
        }
        let lowercase = name.to_ascii_lowercase();
        if matches!(lowercase.as_str(), "innerhtml" | "outerhtml" | "srcdoc") {
            return Err(ViewError::DangerousPropertyName(name));
        }
        Ok(Self(name))
    }
    /// Return the property name exactly as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A lowercase event name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventName(String);
impl EventName {
    /// Validate a browser-style event name, such as click or key-down.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
            && name
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        if !valid {
            return Err(ViewError::InvalidEventName(name));
        }
        Ok(Self(name))
    }
    /// Return the event name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A component type name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentName(String);
impl ComponentName {
    /// Validate a Rust-style component identifier.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidComponentName(name));
        }
        Ok(Self(name))
    }
    /// Return the component name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A declarative attribute value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// A textual attribute value.
    Text(String),
    /// A boolean attribute: false should be omitted by a renderer, true emitted as present.
    Boolean(bool),
}
impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for AttributeValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for AttributeValue {
    fn from(value: i32) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<i64> for AttributeValue {
    fn from(value: i64) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<f64> for AttributeValue {
    fn from(value: f64) -> Self {
        Self::Text(value.to_string())
    }
}

/// A DOM property value, kept separate from HTML attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// A textual property.
    Text(String),
    /// A boolean property.
    Boolean(bool),
    /// An integer property.
    Integer(i64),
    /// A floating-point property.
    Float(f64),
}
impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for PropertyValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for PropertyValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}
impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<f64> for PropertyValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// A named attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    name: AttributeName,
    value: AttributeValue,
}
impl Attribute {
    /// Create a validated attribute.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: AttributeName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the attribute name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the attribute value.
    pub fn value(&self) -> &AttributeValue {
        &self.value
    }
}

/// A named DOM property or component prop.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    name: PropertyName,
    value: PropertyValue,
}
impl Property {
    /// Create a validated property.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: PropertyName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the property name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the property value.
    pub fn value(&self) -> &PropertyValue {
        &self.value
    }
}

/// The subset of event data shared by platform adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPayload {
    /// Current textual value, when the target exposes one.
    pub value: Option<String>,
    /// Current checked state, when the target exposes one.
    pub checked: Option<bool>,
}

/// An owned event callback.
#[derive(Clone)]
pub struct EventHandler {
    id: u64,
    callback: Rc<dyn Fn(EventPayload)>,
}
impl EventHandler {
    /// Create an event callback with a stable identity.
    pub fn new(callback: impl Fn(EventPayload) + 'static) -> Self {
        Self {
            id: next_id(),
            callback: Rc::new(callback),
        }
    }
    /// Invoke the callback with normalized event data.
    pub fn invoke(&self, payload: EventPayload) {
        (self.callback)(payload);
    }
    /// Return the handler identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventHandler")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for EventHandler {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// An event name and its callback.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    name: EventName,
    handler: EventHandler,
}
impl EventBinding {
    /// Return the event name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return a clone of the callback handle.
    pub fn handler(&self) -> EventHandler {
        self.handler.clone()
    }
}

/// A structural view tree, independent of any rendering target.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Text content, stored verbatim.
    Text(String),
    /// An element with attributes, properties, events, and ordered children.
    Element(Element),
    /// An explicit grouping of ordered children.
    Fragment(Fragment),
    /// A component descriptor resolved by a component host.
    Component(ComponentNode),
    /// A deferred child evaluator.
    Dynamic(DynamicChild),
    /// An ordered list with unique stable keys.
    KeyedList(KeyedList),
}
impl View {
    /// Create a text node without interpreting or transforming its content.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }
    /// Create an element with a validated name and ordered children.
    pub fn element(
        tag: impl Into<String>,
        children: impl IntoIterator<Item = View>,
    ) -> Result<Self, ViewError> {
        Ok(Element::new(tag)?.with_children(children).build())
    }
    /// Create a fragment preserving child grouping.
    pub fn fragment(children: impl IntoIterator<Item = View>) -> Self {
        Self::Fragment(Fragment::new(children))
    }
    /// Create a component descriptor. Executing it requires a component host.
    pub fn component(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        Ok(Self::Component(ComponentNode::new(
            name, properties, children, key,
        )?))
    }
    /// Defer evaluation of a child until an adapter requests it.
    pub fn dynamic(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self::Dynamic(DynamicChild::new(evaluator))
    }
    /// Create a keyed list, rejecting duplicate keys.
    pub fn keyed_list(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        Ok(Self::KeyedList(KeyedList::new(children)?))
    }
}

/// Convert a supported value into a view tree for declarative syntax.
pub trait IntoView {
    /// Convert the value into a view or return a construction error.
    fn into_view(self) -> Result<View, ViewError>;
}
impl IntoView for View {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(self)
    }
}
impl IntoView for String {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}
impl IntoView for &str {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}
impl IntoView for bool {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for i32 {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for i64 {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for f64 {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}
impl IntoView for char {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self.to_string()))
    }
}

/// A validated element node.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    tag: ElementName,
    attributes: Vec<Attribute>,
    properties: Vec<Property>,
    events: Vec<EventBinding>,
    children: Vec<View>,
    key: Option<Key>,
}
impl Element {
    /// Create an element after validating its name.
    pub fn new(tag: impl Into<String>) -> Result<Self, ViewError> {
        Ok(Self {
            tag: ElementName::new(tag)?,
            attributes: Vec::new(),
            properties: Vec::new(),
            events: Vec::new(),
            children: Vec::new(),
            key: None,
        })
    }
    /// Set ordered children for this element.
    pub fn with_children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
    /// Add a validated attribute. Duplicate names are rejected.
    pub fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        let attribute = Attribute::new(name, value)?;
        if self
            .attributes
            .iter()
            .any(|existing| existing.name == attribute.name)
        {
            return Err(ViewError::DuplicateAttribute(attribute.name().to_owned()));
        }
        self.attributes.push(attribute);
        Ok(self)
    }
    /// Add a boolean HTML attribute.
    pub fn with_boolean_attribute(
        self,
        name: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ViewError> {
        self.with_attribute(name, AttributeValue::Boolean(enabled))
    }
    /// Add a DOM property. Duplicate names are rejected.
    pub fn with_property(
        mut self,
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        let property = Property::new(name, value)?;
        if self
            .properties
            .iter()
            .any(|existing| existing.name == property.name)
        {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
        self.properties.push(property);
        Ok(self)
    }
    /// Add an event callback. Events are not encoded as inline HTML attributes.
    pub fn with_event(
        mut self,
        name: impl Into<String>,
        callback: impl Fn(EventPayload) + 'static,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler: EventHandler::new(callback),
        });
        Ok(self)
    }
    /// Add a preconstructed event handler.
    pub fn with_event_handler(
        mut self,
        name: impl Into<String>,
        handler: EventHandler,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler,
        });
        Ok(self)
    }
    /// Attach a stable identity to this element.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    /// Return the validated element name.
    pub fn tag(&self) -> &str {
        self.tag.as_str()
    }
    /// Return attributes in declaration order.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// Return DOM properties in declaration order.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return event bindings in declaration order.
    pub fn events(&self) -> &[EventBinding] {
        &self.events
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
    /// Finish the builder and produce a view node.
    pub fn build(self) -> View {
        View::Element(self)
    }
}

/// An explicit group of view nodes that adds no element of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    children: Vec<View>,
}
impl Fragment {
    /// Create a fragment preserving child order and grouping.
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

/// A component descriptor passed to the component host.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentNode {
    name: ComponentName,
    properties: Vec<Property>,
    children: Vec<View>,
    key: Option<Key>,
}
impl ComponentNode {
    /// Create a component descriptor and reject duplicate properties.
    pub fn new(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        let name = ComponentName::new(name)?;
        let properties = properties.into_iter().collect::<Vec<_>>();
        ensure_unique_properties(&properties)?;
        Ok(Self {
            name,
            properties,
            children: children.into_iter().collect(),
            key,
        })
    }
    /// Return the component type name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return component properties.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
}
fn ensure_unique_properties(properties: &[Property]) -> Result<(), ViewError> {
    let mut names = HashSet::new();
    for property in properties {
        if !names.insert(property.name().to_owned()) {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
    }
    Ok(())
}

/// A child whose view tree is evaluated on demand.
#[derive(Clone)]
pub struct DynamicChild {
    id: u64,
    evaluator: Rc<dyn Fn() -> Result<View, ViewError>>,
}
impl DynamicChild {
    /// Create a deferred evaluator.
    pub fn new(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self {
            id: next_id(),
            evaluator: Rc::new(evaluator),
        }
    }
    /// Evaluate the child. A reactive renderer should call it within its owned effect.
    pub fn evaluate(&self) -> Result<View, ViewError> {
        (self.evaluator)()
    }
    /// Return this evaluator's identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for DynamicChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicChild")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for DynamicChild {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A keyed child, useful for identity-preserving list updates.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedChild {
    key: Key,
    view: View,
}
impl KeyedChild {
    /// Create a keyed child.
    pub fn new(key: impl Into<Key>, view: View) -> Self {
        Self {
            key: key.into(),
            view,
        }
    }
    /// Return this child's key.
    pub fn key(&self) -> &Key {
        &self.key
    }
    /// Return this child's view.
    pub fn view(&self) -> &View {
        &self.view
    }
}

/// An ordered keyed collection. Duplicate identities are rejected.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedList {
    children: Vec<KeyedChild>,
}
impl KeyedList {
    /// Create a keyed list while preserving order and checking key uniqueness.
    pub fn new(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        let children = children.into_iter().collect::<Vec<_>>();
        let mut keys = HashSet::new();
        for child in &children {
            if !keys.insert(child.key.clone()) {
                return Err(ViewError::DuplicateKey(child.key.clone()));
            }
        }
        Ok(Self { children })
    }
    /// Return keyed items in declaration order.
    pub fn children(&self) -> &[KeyedChild] {
        &self.children
    }
}

/// Contract implemented by a platform-specific renderer.
///
/// Implementations must mount each node once, update without leaving a
/// partially-mutated tree on recoverable errors, and release event handlers and
/// owned resources on unmount. Text and attribute values must be escaped;
/// URL-bearing values must be checked by a platform-appropriate policy.
pub trait Renderer {
    /// Handle representing the mounted root.
    type Mounted;
    /// Rendering failure.
    type Error;
    /// Construct and mount a view tree.
    fn mount(&mut self, view: &View) -> Result<Self::Mounted, Self::Error>;
    /// Reconcile a mounted root with a new view tree.
    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        previous: &View,
        next: &View,
    ) -> Result<(), Self::Error>;
    /// Unmount a tree and release listeners and resources it owns.
    fn unmount(&mut self, mounted: Self::Mounted) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{
        AttributeValue, EventPayload, Key, KeyedChild, KeyedList, Property, PropertyValue, View,
        ViewError,
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn text_content_is_preserved_verbatim() {
        let content = "<b>not markup</b>\n";
        assert_eq!(View::text(content), View::Text(content.to_owned()));
    }
    #[test]
    fn elements_preserve_their_name_and_child_order() {
        let first = View::element("li", [View::text("first")]).expect("valid element");
        let second = View::element("li", [View::text("second")]).expect("valid element");
        let tree = View::element("ul", [first.clone(), second.clone()]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.tag(), "ul");
        assert_eq!(element.children(), &[first, second]);
    }
    #[test]
    fn fragments_preserve_grouping_and_order() {
        let group = View::fragment([View::text("first"), View::text("second")]);
        let tree =
            View::element("div", [group.clone(), View::text("third")]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.children(), &[group, View::text("third")]);
    }
    #[test]
    fn empty_elements_and_fragments_are_supported() {
        let element = View::element("hr", std::iter::empty::<View>()).expect("valid element");
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
    #[test]
    fn malformed_element_names_are_rejected() {
        assert!(matches!(
            View::element("div onclick=evil", []),
            Err(ViewError::InvalidElementName(_))
        ));
        assert!(matches!(
            View::element("img/>", []),
            Err(ViewError::InvalidElementName(_))
        ));
    }
    #[test]
    fn inline_event_and_raw_html_attributes_are_rejected() {
        assert!(matches!(
            super::Element::new("button")
                .expect("valid name")
                .with_attribute("onclick", "alert(1)"),
            Err(ViewError::DangerousAttributeName(_))
        ));
        assert!(matches!(
            super::Element::new("iframe")
                .expect("valid name")
                .with_attribute("srcdoc", "<script>"),
            Err(ViewError::DangerousAttributeName(_))
        ));
    }
    #[test]
    fn attributes_properties_and_boolean_attributes_remain_distinct() {
        let node = super::Element::new("input")
            .expect("valid element")
            .with_attribute("aria-label", "Search")
            .expect("valid attribute")
            .with_boolean_attribute("disabled", true)
            .expect("valid boolean attribute")
            .with_property("value", "query")
            .expect("valid property")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        assert_eq!(element.attributes().len(), 2);
        assert_eq!(
            element.attributes()[1].value(),
            &AttributeValue::Boolean(true)
        );
        assert_eq!(element.properties().len(), 1);
        assert_eq!(
            element.properties()[0].value(),
            &PropertyValue::Text("query".into())
        );
    }
    #[test]
    fn duplicate_attributes_and_properties_are_rejected() {
        let duplicate_attribute = super::Element::new("div")
            .expect("valid element")
            .with_attribute("class", "one")
            .expect("first attribute")
            .with_attribute("class", "two");
        assert!(matches!(
            duplicate_attribute,
            Err(ViewError::DuplicateAttribute(_))
        ));
        let duplicate_property = super::Element::new("input")
            .expect("valid element")
            .with_property("value", "one")
            .expect("first property")
            .with_property("value", "two");
        assert!(matches!(
            duplicate_property,
            Err(ViewError::DuplicateProperty(_))
        ));
    }
    #[test]
    fn event_handler_receives_payload() {
        let observed = Rc::new(Cell::new(false));
        let callback_observed = observed.clone();
        let node = super::Element::new("button")
            .expect("valid element")
            .with_event("click", move |payload| {
                callback_observed.set(payload.checked == Some(true))
            })
            .expect("valid event")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        element.events()[0].handler().invoke(EventPayload {
            value: None,
            checked: Some(true),
        });
        assert!(observed.get());
    }
    #[test]
    fn dynamic_children_are_evaluated_on_demand() {
        let calls = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let dynamic = View::dynamic(move || {
            callback_calls.set(callback_calls.get() + 1);
            Ok(View::text("current"))
        });
        let View::Dynamic(child) = dynamic else {
            panic!("dynamic constructor must create a dynamic node");
        };
        assert_eq!(calls.get(), 0);
        assert_eq!(child.evaluate().expect("evaluation"), View::text("current"));
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn keyed_list_preserves_order_and_rejects_duplicate_keys() {
        let list = KeyedList::new([
            KeyedChild::new("first", View::text("a")),
            KeyedChild::new("second", View::text("b")),
        ])
        .expect("unique keys");
        assert_eq!(list.children()[0].key(), &Key::from("first"));
        assert_eq!(list.children()[1].view(), &View::text("b"));
        let duplicate = KeyedList::new([
            KeyedChild::new("same", View::text("a")),
            KeyedChild::new("same", View::text("b")),
        ]);
        assert!(matches!(duplicate, Err(ViewError::DuplicateKey(_))));
    }
    #[test]
    fn component_descriptors_validate_names_and_duplicate_properties() {
        let properties = [
            Property::new("title", "Hello").expect("valid property"),
            Property::new("disabled", true).expect("valid property"),
        ];
        let component = View::component(
            "Greeting",
            properties,
            [View::text("body")],
            Some(Key::from("greeting")),
        )
        .expect("valid component");
        let View::Component(component) = component else {
            panic!("component constructor must create a component descriptor");
        };
        assert_eq!(component.name(), "Greeting");
        assert_eq!(component.properties().len(), 2);
        assert_eq!(component.children(), &[View::text("body")]);
        assert!(matches!(
            View::component(
                "Greeting",
                [
                    Property::new("title", "one").expect("valid property"),
                    Property::new("title", "two").expect("valid property")
                ],
                [],
                None
            ),
            Err(ViewError::DuplicateProperty(_))
        ));
        assert!(matches!(
            View::component("bad name", [], [], None),
            Err(ViewError::InvalidComponentName(_))
        ));
    }
}
 | '-'));
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidPropertyName(name));
        }
        let lowercase = name.to_ascii_lowercase();
        if matches!(lowercase.as_str(), "innerhtml" | "outerhtml" | "srcdoc") {
            return Err(ViewError::DangerousPropertyName(name));
        }
        Ok(Self(name))
    }
    /// Return the property name exactly as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A lowercase event name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventName(String);
impl EventName {
    /// Validate a browser-style event name, such as click or key-down.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid = !name.is_empty()
            && name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
            && name
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        if !valid {
            return Err(ViewError::InvalidEventName(name));
        }
        Ok(Self(name))
    }
    /// Return the event name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A component type name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentName(String);
impl ComponentName {
    /// Validate a Rust-style component identifier.
    pub fn new(name: impl Into<String>) -> Result<Self, ViewError> {
        let name = name.into();
        let valid_first = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
        let valid_rest = name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !valid_first || !valid_rest {
            return Err(ViewError::InvalidComponentName(name));
        }
        Ok(Self(name))
    }
    /// Return the component name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A declarative attribute value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// A textual attribute value.
    Text(String),
    /// A boolean attribute: false should be omitted by a renderer, true emitted as present.
    Boolean(bool),
}
impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for AttributeValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for AttributeValue {
    fn from(value: i32) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<i64> for AttributeValue {
    fn from(value: i64) -> Self {
        Self::Text(value.to_string())
    }
}
impl From<f64> for AttributeValue {
    fn from(value: f64) -> Self {
        Self::Text(value.to_string())
    }
}

/// A DOM property value, kept separate from HTML attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// A textual property.
    Text(String),
    /// A boolean property.
    Boolean(bool),
    /// An integer property.
    Integer(i64),
    /// A floating-point property.
    Float(f64),
}
impl From<String> for PropertyValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for PropertyValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<bool> for PropertyValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<i32> for PropertyValue {
    fn from(value: i32) -> Self {
        Self::Integer(i64::from(value))
    }
}
impl From<i64> for PropertyValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<f64> for PropertyValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// A named attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    name: AttributeName,
    value: AttributeValue,
}
impl Attribute {
    /// Create a validated attribute.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: AttributeName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the attribute name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the attribute value.
    pub fn value(&self) -> &AttributeValue {
        &self.value
    }
}

/// A named DOM property or component prop.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    name: PropertyName,
    value: PropertyValue,
}
impl Property {
    /// Create a validated property.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            name: PropertyName::new(name)?,
            value: value.into(),
        })
    }
    /// Return the property name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return the property value.
    pub fn value(&self) -> &PropertyValue {
        &self.value
    }
}

/// The subset of event data shared by platform adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPayload {
    /// Current textual value, when the target exposes one.
    pub value: Option<String>,
    /// Current checked state, when the target exposes one.
    pub checked: Option<bool>,
}

/// An owned event callback.
#[derive(Clone)]
pub struct EventHandler {
    id: u64,
    callback: Rc<dyn Fn(EventPayload)>,
}
impl EventHandler {
    /// Create an event callback with a stable identity.
    pub fn new(callback: impl Fn(EventPayload) + 'static) -> Self {
        Self {
            id: next_id(),
            callback: Rc::new(callback),
        }
    }
    /// Invoke the callback with normalized event data.
    pub fn invoke(&self, payload: EventPayload) {
        (self.callback)(payload);
    }
    /// Return the handler identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventHandler")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for EventHandler {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// An event name and its callback.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    name: EventName,
    handler: EventHandler,
}
impl EventBinding {
    /// Return the event name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return a clone of the callback handle.
    pub fn handler(&self) -> EventHandler {
        self.handler.clone()
    }
}

/// A structural view tree, independent of any rendering target.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Text content, stored verbatim.
    Text(String),
    /// An element with attributes, properties, events, and ordered children.
    Element(Element),
    /// An explicit grouping of ordered children.
    Fragment(Fragment),
    /// A component descriptor resolved by a component host.
    Component(ComponentNode),
    /// A deferred child evaluator.
    Dynamic(DynamicChild),
    /// An ordered list with unique stable keys.
    KeyedList(KeyedList),
}
impl View {
    /// Create a text node without interpreting or transforming its content.
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }
    /// Create an element with a validated name and ordered children.
    pub fn element(
        tag: impl Into<String>,
        children: impl IntoIterator<Item = View>,
    ) -> Result<Self, ViewError> {
        Ok(Element::new(tag)?.with_children(children).build())
    }
    /// Create a fragment preserving child grouping.
    pub fn fragment(children: impl IntoIterator<Item = View>) -> Self {
        Self::Fragment(Fragment::new(children))
    }
    /// Create a component descriptor. Executing it requires a component host.
    pub fn component(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        Ok(Self::Component(ComponentNode::new(
            name, properties, children, key,
        )?))
    }
    /// Defer evaluation of a child until an adapter requests it.
    pub fn dynamic(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self::Dynamic(DynamicChild::new(evaluator))
    }
    /// Create a keyed list, rejecting duplicate keys.
    pub fn keyed_list(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        Ok(Self::KeyedList(KeyedList::new(children)?))
    }
}

/// Convert a supported value into a view tree for declarative syntax.
pub trait IntoView {
    /// Convert the value into a view or return a construction error.
    fn into_view(self) -> Result<View, ViewError>;
}
impl IntoView for View {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(self)
    }
}
impl IntoView for String {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}
impl IntoView for &str {
    fn into_view(self) -> Result<View, ViewError> {
        Ok(View::text(self))
    }
}

/// A validated element node.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    tag: ElementName,
    attributes: Vec<Attribute>,
    properties: Vec<Property>,
    events: Vec<EventBinding>,
    children: Vec<View>,
    key: Option<Key>,
}
impl Element {
    /// Create an element after validating its name.
    pub fn new(tag: impl Into<String>) -> Result<Self, ViewError> {
        Ok(Self {
            tag: ElementName::new(tag)?,
            attributes: Vec::new(),
            properties: Vec::new(),
            events: Vec::new(),
            children: Vec::new(),
            key: None,
        })
    }
    /// Set ordered children for this element.
    pub fn with_children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
    /// Add a validated attribute. Duplicate names are rejected.
    pub fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Result<Self, ViewError> {
        let attribute = Attribute::new(name, value)?;
        if self
            .attributes
            .iter()
            .any(|existing| existing.name == attribute.name)
        {
            return Err(ViewError::DuplicateAttribute(attribute.name().to_owned()));
        }
        self.attributes.push(attribute);
        Ok(self)
    }
    /// Add a boolean HTML attribute.
    pub fn with_boolean_attribute(
        self,
        name: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ViewError> {
        self.with_attribute(name, AttributeValue::Boolean(enabled))
    }
    /// Add a DOM property. Duplicate names are rejected.
    pub fn with_property(
        mut self,
        name: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Result<Self, ViewError> {
        let property = Property::new(name, value)?;
        if self
            .properties
            .iter()
            .any(|existing| existing.name == property.name)
        {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
        self.properties.push(property);
        Ok(self)
    }
    /// Add an event callback. Events are not encoded as inline HTML attributes.
    pub fn with_event(
        mut self,
        name: impl Into<String>,
        callback: impl Fn(EventPayload) + 'static,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler: EventHandler::new(callback),
        });
        Ok(self)
    }
    /// Add a preconstructed event handler.
    pub fn with_event_handler(
        mut self,
        name: impl Into<String>,
        handler: EventHandler,
    ) -> Result<Self, ViewError> {
        let event_name = EventName::new(name)?;
        if self
            .events
            .iter()
            .any(|existing| existing.name == event_name)
        {
            return Err(ViewError::DuplicateEvent(event_name.as_str().to_owned()));
        }
        self.events.push(EventBinding {
            name: event_name,
            handler,
        });
        Ok(self)
    }
    /// Attach a stable identity to this element.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    /// Return the validated element name.
    pub fn tag(&self) -> &str {
        self.tag.as_str()
    }
    /// Return attributes in declaration order.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// Return DOM properties in declaration order.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return event bindings in declaration order.
    pub fn events(&self) -> &[EventBinding] {
        &self.events
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
    /// Finish the builder and produce a view node.
    pub fn build(self) -> View {
        View::Element(self)
    }
}

/// An explicit group of view nodes that adds no element of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    children: Vec<View>,
}
impl Fragment {
    /// Create a fragment preserving child order and grouping.
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

/// A component descriptor passed to the component host.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentNode {
    name: ComponentName,
    properties: Vec<Property>,
    children: Vec<View>,
    key: Option<Key>,
}
impl ComponentNode {
    /// Create a component descriptor and reject duplicate properties.
    pub fn new(
        name: impl Into<String>,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
        key: Option<Key>,
    ) -> Result<Self, ViewError> {
        let name = ComponentName::new(name)?;
        let properties = properties.into_iter().collect::<Vec<_>>();
        ensure_unique_properties(&properties)?;
        Ok(Self {
            name,
            properties,
            children: children.into_iter().collect(),
            key,
        })
    }
    /// Return the component type name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Return component properties.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return children in construction order.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the optional stable key.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }
}
fn ensure_unique_properties(properties: &[Property]) -> Result<(), ViewError> {
    let mut names = HashSet::new();
    for property in properties {
        if !names.insert(property.name().to_owned()) {
            return Err(ViewError::DuplicateProperty(property.name().to_owned()));
        }
    }
    Ok(())
}

/// A child whose view tree is evaluated on demand.
#[derive(Clone)]
pub struct DynamicChild {
    id: u64,
    evaluator: Rc<dyn Fn() -> Result<View, ViewError>>,
}
impl DynamicChild {
    /// Create a deferred evaluator.
    pub fn new(evaluator: impl Fn() -> Result<View, ViewError> + 'static) -> Self {
        Self {
            id: next_id(),
            evaluator: Rc::new(evaluator),
        }
    }
    /// Evaluate the child. A reactive renderer should call it within its owned effect.
    pub fn evaluate(&self) -> Result<View, ViewError> {
        (self.evaluator)()
    }
    /// Return this evaluator's identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl fmt::Debug for DynamicChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicChild")
            .field("id", &self.id)
            .finish()
    }
}
impl PartialEq for DynamicChild {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A keyed child, useful for identity-preserving list updates.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedChild {
    key: Key,
    view: View,
}
impl KeyedChild {
    /// Create a keyed child.
    pub fn new(key: impl Into<Key>, view: View) -> Self {
        Self {
            key: key.into(),
            view,
        }
    }
    /// Return this child's key.
    pub fn key(&self) -> &Key {
        &self.key
    }
    /// Return this child's view.
    pub fn view(&self) -> &View {
        &self.view
    }
}

/// An ordered keyed collection. Duplicate identities are rejected.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedList {
    children: Vec<KeyedChild>,
}
impl KeyedList {
    /// Create a keyed list while preserving order and checking key uniqueness.
    pub fn new(children: impl IntoIterator<Item = KeyedChild>) -> Result<Self, ViewError> {
        let children = children.into_iter().collect::<Vec<_>>();
        let mut keys = HashSet::new();
        for child in &children {
            if !keys.insert(child.key.clone()) {
                return Err(ViewError::DuplicateKey(child.key.clone()));
            }
        }
        Ok(Self { children })
    }
    /// Return keyed items in declaration order.
    pub fn children(&self) -> &[KeyedChild] {
        &self.children
    }
}

/// Contract implemented by a platform-specific renderer.
///
/// Implementations must mount each node once, update without leaving a
/// partially-mutated tree on recoverable errors, and release event handlers and
/// owned resources on unmount. Text and attribute values must be escaped;
/// URL-bearing values must be checked by a platform-appropriate policy.
pub trait Renderer {
    /// Handle representing the mounted root.
    type Mounted;
    /// Rendering failure.
    type Error;
    /// Construct and mount a view tree.
    fn mount(&mut self, view: &View) -> Result<Self::Mounted, Self::Error>;
    /// Reconcile a mounted root with a new view tree.
    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        previous: &View,
        next: &View,
    ) -> Result<(), Self::Error>;
    /// Unmount a tree and release listeners and resources it owns.
    fn unmount(&mut self, mounted: Self::Mounted) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{
        AttributeValue, EventPayload, Key, KeyedChild, KeyedList, Property, PropertyValue, View,
        ViewError,
    };
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn text_content_is_preserved_verbatim() {
        let content = "<b>not markup</b>\n";
        assert_eq!(View::text(content), View::Text(content.to_owned()));
    }
    #[test]
    fn elements_preserve_their_name_and_child_order() {
        let first = View::element("li", [View::text("first")]).expect("valid element");
        let second = View::element("li", [View::text("second")]).expect("valid element");
        let tree = View::element("ul", [first.clone(), second.clone()]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.tag(), "ul");
        assert_eq!(element.children(), &[first, second]);
    }
    #[test]
    fn fragments_preserve_grouping_and_order() {
        let group = View::fragment([View::text("first"), View::text("second")]);
        let tree =
            View::element("div", [group.clone(), View::text("third")]).expect("valid element");
        let View::Element(element) = tree else {
            panic!("element constructor must create an element node");
        };
        assert_eq!(element.children(), &[group, View::text("third")]);
    }
    #[test]
    fn empty_elements_and_fragments_are_supported() {
        let element = View::element("hr", std::iter::empty::<View>()).expect("valid element");
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
    #[test]
    fn malformed_element_names_are_rejected() {
        assert!(matches!(
            View::element("div onclick=evil", []),
            Err(ViewError::InvalidElementName(_))
        ));
        assert!(matches!(
            View::element("img/>", []),
            Err(ViewError::InvalidElementName(_))
        ));
    }
    #[test]
    fn inline_event_and_raw_html_attributes_are_rejected() {
        assert!(matches!(
            super::Element::new("button")
                .expect("valid name")
                .with_attribute("onclick", "alert(1)"),
            Err(ViewError::DangerousAttributeName(_))
        ));
        assert!(matches!(
            super::Element::new("iframe")
                .expect("valid name")
                .with_attribute("srcdoc", "<script>"),
            Err(ViewError::DangerousAttributeName(_))
        ));
    }
    #[test]
    fn attributes_properties_and_boolean_attributes_remain_distinct() {
        let node = super::Element::new("input")
            .expect("valid element")
            .with_attribute("aria-label", "Search")
            .expect("valid attribute")
            .with_boolean_attribute("disabled", true)
            .expect("valid boolean attribute")
            .with_property("value", "query")
            .expect("valid property")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        assert_eq!(element.attributes().len(), 2);
        assert_eq!(
            element.attributes()[1].value(),
            &AttributeValue::Boolean(true)
        );
        assert_eq!(element.properties().len(), 1);
        assert_eq!(
            element.properties()[0].value(),
            &PropertyValue::Text("query".into())
        );
    }
    #[test]
    fn duplicate_attributes_and_properties_are_rejected() {
        let duplicate_attribute = super::Element::new("div")
            .expect("valid element")
            .with_attribute("class", "one")
            .expect("first attribute")
            .with_attribute("class", "two");
        assert!(matches!(
            duplicate_attribute,
            Err(ViewError::DuplicateAttribute(_))
        ));
        let duplicate_property = super::Element::new("input")
            .expect("valid element")
            .with_property("value", "one")
            .expect("first property")
            .with_property("value", "two");
        assert!(matches!(
            duplicate_property,
            Err(ViewError::DuplicateProperty(_))
        ));
    }
    #[test]
    fn event_handler_receives_payload() {
        let observed = Rc::new(Cell::new(false));
        let callback_observed = observed.clone();
        let node = super::Element::new("button")
            .expect("valid element")
            .with_event("click", move |payload| {
                callback_observed.set(payload.checked == Some(true))
            })
            .expect("valid event")
            .build();
        let View::Element(element) = node else {
            panic!("builder must create an element node");
        };
        element.events()[0].handler().invoke(EventPayload {
            value: None,
            checked: Some(true),
        });
        assert!(observed.get());
    }
    #[test]
    fn dynamic_children_are_evaluated_on_demand() {
        let calls = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let dynamic = View::dynamic(move || {
            callback_calls.set(callback_calls.get() + 1);
            Ok(View::text("current"))
        });
        let View::Dynamic(child) = dynamic else {
            panic!("dynamic constructor must create a dynamic node");
        };
        assert_eq!(calls.get(), 0);
        assert_eq!(child.evaluate().expect("evaluation"), View::text("current"));
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn keyed_list_preserves_order_and_rejects_duplicate_keys() {
        let list = KeyedList::new([
            KeyedChild::new("first", View::text("a")),
            KeyedChild::new("second", View::text("b")),
        ])
        .expect("unique keys");
        assert_eq!(list.children()[0].key(), &Key::from("first"));
        assert_eq!(list.children()[1].view(), &View::text("b"));
        let duplicate = KeyedList::new([
            KeyedChild::new("same", View::text("a")),
            KeyedChild::new("same", View::text("b")),
        ]);
        assert!(matches!(duplicate, Err(ViewError::DuplicateKey(_))));
    }
    #[test]
    fn component_descriptors_validate_names_and_duplicate_properties() {
        let properties = [
            Property::new("title", "Hello").expect("valid property"),
            Property::new("disabled", true).expect("valid property"),
        ];
        let component = View::component(
            "Greeting",
            properties,
            [View::text("body")],
            Some(Key::from("greeting")),
        )
        .expect("valid component");
        let View::Component(component) = component else {
            panic!("component constructor must create a component descriptor");
        };
        assert_eq!(component.name(), "Greeting");
        assert_eq!(component.properties().len(), 2);
        assert_eq!(component.children(), &[View::text("body")]);
        assert!(matches!(
            View::component(
                "Greeting",
                [
                    Property::new("title", "one").expect("valid property"),
                    Property::new("title", "two").expect("valid property")
                ],
                [],
                None
            ),
            Err(ViewError::DuplicateProperty(_))
        ));
        assert!(matches!(
            View::component("bad name", [], [], None),
            Err(ViewError::InvalidComponentName(_))
        ));
    }
}
