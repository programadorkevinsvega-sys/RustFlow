use std::{
    cell::{Cell, RefCell},
    error::Error,
    fmt,
    rc::Rc,
};

use js_sys::Reflect;
use rustflow::{
    components::{ComponentError, ComponentInstance, ComponentRegistry},
    reactivity::{Effect, Runtime, RuntimeError, Scope, untrack},
};
use rustflow_view::{
    AttributeValue, ComponentNode, DynamicChild, Element, EventBinding, EventHandler, EventPayload,
    Key, KeyedList, PropertyValue, Renderer, View, ViewError,
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

#[derive(Clone)]
struct DelegatedBinding {
    element: DomElement,
    name: String,
    handler: EventHandler,
}

#[derive(Default)]
struct RenderedNodes {
    listeners: Vec<EventListener>,
    delegated: Vec<DelegatedBinding>,
    components: Vec<ComponentInstance>,
    component_regions: Vec<ComponentRegion>,
    dynamics: Vec<DynamicRegion>,
}

struct DynamicRegion {
    dynamic: DynamicChild,
    scope: Scope,
    state: Rc<RefCell<Option<DynamicMountState>>>,
    error_sink: Option<Rc<RefCell<Option<DomError>>>>,
    effect: Option<Effect>,
    reused: bool,
}

struct DynamicMountState {
    mounted: MountedDom,
    view: View,
}

struct ComponentRegion {
    descriptor: ComponentNode,
    instance: ComponentInstance,
    mounted: MountedDom,
    view: View,
    reused: bool,
}

/// A mounted DOM range. Comment anchors make fragment roots updateable without
/// introducing a visible wrapper element.
pub struct MountedDom {
    start: Comment,
    end: Comment,
    roots: Vec<Node>,
    listeners: Vec<EventListener>,
    delegated_listeners: Vec<EventListener>,
    delegated_bindings: Rc<RefCell<Vec<DelegatedBinding>>>,
    components: Vec<ComponentInstance>,
    component_regions: Vec<ComponentRegion>,
    dynamics: Vec<DynamicRegion>,
}

impl MountedDom {
    /// Snapshot of nodes from the last root reconciliation, excluding outer anchors.
    /// Dynamic subregions may later change this snapshot.
    pub fn root_nodes(&self) -> &[Node] {
        &self.roots
    }

    /// Current nodes between the root anchors, including dynamically updated content.
    pub fn current_root_nodes(&self) -> Vec<Node> {
        let start: Node = self.start.clone().unchecked_into();
        let end: Node = self.end.clone().unchecked_into();
        nodes_between(&start, &end)
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
    pub fn unmount(self) -> Result<(), DomError> {
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
        drop(self.state.borrow_mut().take());
    }
}

/// Renderer for an explicitly supplied Document and component registry.
///
/// Compatible text/element nodes and keyed lists are reconciled in place. Trees
/// containing deferred dynamic children or component descriptors still use the
/// conservative full-range replacement path until those nodes own explicit DOM subranges.
pub struct DomRenderer {
    document: Document,
    runtime: Runtime,
    root_scope: Scope,
    registry: ComponentRegistry,
    error_sink: Option<Rc<RefCell<Option<DomError>>>>,
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
            error_sink: None,
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
        let mut mounted = self.mount(view)?;
        let parent: Node = target.clone().unchecked_into();
        if let Err(error) = attach_range(&parent, &mounted) {
            let _ = cleanup_mounted_resources(&mut mounted);
            return Err(error);
        }
        if let Err(error) = self.activate_delegated_listeners(&mut mounted, &parent) {
            let _ = detach_range(&mut mounted);
            let _ = cleanup_mounted_resources(&mut mounted);
            return Err(error);
        }
        if let Err(error) = self.activate_component_regions(&mut mounted.component_regions) {
            let _ = detach_range(&mut mounted);
            let _ = cleanup_mounted_resources(&mut mounted);
            return Err(error);
        }
        if let Err(error) = self.activate_dynamic_regions(&mut mounted.dynamics) {
            let _ = detach_range(&mut mounted);
            let _ = cleanup_mounted_resources(&mut mounted);
            return Err(error);
        }
        Ok(mounted)
    }

    /// Mount a view produced by a reactive closure and rerender it whenever
    /// tracked signals change. Compatible text/element nodes are patched in place
    /// and keyed lists retain same-key compatible DOM nodes. Trees containing
    /// dynamic evaluators or component descriptors still use full-range replacement.
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
                    web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                    *error_effect.borrow_mut() = Some(error);
                    return;
                }
            };

            let mut renderer =
                DomRenderer::new(document.clone(), runtime.clone(), registry.clone());
            renderer.root_scope = effect_scope.clone();
            renderer.error_sink = Some(error_effect.clone());

            let mut current = state_effect.borrow_mut();
            if let Some(current) = current.as_mut() {
                let previous_view = current.view.clone();
                match renderer.update(&mut current.mounted, &previous_view, &next_view) {
                    Ok(()) => {
                        current.view = next_view;
                        *error_effect.borrow_mut() = None;
                    }
                    Err(error) => {
                        web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                        *error_effect.borrow_mut() = Some(error);
                    }
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
            drop(state.borrow_mut().take());
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

    fn activate_delegated_listeners(
        &self,
        mounted: &mut MountedDom,
        boundary: &Node,
    ) -> Result<(), DomError> {
        let event_names = {
            let bindings = mounted.delegated_bindings.borrow();
            let mut names = Vec::<String>::new();
            for binding in bindings.iter() {
                if event_uses_delegation(&binding.name)
                    && !names.iter().any(|name| name == &binding.name)
                {
                    names.push(binding.name.clone());
                }
            }
            names
        };

        for name in event_names {
            // Keep one listener per event type and mount boundary. Its shared
            // binding table is replaced during reconciliation, without dropping
            // the currently-dispatching closure or adding duplicate listeners.
            if mounted
                .delegated_listeners
                .iter()
                .any(|listener| listener.name == name)
            {
                continue;
            }

            let target: EventTarget = boundary.clone().unchecked_into();
            let bindings = mounted.delegated_bindings.clone();
            let event_name = name.clone();
            let boundary_node = boundary.clone();
            let callback = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                let Some(event_target) = event.target() else {
                    return;
                };
                let Ok(mut current) = event_target.dyn_into::<Node>() else {
                    return;
                };

                loop {
                    let handlers = {
                        let registered = bindings.borrow();
                        registered
                            .iter()
                            .filter_map(|binding| {
                                if binding.name != event_name {
                                    return None;
                                }
                                let bound_node: Node = binding.element.clone().unchecked_into();
                                bound_node
                                    .is_same_node(Some(&current))
                                    .then(|| binding.handler.clone())
                            })
                            .collect::<Vec<_>>()
                    };
                    for handler in handlers {
                        handler.invoke(event_payload(&event));
                    }

                    if current.is_same_node(Some(&boundary_node)) {
                        break;
                    }
                    let Some(parent_node) = current.parent_node() else {
                        break;
                    };
                    current = parent_node;
                }
            });

            target
                .add_event_listener_with_callback(&name, callback.as_ref().unchecked_ref())
                .map_err(js_error)?;
            mounted.delegated_listeners.push(EventListener {
                target,
                name,
                callback,
            });
        }
        Ok(())
    }

    fn activate_component_regions(&self, regions: &mut [ComponentRegion]) -> Result<(), DomError> {
        for region in regions {
            if let Some(parent) = region.mounted.start.parent_node() {
                self.activate_delegated_listeners(&mut region.mounted, &parent)?;
            }
            self.activate_component_regions(&mut region.mounted.component_regions)?;
            self.activate_dynamic_regions(&mut region.mounted.dynamics)?;
        }
        Ok(())
    }

    fn activate_dynamic_regions(&self, regions: &mut [DynamicRegion]) -> Result<(), DomError> {
        for region in regions {
            self.activate_dynamic_region(region)?;
        }
        Ok(())
    }

    fn activate_dynamic_region(&self, region: &mut DynamicRegion) -> Result<(), DomError> {
        if region.effect.is_some() {
            return Ok(());
        }

        {
            let mut state = region.state.borrow_mut();
            if let Some(state) = state.as_mut() {
                if let Some(parent) = state.mounted.start.parent_node() {
                    self.activate_delegated_listeners(&mut state.mounted, &parent)?;
                }
                self.activate_component_regions(&mut state.mounted.component_regions)?;
                self.activate_dynamic_regions(&mut state.mounted.dynamics)?;
            }
        }

        let dynamic = region.dynamic.clone();
        let scope = region.scope.clone();
        let effect_scope = scope.clone();
        let state = region.state.clone();
        let document = self.document.clone();
        let runtime = self.runtime.clone();
        let registry = self.registry.clone();
        let error_sink = region.error_sink.clone();
        let first_run = Rc::new(Cell::new(true));

        let effect = scope.effect(move || {
            let next_view = match dynamic.evaluate() {
                Ok(view) => view,
                Err(error) => {
                    let error = DomError::from(error);
                    web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                    if let Some(sink) = &error_sink {
                        *sink.borrow_mut() = Some(error);
                    }
                    return;
                }
            };

            // The synchronous builder already mounted this initial view.
            // This first run only collects dependencies for this region.
            if first_run.replace(false) {
                return;
            }

            let mut current = state.borrow_mut();
            let Some(current) = current.as_mut() else {
                return;
            };
            let previous_view = current.view.clone();
            let mut renderer =
                DomRenderer::new(document.clone(), runtime.clone(), registry.clone());
            renderer.root_scope = effect_scope.clone();
            renderer.error_sink = error_sink.clone();
            match renderer.update(&mut current.mounted, &previous_view, &next_view) {
                Ok(()) => {
                    current.view = next_view;
                    if let Some(sink) = &error_sink {
                        *sink.borrow_mut() = None;
                    }
                }
                Err(error) => {
                    web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                    if let Some(sink) = &error_sink {
                        *sink.borrow_mut() = Some(error);
                    }
                }
            }
        })?;
        region.effect = Some(effect);
        Ok(())
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
                PropertyValue::Callback(_) => {
                    return Err(DomError::PropertyRejected(property.name().to_owned()));
                }
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
            if event_uses_delegation(binding.name()) {
                resources.delegated.push(DelegatedBinding {
                    element: element.clone(),
                    name: binding.name().to_owned(),
                    handler: binding.handler(),
                });
            } else {
                resources
                    .listeners
                    .push(create_event_listener(&event_target, binding)?);
            }
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
        let scope = parent_scope.child()?;
        // Initial evaluation is deliberately untracked. The region's own effect,
        // activated after the whole mount is attached, owns future dependencies.
        let view = match untrack(|| dynamic.evaluate()) {
            Ok(view) => view,
            Err(error) => {
                let _ = scope.dispose();
                return Err(error.into());
            }
        };
        let start = self.document.create_comment("rustflow:dynamic:start");
        let end = self.document.create_comment("rustflow:dynamic:end");
        let mut inner = RenderedNodes::default();
        let roots = match self.build_nodes(&view, &scope, &mut inner) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = cleanup_rendered_nodes(&mut inner);
                let _ = scope.dispose();
                return Err(error);
            }
        };
        let mounted = MountedDom {
            start: start.clone(),
            end: end.clone(),
            roots: roots.clone(),
            listeners: inner.listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(inner.delegated)),
            components: inner.components,
            component_regions: inner.component_regions,
            dynamics: inner.dynamics,
        };
        let state = Rc::new(RefCell::new(Some(DynamicMountState { mounted, view })));
        resources.dynamics.push(DynamicRegion {
            dynamic: dynamic.clone(),
            scope,
            state,
            error_sink: self.error_sink.clone(),
            effect: None,
            reused: false,
        });
        let mut nodes = Vec::with_capacity(roots.len() + 2);
        let start_node: Node = start.unchecked_into();
        let end_node: Node = end.unchecked_into();
        nodes.push(start_node);
        nodes.extend(roots);
        nodes.push(end_node);
        Ok(nodes)
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
        let start = self.document.create_comment("rustflow:component:start");
        let end = self.document.create_comment("rustflow:component:end");
        let mut inner = RenderedNodes::default();
        let roots = match self.build_nodes(&view, instance.scope(), &mut inner) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = cleanup_rendered_nodes(&mut inner);
                let _ = instance.unmount();
                return Err(error);
            }
        };
        let mounted = MountedDom {
            start: start.clone(),
            end: end.clone(),
            roots: roots.clone(),
            listeners: inner.listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(inner.delegated)),
            components: inner.components,
            component_regions: inner.component_regions,
            dynamics: inner.dynamics,
        };
        resources.component_regions.push(ComponentRegion {
            descriptor: component.clone(),
            instance,
            mounted,
            view,
            reused: false,
        });
        let mut nodes = Vec::with_capacity(roots.len() + 2);
        let start_node: Node = start.unchecked_into();
        let end_node: Node = end.unchecked_into();
        nodes.push(start_node);
        nodes.extend(roots);
        nodes.push(end_node);
        Ok(nodes)
    }
    fn replace_range(&mut self, mounted: &mut MountedDom, next: &View) -> Result<(), DomError> {
        let mut resources = RenderedNodes::default();
        let next_roots = match self.build_nodes(next, &self.root_scope, &mut resources) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = cleanup_rendered_nodes(&mut resources);
                return Err(error);
            }
        };

        let start: Node = mounted.start.clone().unchecked_into();
        let end: Node = mounted.end.clone().unchecked_into();
        let old_nodes = nodes_between(&start, &end);
        if let Some(parent) = mounted.start.parent_node() {
            let end_node: Node = mounted.end.clone().unchecked_into();
            let mut inserted = Vec::new();
            for node in &next_roots {
                if let Err(error) = parent.insert_before(node, Some(&end_node)) {
                    for inserted_node in inserted {
                        let _ = parent.remove_child(&inserted_node);
                    }
                    let _ = cleanup_rendered_nodes(&mut resources);
                    return Err(js_error(error));
                }
                inserted.push(node.clone());
            }
            for old_node in &old_nodes {
                if let Err(error) = parent.remove_child(old_node) {
                    for inserted_node in inserted {
                        let _ = parent.remove_child(&inserted_node);
                    }
                    let _ = cleanup_rendered_nodes(&mut resources);
                    return Err(js_error(error));
                }
            }
        }

        cleanup_mounted_content(mounted)?;
        mounted.roots = next_roots;
        mounted.listeners = resources.listeners;
        mounted.components = resources.components;
        mounted.component_regions = resources.component_regions;
        mounted.dynamics = resources.dynamics;
        *mounted.delegated_bindings.borrow_mut() = resources.delegated;
        if let Some(parent) = mounted.start.parent_node() {
            self.activate_delegated_listeners(mounted, &parent)?;
        }
        self.activate_component_regions(&mut mounted.component_regions)?;
        self.activate_dynamic_regions(&mut mounted.dynamics)?;
        Ok(())
    }

    fn reconcile_view(
        &self,
        previous: &View,
        next: &View,
        existing: &[Node],
        parent: &Node,
        before: Option<&Node>,
        owner_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        match (previous, next) {
            (View::Text(old_text), View::Text(new_text)) if existing.len() == 1 => {
                if old_text != new_text {
                    existing[0].set_node_value(Some(new_text));
                }
                Ok(vec![existing[0].clone()])
            }
            (View::Element(old_element), View::Element(new_element))
                if old_element.tag().eq_ignore_ascii_case(new_element.tag())
                    && existing.len() == 1 =>
            {
                let dom_element: DomElement = existing[0].clone().dyn_into().map_err(|_| {
                    DomError::JavaScript("expected an element node while reconciling".into())
                })?;
                self.patch_element(old_element, new_element, &dom_element, resources)?;
                let parent_node: Node = dom_element.clone().unchecked_into();
                let current_children = direct_children(&parent_node);
                self.reconcile_sequence(
                    old_element.children(),
                    new_element.children(),
                    &current_children,
                    &parent_node,
                    None,
                    owner_scope,
                    resources,
                )?;
                Ok(vec![existing[0].clone()])
            }
            (View::Fragment(old_fragment), View::Fragment(new_fragment)) => self
                .reconcile_sequence(
                    old_fragment.children(),
                    new_fragment.children(),
                    existing,
                    parent,
                    before,
                    owner_scope,
                    resources,
                ),
            (View::KeyedList(old_list), View::KeyedList(new_list)) => self.reconcile_keyed(
                old_list,
                new_list,
                existing,
                parent,
                before,
                owner_scope,
                resources,
            ),
            _ => self.build_nodes(next, owner_scope, resources),
        }
    }

    fn patch_element(
        &self,
        previous: &Element,
        next: &Element,
        element: &DomElement,
        resources: &mut RenderedNodes,
    ) -> Result<(), DomError> {
        for old_attribute in previous.attributes() {
            if !next
                .attributes()
                .iter()
                .any(|attribute| attribute.name() == old_attribute.name())
            {
                element
                    .remove_attribute(old_attribute.name())
                    .map_err(js_error)?;
            }
        }

        for attribute in next.attributes() {
            match attribute.value() {
                AttributeValue::Boolean(true) => {
                    validate_attribute(attribute.name(), "")?;
                    element
                        .set_attribute(attribute.name(), "")
                        .map_err(js_error)?;
                }
                AttributeValue::Boolean(false) => {
                    validate_attribute(attribute.name(), "")?;
                    element
                        .remove_attribute(attribute.name())
                        .map_err(js_error)?;
                }
                AttributeValue::Text(value) => {
                    validate_attribute(attribute.name(), value)?;
                    element
                        .set_attribute(attribute.name(), value)
                        .map_err(js_error)?;
                }
            }
        }

        for old_property in previous.properties() {
            if !next
                .properties()
                .iter()
                .any(|property| property.name() == old_property.name())
            {
                let default = default_property_value(old_property.name());
                let target: &JsValue = element.as_ref();
                Reflect::set(target, &JsValue::from_str(old_property.name()), &default)
                    .map_err(js_error)?;
            }
        }
        for property in next.properties() {
            validate_property(property.name(), property.value())?;
            let value = match property.value() {
                PropertyValue::Text(value) => JsValue::from_str(value),
                PropertyValue::Boolean(value) => JsValue::from_bool(*value),
                PropertyValue::Integer(value) => JsValue::from_f64(*value as f64),
                PropertyValue::Float(value) => JsValue::from_f64(*value),
                PropertyValue::Callback(_) => {
                    return Err(DomError::PropertyRejected(property.name().to_owned()));
                }
            };
            let target: &JsValue = element.as_ref();
            let accepted = Reflect::set(target, &JsValue::from_str(property.name()), &value)
                .map_err(js_error)?;
            if !accepted {
                return Err(DomError::PropertyRejected(property.name().to_owned()));
            }
        }

        let target: EventTarget = element.clone().unchecked_into();
        for binding in next.events() {
            if event_uses_delegation(binding.name()) {
                resources.delegated.push(DelegatedBinding {
                    element: element.clone(),
                    name: binding.name().to_owned(),
                    handler: binding.handler(),
                });
            } else {
                resources
                    .listeners
                    .push(create_event_listener(&target, binding)?);
            }
        }
        Ok(())
    }

    fn reconcile_sequence(
        &self,
        previous: &[View],
        next: &[View],
        existing_nodes: &[Node],
        parent: &Node,
        before: Option<&Node>,
        owner_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        let mut ranges = Vec::with_capacity(previous.len());
        let mut cursor = 0usize;
        for view in previous {
            let count = tree_node_count(view).ok_or_else(|| {
                DomError::JavaScript(
                    "cannot reconcile a dynamic/component subtree without an owned DOM range"
                        .into(),
                )
            })?;
            let end = cursor.saturating_add(count);
            if end > existing_nodes.len() {
                return Err(DomError::JavaScript(
                    "mounted DOM range does not match the previous view".into(),
                ));
            }
            ranges.push((cursor, end));
            cursor = end;
        }
        if cursor != existing_nodes.len() {
            return Err(DomError::JavaScript(
                "mounted DOM range has unexpected nodes".into(),
            ));
        }

        let mut used = vec![false; previous.len()];
        let mut result = Vec::new();
        for (next_index, next_view) in next.iter().enumerate() {
            let next_key = view_key(next_view);
            let matched = if let Some(key) = next_key {
                previous.iter().enumerate().find_map(|(index, candidate)| {
                    (!used[index] && view_key(candidate) == Some(key)).then_some(index)
                })
            } else if next_index < previous.len()
                && !used[next_index]
                && view_key(&previous[next_index]).is_none()
            {
                Some(next_index)
            } else {
                previous.iter().enumerate().find_map(|(index, candidate)| {
                    (!used[index] && view_key(candidate).is_none()).then_some(index)
                })
            };

            if let Some(previous_index) = matched {
                used[previous_index] = true;
                let (start, end) = ranges[previous_index];
                result.extend(self.reconcile_view(
                    &previous[previous_index],
                    next_view,
                    &existing_nodes[start..end],
                    parent,
                    before,
                    owner_scope,
                    resources,
                )?);
            } else {
                result.extend(self.build_nodes(next_view, owner_scope, resources)?);
            }
        }

        remove_obsolete_nodes(parent, existing_nodes, &result)?;
        for node in &result {
            parent.insert_before(node, before).map_err(js_error)?;
        }
        Ok(result)
    }

    fn reconcile_keyed(
        &self,
        previous: &KeyedList,
        next: &KeyedList,
        existing_nodes: &[Node],
        parent: &Node,
        before: Option<&Node>,
        owner_scope: &Scope,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        let old_children = previous.children();
        let new_children = next.children();
        let mut ranges = Vec::with_capacity(old_children.len());
        let mut cursor = 0usize;
        for child in old_children {
            let count = tree_node_count(child.view()).ok_or_else(|| {
                DomError::JavaScript(
                    "keyed reconciliation cannot reuse a dynamic/component subtree yet".into(),
                )
            })?;
            let end = cursor.saturating_add(count);
            if end > existing_nodes.len() {
                return Err(DomError::JavaScript(
                    "keyed list DOM range does not match its prior snapshot".into(),
                ));
            }
            ranges.push((cursor, end));
            cursor = end;
        }
        if cursor != existing_nodes.len() {
            return Err(DomError::JavaScript(
                "keyed list has unexpected DOM nodes".into(),
            ));
        }

        let mut used = vec![false; old_children.len()];
        let mut result = Vec::new();
        for child in new_children {
            let matched = old_children
                .iter()
                .enumerate()
                .find_map(|(index, candidate)| {
                    (!used[index] && candidate.key() == child.key()).then_some(index)
                });
            if let Some(previous_index) = matched {
                used[previous_index] = true;
                let (start, end) = ranges[previous_index];
                result.extend(self.reconcile_view(
                    old_children[previous_index].view(),
                    child.view(),
                    &existing_nodes[start..end],
                    parent,
                    before,
                    owner_scope,
                    resources,
                )?);
            } else {
                result.extend(self.build_nodes(child.view(), owner_scope, resources)?);
            }
        }

        remove_obsolete_nodes(parent, existing_nodes, &result)?;
        for node in &result {
            parent.insert_before(node, before).map_err(js_error)?;
        }
        Ok(result)
    }
}

