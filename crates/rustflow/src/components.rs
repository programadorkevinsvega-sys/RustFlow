//! Component setup, rendering, and deterministic ownership lifecycle.

use std::{error::Error, fmt, rc::Rc};

use rustflow_view::{ComponentName, Property, View, ViewError};
use crate::reactivity::{Runtime, RuntimeError, Scope};

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
}
impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::View(error) => write!(f, "component view error: {error}"),
            Self::Runtime(error) => write!(f, "component runtime error: {error}"),
            Self::Unmounted => write!(f, "component instance has been unmounted"),
            Self::User(message) => write!(f, "component callback failed: {message}"),
        }
    }
}
impl Error for ComponentError {}
impl From<ViewError> for ComponentError { fn from(value: ViewError) -> Self { Self::View(value) } }
impl From<RuntimeError> for ComponentError { fn from(value: RuntimeError) -> Self { Self::Runtime(value) } }

/// Context provided once during component setup.
///
/// Register effects and resource cleanup against this component's owner scope.
/// The render callback must be pure and only produce a new view tree.
#[derive(Clone)]
pub struct ComponentContext { runtime: Runtime, scope: Scope }
impl ComponentContext {
    /// Return the runtime this component belongs to.
    pub fn runtime(&self) -> &Runtime { &self.runtime }
    /// Return this component's owner scope.
    pub fn scope(&self) -> &Scope { &self.scope }
    /// Register cleanup in LIFO order. A DOM adapter should register listener removal here.
    pub fn on_cleanup(&self, callback: impl FnOnce() + 'static) -> Result<(), ComponentError> {
        self.scope.on_cleanup(callback)?;
        Ok(())
    }
}

/// Pure render callback called after one-time component setup.
pub type ComponentRender = Rc<dyn Fn(&[Property], &[View]) -> Result<View, ComponentError>>;
/// One-time setup callback. It may create scoped effects and returns a repeatable render function.
pub type ComponentSetup = Rc<dyn Fn(&ComponentContext, &[Property]) -> Result<ComponentRender, ComponentError>>;

