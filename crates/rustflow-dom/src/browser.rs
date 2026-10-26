use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use js_sys::Reflect;
use rustflow::{
    components::{ComponentError, ComponentInstance, ComponentRegistry},
    reactivity::{Runtime, RuntimeError, Scope},
};
use rustflow_view::{
    AttributeValue, ComponentNode, DynamicChild, Element, EventBinding, EventHandler, EventPayload,
    KeyedList, PropertyValue, Renderer, View, ViewError,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    Comment, Document, Element as DomElement, Event, EventTarget, HtmlInputElement,
    HtmlSelectElement, HtmlTextAreaElement, Node,
};

use crate::policy::{DomPolicyError, validate_attribute, validate_element, validate_property};

/// Errors returned by the WebAssembly DOM adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomError {
    WindowUnavailable,
    DocumentUnavailable,
    JavaScript(String),
    Policy(DomPolicyError),
    View(ViewError),
    Component(String),
    Runtime(String),
    PropertyRejected(String),
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowUnavailable => write!(f, "browser window is unavailable"),
            Self::DocumentUnavailable => write!(f, "browser document is unavailable"),
            Self::JavaScript(message) => write!(f, "DOM operation failed: {message}"),
            Self::Policy(error) => write!(f, "{error}"),
            Self::View(error) => write!(f, "view construction failed: {error}"),
            Self::Component(message) => write!(f, "component rendering failed: {message}"),
            Self::Runtime(message) => write!(f, "reactive DOM update failed: {message}"),
            Self::PropertyRejected(name) => {
                write!(f, "browser rejected DOM property: {name}")
            }
        }
    }
}
impl Error for DomError {}

impl From<ViewError> for DomError {
    fn from(error: ViewError) -> Self {
        Self::View(error)
    }
}
impl From<ComponentError> for DomError {
    fn from(error: ComponentError) -> Self {
        Self::Component(error.to_string())
    }
}
impl From<RuntimeError> for DomError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error.to_string())
    }
}
impl From<DomPolicyError> for DomError {
    fn from(error: DomPolicyError) -> Self {
        Self::Policy(error)
    }
}

fn js_error(value: JsValue) -> DomError {
    DomError::JavaScript(
        value
            .as_string()
            .unwrap_or_else(|| "unknown JavaScript error".to_owned()),
    )
}

struct EventListener {
    target: EventTarget,
    name: String,
    callback: Closure<dyn FnMut(Event)>,
}

#[derive(Default)]
struct RenderedNodes {
    listeners: Vec<EventListener>,
    components: Vec<ComponentInstance>,
}

/// A mounted DOM range. Comment anchors make fragment roots updateable without
/// introducing a visible wrapper element.
pub struct MountedDom {
    start: Comment,
    end: Comment,
    roots: Vec<Node>,
    listeners: Vec<EventListener>,
    components: Vec<ComponentInstance>,
}

impl MountedDom {
    /// Top-level rendered nodes, excluding invisible range anchors.
    pub fn root_nodes(&self) -> &[Node] {
        &self.roots
    }

    /// Whether the range is currently attached to a parent node.
    pub fn is_attached(&self) -> bool {
        self.start.parent_node().is_some()
    }
}

struct ReactiveMountState {
    mounted: MountedDom,
    view: View,
}

/// A signal-driven mount. Dropping or explicitly unmounting this handle disposes
/// the owned reactive scope, event listeners, and component instances.
pub struct ReactiveMountedDom {
    scope: Scope,
    state: Rc<RefCell<Option<ReactiveMountState>>>,
    last_error: Rc<RefCell<Option<DomError>>>,
    document: Document,
    runtime: Runtime,
    registry: ComponentRegistry,
}

impl ReactiveMountedDom {
    /// Whether the initial view has mounted successfully.
    pub fn is_mounted(&self) -> bool {
        self.state.borrow().is_some()
    }

    /// Current view snapshot, if mounted.
    pub fn current_view(&self) -> Option<View> {
        self.state.borrow().as_ref().map(|state| state.view.clone())
    }

    /// Take the most recent render/update error. A failed update leaves the
    /// last successful DOM range intact.
    pub fn take_error(&self) -> Option<DomError> {
        self.last_error.borrow_mut().take()
    }

    /// Dispose effects and explicitly unmount the currently rendered range.
    pub fn unmount(mut self) -> Result<(), DomError> {
        self.scope.dispose()?;
        if let Some(state) = self.state.borrow_mut().take() {
            let mut renderer = DomRenderer::new(
                self.document.clone(),
                self.runtime.clone(),
                self.registry.clone(),
            );
            renderer.root_scope = self.scope.clone();
            renderer.unmount(state.mounted)?;
        }
        Ok(())
    }
}

