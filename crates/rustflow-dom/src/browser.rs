mod navigation;
pub use navigation::ClientNavigation;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    error::Error,
    fmt,
    rc::Rc,
};

use js_sys::Reflect;
use rustflow::{
    components::{ComponentError, ComponentInstance, ComponentRegistry, RenderTarget},
    reactivity::{Effect, Runtime, RuntimeError, Scope, untrack},
};
use rustflow_view::{
    AttributeValue, ComponentNode, DynamicChild, Element, EventBinding, EventHandler, EventPayload,
    Key, KeyedList, PropertyValue, Renderer, View, ViewError,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    Attr, Comment, Document, Element as DomElement, Event, EventTarget, HtmlInputElement,
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
    HydrationMismatch { expected: String, found: String },
    AlreadyHydrated,
    InvalidHydrationData,
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
            Self::HydrationMismatch { expected, found } => {
                write!(f, "hydration mismatch: expected {expected}, found {found}")
            }
            Self::AlreadyHydrated => write!(f, "this SSR boundary has already been hydrated"),
            Self::InvalidHydrationData => write!(f, "invalid JSON in #rustflow-hydration-data"),
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

struct PendingEventListener {
    target: EventTarget,
    name: String,
    handler: EventHandler,
}

#[derive(Default)]
struct HydrationCursor {
    next_boundary: usize,
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
    pending_listeners: Vec<PendingEventListener>,
    delegated: Vec<DelegatedBinding>,
    components: Vec<ComponentInstance>,
    component_regions: Vec<ComponentRegion>,
    dynamics: Vec<DynamicRegion>,
}

fn append_rendered_nodes(target: &mut RenderedNodes, mut source: RenderedNodes) {
    target.listeners.append(&mut source.listeners);
    target
        .pending_listeners
        .append(&mut source.pending_listeners);
    target.delegated.append(&mut source.delegated);
    target.components.append(&mut source.components);
    target
        .component_regions
        .append(&mut source.component_regions);
    target.dynamics.append(&mut source.dynamics);
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

/// Detached replacement nodes are built during diff/evaluation and transferred
/// into the mounted tree only during commit. Dropping an uncommitted plan disposes
/// any component scopes, dynamic scopes, and listeners created while preparing it.
struct PreparedReplacement {
    nodes: Vec<Node>,
    resources: Option<RenderedNodes>,
}

impl PreparedReplacement {
    fn new(nodes: Vec<Node>, resources: RenderedNodes) -> Self {
        Self {
            nodes,
            resources: Some(resources),
        }
    }

    fn take(mut self) -> (Vec<Node>, RenderedNodes) {
        (
            std::mem::take(&mut self.nodes),
            self.resources.take().unwrap_or_default(),
        )
    }
}

impl Drop for PreparedReplacement {
    fn drop(&mut self) {
        if let Some(mut resources) = self.resources.take() {
            let _ = cleanup_rendered_nodes(&mut resources);
        }
    }
}

/// Immutable output of the view-diff/evaluation phase. The commit phase consumes
/// this plan and applies its prebuilt replacements and compatible-node patches.
struct ReconciliationPlan {
    previous: View,
    next: View,
    action: ReconciliationAction,
}

enum ReconciliationAction {
    Replace(PreparedReplacement),
    Text,
    Element(Vec<PlannedChild>),
    Fragment(Vec<PlannedChild>),
    Keyed(Vec<PlannedChild>),
    Component {
        region_index: usize,
        next_view: View,
        child_plan: Box<ReconciliationPlan>,
    },
    Dynamic {
        region_index: usize,
        next_view: View,
        child_plan: Box<ReconciliationPlan>,
    },
}

struct PlannedChild {
    previous_index: Option<usize>,
    plan: Box<ReconciliationPlan>,
}

struct DiffContext<'a> {
    component_regions: &'a [ComponentRegion],
    dynamic_regions: &'a [DynamicRegion],
    used_components: Vec<bool>,
    used_dynamics: Vec<bool>,
}

impl<'a> DiffContext<'a> {
    fn new(component_regions: &'a [ComponentRegion], dynamic_regions: &'a [DynamicRegion]) -> Self {
        Self {
            component_regions,
            dynamic_regions,
            used_components: vec![false; component_regions.len()],
            used_dynamics: vec![false; dynamic_regions.len()],
        }
    }

    fn take_component(&mut self, descriptor: &ComponentNode) -> Option<usize> {
        let index = self
            .component_regions
            .iter()
            .enumerate()
            .find_map(|(index, region)| {
                (!self.used_components[index] && region.descriptor == *descriptor).then_some(index)
            })?;
        self.used_components[index] = true;
        Some(index)
    }

    fn take_dynamic(&mut self, dynamic: &DynamicChild) -> Option<usize> {
        let index = self
            .dynamic_regions
            .iter()
            .enumerate()
            .find_map(|(index, region)| {
                (!self.used_dynamics[index] && region.dynamic.id() == dynamic.id()).then_some(index)
            })?;
        self.used_dynamics[index] = true;
        Some(index)
    }
}

/// A mounted DOM range. Comment anchors make fragment roots updateable without
/// introducing a visible wrapper element.
pub struct MountedDom {
    start: Comment,
    end: Comment,
    roots: Vec<Node>,
    listeners: Vec<EventListener>,
    pending_listeners: Vec<PendingEventListener>,
    delegated_listeners: Vec<EventListener>,
    delegated_bindings: Rc<RefCell<Vec<DelegatedBinding>>>,
    components: Vec<ComponentInstance>,
    component_regions: Vec<ComponentRegion>,
    dynamics: Vec<DynamicRegion>,
    // While a hydration candidate is being validated, dropping it must clean up
    // temporary Rust resources without detaching or mutating the server DOM.
    preserve_on_drop: bool,
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

    /// Validate an SSR DOM range and attach client behavior without rebuilding it.
    ///
    /// The input must have been rendered with hydration boundaries. Validation completes before
    /// event listeners/effects activate. A mismatch reports an error and preserves server HTML.
    pub fn hydrate_into(
        &mut self,
        target: &DomElement,
        view: &View,
    ) -> Result<MountedDom, DomError> {
        let parent: Node = target.clone().unchecked_into();
        let siblings = direct_children(&parent);
        if siblings
            .iter()
            .any(|node| is_comment_value(node, "rustflow:root:hydrated:start"))
        {
            return Err(DomError::AlreadyHydrated);
        }
        let starts = marker_positions(&siblings, "rustflow:root:start");
        let ends = marker_positions(&siblings, "rustflow:root:end");
        if starts.len() != 1 || ends.len() != 1 || starts[0] >= ends[0] {
            return Err(DomError::HydrationMismatch {
                expected: "one ordered rustflow root boundary".to_owned(),
                found: "missing, duplicated, or reversed root comments".to_owned(),
            });
        }

        let start_index = starts[0];
        let end_index = ends[0];
        let start_comment = siblings[start_index]
            .clone()
            .dyn_into::<Comment>()
            .map_err(|_| hydration_mismatch("root start comment", Some(&siblings[start_index])))?;
        let end_comment = siblings[end_index]
            .clone()
            .dyn_into::<Comment>()
            .map_err(|_| hydration_mismatch("root end comment", Some(&siblings[end_index])))?;
        let root_children = siblings[start_index + 1..end_index].to_vec();

        let mut cursor = HydrationCursor::default();
        let mut child_index = 0;
        let mut resources = RenderedNodes::default();
        let validation = self.hydrate_view(
            view,
            &root_children,
            &mut child_index,
            &self.root_scope,
            &mut cursor,
            &mut resources,
        );
        if let Err(error) = validation {
            let _ = cleanup_rendered_nodes(&mut resources);
            return Err(error);
        }
        if child_index != root_children.len() {
            let error =
                hydration_mismatch("end of the SSR root range", root_children.get(child_index));
            let _ = cleanup_rendered_nodes(&mut resources);
            return Err(error);
        }

        let mut mounted = MountedDom {
            start: start_comment,
            end: end_comment,
            roots: root_children,
            listeners: resources.listeners,
            pending_listeners: resources.pending_listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(resources.delegated)),
            components: resources.components,
            component_regions: resources.component_regions,
            dynamics: resources.dynamics,
            preserve_on_drop: true,
        };

        // If activation fails, Drop cleans up temporary resources but preserves the SSR DOM.
        self.activate_pending_listeners(&mut mounted)?;
        self.activate_delegated_listeners(&mut mounted, &parent)?;
        self.activate_component_regions(&mut mounted.component_regions)?;
        self.activate_dynamic_regions(&mut mounted.dynamics)?;

        let start_node: Node = mounted.start.clone().unchecked_into();
        start_node.set_node_value(Some("rustflow:root:hydrated:start"));
        set_preserve_on_drop(&mut mounted, false);
        Ok(mounted)
    }

    /// Parse the optional inert JSON payload emitted by the server page helpers.
    /// Callers choose how to project this public value into the initial client state.
    pub fn read_hydration_data(document: &Document) -> Result<Option<JsValue>, DomError> {
        let Some(element) = document.get_element_by_id("rustflow-hydration-data") else {
            return Ok(None);
        };
        let text = element.text_content().unwrap_or_default();
        js_sys::JSON::parse(&text)
            .map(Some)
            .map_err(|_| DomError::InvalidHydrationData)
    }

    fn hydrate_view(
        &self,
        view: &View,
        siblings: &[Node],
        index: &mut usize,
        parent_scope: &Scope,
        cursor: &mut HydrationCursor,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        match view {
            View::Text(text) => {
                let node = siblings
                    .get(*index)
                    .ok_or_else(|| hydration_mismatch("matching text node", None))?;
                if node.node_type() != 3 {
                    return Err(hydration_mismatch("matching text node", Some(node)));
                }
                if node.node_value().as_deref() != Some(text.as_str()) {
                    return Err(DomError::HydrationMismatch {
                        expected: "matching initial text".to_owned(),
                        found: "different text content".to_owned(),
                    });
                }
                *index += 1;
                Ok(vec![node.clone()])
            }
            View::Element(expected) => {
                let node = siblings.get(*index).ok_or_else(|| {
                    hydration_mismatch(
                        &format!("<{}> element", expected.tag().to_ascii_lowercase()),
                        None,
                    )
                })?;
                let element: DomElement = node.clone().dyn_into().map_err(|_| {
                    hydration_mismatch(
                        &format!("<{}> element", expected.tag().to_ascii_lowercase()),
                        Some(node),
                    )
                })?;
                let tag = expected.tag().to_ascii_lowercase();
                if element.tag_name().to_ascii_lowercase() != tag {
                    return Err(hydration_mismatch(&format!("<{tag}> element"), Some(node)));
                }
                validate_hydrated_element(expected, &element)?;

                let child_nodes = direct_children(node);
                let mut child_index = 0;
                let textarea_has_value = tag == "textarea"
                    && expected
                        .properties()
                        .iter()
                        .any(|property| property.name().eq_ignore_ascii_case("value"));
                if textarea_has_value {
                    child_index = child_nodes.len();
                } else {
                    for child in expected.children() {
                        self.hydrate_view(
                            child,
                            &child_nodes,
                            &mut child_index,
                            parent_scope,
                            cursor,
                            resources,
                        )?;
                    }
                }
                if child_index != child_nodes.len() {
                    return Err(hydration_mismatch(
                        "no additional children in the SSR element",
                        child_nodes.get(child_index),
                    ));
                }

                let target: EventTarget = element.clone().unchecked_into();
                for binding in expected.events() {
                    if event_uses_delegation(binding.name()) {
                        resources.delegated.push(DelegatedBinding {
                            element: element.clone(),
                            name: binding.name().to_owned(),
                            handler: binding.handler(),
                        });
                    } else {
                        resources.pending_listeners.push(PendingEventListener {
                            target: target.clone(),
                            name: binding.name().to_owned(),
                            handler: binding.handler(),
                        });
                    }
                }
                *index += 1;
                Ok(vec![node.clone()])
            }
            View::Fragment(fragment) => {
                let mut nodes = Vec::new();
                for child in fragment.children() {
                    nodes.extend(self.hydrate_view(
                        child,
                        siblings,
                        index,
                        parent_scope,
                        cursor,
                        resources,
                    )?);
                }
                Ok(nodes)
            }
            View::KeyedList(list) => {
                let mut nodes = Vec::new();
                for child in list.children() {
                    nodes.extend(self.hydrate_view(
                        child.view(),
                        siblings,
                        index,
                        parent_scope,
                        cursor,
                        resources,
                    )?);
                }
                Ok(nodes)
            }
            View::Dynamic(dynamic) => {
                let boundary_id = cursor.next_boundary;
                cursor.next_boundary = cursor.next_boundary.saturating_add(1);
                let start_index = *index;
                let start_marker = format!("rustflow:dynamic:start:{boundary_id}");
                let start_node = siblings
                    .get(start_index)
                    .ok_or_else(|| hydration_mismatch(&format!("comment {start_marker}"), None))?;
                if !is_comment_value(start_node, &start_marker) {
                    return Err(hydration_mismatch(
                        &format!("comment {start_marker}"),
                        Some(start_node),
                    ));
                }
                let end_index = find_boundary_end(siblings, start_index, "dynamic", boundary_id)?;
                let start = start_node.clone().dyn_into::<Comment>().map_err(|_| {
                    hydration_mismatch(&format!("comment {start_marker}"), Some(start_node))
                })?;
                let end = siblings[end_index]
                    .clone()
                    .dyn_into::<Comment>()
                    .map_err(|_| {
                        hydration_mismatch(
                            &format!("comment rustflow:dynamic:end:{boundary_id}"),
                            Some(&siblings[end_index]),
                        )
                    })?;
                let scope = parent_scope.child()?;
                let child = match untrack(|| dynamic.evaluate()) {
                    Ok(child) => child,
                    Err(error) => {
                        let _ = scope.dispose();
                        return Err(error.into());
                    }
                };
                let child_nodes = &siblings[start_index + 1..end_index];
                let mut inner = RenderedNodes::default();
                let mut child_index = 0;
                if let Err(error) = self.hydrate_view(
                    &child,
                    child_nodes,
                    &mut child_index,
                    &scope,
                    cursor,
                    &mut inner,
                ) {
                    let _ = cleanup_rendered_nodes(&mut inner);
                    let _ = scope.dispose();
                    return Err(error);
                }
                if child_index != child_nodes.len() {
                    let error = hydration_mismatch(
                        "end of the dynamic SSR boundary",
                        child_nodes.get(child_index),
                    );
                    let _ = cleanup_rendered_nodes(&mut inner);
                    let _ = scope.dispose();
                    return Err(error);
                }
                let mounted_inner = MountedDom {
                    start,
                    end,
                    roots: child_nodes.to_vec(),
                    listeners: inner.listeners,
                    pending_listeners: inner.pending_listeners,
                    delegated_listeners: Vec::new(),
                    delegated_bindings: Rc::new(RefCell::new(inner.delegated)),
                    components: inner.components,
                    component_regions: inner.component_regions,
                    dynamics: inner.dynamics,
                    preserve_on_drop: true,
                };
                resources.dynamics.push(DynamicRegion {
                    dynamic: dynamic.clone(),
                    scope,
                    state: Rc::new(RefCell::new(Some(DynamicMountState {
                        mounted: mounted_inner,
                        view: child,
                    }))),
                    error_sink: self.error_sink.clone(),
                    effect: None,
                    reused: false,
                });
                *index = end_index + 1;
                Ok(siblings[start_index..=end_index].to_vec())
            }
            View::Component(component) => {
                let boundary_id = cursor.next_boundary;
                cursor.next_boundary = cursor.next_boundary.saturating_add(1);
                let start_index = *index;
                let start_marker = format!("rustflow:component:start:{boundary_id}");
                let static_start = format!("rustflow:static:start:{boundary_id}");
                let start_node = siblings.get(start_index).ok_or_else(|| {
                    hydration_mismatch(&format!("comment {start_marker} or {static_start}"), None)
                })?;
                if is_comment_value(start_node, &static_start) {
                    let end_index =
                        find_boundary_end(siblings, start_index, "static", boundary_id)?;
                    *index = end_index + 1;
                    return Ok(siblings[start_index..=end_index].to_vec());
                }
                if !is_comment_value(start_node, &start_marker) {
                    return Err(hydration_mismatch(
                        &format!("comment {start_marker} or {static_start}"),
                        Some(start_node),
                    ));
                }
                let end_index = find_boundary_end(siblings, start_index, "component", boundary_id)?;
                let start = start_node.clone().dyn_into::<Comment>().map_err(|_| {
                    hydration_mismatch(&format!("comment {start_marker}"), Some(start_node))
                })?;
                let end = siblings[end_index]
                    .clone()
                    .dyn_into::<Comment>()
                    .map_err(|_| {
                        hydration_mismatch(
                            &format!("comment rustflow:component:end:{boundary_id}"),
                            Some(&siblings[end_index]),
                        )
                    })?;
                let mut instance = self.registry.mount_for_target(
                    component,
                    self.runtime.clone(),
                    parent_scope,
                    RenderTarget::Browser,
                )?;
                let child_view = instance.view().clone();
                let child_nodes = &siblings[start_index + 1..end_index];
                let mut inner = RenderedNodes::default();
                let mut child_index = 0;
                if let Err(error) = self.hydrate_view(
                    &child_view,
                    child_nodes,
                    &mut child_index,
                    instance.scope(),
                    cursor,
                    &mut inner,
                ) {
                    let _ = cleanup_rendered_nodes(&mut inner);
                    let _ = instance.unmount();
                    return Err(error);
                }
                if child_index != child_nodes.len() {
                    let error = hydration_mismatch(
                        "end of the component SSR boundary",
                        child_nodes.get(child_index),
                    );
                    let _ = cleanup_rendered_nodes(&mut inner);
                    let _ = instance.unmount();
                    return Err(error);
                }
                let mounted_inner = MountedDom {
                    start,
                    end,
                    roots: child_nodes.to_vec(),
                    listeners: inner.listeners,
                    pending_listeners: inner.pending_listeners,
                    delegated_listeners: Vec::new(),
                    delegated_bindings: Rc::new(RefCell::new(inner.delegated)),
                    components: inner.components,
                    component_regions: inner.component_regions,
                    dynamics: inner.dynamics,
                    preserve_on_drop: true,
                };
                resources.component_regions.push(ComponentRegion {
                    descriptor: component.clone(),
                    instance,
                    mounted: mounted_inner,
                    view: child_view,
                    reused: false,
                });
                *index = end_index + 1;
                Ok(siblings[start_index..=end_index].to_vec())
            }
        }
    }

    fn activate_pending_listeners(&self, mounted: &mut MountedDom) -> Result<(), DomError> {
        for region in &mut mounted.component_regions {
            self.activate_pending_listeners(&mut region.mounted)?;
        }
        for region in &mut mounted.dynamics {
            let mut state = region.state.borrow_mut();
            if let Some(state) = state.as_mut() {
                self.activate_pending_listeners(&mut state.mounted)?;
            }
        }
        for pending in mounted.pending_listeners.drain(..) {
            mounted.listeners.push(create_handler_listener(
                &pending.target,
                &pending.name,
                pending.handler,
            )?);
        }
        Ok(())
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
        self.mount_reactive_inner(target, owner_scope, render, false)
    }

    /// Hydrate a reactive view from existing SSR markup, preserving compatible DOM nodes.
    ///
    /// Unlike mount_reactive, this method requires SSR root/component/dynamic markers and
    /// returns an error rather than clearing or replacing the server content on a mismatch.
    pub fn hydrate_reactive<F>(
        &mut self,
        target: &DomElement,
        owner_scope: &Scope,
        render: F,
    ) -> Result<ReactiveMountedDom, DomError>
    where
        F: Fn() -> Result<View, DomError> + 'static,
    {
        self.mount_reactive_inner(target, owner_scope, render, true)
    }

    fn mount_reactive_inner<F>(
        &mut self,
        target: &DomElement,
        owner_scope: &Scope,
        render: F,
        hydrate: bool,
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
                let result = if hydrate {
                    renderer.hydrate_into(&target, &next_view)
                } else {
                    renderer.mount_into(&target, &next_view)
                };
                match result {
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
            pending_listeners: inner.pending_listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(inner.delegated)),
            components: inner.components,
            component_regions: inner.component_regions,
            dynamics: inner.dynamics,
            preserve_on_drop: false,
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
        let mut instance = self.registry.mount_for_target(
            component,
            self.runtime.clone(),
            parent_scope,
            RenderTarget::Browser,
        )?;
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
            pending_listeners: inner.pending_listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(inner.delegated)),
            components: inner.components,
            component_regions: inner.component_regions,
            dynamics: inner.dynamics,
            preserve_on_drop: false,
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
        mounted.pending_listeners = resources.pending_listeners;
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

    fn plan_diff(
        &self,
        previous: &View,
        next: &View,
        component_regions: &[ComponentRegion],
        dynamic_regions: &[DynamicRegion],
    ) -> Result<ReconciliationPlan, DomError> {
        let mut context = DiffContext::new(component_regions, dynamic_regions);
        self.plan_diff_with_context(previous, next, &mut context, &self.root_scope)
    }

    fn plan_diff_with_context(
        &self,
        previous: &View,
        next: &View,
        context: &mut DiffContext<'_>,
        owner_scope: &Scope,
    ) -> Result<ReconciliationPlan, DomError> {
        self.validate_view(next)?;
        let action = match (previous, next) {
            (View::Text(_), View::Text(_)) => ReconciliationAction::Text,
            (View::Element(old), View::Element(new))
                if old.tag().eq_ignore_ascii_case(new.tag()) =>
            {
                ReconciliationAction::Element(self.plan_sequence(
                    old.children(),
                    new.children(),
                    context,
                    owner_scope,
                )?)
            }
            (View::Fragment(old), View::Fragment(new)) => ReconciliationAction::Fragment(
                self.plan_sequence(old.children(), new.children(), context, owner_scope)?,
            ),
            (View::KeyedList(old), View::KeyedList(new)) => ReconciliationAction::Keyed(
                self.plan_keyed_children(old, new, context, owner_scope)?,
            ),
            (View::Component(old), View::Component(new))
                if old.name() == new.name() && old.key() == new.key() =>
            {
                let Some(region_index) = context.take_component(old) else {
                    return self.replacement_plan(previous, next, owner_scope);
                };
                let region = &context.component_regions[region_index];
                let next_view = region
                    .instance
                    .preview_update(&new.properties().to_vec(), &new.children().to_vec())?;
                let mut nested_context =
                    DiffContext::new(&region.mounted.component_regions, &region.mounted.dynamics);
                let child_plan = self.plan_diff_with_context(
                    &region.view,
                    &next_view,
                    &mut nested_context,
                    region.instance.scope(),
                )?;
                ReconciliationAction::Component {
                    region_index,
                    next_view,
                    child_plan: Box::new(child_plan),
                }
            }
            (View::Dynamic(old), View::Dynamic(next_dynamic)) => {
                let Some(region_index) = context.take_dynamic(old) else {
                    return self.replacement_plan(previous, next, owner_scope);
                };
                let region = &context.dynamic_regions[region_index];
                let next_view = untrack(|| next_dynamic.evaluate())?;
                let state = region.state.borrow();
                let current = state.as_ref().ok_or_else(|| {
                    DomError::JavaScript("dynamic region has already been disposed".into())
                })?;
                let mut nested_context = DiffContext::new(
                    &current.mounted.component_regions,
                    &current.mounted.dynamics,
                );
                let child_plan = self.plan_diff_with_context(
                    &current.view,
                    &next_view,
                    &mut nested_context,
                    &region.scope,
                )?;
                ReconciliationAction::Dynamic {
                    region_index,
                    next_view,
                    child_plan: Box::new(child_plan),
                }
            }
            _ => return self.replacement_plan(previous, next, owner_scope),
        };
        Ok(ReconciliationPlan {
            previous: previous.clone(),
            next: next.clone(),
            action,
        })
    }

    fn validate_view(&self, view: &View) -> Result<(), DomError> {
        match view {
            View::Text(_) => Ok(()),
            View::Element(element) => {
                validate_element(element.tag(), element.children().len())?;
                for attribute in element.attributes() {
                    match attribute.value() {
                        AttributeValue::Boolean(_) => validate_attribute(attribute.name(), "")?,
                        AttributeValue::Text(value) => validate_attribute(attribute.name(), value)?,
                    }
                }
                for property in element.properties() {
                    if matches!(property.value(), PropertyValue::Callback(_)) {
                        return Err(DomError::PropertyRejected(property.name().to_owned()));
                    }
                    validate_property(property.name(), property.value())?;
                }
                for child in element.children() {
                    self.validate_view(child)?;
                }
                Ok(())
            }
            View::Fragment(fragment) => {
                for child in fragment.children() {
                    self.validate_view(child)?;
                }
                Ok(())
            }
            View::KeyedList(list) => {
                for child in list.children() {
                    self.validate_view(child.view())?;
                }
                Ok(())
            }
            View::Component(component) => {
                if !self.registry.contains(component.name()) {
                    return Err(DomError::Component(format!(
                        "no component definition registered for: {}",
                        component.name()
                    )));
                }
                for child in component.children() {
                    self.validate_view(child)?;
                }
                Ok(())
            }
            // A deferred evaluator belongs to its own scope/effect. It is evaluated
            // and validated by that region before its own DOM range is committed.
            View::Dynamic(_) => Ok(()),
        }
    }

    fn plan_sequence(
        &self,
        previous: &[View],
        next: &[View],
        context: &mut DiffContext<'_>,
        owner_scope: &Scope,
    ) -> Result<Vec<PlannedChild>, DomError> {
        let mut used = vec![false; previous.len()];
        let mut planned = Vec::with_capacity(next.len());
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

            let child_plan = if let Some(previous_index) = matched {
                used[previous_index] = true;
                self.plan_diff_with_context(
                    &previous[previous_index],
                    next_view,
                    context,
                    owner_scope,
                )?
            } else {
                self.replacement_plan(next_view, next_view, owner_scope)?
            };
            planned.push(PlannedChild {
                previous_index: matched,
                plan: Box::new(child_plan),
            });
        }
        Ok(planned)
    }

    fn plan_keyed_children(
        &self,
        previous: &KeyedList,
        next: &KeyedList,
        context: &mut DiffContext<'_>,
        owner_scope: &Scope,
    ) -> Result<Vec<PlannedChild>, DomError> {
        let old_children = previous.children();
        let mut used = vec![false; old_children.len()];
        let mut planned = Vec::with_capacity(next.children().len());
        for child in next.children() {
            let matched = old_children
                .iter()
                .enumerate()
                .find_map(|(index, candidate)| {
                    (!used[index] && candidate.key() == child.key()).then_some(index)
                });
            let child_plan = if let Some(previous_index) = matched {
                used[previous_index] = true;
                self.plan_diff_with_context(
                    old_children[previous_index].view(),
                    child.view(),
                    context,
                    owner_scope,
                )?
            } else {
                self.replacement_plan(child.view(), child.view(), owner_scope)?
            };
            planned.push(PlannedChild {
                previous_index: matched,
                plan: Box::new(child_plan),
            });
        }
        Ok(planned)
    }

    fn replacement_plan(
        &self,
        previous: &View,
        next: &View,
        owner_scope: &Scope,
    ) -> Result<ReconciliationPlan, DomError> {
        // Build the replacement off-DOM during the evaluation phase. This avoids
        // running component setup twice and ensures render errors are reported
        // before commit starts mutating existing nodes.
        let mut prepared_resources = RenderedNodes::default();
        let nodes = match self.build_nodes(next, owner_scope, &mut prepared_resources) {
            Ok(nodes) => nodes,
            Err(error) => {
                let _ = cleanup_rendered_nodes(&mut prepared_resources);
                return Err(error);
            }
        };
        Ok(ReconciliationPlan {
            previous: previous.clone(),
            next: next.clone(),
            action: ReconciliationAction::Replace(PreparedReplacement::new(
                nodes,
                prepared_resources,
            )),
        })
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

    fn commit_plan(
        &self,
        plan: ReconciliationPlan,
        existing_nodes: &[Node],
        parent: &Node,
        before: Option<&Node>,
        previous_components: &mut Vec<ComponentRegion>,
        previous_dynamics: &mut Vec<DynamicRegion>,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        match plan.action {
            ReconciliationAction::Replace(prepared) => {
                let (nodes, prepared_resources) = prepared.take();
                append_rendered_nodes(resources, prepared_resources);
                Ok(nodes)
            }
            ReconciliationAction::Text => {
                let View::Text(text) = &plan.next else {
                    return Err(DomError::JavaScript(
                        "text reconciliation plan contains a non-text view".into(),
                    ));
                };
                if let [node] = existing_nodes {
                    if node.node_value().as_deref() != Some(text.as_str()) {
                        node.set_node_value(Some(text));
                    }
                    Ok(vec![node.clone()])
                } else {
                    Err(DomError::JavaScript(
                        "text reconciliation plan does not match its mounted DOM node".into(),
                    ))
                }
            }
            ReconciliationAction::Element(children_plan) => {
                let (View::Element(previous), View::Element(next)) = (&plan.previous, &plan.next)
                else {
                    return Err(DomError::JavaScript(
                        "element reconciliation plan contains an incompatible view".into(),
                    ));
                };
                if existing_nodes.len() != 1 {
                    return Err(DomError::JavaScript(
                        "element reconciliation plan does not match its mounted DOM range".into(),
                    ));
                }
                let element: DomElement = existing_nodes[0].clone().dyn_into().map_err(|_| {
                    DomError::JavaScript("expected an element node while reconciling".into())
                })?;
                self.patch_element(previous, next, &element, resources)?;
                let parent_node: Node = element.clone().unchecked_into();
                let current_children = direct_children(&parent_node);
                self.commit_sequence(
                    children_plan,
                    previous.children(),
                    &current_children,
                    &parent_node,
                    None,
                    previous_components,
                    previous_dynamics,
                    resources,
                )?;
                Ok(vec![existing_nodes[0].clone()])
            }
            ReconciliationAction::Fragment(children_plan) => {
                let (View::Fragment(previous), View::Fragment(_next)) =
                    (&plan.previous, &plan.next)
                else {
                    return Err(DomError::JavaScript(
                        "fragment reconciliation plan contains an incompatible view".into(),
                    ));
                };
                self.commit_sequence(
                    children_plan,
                    previous.children(),
                    existing_nodes,
                    parent,
                    before,
                    previous_components,
                    previous_dynamics,
                    resources,
                )
            }
            ReconciliationAction::Keyed(children_plan) => {
                let (View::KeyedList(previous), View::KeyedList(_next)) =
                    (&plan.previous, &plan.next)
                else {
                    return Err(DomError::JavaScript(
                        "keyed reconciliation plan contains an incompatible view".into(),
                    ));
                };
                let previous_views = previous
                    .children()
                    .iter()
                    .map(|child| child.view().clone())
                    .collect::<Vec<_>>();
                self.commit_sequence(
                    children_plan,
                    &previous_views,
                    existing_nodes,
                    parent,
                    before,
                    previous_components,
                    previous_dynamics,
                    resources,
                )
            }
            ReconciliationAction::Component {
                region_index,
                next_view,
                child_plan,
            } => {
                let View::Component(next) = &plan.next else {
                    return Err(DomError::JavaScript(
                        "component reconciliation plan contains a non-component view".into(),
                    ));
                };
                let region = previous_components.get_mut(region_index).ok_or_else(|| {
                    DomError::JavaScript(
                        "component diff plan references a missing mounted region".into(),
                    )
                })?;
                let next_properties = next.properties().to_vec();
                let next_children = next.children().to_vec();
                let mut renderer = DomRenderer::new(
                    self.document.clone(),
                    self.runtime.clone(),
                    self.registry.clone(),
                );
                renderer.root_scope = region.instance.scope().clone();
                renderer.error_sink = self.error_sink.clone();

                // The rendered view was precomputed during diff planning. Commit
                // its child plan before accepting the new props on the instance.
                renderer.update_with_plan(&mut region.mounted, *child_plan)?;
                region.instance.commit_preview_update(
                    next_properties,
                    next_children,
                    next_view.clone(),
                )?;
                region.descriptor = next.clone();
                region.view = next_view;
                region.reused = true;
                Ok(mounted_range_nodes(&region.mounted))
            }
            ReconciliationAction::Dynamic {
                region_index,
                next_view,
                child_plan,
            } => {
                let View::Dynamic(next) = &plan.next else {
                    return Err(DomError::JavaScript(
                        "dynamic reconciliation plan contains a non-dynamic view".into(),
                    ));
                };
                let region = previous_dynamics.get_mut(region_index).ok_or_else(|| {
                    DomError::JavaScript(
                        "dynamic diff plan references a missing mounted region".into(),
                    )
                })?;
                let mut renderer = DomRenderer::new(
                    self.document.clone(),
                    self.runtime.clone(),
                    self.registry.clone(),
                );
                renderer.root_scope = region.scope.clone();
                renderer.error_sink = region.error_sink.clone();
                {
                    let mut state = region.state.borrow_mut();
                    let current = state.as_mut().ok_or_else(|| {
                        DomError::JavaScript("dynamic region has already been disposed".into())
                    })?;
                    renderer.update_with_plan(&mut current.mounted, *child_plan)?;
                    current.view = next_view.clone();
                }
                region.dynamic = next.clone();
                region.reused = true;
                if let Some(effect) = region.effect.take() {
                    effect.dispose()?;
                }
                self.activate_dynamic_region(region)?;
                let state = region.state.borrow();
                let current = state.as_ref().ok_or_else(|| {
                    DomError::JavaScript("dynamic region has already been disposed".into())
                })?;
                Ok(mounted_range_nodes(&current.mounted))
            }
        }
    }

    fn commit_sequence(
        &self,
        planned: Vec<PlannedChild>,
        previous: &[View],
        existing_nodes: &[Node],
        parent: &Node,
        before: Option<&Node>,
        previous_components: &mut Vec<ComponentRegion>,
        previous_dynamics: &mut Vec<DynamicRegion>,
        resources: &mut RenderedNodes,
    ) -> Result<Vec<Node>, DomError> {
        let mut ranges = Vec::with_capacity(previous.len());
        let mut cursor = 0usize;
        for view in previous {
            let count = view_node_count_at(
                view,
                existing_nodes,
                cursor,
                previous_components,
                previous_dynamics,
            )
            .ok_or_else(|| {
                DomError::JavaScript(
                    "mounted DOM range does not match the previous view and owned regions".into(),
                )
            })?;
            let end = cursor.checked_add(count).ok_or_else(|| {
                DomError::JavaScript("mounted DOM range length overflowed".into())
            })?;
            if end > existing_nodes.len() {
                return Err(DomError::JavaScript(
                    "mounted DOM range extends beyond its parent".into(),
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

        let mut result = Vec::new();
        for child in planned {
            let existing = if let Some(previous_index) = child.previous_index {
                let (start, end) = *ranges.get(previous_index).ok_or_else(|| {
                    DomError::JavaScript("diff plan references a missing previous child".into())
                })?;
                &existing_nodes[start..end]
            } else {
                &[]
            };
            result.extend(self.commit_plan(
                *child.plan,
                existing,
                parent,
                before,
                previous_components,
                previous_dynamics,
                resources,
            )?);
        }

        remove_obsolete_nodes(parent, existing_nodes, &result)?;
        for node in &result {
            parent.insert_before(node, before).map_err(js_error)?;
        }
        Ok(result)
    }

    fn update_with_plan(
        &mut self,
        mounted: &mut MountedDom,
        plan: ReconciliationPlan,
    ) -> Result<(), DomError> {
        let Some(parent) = mounted.start.parent_node() else {
            return self.replace_range(mounted, &plan.next);
        };
        let start: Node = mounted.start.clone().unchecked_into();
        let end: Node = mounted.end.clone().unchecked_into();
        let existing_nodes = nodes_between(&start, &end);

        for region in &mut mounted.component_regions {
            region.reused = false;
        }
        for region in &mut mounted.dynamics {
            region.reused = false;
        }

        let mut resources = RenderedNodes::default();
        let previous_view = plan.previous.clone();
        let planned_root = vec![PlannedChild {
            previous_index: Some(0),
            plan: Box::new(plan),
        }];
        let roots = match self.commit_sequence(
            planned_root,
            std::slice::from_ref(&previous_view),
            &existing_nodes,
            &parent,
            Some(&end),
            &mut mounted.component_regions,
            &mut mounted.dynamics,
            &mut resources,
        ) {
            Ok(roots) => roots,
            Err(error) => {
                let _ = cleanup_rendered_nodes(&mut resources);
                for region in &mut mounted.component_regions {
                    region.reused = false;
                }
                for region in &mut mounted.dynamics {
                    region.reused = false;
                }
                return Err(error);
            }
        };

        let mut cleanup_error =
            cleanup_resources(&mut mounted.listeners, &mut mounted.components).err();
        first_error(
            &mut cleanup_error,
            cleanup_stale_component_regions(&mut mounted.component_regions),
        );
        first_error(
            &mut cleanup_error,
            cleanup_stale_dynamic_regions(&mut mounted.dynamics),
        );

        mounted.roots = roots;
        mounted.listeners = resources.listeners;
        mounted.pending_listeners = resources.pending_listeners;
        mounted.components = resources.components;
        mounted
            .component_regions
            .extend(resources.component_regions);
        mounted.dynamics.extend(resources.dynamics);
        *mounted.delegated_bindings.borrow_mut() = resources.delegated;

        if let Err(error) = self.activate_delegated_listeners(mounted, &parent) {
            if cleanup_error.is_none() {
                cleanup_error = Some(error);
            }
        }
        if let Err(error) = self.activate_component_regions(&mut mounted.component_regions) {
            if cleanup_error.is_none() {
                cleanup_error = Some(error);
            }
        }
        if let Err(error) = self.activate_dynamic_regions(&mut mounted.dynamics) {
            if cleanup_error.is_none() {
                cleanup_error = Some(error);
            }
        }

        match cleanup_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn mounted_range_nodes(mounted: &MountedDom) -> Vec<Node> {
    let start: Node = mounted.start.clone().unchecked_into();
    let end: Node = mounted.end.clone().unchecked_into();
    let mut result = Vec::new();
    result.push(start.clone());
    result.extend(nodes_between(&start, &end));
    result.push(end);
    result
}

fn range_node_count(
    start: &Comment,
    end: &Comment,
    existing_nodes: &[Node],
    cursor: usize,
) -> Option<usize> {
    let start_node: Node = start.clone().unchecked_into();
    if !existing_nodes
        .get(cursor)
        .is_some_and(|node| node.is_same_node(Some(&start_node)))
    {
        return None;
    }
    let end_node: Node = end.clone().unchecked_into();
    existing_nodes
        .iter()
        .enumerate()
        .skip(cursor + 1)
        .find_map(|(index, node)| {
            node.is_same_node(Some(&end_node))
                .then_some(index.saturating_sub(cursor).saturating_add(1))
        })
}

fn view_node_count_at(
    view: &View,
    existing_nodes: &[Node],
    cursor: usize,
    components: &[ComponentRegion],
    dynamics: &[DynamicRegion],
) -> Option<usize> {
    match view {
        View::Text(_) | View::Element(_) => {
            existing_nodes.get(cursor)?;
            Some(1)
        }
        View::Fragment(fragment) => {
            let mut offset = cursor;
            for child in fragment.children() {
                offset = offset.checked_add(view_node_count_at(
                    child,
                    existing_nodes,
                    offset,
                    components,
                    dynamics,
                )?)?;
            }
            offset.checked_sub(cursor)
        }
        View::KeyedList(list) => {
            let mut offset = cursor;
            for child in list.children() {
                offset = offset.checked_add(view_node_count_at(
                    child.view(),
                    existing_nodes,
                    offset,
                    components,
                    dynamics,
                )?)?;
            }
            offset.checked_sub(cursor)
        }
        View::Component(_) => {
            let first = existing_nodes.get(cursor)?;
            let region = components.iter().find(|region| {
                let start: Node = region.mounted.start.clone().unchecked_into();
                start.is_same_node(Some(first))
            })?;
            range_node_count(
                &region.mounted.start,
                &region.mounted.end,
                existing_nodes,
                cursor,
            )
        }
        View::Dynamic(_) => {
            let first = existing_nodes.get(cursor)?;
            let region = dynamics.iter().find(|region| {
                let state = region.state.borrow();
                state.as_ref().is_some_and(|current| {
                    let start: Node = current.mounted.start.clone().unchecked_into();
                    start.is_same_node(Some(first))
                })
            })?;
            let state = region.state.borrow();
            let current = state.as_ref()?;
            range_node_count(
                &current.mounted.start,
                &current.mounted.end,
                existing_nodes,
                cursor,
            )
        }
    }
}

fn view_key(view: &View) -> Option<&Key> {
    match view {
        View::Element(element) => element.key(),
        View::Component(component) => component.key(),
        _ => None,
    }
}

fn cleanup_stale_component_regions(regions: &mut Vec<ComponentRegion>) -> Result<(), DomError> {
    let mut retained = Vec::new();
    let mut error = None;
    for mut region in regions.drain(..).rev() {
        if region.reused {
            region.reused = false;
            retained.push(region);
        } else {
            first_error(&mut error, cleanup_component_region(&mut region));
        }
    }
    retained.reverse();
    *regions = retained;
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_one_dynamic_region(region: &mut DynamicRegion) -> Result<(), DomError> {
    let mut error = None;
    if let Some(effect) = region.effect.take() {
        first_error(&mut error, effect.dispose().map_err(Into::into));
    }
    first_error(&mut error, region.scope.dispose().map_err(Into::into));
    if let Some(mut state) = region.state.borrow_mut().take() {
        if !state.mounted.preserve_on_drop {
            first_error(&mut error, detach_range(&mut state.mounted));
        }
        first_error(&mut error, cleanup_mounted_resources(&mut state.mounted));
    }
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_stale_dynamic_regions(regions: &mut Vec<DynamicRegion>) -> Result<(), DomError> {
    let mut retained = Vec::new();
    let mut error = None;
    for mut region in regions.drain(..).rev() {
        if region.reused {
            region.reused = false;
            retained.push(region);
        } else {
            first_error(&mut error, cleanup_one_dynamic_region(&mut region));
        }
    }
    retained.reverse();
    *regions = retained;
    match error {
        Some(error) => Err(error),
        None => Ok(()),
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

fn is_comment_value(node: &Node, expected: &str) -> bool {
    node.node_type() == 8 && node.node_value().as_deref() == Some(expected)
}

fn marker_positions(nodes: &[Node], marker: &str) -> Vec<usize> {
    nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| is_comment_value(node, marker).then_some(index))
        .collect()
}

fn node_description(node: Option<&Node>) -> String {
    match node {
        None => "end of range".to_owned(),
        Some(node) => match node.node_type() {
            1 => node
                .dyn_ref::<DomElement>()
                .map(|element| format!("<{}> element", element.tag_name().to_ascii_lowercase()))
                .unwrap_or_else(|| "element node".to_owned()),
            3 => "text node".to_owned(),
            8 => "comment node".to_owned(),
            kind => format!("DOM node type {kind}"),
        },
    }
}

fn hydration_mismatch(expected: &str, found: Option<&Node>) -> DomError {
    DomError::HydrationMismatch {
        expected: expected.to_owned(),
        found: node_description(found),
    }
}

fn find_boundary_end(
    siblings: &[Node],
    start_index: usize,
    kind: &str,
    id: usize,
) -> Result<usize, DomError> {
    let marker = format!("rustflow:{kind}:end:{id}");
    siblings
        .iter()
        .enumerate()
        .skip(start_index + 1)
        .find_map(|(index, node)| is_comment_value(node, &marker).then_some(index))
        .ok_or_else(|| DomError::HydrationMismatch {
            expected: format!("matching comment {marker}"),
            found: "no matching end marker".to_owned(),
        })
}

fn hydration_property_attribute_name(tag: &str, name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    let attribute = match name.as_str() {
        "autofocus" | "allowfullscreen" | "checked" | "controls" | "disabled"
        | "formnovalidate" | "hidden" | "inert" | "loop" | "multiple" | "muted" | "novalidate"
        | "open" | "readonly" | "required" | "selected" => name.as_str(),
        "accept" | "action" | "alt" | "autocomplete" | "cols" | "colspan" | "contenteditable"
        | "dir" | "draggable" | "enctype" | "formaction" | "height" | "href" | "id" | "lang"
        | "maxlength" | "method" | "min" | "minlength" | "max" | "name" | "pattern"
        | "placeholder" | "poster" | "rel" | "role" | "rowspan" | "rows" | "size"
        | "spellcheck" | "src" | "step" | "tabindex" | "target" | "title" | "type" | "width" => {
            name.as_str()
        }
        "class" | "classname" => "class",
        "for" | "htmlfor" => "for",
        "value" if tag != "textarea" => "value",
        _ => return None,
    };
    Some(attribute.to_owned())
}

fn property_value_as_hydration_text(value: &PropertyValue) -> Result<String, DomError> {
    match value {
        PropertyValue::Text(value) => Ok(value.clone()),
        PropertyValue::Boolean(value) => Ok(value.to_string()),
        PropertyValue::Integer(value) => Ok(value.to_string()),
        PropertyValue::Float(value) => Ok(value.to_string()),
        PropertyValue::Callback(_) => Err(DomError::PropertyRejected("callback".to_owned())),
    }
}

fn validate_hydrated_element(expected: &Element, actual: &DomElement) -> Result<(), DomError> {
    let tag = expected.tag().to_ascii_lowercase();
    let mut expected_attributes = HashMap::<String, String>::new();
    let mut textarea_value = None;

    for attribute in expected.attributes() {
        let name = attribute.name().to_ascii_lowercase();
        let value = match attribute.value() {
            AttributeValue::Boolean(true) => Some(String::new()),
            AttributeValue::Boolean(false) => None,
            AttributeValue::Text(value) => Some(value.clone()),
        };
        if let Some(value) = value {
            validate_attribute(&name, &value)?;
            expected_attributes.insert(name, value);
        } else {
            expected_attributes.remove(&name);
        }
    }

    for property in expected.properties() {
        validate_property(property.name(), property.value())?;
        let name = property.name().to_ascii_lowercase();
        if name == "value" && tag == "textarea" {
            textarea_value = Some(match property.value() {
                PropertyValue::Text(value) => value.clone(),
                _ => return Err(DomError::PropertyRejected(property.name().to_owned())),
            });
            continue;
        }
        let Some(attribute_name) = hydration_property_attribute_name(&tag, &name) else {
            // Properties without an SSR attribute representation are client-only.
            continue;
        };
        let is_boolean = matches!(
            name.as_str(),
            "autofocus"
                | "allowfullscreen"
                | "checked"
                | "controls"
                | "disabled"
                | "formnovalidate"
                | "hidden"
                | "inert"
                | "loop"
                | "multiple"
                | "muted"
                | "novalidate"
                | "open"
                | "readonly"
                | "required"
                | "selected"
        );
        if is_boolean {
            let PropertyValue::Boolean(value) = property.value() else {
                return Err(DomError::PropertyRejected(property.name().to_owned()));
            };
            if *value {
                expected_attributes.insert(attribute_name, String::new());
            } else {
                expected_attributes.remove(&attribute_name);
            }
        } else {
            let value = property_value_as_hydration_text(property.value())?;
            validate_attribute(&attribute_name, &value)?;
            expected_attributes.insert(attribute_name, value);
        }
    }

    let actual_attributes = actual.attributes();
    let mut found_attributes = HashMap::<String, String>::new();
    for index in 0..actual_attributes.length() {
        let attribute: Attr =
            actual_attributes
                .item(index)
                .ok_or_else(|| DomError::HydrationMismatch {
                    expected: "complete DOM attribute list".to_owned(),
                    found: "unreadable attribute".to_owned(),
                })?;
        let name = attribute.name().to_ascii_lowercase();
        let value = attribute.value();
        validate_attribute(&name, &value)?;
        found_attributes.insert(name, value);
    }
    if found_attributes != expected_attributes {
        return Err(DomError::HydrationMismatch {
            expected: format!("the SSR attribute set for <{tag}>"),
            found: "different attribute names or values".to_owned(),
        });
    }
    if let Some(expected_value) = textarea_value {
        if actual.text_content().as_deref() != Some(expected_value.as_str()) {
            return Err(DomError::HydrationMismatch {
                expected: "matching textarea initial value".to_owned(),
                found: "different textarea text content".to_owned(),
            });
        }
    }
    Ok(())
}

fn set_preserve_on_drop(mounted: &mut MountedDom, preserve: bool) {
    mounted.preserve_on_drop = preserve;
    for region in &mut mounted.component_regions {
        set_preserve_on_drop(&mut region.mounted, preserve);
    }
    for region in &mut mounted.dynamics {
        let mut state = region.state.borrow_mut();
        if let Some(state) = state.as_mut() {
            set_preserve_on_drop(&mut state.mounted, preserve);
        }
    }
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
            pending_listeners: resources.pending_listeners,
            delegated_listeners: Vec::new(),
            delegated_bindings: Rc::new(RefCell::new(resources.delegated)),
            components: resources.components,
            component_regions: resources.component_regions,
            dynamics: resources.dynamics,
            preserve_on_drop: false,
        })
    }

    fn update(
        &mut self,
        mounted: &mut Self::Mounted,
        previous: &View,
        next: &View,
    ) -> Result<(), Self::Error> {
        // A detached mount has no live DOM range to reconcile. Skip preparation
        // and perform a single replacement build so component setup is not run twice.
        if mounted.start.parent_node().is_none() {
            return self.replace_range(mounted, next);
        }

        let plan = self.plan_diff(
            previous,
            next,
            &mounted.component_regions,
            &mounted.dynamics,
        )?;
        self.update_with_plan(mounted, plan)
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
        if !self.preserve_on_drop {
            let _ = detach_range(self);
        }
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
    resources.pending_listeners.clear();
    resources.delegated.clear();
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn cleanup_dynamic_regions(regions: &mut Vec<DynamicRegion>) -> Result<(), DomError> {
    let mut error = None;
    for mut region in regions.drain(..).rev() {
        first_error(&mut error, cleanup_one_dynamic_region(&mut region));
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
    mounted.pending_listeners.clear();
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
    if !region.mounted.preserve_on_drop {
        first_error(&mut error, detach_range(&mut region.mounted));
    }
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
    create_handler_listener(target, binding.name(), binding.handler())
}

fn create_handler_listener(
    target: &EventTarget,
    name: &str,
    handler: EventHandler,
) -> Result<EventListener, DomError> {
    let callback = Closure::<dyn FnMut(Event)>::new(move |event| {
        handler.invoke(event_payload(&event));
    });
    target
        .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
        .map_err(js_error)?;
    Ok(EventListener {
        target: target.clone(),
        name: name.to_owned(),
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
