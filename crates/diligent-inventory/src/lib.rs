//! Fetch-only site inventory and WCAG-EM 2.0 sampler.
//!
//! Audit specification 21.1: sampling includes common templates, unique features, high-consequence tasks,
//! vendors, documents and complete processes, plus a random sample that challenges the
//! structured one. The seed and selection method are always recorded, coverage is reported as
//! counts with denominators, and a sample never supports a whole-site conformance claim.
//!
//! Flow: [`run`] reads robots.txt, walks sitemaps, then crawls same-origin links through the
//! `diligent-infra` fetch layer (vetted, pinned, redirect-revalidated, rate limited, one request
//! at a time). It returns an [`Inventory`] (`INVENTORY.json`) and a [`Sample`] (`SAMPLE.json`).
//!
//! Entry points: [`run`], [`inventory`], [`cli::cli`] (`inventory --url <https://host/> --out <dir>`),
//! and [`audit_plan_sampling`].

pub mod cli;
pub mod clock;
pub mod crawl;
pub mod fixture;
pub mod links;
pub mod model;
pub mod prng;
pub mod robots;
pub mod sample;
pub mod signature;
pub mod sitemap;
pub mod urls;

pub use clock::{Clock, ManualClock, SystemClock};
pub use crawl::{inventory, run};
pub use model::{
    Coverage, Document, Inventory, InventoryOptions, Outcome, ProcessResult, ProcessSpec,
    RandomItem, Route, Sample, StructuredItem, Template, Vendor,
};
pub use sample::audit_plan_sampling;
pub use signature::{signature, Signature};

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str = "File, template, route, dependency and document inventory with explicit exclusions and coverage denominators.";

#[cfg(test)]
mod tests {
    use super::CRATE_ROLE;

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }
}
