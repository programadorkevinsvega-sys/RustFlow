//! Component setup, rendering, and deterministic ownership lifecycle.

use std::{collections::HashMap, error::Error, fmt, rc::Rc};

use crate::reactivity::{Runtime, RuntimeError, Scope};
use rustflow_view::{ComponentName, ComponentNode, Property, View, ViewError};

/// Errors produced while setting up or updating a component instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentError {
    /// A view node could not be constructed.
    View(ViewError),
    /// Reactive scope setup or disposal failed.
    Runtime(RuntimeError),
    /// The component has already been unmounted.
    Unmounted,
    /// A component callback reported an application error.
    User(String),
    /// A registry already contains a definition with this name.
    DuplicateDefinition(String),
    /// A view references a component without a registered definition.
    UnknownComponent(String),
}
impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::View(error) => write!(f, "component view error: {error}"),
            Self::Runtime(error) => write!(f, "component runtime error: {error}"),
            Self::Unmounted => write!(f, "component instance has been unmounted"),
            Self::User(message) => write!(f, "component callback failed: {message}"),
            Self::DuplicateDefinition(name) => {
                write!(f, "component definition already registered: {name}")
            }
            Self::UnknownComponent(name) => {
                write!(f, "no component definition registered for: {name}")
            }
        }
    }
}
impl Error for ComponentError {}
impl From<ViewError> for ComponentError {
    fn from(value: ViewError) -> Self {
        Self::View(value)
    }
}
impl From<RuntimeError> for ComponentError {
    fn from(value: RuntimeError) -> Self {
        Self::Runtime(value)
    }
}

/// Context provided once during component setup.
///
/// Register effects and resource cleanup against this component's owner scope.
/// The render callback must be pure and only produce a new view tree.
#[derive(Clone)]
pub struct ComponentContext {
    runtime: Runtime,
    scope: Scope,
}
impl ComponentContext {
    /// Return the runtime this component belongs to.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }
    /// Return this component's owner scope.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    /// Register cleanup in LIFO order. A DOM adapter should register listener removal here.
    pub fn on_cleanup(&self, callback: impl FnOnce() + 'static) -> Result<(), ComponentError> {
        self.scope.on_cleanup(callback)?;
        Ok(())
    }
}

/// Pure render callback called after one-time component setup.
pub type ComponentRender = Rc<dyn Fn(&[Property], &[View]) -> Result<View, ComponentError>>;
/// One-time setup callback. It may create scoped effects and returns a repeatable render function.
pub type ComponentSetup =
    Rc<dyn Fn(&ComponentContext, &[Property]) -> Result<ComponentRender, ComponentError>>;

/// A reusable component definition.
#[derive(Clone)]
pub struct ComponentDefinition {
    name: ComponentName,
    setup: ComponentSetup,
}
impl ComponentDefinition {
    /// Define a component with one-time setup and repeatable rendering.
    pub fn new(
        name: impl Into<String>,
        setup: impl Fn(&ComponentContext, &[Property]) -> Result<ComponentRender, ComponentError>
        + 'static,
    ) -> Result<Self, ComponentError> {
        Ok(Self {
            name: ComponentName::new(name)?,
            setup: Rc::new(setup),
        })
    }
    /// Return the public component type name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

/// Registry that resolves component descriptors to reusable definitions.
///
/// The registry can mount one descriptor into a native component instance. A
/// renderer is responsible for recursively traversing a view tree and managing
/// child component identity during reconciliation.
#[derive(Clone, Default)]
pub struct ComponentRegistry {
    definitions: HashMap<String, ComponentDefinition>,
}

impl ComponentRegistry {
    /// Create an empty component registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a reusable definition. Duplicate names are rejected.
    pub fn register(&mut self, definition: ComponentDefinition) -> Result<(), ComponentError> {
        let name = definition.name().to_owned();
        if self.definitions.contains_key(&name) {
            return Err(ComponentError::DuplicateDefinition(name));
        }
        self.definitions.insert(name, definition);
        Ok(())
    }