fn can_reconcile(view: &View) -> bool {
    match view {
        View::Text(_) => true,
        View::Element(element) => element.children().iter().all(can_reconcile),
        View::Fragment(fragment) => fragment.children().iter().all(can_reconcile),
        View::KeyedList(list) => list
            .children()
            .iter()
            .all(|child| can_reconcile(child.view())),
        View::Dynamic(_) | View::Component(_) => false,
    }
}

fn tree_node_count(view: &View) -> Option<usize> {
    match view {
        View::Text(_) | View::Element(_) => Some(1),
        View::Fragment(fragment) => fragment.children().iter().try_fold(0usize, |total, child| {
            total.checked_add(tree_node_count(child)?)
        }),
        View::KeyedList(list) => list.children().iter().try_fold(0usize, |total, child| {
            total.checked_add(tree_node_count(child.view())?)
        }),
        View::Dynamic(_) | View::Component(_) => None,
    }
}

fn view_key(view: &View) -> Option<&Key> {
    match view {
        View::Element(element) => element.key(),
        View::Component(component) => component.key(),
        _ => None,
    }
}

fn direct_children(parent: &Node) -> Vec<Node> {
    let mut result = Vec::new();
    let mut current = parent.first_child();
    while let Some(node) = current {
        current = node.next_sibling();
        result.push(node);
    }
    result
}

