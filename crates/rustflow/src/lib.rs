//! RustFlow is an early-stage Rust-first full-stack web framework.
//!
//! This crate is the initial workspace anchor. Public APIs will be introduced
//! only after their behavior and architectural boundaries are specified.
//!
//! # Project status
//!
//! RustFlow is not production-ready. See the repository README and roadmap
//! before relying on the project.

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
