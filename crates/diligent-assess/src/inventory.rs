//! The optional site inventory that `BRW-FED-TITLE-CRITERIA` compares titles against.
//!
//! Only `routes[].{url, final_url, title, fetched}` of an `INVENTORY.json` are
//! read, and leniently: unknown fields are ignored, a route that is not an
//! object or has fields of the wrong type contributes what it can, and a file
//! that cannot be read or has no `routes` array becomes
//! [`InventoryState::Unreadable`] so the assessment still runs with a
//! limitation instead of stopping.

use std::path::Path;

use serde_json::Value;

/// Largest inventory file read, in bytes.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// One route of an inventory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InventoryRoute {
    /// Normalized URL as listed.
    pub url: Option<String>,
    /// URL of the final response.
    pub final_url: Option<String>,
    /// Page title, as the inventory cut it.
    pub title: Option<String>,
    /// A response was received. Absent means false.
    pub fetched: bool,
}

/// The routes of one inventory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inventory {
    /// Routes in file order.
    pub routes: Vec<InventoryRoute>,
}

/// What the caller gave `assess` for an inventory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InventoryState {
    /// No inventory was supplied.
    #[default]
    NotSupplied,
    /// The inventory was read.
    Loaded(Inventory),
    /// An inventory was supplied but could not be used; the text says why.
    Unreadable(String),
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Reads the routes out of parsed JSON. `Err` when there is no `routes` array.
pub fn from_value(doc: &Value) -> Result<Inventory, String> {
    let routes = doc
        .get("routes")
        .and_then(Value::as_array)
        .ok_or_else(|| "the file has no `routes` array".to_string())?;
    Ok(Inventory {
        routes: routes
            .iter()
            .filter(|r| r.is_object())
            .map(|r| InventoryRoute {
                url: text(r, "url"),
                final_url: text(r, "final_url"),
                title: text(r, "title"),
                fetched: r.get("fetched").and_then(Value::as_bool).unwrap_or(false),
            })
            .collect(),
    })
}

/// Reads an inventory file into an [`InventoryState`]. Never fails: problems become `Unreadable`.
pub fn load(path: &Path) -> InventoryState {
    let shown = path.display();
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_BYTES => {
            return InventoryState::Unreadable(format!("{shown}: larger than {MAX_BYTES} bytes"));
        }
        Ok(_) => {}
        Err(e) => return InventoryState::Unreadable(format!("{shown}: {e}")),
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return InventoryState::Unreadable(format!("{shown}: {e}")),
    };
    let doc: Value = match serde_json::from_slice(&bytes) {
        Ok(d) => d,
        Err(e) => return InventoryState::Unreadable(format!("{shown}: not JSON ({e})")),
    };
    match from_value(&doc) {
        Ok(inv) => InventoryState::Loaded(inv),
        Err(e) => InventoryState::Unreadable(format!("{shown}: {e}")),
    }
}