impl Drop for ReactiveMountedDom {
    fn drop(&mut self) {
        let _ = self.scope.dispose();
        self.state.borrow_mut().take();
    }
}

/// Renderer for an explicitly supplied Document and component registry.
///
/// The initial adapter uses full replacement on update. Keyed reconciliation and
/// automatic signal-to-DOM scheduling are not implemented in this first slice.
pub struct DomRenderer {
    document: Document,
    runtime: Runtime,
    root_scope: Scope,
    registry: ComponentRegistry,
}

impl DomRenderer {
    /// Create a renderer for the provided document.
    pub fn new(document: Document, runtime: Runtime, registry: ComponentRegistry) -> Self {
        let root_scope = runtime.root_scope();
        Self {
            document,
            runtime,
            root_scope,
            registry,
        }
    }

    /// Create a renderer from the current browser window.
    pub fn from_window(runtime: Runtime, registry: ComponentRegistry) -> Result<Self, DomError> {
        let window = web_sys::window().ok_or(DomError::WindowUnavailable)?;
        let document = window.document().ok_or(DomError::DocumentUnavailable)?;
        Ok(Self::new(document, runtime, registry))
    }

    /// Mount a view directly inside an element and return its owned range.
    pub fn mount_into(&mut self, target: &DomElement, view: &View) -> Result<MountedDom, DomError> {
        let mounted = self.mount(view)?;
        let parent: Node = target.clone().unchecked_into();
        if let Err(error) = attach_range(&parent, &mounted) {
            let mut mounted = mounted;
            let _ = cleanup_resources(&mut mounted.listeners, &mut mounted.components);
            return Err(error);
        }
        Ok(mounted)
    }

    /// Mount a view produced by a reactive closure and rerender it whenever
    /// tracked signals change. Updates currently replace the full range; keyed
    /// reconciliation and targeted DOM patching remain future improvements.
    ///
    /// Errors from the initial render are returned immediately. Later render
    /// failures are available through `ReactiveMountedDom::take_error`, while
    /// the last successful DOM remains mounted.
    pub fn mount_reactive<F>(
        &mut self,
        target: &DomElement,
        owner_scope: &Scope,
        render: F,
    ) -> Result<ReactiveMountedDom, DomError>
    where
        F: Fn() -> Result<View, DomError> + 'static,
    {
        let scope = owner_scope.child()?;
        let state: Rc<RefCell<Option<ReactiveMountState>>> = Rc::new(RefCell::new(None));
        let last_error: Rc<RefCell<Option<DomError>>> = Rc::new(RefCell::new(None));

        let document = self.document.clone();
        let runtime = self.runtime.clone();
        let registry = self.registry.clone();
        let effect_scope = scope.clone();
        let target = target.clone();
        let state_effect = state.clone();
        let error_effect = last_error.clone();

        let effect_result = scope.effect(move || {
            let next_view = match render() {
                Ok(view) => view,
                Err(error) => {
                    *error_effect.borrow_mut() = Some(error);
                    return;
                }
            };

            let mut renderer = DomRenderer::new(
                document.clone(),
                runtime.clone(),
                registry.clone(),
            );
            renderer.root_scope = effect_scope.clone();

            let mut current = state_effect.borrow_mut();
            if let Some(current) = current.as_mut() {
                let previous_view = current.view.clone();
                match renderer.update(&mut current.mounted, &previous_view, &next_view) {
                    Ok(()) => {
                        current.view = next_view;
                        *error_effect.borrow_mut() = None;
                    }
                    Err(error) => *error_effect.borrow_mut() = Some(error),
                }
            } else {
                match renderer.mount_into(&target, &next_view) {
                    Ok(mounted) => {
                        *current = Some(ReactiveMountState {
                            mounted,
                            view: next_view,
                        });
                        *error_effect.borrow_mut() = None;
                    }
                    Err(error) => *error_effect.borrow_mut() = Some(error),
                }
            }
        });

        let _effect = match effect_result {
            Ok(effect) => effect,
            Err(error) => {
                let _ = scope.dispose();
                return Err(error.into());
            }
        };

        if let Some(error) = last_error.borrow_mut().take() {
            let _ = scope.dispose();
            state.borrow_mut().take();
            return Err(error);
        }

        Ok(ReactiveMountedDom {
            scope,
            state,
            last_error,
            document: self.document.clone(),
            runtime: self.runtime.clone(),
            registry: self.registry.clone(),
        })
    }