    /// Return whether a definition is registered under this name.
    pub fn contains(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    /// Mount a component descriptor using its registered definition.
    pub fn mount(
        &self,
        node: &ComponentNode,
        runtime: Runtime,
        parent_scope: &Scope,
    ) -> Result<ComponentInstance, ComponentError> {
        let definition = self
            .definitions
            .get(node.name())
            .cloned()
            .ok_or_else(|| ComponentError::UnknownComponent(node.name().to_owned()))?;
        ComponentInstance::mount(
            definition,
            runtime,
            parent_scope,
            node.properties().iter().cloned(),
            node.children().iter().cloned(),
        )
    }
}

/// Current lifecycle state of a mounted component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentState {
    /// Setup succeeded and the component owns a live reactive scope.
    Mounted,
    /// Unmount has started; updates are no longer permitted.
    Unmounted,
}

/// A mounted component instance. It produces replacement view trees but does
/// not manipulate a DOM or server response itself.
pub struct ComponentInstance {
    definition: ComponentDefinition,
    runtime: Runtime,
    scope: Scope,
    render: ComponentRender,
    properties: Vec<Property>,
    children: Vec<View>,
    view: View,
    state: ComponentState,
}
impl ComponentInstance {
    /// Set up and mount a component under a parent owner scope.
    pub fn mount(
        definition: ComponentDefinition,
        runtime: Runtime,
        parent_scope: &Scope,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
    ) -> Result<Self, ComponentError> {
        let properties = properties.into_iter().collect::<Vec<_>>();
        validate_properties(&properties)?;
        let children = children.into_iter().collect::<Vec<_>>();
        let scope = parent_scope.child()?;
        let context = ComponentContext {
            runtime: runtime.clone(),
            scope: scope.clone(),
        };
        let render = match crate::reactivity::with_owner_scope(&scope, || {
            (definition.setup)(&context, &properties)
        }) {
            Ok(Ok(render)) => render,
            Ok(Err(error)) => {
                scope.dispose()?;
                return Err(error);
            }
            Err(error) => {
                scope.dispose()?;
                return Err(error.into());
            }
        };
        let view = match render(&properties, &children) {
            Ok(view) => view,
            Err(error) => {
                scope.dispose()?;
                return Err(error);
            }
        };
        Ok(Self {
            definition,
            runtime,
            scope,
            render,
            properties,
            children,
            view,
            state: ComponentState::Mounted,
        })
    }
    /// Return the component definition name.
    pub fn name(&self) -> &str {
        self.definition.name()
    }
    /// Return lifecycle state.
    pub fn state(&self) -> ComponentState {
        self.state
    }
    /// Return the most recently accepted properties.
    pub fn properties(&self) -> &[Property] {
        &self.properties
    }
    /// Return the most recently accepted children.
    pub fn children(&self) -> &[View] {
        &self.children
    }
    /// Return the current rendered view tree.
    pub fn view(&self) -> &View {
        &self.view
    }
    /// Return the associated reactive runtime.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Return the owner scope for nested component/resource cleanup.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    /// Preview a view rendered with updated properties/children without accepting them.
    ///
    /// The component setup and its owner scope are not recreated. This method is
    /// useful for preflighting a reconciliation before committing DOM changes.
    pub fn preview_update(
        &self,
        properties: &[Property],
        children: &[View],
    ) -> Result<View, ComponentError> {
        if self.state == ComponentState::Unmounted {
            return Err(ComponentError::Unmounted);
        }
        validate_properties(properties)?;
        (self.render)(properties, children)
    }

