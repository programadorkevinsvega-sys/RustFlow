//! Typed validation contracts for untrusted server input.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Field-level and form-level validation messages.
///
/// Field names are application-defined identifiers. Messages are public output:
/// do not put submitted secrets, credentials, or raw input values in them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationErrors {
    field_errors: BTreeMap<String, Vec<String>>,
    form_errors: Vec<String>,
}

impl ValidationErrors {
    /// Create an empty collection of validation errors.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a public error message for one field.
    pub fn add_field_error(
        &mut self,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> &mut Self {
        self.field_errors
            .entry(field.into())
            .or_default()
            .push(message.into());
        self
    }

    /// Append an error that applies to the form as a whole.
    pub fn add_form_error(&mut self, message: impl Into<String>) -> &mut Self {
        self.form_errors.push(message.into());
        self
    }

    /// Return field errors in deterministic key order.
    pub fn field_errors(&self) -> &BTreeMap<String, Vec<String>> {
        &self.field_errors
    }

    /// Return errors that are not associated with one field.
    pub fn form_errors(&self) -> &[String] {
        &self.form_errors
    }

    /// Whether the collection contains no messages.
    pub fn is_empty(&self) -> bool {
        self.field_errors.values().all(Vec::is_empty) && self.form_errors.is_empty()
    }
}

/// Validate a typed value after it has been decoded at an input boundary.
///
/// Implementations should be deterministic and side-effect free. Return
/// `Ok(())` for a valid value, or structured public messages for invalid data.
pub trait ValidateInput {
    /// Return structured validation errors, if any.
    fn validate(&self) -> Result<(), ValidationErrors>;
}

#[cfg(test)]
mod tests {
    use super::ValidationErrors;

    #[test]
    fn validation_errors_keep_field_and_form_messages_separate_and_ordered() {
        let mut errors = ValidationErrors::new();
        errors
            .add_field_error("password", "The password is too short.")
            .add_form_error("The submitted values could not be accepted.")
            .add_field_error("email", "Enter a valid email address.");

        assert_eq!(
            errors
                .field_errors()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["email", "password"]
        );
        assert_eq!(
            errors.field_errors()["email"],
            ["Enter a valid email address."]
        );
        assert_eq!(
            errors.form_errors(),
            ["The submitted values could not be accepted."]
        );
        assert!(!errors.is_empty());
        assert!(ValidationErrors::new().is_empty());
    }
}