    fn build_nodes(
        &self,
        view: &View,
        parent_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        match view {
            View::Text(text) => {
                let node: Node = self.document.create_text_node(text).unchecked_into();
                Ok(vec![node])
            }
            View::Fragment(fragment) => {
                let mut nodes = Vec::new();
                for child in fragment.children() {
                    nodes.extend(self.build_nodes(child, parent_scope, resources)?);
                }
                Ok(nodes)
            }
            View::KeyedList(list) => self.build_keyed_list(list, parent_scope, resources),
            View::Dynamic(dynamic) => self.build_dynamic(dynamic, parent_scope, resources),
            View::Component(component) => self.build_component(component, parent_scope, resources),
            View::Element(element) => self.build_element(element, parent_scope, resources),
        }
    }

    fn build_element(
        &self,
        view: &Element,
        parent_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        validate_element(view.tag(), view.children().len())?;
        let element = self
            .document
            .create_element(&view.tag().to_ascii_lowercase())
            .map_err(js_error)?;

        for attribute in view.attributes() {
            let value = match attribute.value() {
                AttributeValue::Boolean(true) => "",
                AttributeValue::Boolean(false) => {
                    validate_attribute(attribute.name(), "")?;
                    element
                        .remove_attribute(attribute.name())
                        .map_err(js_error)?;
                    continue;
                }
                AttributeValue::Text(value) => value.as_str(),
            };
            validate_attribute(attribute.name(), value)?;
            match attribute.value() {
                AttributeValue::Boolean(true) => {
                    element
                        .set_attribute(attribute.name(), "")
                        .map_err(js_error)?;
                }
                AttributeValue::Boolean(false) => unreachable!("false was handled above"),
                AttributeValue::Text(value) => {
                    element
                        .set_attribute(attribute.name(), value)
                        .map_err(js_error)?;
                }
            }
        }

        for property in view.properties() {
            validate_property(property.name(), property.value())?;
            let value = match property.value() {
                PropertyValue::Text(value) => JsValue::from_str(value),
                PropertyValue::Boolean(value) => JsValue::from_bool(*value),
                PropertyValue::Integer(value) => JsValue::from_f64(*value as f64),
                PropertyValue::Float(value) => JsValue::from_f64(*value),
            };
            let target: &JsValue = element.as_ref();
            let accepted = Reflect::set(target, &JsValue::from_str(property.name()), &value)
                .map_err(js_error)?;
            if !accepted {
                return Err(DomError::PropertyRejected(property.name().to_owned()));
            }
        }

        let event_target: EventTarget = element.clone().unchecked_into();
        for binding in view.events() {
            resources
                .listeners
                .push(create_event_listener(&event_target, binding)?);
        }

        for child in view.children() {
            let child_nodes = self.build_nodes(child, parent_scope, resources)?;
            for child_node in child_nodes {
                element.append_child(&child_node).map_err(js_error)?;
            }
        }

        let node: Node = element.unchecked_into();
        Ok(vec![node])
    }

    fn build_keyed_list(
        &self,
        list: &KeyedList,
        parent_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        let mut nodes = Vec::new();
        for child in list.children() {
            nodes.extend(self.build_nodes(child.view(), parent_scope, resources)?);
        }
        Ok(nodes)
    }

    fn build_dynamic(
        &self,
        dynamic: &DynamicChild,
        parent_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        let view = dynamic.evaluate()?;
        self.build_nodes(&view, parent_scope, resources)
    }

    fn build_component(
        &self,
        component: &ComponentNode,
        parent_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        let mut instance = self
            .registry
            .mount(component, self.runtime.clone(), parent_scope)?;
        let view = instance.view().clone();
        let nodes = match self.build_nodes(&view, instance.scope(), resources) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = instance.unmount();
                return Err(error);
            }
        };
        resources.components.push(instance);
        Ok(nodes)
    }
}

impl Renderer for DomRenderer {
    type Mounted = MountedDom;
    type Error = DomError;