fn remove_obsolete_nodes(parent: &Node, previous: &[Node], next: &[Node]) -> Result<(), DomError> {
    for old_node in previous {
        let retained = next
            .iter()
            .any(|new_node| new_node.is_same_node(Some(old_node)));
        if !retained
            && old_node
                .parent_node()
                .is_some_and(|actual_parent| actual_parent.is_same_node(Some(parent)))
        {
            parent.remove_child(old_node).map_err(js_error)?;
        }
    }
    Ok(())
}

fn default_property_value(name: &str) -> JsValue {
    match name.to_ascii_lowercase().as_str() {
        "value" | "defaultvalue" => JsValue::from_str(""),
        "checked" | "selected" | "disabled" | "multiple" | "required" | "readonly"
        | "autofocus" | "hidden" | "muted" => JsValue::from_bool(false),
        _ => JsValue::UNDEFINED,
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
                let _ = cleanup_rendered_nodes(&mut resources);
                return Err(error);
            }
        };
        Ok(MountedDom {
            start,
            end,
            roots,
            listeners: resources.listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(resources.delegated)),
            components: resources.components,
            component_regions: resources.component_regions,
            dynamics: resources.dynamics,
        })
    }

    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        previous: &View,
        next: &View,
    ) -> Result<(), Self::Error> {
        // Opaque component descriptors currently use safe range replacement.
        // Dynamic nodes have their own child effects/ranges and are not read by
        // this root effect; static trees can therefore be reconciled in place.
        if can_reconcile(previous) && can_reconcile(next) {
            let mut validation = RenderedNodes::default();
            if let Err(error) = self.build_nodes(next, &self.root_scope, &mut validation) {
                let _ = cleanup_rendered_nodes(&mut validation);
                return Err(error);
            }
            cleanup_rendered_nodes(&mut validation)?;

            let Some(parent) = mounted.start.parent_node() else {
                return self.replace_range(mounted, next);
            };
            let end: Node = mounted.end.clone().unchecked_into();
            let mut resources = RenderedNodes::default();
            let roots = match self.reconcile_sequence(
                std::slice::from_ref(previous),
                std::slice::from_ref(next),
                &mounted.roots,
                &parent,
                Some(&end),
                &self.root_scope.clone(),
                &mut resources,
            ) {
                Ok(nodes) => nodes,
                Err(error) => {
                    let _ = cleanup_rendered_nodes(&mut resources);
                    return Err(error);
                }
            };

            let cleanup_error = cleanup_mounted_content(mounted).err();
            mounted.roots = roots;
            mounted.listeners = resources.listeners;
            mounted.components = resources.components;
            mounted.dynamics = resources.dynamics;
            *mounted.delegated_bindings.borrow_mut() = resources.delegated;
            if let Some(error) = cleanup_error {
                return Err(error);
            }
            self.activate_delegated_listeners(mounted, &parent)?;
            self.activate_dynamic_regions(&mut mounted.dynamics)
        } else {
            self.replace_range(mounted, next)
        }
    }

    fn unmount(&mut self, mut mounted: Self::Mounted) -> Result<(), Self::Error> {
        let mut first_error = detach_range(&mut mounted).err();
        if let Err(error) = cleanup_mounted_resources(&mut mounted) {
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
        let _ = cleanup_mounted_resources(self);
    }
}

