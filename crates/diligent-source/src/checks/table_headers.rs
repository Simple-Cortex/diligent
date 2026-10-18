//! `SRC-TABLE-HEADERS` (no ACT rule, informs A11Y-1.3.1).
//!
//! Decides, for each visible `table` that has at least one `td` and no
//! presentational role:
//! - a `headers` token that matches no id: `fail`;
//! - at least one `th`: `pass`;
//! - no `th` but a `scope` or `headers` attribute on a cell: `pass`;
//! - no header cells at all: `needs_review`, since the table may be layout.
//!
//! Does not decide: whether header cells are associated with the right data
//! cells, nested table semantics, or tables built from `div` and CSS grid.

use scraper::ElementRef;
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-TABLE-HEADERS",
    act: &[],
    informs: &["A11Y-1.3.1"],
    summary: "Data tables have header cells or explicit header associations",
    absent_reason: "No visible table with data cells.",
};

const EXPECTED: &str = "data table has th cells, or scope/headers associations";

fn owning_table<'a>(cell: ElementRef<'a>) -> Option<ElementRef<'a>> {
    dom::ancestors(cell).find(|a| dom::tag(*a) == "table" && dom::is_html(*a))
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for table in ctx.elements.iter().copied() {
        if !(dom::is_html(table) && dom::tag(table) == "table") || dom::is_hidden(table) {
            continue;
        }
        if dom::explicit_role(table).is_some_and(|r| dom::is_presentational_role(&r)) {
            continue;
        }
        let cells: Vec<ElementRef<'a>> = table
            .descendent_elements()
            .filter(|e| {
                dom::is_html(*e)
                    && matches!(dom::tag(*e), "td" | "th")
                    && owning_table(*e) == Some(table)
            })
            .collect();
        let data_cells = cells.iter().filter(|c| dom::tag(**c) == "td").count();
        if data_cells == 0 {
            continue;
        }
        let header_cells = cells.len() - data_cells;
        let associations = cells
            .iter()
            .any(|c| dom::has_attr(*c, "scope") || dom::has_attr(*c, "headers"));
        let missing: Vec<&str> = cells
            .iter()
            .filter_map(|c| dom::attr_trim(*c, "headers"))
            .flat_map(|v| v.split_ascii_whitespace())
            .filter(|t| ctx.by_id(t).is_none())
            .collect();
        let observed = json!({
            "data_cells": data_cells,
            "header_cells": header_cells,
            "has_scope_or_headers": associations,
            "missing_header_ids": missing,
        });
        let outcome = if !missing.is_empty() {
            Outcome::new(State::Fail, EXPECTED, observed)
        } else if header_cells > 0 || associations {
            Outcome::new(State::Pass, EXPECTED, observed)
        } else {
            Outcome::new(State::NeedsReview, EXPECTED, observed)
                .limit("No header cells found; the table may be used for layout.")
        };
        em.emit(&INFO, table, outcome);
    }
}