    fn mount(&mut self, view: &View) -> Result<Self::Mounted, Self::Error> {
        let start = self.document.create_comment("rustflow:start");
        let end = self.document.create_comment("rustflow:end");
        let mut resources = RenderedNodes::default();
        let roots = match self.build_nodes(view, &self.root_scope, &mut resources) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = cleanup_resources(&mut resources.listeners, &mut resources.components);
                return Err(error);
            }
        };
        Ok(MountedDom {
            start,
            end,
            roots,
            listeners: resources.listeners,
            components: resources.components,
        })
    }

    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        _previous: &View,
        next: &View,
    ) -> Result<(), Self::Error> {
        let mut resources = RenderedNodes::default();
        let next_roots = match self.build_nodes(next, &self.root_scope, &mut resources) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = cleanup_resources(&mut resources.listeners, &mut resources.components);
                return Err(error);
            }
        };

        if let Some(parent) = mounted.start.parent_node() {
            let end: Node = mounted.end.clone().unchecked_into();
            let mut inserted = Vec::new();
            for node in &next_roots {
                if let Err(error) = parent.insert_before(node, Some(&end)) {
                    for inserted_node in inserted {
                        let _ = parent.remove_child(&inserted_node);
                    }
                    let _ = cleanup_resources(&mut resources.listeners, &mut resources.components);
                    return Err(js_error(error));
                }
                inserted.push(node.clone());
            }
            for old_node in &mounted.roots {
                if let Err(error) = parent.remove_child(old_node) {
                    for inserted_node in inserted {
                        let _ = parent.remove_child(&inserted_node);
                    }
                    let _ = cleanup_resources(&mut resources.listeners, &mut resources.components);
                    return Err(js_error(error));
                }
            }
        }

        cleanup_resources(&mut mounted.listeners, &mut mounted.components)?;
        mounted.roots = next_roots;
        mounted.listeners = resources.listeners;
        mounted.components = resources.components;
        Ok(())
    }

    fn unmount(&mut self, mut mounted: Self::Mounted) -> Result<(), Self::Error> {
        let mut first_error = detach_range(&mut mounted).err();
        if let Err(error) = cleanup_resources(&mut mounted.listeners, &mut mounted.components) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for MountedDom {
    fn drop(&mut self) {
        let _ = detach_range(self);
        let _ = cleanup_resources(&mut self.listeners, &mut self.components);
    }
}

fn attach_range(parent: &Node, mounted: &MountedDom) -> Result<(), DomError> {
    let start: Node = mounted.start.clone().unchecked_into();
    let end: Node = mounted.end.clone().unchecked_into();
    let mut inserted: Vec<Node> = Vec::new();
    let mut range = Vec::with_capacity(mounted.roots.len() + 2);
    range.push(start);
    range.extend(mounted.roots.iter().cloned());
    range.push(end);

    for node in range {
        if let Err(error) = parent.append_child(&node) {
            for inserted_node in inserted {
                let _ = parent.remove_child(&inserted_node);
            }
            return Err(js_error(error));
        }
        inserted.push(node);
    }
    Ok(())
}

fn detach_range(mounted: &mut MountedDom) -> Result<(), DomError> {
    let Some(parent) = mounted.start.parent_node() else {
        return Ok(());
    };
    let mut first_error = None;
    for node in &mounted.roots {
        if let Err(error) = parent.remove_child(node) {
            if first_error.is_none() {
                first_error = Some(js_error(error));
            }
        }
    }
    let start: Node = mounted.start.clone().unchecked_into();
    if let Err(error) = parent.remove_child(&start) {
        if first_error.is_none() {
            first_error = Some(js_error(error));
        }
    }
    let end: Node = mounted.end.clone().unchecked_into();
    if let Err(error) = parent.remove_child(&end) {
        if first_error.is_none() {
            first_error = Some(js_error(error));
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_resources(
    listeners: &mut Vec<EventListener>,
    components: &mut Vec<ComponentInstance>,
) -> Result<(), DomError> {
    let mut first_error = None;
    for listener in listeners.drain(..) {
        if let Err(error) = listener.target.remove_event_listener_with_callback(
            &listener.name,
            listener.callback.as_ref().unchecked_ref(),
        ) {
            if first_error.is_none() {
                first_error = Some(js_error(error));
            }
        }
        drop(listener.callback);
    }
    for component in components.iter_mut().rev() {
        if let Err(error) = component.unmount() {
            if first_error.is_none() {
                first_error = Some(error.into());
            }
        }
    }
    components.clear();
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn create_event_listener(
    target: &EventTarget,
    binding: &EventBinding,
) -> Result<EventListener, DomError> {
    let handler: EventHandler = binding.handler();
    let callback = Closure::<dyn FnMut(Event)>::new(move |event| {
        handler.invoke(event_payload(&event));
    });
    target
        .add_event_listener_with_callback(binding.name(), callback.as_ref().unchecked_ref())
        .map_err(js_error)?;
    Ok(EventListener {
        target: target.clone(),
        name: binding.name().to_owned(),
        callback,
    })
}

fn event_payload(event: &Event) -> EventPayload {
    let target = event.target();
    let value = target.as_ref().and_then(|target| {
        target
            .dyn_ref::<HtmlInputElement>()
            .map(|input| input.value())
            .or_else(|| {
                target
                    .dyn_ref::<HtmlTextAreaElement>()
                    .map(|textarea| textarea.value())
            })
            .or_else(|| {
                target
                    .dyn_ref::<HtmlSelectElement>()
                    .map(|select| select.value())
            })
    });
    let checked = target.as_ref().and_then(|target| {
        target
            .dyn_ref::<HtmlInputElement>()
            .map(|input| input.checked())
    });
    EventPayload { value, checked }
}