fn nodes_between(start: &Node, end: &Node) -> Vec<Node> {
    let mut nodes = Vec::new();
    if !start.parent_node().is_some_and(|parent| {
        end.parent_node()
            .is_some_and(|end_parent| parent.is_same_node(Some(&end_parent)))
    }) {
        return nodes;
    }
    let mut current = start.next_sibling();
    while let Some(node) = current {
        if node.is_same_node(Some(end)) {
            break;
        }
        current = node.next_sibling();
        nodes.push(node);
    }
    nodes
}

fn first_error(current: &mut Option<DomError>, next: Result<(), DomError>) {
    if let Err(error) = next {
        if current.is_none() {
            *current = Some(error);
        }
    }
}

fn cleanup_rendered_nodes(resources: &mut RenderedNodes) -> Result<(), DomError> {
    let mut error = None;
    first_error(
        &mut error,
        cleanup_resources(&mut resources.listeners, &mut resources.components),
    );
    first_error(&mut error, cleanup_dynamic_regions(&mut resources.dynamics));
    first_error(
        &mut error,
        cleanup_component_regions(&mut resources.component_regions),
    );
    resources.delegated.clear();
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_dynamic_regions(regions: &mut Vec<DynamicRegion>) -> Result<(), DomError> {
    let mut error = None;
    for region in regions.drain(..).rev() {
        first_error(&mut error, region.scope.dispose().map_err(Into::into));
        if let Some(mut state) = region.state.borrow_mut().take() {
            first_error(&mut error, detach_range(&mut state.mounted));
            first_error(&mut error, cleanup_mounted_resources(&mut state.mounted));
        }
    }
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_mounted_non_component_content(mounted: &mut MountedDom) -> Result<(), DomError> {
    let mut error = None;
    first_error(
        &mut error,
        cleanup_resources(&mut mounted.listeners, &mut mounted.components),
    );
    first_error(&mut error, cleanup_dynamic_regions(&mut mounted.dynamics));
    mounted.delegated_bindings.borrow_mut().clear();
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_mounted_content(mounted: &mut MountedDom) -> Result<(), DomError> {
    let mut error = cleanup_mounted_non_component_content(mounted).err();
    first_error(
        &mut error,
        cleanup_component_regions(&mut mounted.component_regions),
    );
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_component_regions(regions: &mut Vec<ComponentRegion>) -> Result<(), DomError> {
    let mut error = None;
    for mut region in regions.drain(..).rev() {
        first_error(&mut error, cleanup_component_region(&mut region));
    }
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_component_region(region: &mut ComponentRegion) -> Result<(), DomError> {
    let mut error = None;
    first_error(&mut error, region.instance.unmount().map_err(Into::into));
    first_error(&mut error, detach_range(&mut region.mounted));
    first_error(&mut error, cleanup_mounted_resources(&mut region.mounted));
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_mounted_resources(mounted: &mut MountedDom) -> Result<(), DomError> {
    let mut error = None;
    first_error(&mut error, cleanup_mounted_content(mounted));
    first_error(
        &mut error,
        cleanup_event_listeners(&mut mounted.delegated_listeners),
    );
    match error {
        Some(error) => Err(error),
        None => Ok(()),
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
    let start: Node = mounted.start.clone().unchecked_into();
    let end: Node = mounted.end.clone().unchecked_into();
    let Some(parent) = start.parent_node() else {
        return Ok(());
    };
    let mut first_error = None;
    // Root arrays are snapshots: a dynamic region may have changed its child
    // count since mount. Remove the live DOM range rather than stale node handles.
    for node in nodes_between(&start, &end) {
        if let Err(error) = parent.remove_child(&node) {
            if first_error.is_none() {
                first_error = Some(js_error(error));
            }
        }
    }
    if let Err(error) = parent.remove_child(&start) {
        if first_error.is_none() {
            first_error = Some(js_error(error));
        }
    }
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

fn cleanup_event_listeners(listeners: &mut Vec<EventListener>) -> Result<(), DomError> {
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
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_resources(
    listeners: &mut Vec<EventListener>,
    components: &mut Vec<ComponentInstance>,
) -> Result<(), DomError> {
    let mut first_error = cleanup_event_listeners(listeners).err();
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

fn event_uses_delegation(name: &str) -> bool {
    matches!(
        name,
        "click"
            | "dblclick"
            | "input"
            | "change"
            | "submit"
            | "keydown"
            | "keyup"
            | "keypress"
            | "pointerdown"
            | "pointerup"
            | "pointermove"
            | "mousedown"
            | "mouseup"
            | "mouseover"
            | "mouseout"
            | "contextmenu"
            | "focusin"
            | "focusout"
            | "touchstart"
            | "touchend"
            | "dragstart"
            | "dragend"
            | "drop"
    )
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
