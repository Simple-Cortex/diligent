//! Report rendering.
//!
//! Audit specification section 9.5 responsibility: JSON and readable report rendering, traceability, repair plans and retest status.
//!
//! This crate is a compiling scaffold in 0.1.0. The Node implementation in
//! `scripts/` is the executable engine until this crate takes over.

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str =
    "JSON and readable report rendering, traceability, repair plans and retest status.";

#[cfg(test)]
mod tests {
    use super::CRATE_ROLE;

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }
}
