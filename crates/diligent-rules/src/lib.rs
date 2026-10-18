//! Rules registry: typed data, loading and validation.
//!
//! Audit specification section 9.5 responsibility: Versioned registry, authority records, predicates, source-status checks, stable IDs and schema validation.
//!
//! * [`model`] holds typed serde models for `authorities/registry.json`,
//!   `rules/packs/*.json` and `profiles/*.json`.
//! * [`load_data_root`] reads all three into a [`DataSet`].
//! * [`validate`] ports `scripts/validate-rules.mjs`, including its schema
//!   step, so the Rust and Node outputs can be compared byte for byte.
//! * [`schema_crosscheck`] asks the `jsonschema` crate for a second opinion.
//!
//! The Node implementation in `scripts/` stays authoritative until a shadow
//! suite proves identical output (see `docs/distribution-adr.md` and
//! `docs/rust-shadow-notes.md`).

pub mod applicability;
pub mod crosscheck;
pub mod jsval;
pub mod load;
pub mod model;
pub mod predicate;
pub mod schema;
pub mod validate;

pub use applicability::{
    lint_predicates, parse_scope, resolve_applicability, ApplicabilityError, ApplicabilityFile,
    ApplicabilityOptions, Obligation, ObligationState, Summary, TemporalState,
};
pub use crosscheck::schema_crosscheck;
pub use load::{load_data_root, DataFile, DataSet, FileProblem, LoadError};
pub use model::*;
pub use validate::{validate, Stats, Validation};

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str = "Versioned registry, authority records, predicates, source-status checks, stable IDs and schema validation.";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }

    #[test]
    fn shipped_data_loads_into_typed_models() {
        let data = load_data_root(&repo_root()).unwrap();
        assert_eq!(data.typed_problems(), Vec::<String>::new());
        assert!(data.registry().is_some());
        assert_eq!(data.typed_packs().count(), data.packs.len());
        assert_eq!(data.typed_profiles().count(), data.profiles.len());
    }

    #[test]
    fn forward_compatible_rule_fields_are_optional() {
        // Strip the three optional fields from every shipped rule: each must
        // still deserialize, with the fields as `None`.
        let data = load_data_root(&repo_root()).unwrap();
        let mut checked = 0;
        for file in &data.packs {
            let mut raw = file.raw.clone().unwrap();
            for rule in raw["rules"].as_array_mut().unwrap() {
                let obj = rule.as_object_mut().unwrap();
                for key in ["baseline_test_id", "wcag_mappings", "standards_status"] {
                    obj.remove(key);
                }
            }
            let pack: RulePack = serde_json::from_value(raw).unwrap();
            for rule in &pack.rules {
                assert!(rule.baseline_test_id.is_none());
                assert!(rule.wcag_mappings.is_none());
                assert!(rule.standards_status.is_none());
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn forward_compatible_rule_fields_deserialize_when_present() {
        let data = load_data_root(&repo_root()).unwrap();
        let rules: Vec<&Rule> = data.typed_packs().flat_map(|p| p.rules.iter()).collect();
        // Skip quietly on a data root that predates the fields.
        if rules.iter().all(|r| r.wcag_mappings.is_none()) {
            return;
        }
        let mapping = rules
            .iter()
            .filter_map(|r| r.wcag_mappings.as_ref())
            .flatten()
            .next()
            .unwrap();
        assert!(!mapping.criterion.is_empty());
        assert!(rules.iter().any(|r| r.baseline_test_id.is_some()));
    }

    #[test]
    fn shipped_data_validates_and_both_validators_agree() {
        let root = repo_root();
        let data = load_data_root(&root).unwrap();
        let schemas = root.join("schemas");
        let result = validate(&data, &schemas);
        assert_eq!(result.errors, Vec::<String>::new());
        assert_eq!(schema_crosscheck(&data, &schemas), Vec::<String>::new());
    }

    #[test]
    fn missing_data_root_reports_node_style_errors() {
        let data = load_data_root(Path::new("/nonexistent-diligent-root")).unwrap();
        let result = validate(&data, &repo_root().join("schemas"));
        assert_eq!(
            result.errors,
            vec![
                "authorities/registry.json: file is missing".to_string(),
                "profiles/: no profile files found".to_string(),
                "rules/packs/: no rule pack files found".to_string(),
            ]
        );
        assert!(!result.ok());
    }

    #[test]
    fn json_output_keeps_node_key_order() {
        let v = Validation::default();
        let text = v.to_json();
        let ok = text.find("\"ok\"").unwrap();
        let errors = text.find("\"errors\"").unwrap();
        let stats = text.find("\"stats\"").unwrap();
        assert!(ok < errors && errors < stats);
        assert!(text.contains("\"authorities\": 0"));
    }
}
