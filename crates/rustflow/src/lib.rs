//! RustFlow is an early-stage Rust-first full-stack web framework.
//!
//! This crate contains the initial RustFlow runtime foundations.
//!
//! The first implemented subsystem is the single-threaded reactive runtime.
//!
//! # Project status
//!
//! RustFlow is not production-ready. See the repository README and roadmap
//! before relying on the project.

extern crate self as rustflow;

pub mod components;
pub mod reactivity;

#[doc = "Builds a typed view tree from constrained HTML-like syntax."]
#[doc = ""]
#[doc = "Text is written as a Rust literal, expressions are wrapped in braces, event handlers use on:event={closure}, DOM properties use prop:name={value}, and @dynamic { expression } defers a child evaluator. Uppercase tags create component descriptors."]
#[doc = ""]
#[doc = "The macro returns Result<View, ViewError>."]
#[doc = ""]
#[doc = "Example:"]
#[doc = "```"]
#[doc = "use rustflow::{view, view::View};"]
#[doc = "fn page() -> Result<View, rustflow::view::ViewError> {"]
#[doc = "    view! { <main class=\"welcome\"><h1>\"Hello, RustFlow\"</h1><button disabled on:click={move |_event| {}}> \"Continue\" </button></main> }"]
#[doc = "}"]
#[doc = "let _page = page().expect(\"valid view\");"]
#[doc = "```"]
#[doc = ""]
#[doc = "A mismatched closing tag produces a source-located compile error:"]
#[doc = ""]
#[doc = "```compile_fail"]
#[doc = "fn invalid() -> Result<rustflow::view::View, rustflow::view::ViewError> {"]
#[doc = "    rustflow::view! { <div>\"text\"</span> }"]
#[doc = "}"]
#[doc = "```"]
pub use rustflow_macros::{component, view, ModelDiagnostics};

/// Common imports for authoring RustFlow reactive views and components.
pub mod prelude {
    pub use crate::components::{
        ComponentCapability, ComponentDefinition, ComponentRegistry, RenderTarget,
        SharedComponentCatalog, SharedComponentCatalogBuilder, SharedComponentDefinition,
    };
    pub use crate::reactivity::{
        Effect, Runtime, RuntimeError, Scope, Signal, on_cleanup, signal, try_signal, use_effect,
        use_state, with_owner_scope,
    };
    pub use crate::view::{
        EventHandler, EventPayload, IntoView, Key, KeyedChild, Property, PropertyValue, View,
    };
    pub use rustflow_macros::{component, view};
}
/// Public, target-independent view-model types.
///
/// This re-export currently supports structural text, element, and fragment
/// nodes only. It does not provide components, rendering, or DOM integration.
///
/// ```
/// use rustflow::view::View;
///
/// fn page() -> Result<View, rustflow::view::ViewError> {
///     View::element(
///         "main",
///         [View::element("h1", [View::text("Hello, RustFlow")])?],
///     )
/// }
///
/// let _page = page().expect("valid static view tree");
/// ```
pub use rustflow_view as view;

/// Returns the framework's current pre-release version.
///
/// This small function exists only to give the initial crate a verifiable
/// public API while the workspace and CI baseline are established.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn package_version_is_available() {
        assert_eq!(version(), "0.1.0");
    }
}