    /// Recompute the view with new properties/children. Setup is not repeated.
    ///
    /// On render failure, the old view and inputs remain unchanged.
    pub fn update(
        &mut self,
        properties: impl IntoIterator<Item = Property>,
        children: impl IntoIterator<Item = View>,
    ) -> Result<&View, ComponentError> {
        if self.state == ComponentState::Unmounted {
            return Err(ComponentError::Unmounted);
        }
        let properties = properties.into_iter().collect::<Vec<_>>();
        let children = children.into_iter().collect::<Vec<_>>();
        let next_view = self.preview_update(&properties, &children)?;
        self.properties = properties;
        self.children = children;
        self.view = next_view;
        Ok(&self.view)
    }
    /// Dispose the component scope, effects, descendants, and registered cleanups.
    ///
    /// Unmount is idempotent. The instance becomes unavailable before cleanup runs.
    pub fn unmount(&mut self) -> Result<(), ComponentError> {
        if self.state == ComponentState::Unmounted {
            return Ok(());
        }
        self.state = ComponentState::Unmounted;
        self.scope.dispose()?;
        Ok(())
    }
}
fn validate_properties(properties: &[Property]) -> Result<(), ComponentError> {
    let mut names = std::collections::HashSet::new();
    for property in properties {
        if !names.insert(property.name().to_owned()) {
            return Err(ComponentError::View(ViewError::DuplicateProperty(
                property.name().to_owned(),
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ComponentDefinition, ComponentError, ComponentInstance, ComponentRender, ComponentState,
    };
    use crate::reactivity::Runtime;
    use rustflow_view::{Property, View};
    use std::{cell::Cell, rc::Rc};

    fn text_component() -> ComponentDefinition {
        ComponentDefinition::new("TextLabel", |_context, _initial| {
            let render: ComponentRender = Rc::new(|properties, children| {
                if let Some(label) = properties
                    .iter()
                    .find(|property| property.name() == "label")
                {
                    let rustflow_view::PropertyValue::Text(text) = label.value() else {
                        return Err(ComponentError::User("label must be text".into()));
                    };
                    Ok(View::fragment([
                        View::text(text.clone()),
                        View::fragment(children.to_vec()),
                    ]))
                } else {
                    Ok(View::fragment(children.to_vec()))
                }
            });
            Ok(render)
        })
        .expect("valid component definition")
    }

    #[test]
    fn registry_mounts_descriptors_and_rejects_unknown_or_duplicate_names() {
        use super::{ComponentError, ComponentRegistry};

        let definition = ComponentDefinition::new("Greeting", |_context, _properties| {
            let render: ComponentRender = Rc::new(|properties, children| {
                let greeting = properties
                    .iter()
                    .find(|property| property.name() == "label")
                    .and_then(|property| match property.value() {
                        rustflow_view::PropertyValue::Text(value) => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "hello".to_owned());
                Ok(View::fragment([
                    View::text(greeting),
                    View::fragment(children.to_vec()),
                ]))
            });
            Ok(render)
        })
        .expect("valid definition");

        let mut registry = ComponentRegistry::new();
        registry
            .register(definition.clone())
            .expect("first registration");
        assert!(registry.contains("Greeting"));
        assert!(matches!(
            registry.register(definition),
            Err(ComponentError::DuplicateDefinition(name)) if name == "Greeting"
        ));

        let descriptor = match View::component(
            "Greeting",
            [Property::new("label", "welcome").expect("property")],
            [View::text("child")],
            None,
        )
        .expect("component descriptor")
        {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let mut instance = registry
            .mount(&descriptor, runtime.clone(), &root)
            .expect("mount");
        assert_eq!(instance.name(), "Greeting");
        assert_eq!(
            instance.view(),
            &View::fragment([View::text("welcome"), View::fragment([View::text("child")])])
        );
        instance.unmount().expect("unmount");

        let unknown = match View::component("Missing", [], [], None).expect("descriptor") {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        assert!(matches!(
            registry.mount(&unknown, runtime.clone(), &root),
            Err(ComponentError::UnknownComponent(name)) if name == "Missing"
        ));
        runtime.dispose().expect("runtime dispose");
    }

    #[test]
    fn view_macro_builds_nested_components_elements_attributes_events_and_dynamic_children() {
        let view = crate::view! {
            <Panel key={"panel"} title="container">
                <Greeting label={"hello"} />
                <button disabled on:click={move |_event| {}}>
                    "Continue"
                </button>
                @dynamic { "later" }
            </Panel>
        }
        .expect("valid macro expansion");

        let View::Component(panel) = view else {
            panic!("component descriptor expected");
        };
        assert_eq!(panel.name(), "Panel");
        assert_eq!(panel.key(), Some(&rustflow_view::Key::from("panel")));
        assert_eq!(panel.properties()[0].name(), "title");

        let View::Component(greeting) = &panel.children()[0] else {
            panic!("nested component descriptor expected");
        };
        assert_eq!(greeting.name(), "Greeting");
        assert_eq!(greeting.properties()[0].name(), "label");

        let View::Element(button) = &panel.children()[1] else {
            panic!("element child expected");
        };
        assert_eq!(button.attributes()[0].name(), "disabled");
        assert_eq!(
            button.attributes()[0].value(),
            &rustflow_view::AttributeValue::Boolean(true)
        );
        assert_eq!(button.events().len(), 1);
        assert_eq!(button.events()[0].name(), "click");
        assert_eq!(button.children(), &[View::text("Continue")]);

        assert!(matches!(&panel.children()[2], View::Dynamic(_)));
    }

    #[test]
    fn setup_is_once_and_update_reuses_the_instance() {
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let setups = Rc::new(Cell::new(0));
        let setup_counter = setups.clone();
        let definition = ComponentDefinition::new("CounterView", move |_context, _props| {
            setup_counter.set(setup_counter.get() + 1);
            let render: ComponentRender = Rc::new(|props, _children| {
                let label = props
                    .first()
                    .map(|p| format!("{:?}", p.value()))
                    .unwrap_or_default();
                Ok(View::text(label))
            });
            Ok(render)
        })
        .expect("valid component");

        let mut instance = ComponentInstance::mount(
            definition,
            runtime.clone(),
            &root,
            [Property::new("count", 1).expect("property")],
            [],
        )
        .expect("mount");
        assert_eq!(setups.get(), 1);
        instance
            .update([Property::new("count", 2).expect("property")], [])
            .expect("update");
        assert_eq!(setups.get(), 1);
        assert_eq!(instance.state(), ComponentState::Mounted);
        instance.unmount().expect("unmount");
        assert_eq!(instance.state(), ComponentState::Unmounted);
        assert!(matches!(
            instance.update([], []),
            Err(ComponentError::Unmounted)
        ));
        runtime.dispose().expect("runtime dispose");
    }

    #[test]
    fn unmount_runs_registered_cleanup_once() {
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let cleanups = Rc::new(Cell::new(0));
        let setup_cleanups = cleanups.clone();
        let definition = ComponentDefinition::new("OwnedResources", move |context, _props| {
            let cleanup_count = setup_cleanups.clone();
            context.on_cleanup(move || cleanup_count.set(cleanup_count.get() + 1))?;
            let render: ComponentRender = Rc::new(|_props, _children| Ok(View::text("mounted")));
            Ok(render)
        })
        .expect("valid component");
        let mut instance =
            ComponentInstance::mount(definition, runtime.clone(), &root, [], []).expect("mount");
        assert_eq!(cleanups.get(), 0);
        instance.unmount().expect("unmount");
        instance.unmount().expect("idempotent unmount");
        assert_eq!(cleanups.get(), 1);
        runtime.dispose().expect("runtime dispose");
    }

    #[crate::component]
    fn MacroCounter(initial: i32) -> impl rustflow_view::IntoView {
        let count = crate::reactivity::signal(initial);
        let increment = count.clone();
        crate::view! {
            <button on:click={move |_event| increment.update(|value| *value += 1)}>
                "Count "
                <span>{count}</span>
            </button>
        }
    }

    #[crate::component]
    fn MacroPropCounter(initial: i32, label: String) -> impl rustflow_view::IntoView {
        let count = crate::reactivity::signal(initial);
        let increment = count.clone();
        crate::view! {
            <section>
                <span data-testid="prop-label">{label}</span>
                <button on:click={move |_event| increment.update(|value| *value += 1)}>
                    "Increment"
                </button>
                <span data-testid="local-count">{count}</span>
            </section>
        }
    }

    #[crate::component]
    fn CallbackReceiver(on_increment: rustflow_view::EventHandler) -> impl rustflow_view::IntoView {
        crate::view! {
            <button on:click={move |payload| on_increment.invoke(payload)}>
                "Forward"
            </button>
        }
    }

    #[test]
    fn macro_component_prop_updates_preserve_owner_state_and_refresh_the_view() {
        use super::ComponentRegistry;
        use rustflow_view::{EventPayload, PropertyValue};

        let mut registry = ComponentRegistry::new();
        registry
            .register(__rustflow_component_definition_macropropcounter().expect("component adapter"))
            .expect("register component");
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let descriptor = match crate::view! {
            <MacroPropCounter initial={4} label="before" />
        }
        .expect("component descriptor")
        {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        let mut instance = registry
            .mount(&descriptor, runtime.clone(), &root)
            .expect("mount component");

        let View::Element(section) = instance.view() else {
            panic!("component returns a section");
        };
        let View::Element(button) = &section.children()[1] else {
            panic!("component includes an increment button");
        };
        button.events()[0].handler().invoke(EventPayload::default());

        let updated = instance
            .update(
                [
                    Property::new("initial", 99).expect("initial prop"),
                    Property::new("label", "after").expect("label prop"),
                ],
                [],
            )
            .expect("update component props")
            .clone();
        let View::Element(section) = &updated else {
            panic!("updated component returns a section");
        };
        assert_eq!(section.children()[0], View::element("span", [View::text("after")]).expect("span"));

        let View::Element(counter) = &section.children()[2] else {
            panic!("component includes the local counter");
        };
        assert_eq!(counter.attributes()[0].name(), "data-testid");
        assert_eq!(counter.attributes()[0].value(), &rustflow_view::AttributeValue::Text("local-count".into()));
        let View::Dynamic(dynamic) = &counter.children()[0] else {
            panic!("signal interpolation remains a dynamic region");
        };
        assert_eq!(dynamic.evaluate().expect("current signal view"), View::text("5"));
        assert_eq!(
            instance.properties().iter().find(|property| property.name() == "initial").map(|property| property.value()),
            Some(&PropertyValue::Integer(99))
        );

        instance.unmount().expect("unmount component");
        runtime.dispose().expect("dispose runtime");
    }

    #[test]
    fn component_attribute_generates_typed_registry_adapter_and_reactive_state() {
        use super::ComponentRegistry;
        use rustflow_view::{EventPayload, PropertyValue};

        let mut registry = ComponentRegistry::new();
        registry
            .register(__rustflow_component_definition_macrocounter().expect("component adapter"))
            .expect("register component");
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let descriptor = match crate::view! { <MacroCounter initial={3} /> }.expect("descriptor") {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        let mut instance = registry
            .mount(&descriptor, runtime.clone(), &root)
            .expect("typed component mounts");
        let View::Element(button) = instance.view() else {
            panic!("component returns a button");
        };
        assert_eq!(button.events().len(), 1);
        let View::Element(span) = &button.children()[1] else {
            panic!("dynamic count lives in a span");
        };
        assert!(matches!(span.children(), [View::Dynamic(_)]));
        instance.unmount().expect("unmount component");
        runtime.dispose().expect("dispose runtime");

        let invoked = Rc::new(Cell::new(false));
        let called = invoked.clone();
        let parent = match crate::view! {
            <CallbackReceiver on:increment={move |payload| {
                called.set(payload.checked == Some(true));
            }} />
        }
        .expect("callback component descriptor")
        {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        assert!(matches!(
            parent.properties()[0].value(),
            PropertyValue::Callback(_)
        ));
        let callback_runtime = Runtime::new();
        let mut callback_registry = ComponentRegistry::new();
        callback_registry
            .register(__rustflow_component_definition_callbackreceiver().expect("callback adapter"))
            .expect("register callback component");
        let mut callback_instance = callback_registry
            .mount(
                &parent,
                callback_runtime.clone(),
                &callback_runtime.root_scope(),
            )
            .expect("callback component mounts");
        let View::Element(button) = callback_instance.view() else {
            panic!("callback component renders a button");
        };
        button.events()[0].handler().invoke(EventPayload {
            value: None,
            checked: Some(true),
        });
        assert!(invoked.get());
        callback_instance
            .unmount()
            .expect("unmount callback component");
        callback_runtime.dispose().expect("dispose runtime");
    }

    #[test]
    fn component_macro_rejects_missing_unknown_and_mismatched_props() {
        use super::ComponentRegistry;

        let mut registry = ComponentRegistry::new();
        registry
            .register(__rustflow_component_definition_macrocounter().expect("component adapter"))
            .expect("register component");
        let runtime = Runtime::new();
        let root = runtime.root_scope();

        let missing = match crate::view! { <MacroCounter /> }.expect("missing prop descriptor") {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        let unknown = match crate::view! {
            <MacroCounter initial={3} unexpected={"not accepted".to_owned()} />
        }
        .expect("unknown prop descriptor")
        {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };
        let mismatched = match crate::view! {
            <MacroCounter initial={"not an integer".to_owned()} />
        }
        .expect("mismatched prop descriptor")
        {
            View::Component(node) => node,
            _ => panic!("component descriptor expected"),
        };

        assert!(registry.mount(&missing, runtime.clone(), &root).is_err());
        assert!(registry.mount(&unknown, runtime.clone(), &root).is_err());
        assert!(registry.mount(&mismatched, runtime.clone(), &root).is_err());
        runtime.dispose().expect("dispose runtime");
    }

    #[test]
    fn failed_update_keeps_the_previous_view() {
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let mut instance = ComponentInstance::mount(
            text_component(),
            runtime.clone(),
            &root,
            [Property::new("label", "before").expect("property")],
            [],
        )
        .expect("mount");
        let before = instance.view().clone();
        let result = instance.update([Property::new("label", true).expect("property")], []);
        assert!(result.is_err());
        assert_eq!(instance.view(), &before);
        instance.unmount().expect("unmount");
        runtime.dispose().expect("runtime dispose");
    }
    #[test]
    fn component_setup_can_create_owner_bound_state_and_effects() {
        use crate::reactivity::{on_cleanup, signal, use_effect};

        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let retained = Rc::new(std::cell::RefCell::new(None));
        let retained_setup = retained.clone();
        let cleanup_count = Rc::new(Cell::new(0));
        let cleanup_setup = cleanup_count.clone();

        let definition = ComponentDefinition::new("SignalCounter", move |_context, _props| {
            let count = signal(1_i32);
            *retained_setup.borrow_mut() = Some(count.clone());

            let observed = count.clone();
            let cleanup_setup = cleanup_setup.clone();
            use_effect(move || {
                let _ = observed.get();
                let cleanup_setup = cleanup_setup.clone();
                on_cleanup(move || cleanup_setup.set(cleanup_setup.get() + 1)).unwrap();
            });

            let render: ComponentRender =
                Rc::new(move |_props, _children| Ok(View::text(count.get().to_string())));
            Ok(render)
        })
        .expect("valid component definition");

        let mut instance = ComponentInstance::mount(definition, runtime.clone(), &root, [], [])
            .expect("component setup can create signals");
        let count = retained.borrow().as_ref().unwrap().clone();
        assert_eq!(count.get(), 1);
        count.set(2);
        assert_eq!(cleanup_count.get(), 1);
        assert_eq!(instance.view(), &View::text("1"));

        instance.unmount().expect("unmount component owner");
        assert_eq!(cleanup_count.get(), 2);
        assert_eq!(
            count.try_get(),
            Err(crate::reactivity::RuntimeError::InvalidSignal)
        );
        runtime.dispose().expect("dispose runtime");
    }
}
