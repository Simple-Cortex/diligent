//! Deterministic applicability-aware gate.
//!
//! Audit specification section 9.5 responsibility: Applicability-aware gate, unresolved obligations and deterministic output.
//!
//! This crate is a compiling scaffold in 0.1.0. The Node implementation in
//! `scripts/` is the executable engine until this crate takes over.

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str =
    "Applicability-aware gate, unresolved obligations and deterministic output.";

#[cfg(test)]
mod tests {
    use super::CRATE_ROLE;

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }
}
