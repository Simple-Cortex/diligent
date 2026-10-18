//! Browser-worker protocol and runtime captures.
//!
//! Audit specification section 9.5 responsibility: Browser-worker protocol, trusted captures, interaction traces, measurements and session labels.
//!
//! The worker talks to a browser over the Chrome DevTools Protocol behind the
//! [`browser::Browser`] trait. Rust owns orchestration; the browser side only
//! runs the reviewed constant scripts in [`scripts`]. See `docs/browser-worker.md`.

pub mod browser;
pub(crate) mod canonical;
#[cfg(feature = "chrome")]
pub mod chrome;
#[cfg(feature = "chrome")]
pub mod cli;
pub mod collect;
pub mod protocol;
pub mod scripts;
pub mod util;
pub mod worker;

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str = "Browser-worker protocol, trusted captures, interaction traces, measurements and session labels.";

#[cfg(test)]
mod tests {
    use super::CRATE_ROLE;

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }
}