/// A reusable component definition.
#[derive(Clone)]
pub struct ComponentDefinition { name: ComponentName, setup: ComponentSetup }
impl ComponentDefinition {
    /// Define a component with one-time setup and repeatable rendering.
    pub fn new(name: impl Into<String>, setup: impl Fn(&ComponentContext, &[Property]) -> Result<ComponentRender, ComponentError> + 'static) -> Result<Self, ComponentError> {
        Ok(Self { name: ComponentName::new(name)?, setup: Rc::new(setup) })
    }
    /// Return the public component type name.
    pub fn name(&self) -> &str { self.name.as_str() }
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
    pub fn mount(definition: ComponentDefinition, runtime: Runtime, parent_scope: &Scope, properties: impl IntoIterator<Item = Property>, children: impl IntoIterator<Item = View>) -> Result<Self, ComponentError> {
        let properties = properties.into_iter().collect::<Vec<_>>();
        validate_properties(&properties)?;
        let children = children.into_iter().collect::<Vec<_>>();
        let scope = parent_scope.child()?;
        let context = ComponentContext { runtime: runtime.clone(), scope: scope.clone() };
        let render = match (definition.setup)(&context, &properties) {
            Ok(render) => render,
            Err(error) => { scope.dispose()?; return Err(error); }
        };
        let view = match render(&properties, &children) {
            Ok(view) => view,
            Err(error) => { scope.dispose()?; return Err(error); }
        };
        Ok(Self { definition, runtime, scope, render, properties, children, view, state: ComponentState::Mounted })
    }
    /// Return the component definition name.
    pub fn name(&self) -> &str { self.definition.name() }
    /// Return lifecycle state.
    pub fn state(&self) -> ComponentState { self.state }
    /// Return the current rendered view tree.
    pub fn view(&self) -> &View { &self.view }
    /// Return the associated reactive runtime.
    pub fn runtime(&self) -> &Runtime { &self.runtime }
    /// Recompute the view with new properties/children. Setup is not repeated.
    ///
    /// On render failure, the old view and inputs remain unchanged.
    pub fn update(&mut self, properties: impl IntoIterator<Item = Property>, children: impl IntoIterator<Item = View>) -> Result<&View, ComponentError> {
        if self.state == ComponentState::Unmounted { return Err(ComponentError::Unmounted); }
        let properties = properties.into_iter().collect::<Vec<_>>();
        validate_properties(&properties)?;
        let children = children.into_iter().collect::<Vec<_>>();
        let next_view = (self.render)(&properties, &children)?;
        self.properties = properties;
        self.children = children;
        self.view = next_view;
        Ok(&self.view)
    }
    /// Dispose the component scope, effects, descendants, and registered cleanups.
    ///
    /// Unmount is idempotent. The instance becomes unavailable before cleanup runs.
    pub fn unmount(&mut self) -> Result<(), ComponentError> {
        if self.state == ComponentState::Unmounted { return Ok(()); }
        self.state = ComponentState::Unmounted;
        self.scope.dispose()?;
        Ok(())
    }
}
fn validate_properties(properties: &[Property]) -> Result<(), ComponentError> {
    let mut names = std::collections::HashSet::new();
    for property in properties {
        if !names.insert(property.name().to_owned()) {
            return Err(ComponentError::View(ViewError::DuplicateProperty(property.name().to_owned())));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ComponentDefinition, ComponentError, ComponentInstance, ComponentRender, ComponentState};
    use crate::reactivity::Runtime;
    use rustflow_view::{Property, View};
    use std::{cell::Cell, rc::Rc};

    fn text_component() -> ComponentDefinition {
        ComponentDefinition::new("TextLabel", |_context, _initial| {
            let render: ComponentRender = Rc::new(|properties, children| {
                if let Some(label) = properties.iter().find(|property| property.name() == "label") {
                    let rustflow_view::PropertyValue::Text(text) = label.value() else {
                        return Err(ComponentError::User("label must be text".into()));
                    };
                    Ok(View::fragment([View::text(text.clone()), View::fragment(children.to_vec())]))
                } else {
                    Ok(View::fragment(children.to_vec()))
                }
            });
            Ok(render)
        }).expect("valid component definition")
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
                let label = props.first().map(|p| format!("{:?}", p.value())).unwrap_or_default();
                Ok(View::text(label))
            });
            Ok(render)
        }).expect("valid component");

        let mut instance = ComponentInstance::mount(definition, runtime.clone(), &root,
            [Property::new("count", 1).expect("property")], []).expect("mount");
        assert_eq!(setups.get(), 1);
        instance.update([Property::new("count", 2).expect("property")], []).expect("update");
        assert_eq!(setups.get(), 1);
        assert_eq!(instance.state(), ComponentState::Mounted);
        instance.unmount().expect("unmount");
        assert_eq!(instance.state(), ComponentState::Unmounted);
        assert!(matches!(instance.update([], []), Err(ComponentError::Unmounted)));
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
        }).expect("valid component");
        let mut instance = ComponentInstance::mount(definition, runtime.clone(), &root, [], []).expect("mount");
        assert_eq!(cleanups.get(), 0);
        instance.unmount().expect("unmount");
        instance.unmount().expect("idempotent unmount");
        assert_eq!(cleanups.get(), 1);
        runtime.dispose().expect("runtime dispose");
    }

    #[test]
    fn failed_update_keeps_the_previous_view() {
        let runtime = Runtime::new();
        let root = runtime.root_scope();
        let mut instance = ComponentInstance::mount(text_component(), runtime.clone(), &root,
            [Property::new("label", "before").expect("property")], []).expect("mount");
        let before = instance.view().clone();
        let result = instance.update([Property::new("label", true).expect("property")], []);
        assert!(result.is_err());
        assert_eq!(instance.view(), &before);
        instance.unmount().expect("unmount");
        runtime.dispose().expect("runtime dispose");
    }
}
