//! WebAssembly DOM adapter for RustFlow.
//!
//! The DOM implementation is only compiled for `wasm32-unknown-unknown`.
//! Its validation policy also compiles natively so security-related rules can
//! be tested without a browser.

mod policy;

pub use policy::{DomPolicyError, validate_attribute, validate_element, validate_property};

#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(target_arch = "wasm32")]
pub use browser::{DomError, DomRenderer, MountedDom, ReactiveMountedDom};
