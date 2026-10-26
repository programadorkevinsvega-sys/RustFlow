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

pub mod components;
pub mod reactivity;

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
