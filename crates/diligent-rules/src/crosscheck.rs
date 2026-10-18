//! Independent second opinion on schema validity using the `jsonschema` crate.
//!
//! The error text printed by `rules validate` comes from the port of the Node
//! validator (see `schema.rs`). This module only answers one question per
//! file: do the port and `jsonschema` agree on valid versus invalid? A
//! disagreement means the port, a schema keyword or the data has drifted and
//! should be investigated before any Node-to-Rust cutover.

use std::path::Path;

use jsonschema::Draft;
use serde_json::Value;

use crate::load::{DataFile, DataSet};
use crate::schema::validate_value;

/// One line per file where the two validators disagree about validity.
/// Empty means they agree on every readable file.
pub fn schema_crosscheck(data: &DataSet, schemas_dir: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    check_one(
        "authority-registry",
        "authorities",
        &data.registry,
        schemas_dir,
        &mut lines,
    );
    for f in &data.packs {
        check_one("rule-pack", "rules/packs", f, schemas_dir, &mut lines);
    }
    for f in &data.profiles {
        check_one("profile", "profiles", f, schemas_dir, &mut lines);
    }
    lines
}

fn check_one<T>(
    schema_name: &str,
    dir_label: &str,
    file: &DataFile<T>,
    schemas_dir: &Path,
    lines: &mut Vec<String>,
) {
    let Ok(instance) = &file.raw else { return };
    let label = format!("{dir_label}/{}", file.file_name);
    let schema_path = schemas_dir.join(format!("{schema_name}.schema.json"));
    let schema: Value = match std::fs::read_to_string(&schema_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
    {
        Some(v) => v,
        None => {
            lines.push(format!("{label}: schema {schema_name} could not be read"));
            return;
        }
    };
    let port_valid = matches!(validate_value(&schema, instance), Ok(issues) if issues.is_empty());
    let validator = match jsonschema::options()
        .with_draft(Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
    {
        Ok(v) => v,
        Err(e) => {
            lines.push(format!(
                "{label}: jsonschema could not compile {schema_name}: {e}"
            ));
            return;
        }
    };
    let crate_valid = validator.is_valid(instance);
    if port_valid != crate_valid {
        let detail = validator
            .iter_errors(instance)
            .next()
            .map(|e| format!("; jsonschema reports: {e}"))
            .unwrap_or_default();
        lines.push(format!(
            "{label}: port says {}, jsonschema says {}{detail}",
            word(port_valid),
            word(crate_valid)
        ));
    }
}

fn word(valid: bool) -> &'static str {
    if valid {
        "valid"
    } else {
        "invalid"
    }
}
