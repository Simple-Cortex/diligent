//! `BRW-TABLE-HEADER-ASSOC` (WCAG 1.3.1; ACT a25f45, d0f69e).
//!
//! For each `table`, laid out or not, that has a `th` or a `headers` attribute:
//!
//! * `fail` when an id in a `headers` attribute does not name a cell of the
//!   same table, or names the cell that carries the attribute;
//! * `fail` when a `th` has no data cell it applies to, by `scope`, by
//!   position (a `th` without `scope` applies down its columns and along its
//!   rows) or by a `headers` reference;
//! * `pass` otherwise;
//! * `not_tested` for tables with `role="presentation"` or `role="none"`.
//!
//! Does not decide: whether the associations are the right ones, tables built
//! from other elements, or header cells that are `td` with a role.

use std::collections::HashMap;

use serde_json::json;

use super::{clip, finding, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-TABLE-HEADER-ASSOC",
    act: &["a25f45", "d0f69e"],
    informs: &["A11Y-1.3.1"],
    summary: "Table headers refer to cells in their table and apply to data cells",
    channel: "browser",
    never_fails: false,
    absent_reason: "No table with header cells or headers attributes in the baseline capture.",
};

const LIMIT: &str = "Association is judged from scope, position and headers attributes in the DOM. Whether they are the right headers for a cell needs a person.";
const MAX_GRID: usize = 250_000;
const MAX_PAIRS: usize = 4_000_000;

struct Placed {
    cell: usize,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
}

fn owning_table(v: &CaptureView<'_>, el: usize) -> Option<usize> {
    v.ancestors(el)
        .find(|a| v.is_element(*a) && v.tag(*a) == "table")
}

fn span(v: &CaptureView<'_>, cell: usize, name: &str, max: usize) -> usize {
    match v
        .attr(cell, name)
        .map(str::trim)
        .and_then(|s| s.parse::<usize>().ok())
    {
        Some(0) if name == "rowspan" => max,
        Some(0) | None => 1,
        Some(n) => n.min(max).max(1),
    }
}

/// Places the cells of `table` on a grid, honouring `colspan` and `rowspan`.
fn place(v: &CaptureView<'_>, table: usize) -> Option<Vec<Placed>> {
    let rows: Vec<usize> = v
        .descendants(table)
        .into_iter()
        .filter(|e| v.is_element(*e) && v.tag(*e) == "tr" && owning_table(v, *e) == Some(table))
        .collect();
    let mut occupied: HashMap<(usize, usize), ()> = HashMap::new();
    let mut placed: Vec<Placed> = Vec::new();
    let mut budget = MAX_GRID;
    for (r, tr) in rows.iter().enumerate() {
        let mut x = 0usize;
        for &cell in v.children(*tr) {
            if !(v.is_element(cell) && matches!(v.tag(cell), "td" | "th")) {
                continue;
            }
            while occupied.contains_key(&(r, x)) {
                x += 1;
            }
            let cols = span(v, cell, "colspan", 1000);
            let rs = span(v, cell, "rowspan", rows.len() - r);
            let rs = rs.min(rows.len() - r);
            let area = cols.saturating_mul(rs);
            if area > budget {
                return None;
            }
            budget -= area;
            for dr in 0..rs {
                for dc in 0..cols {
                    occupied.insert((r + dr, x + dc), ());
                }
            }
            placed.push(Placed {
                cell,
                row: r,
                col: x,
                rows: rs,
                cols,
            });
            x += cols;
        }
    }
    Some(placed)
}

fn evaluate(v: &CaptureView<'_>, table: usize) -> Option<Finding> {
    let in_table = |el: usize| owning_table(v, el) == Some(table);
    let cells: Vec<usize> = v
        .descendants(table)
        .into_iter()
        .filter(|e| v.is_element(*e) && matches!(v.tag(*e), "td" | "th") && in_table(*e))
        .collect();
    let has_th = cells.iter().any(|c| v.tag(*c) == "th");
    let has_headers = cells.iter().any(|c| v.has_attr(*c, "headers"));
    if !has_th && !has_headers {
        return None;
    }
    let make = |state: State, observed: serde_json::Value| {
        finding(
            &INFO,
            v,
            v.selector(table),
            state,
            "every headers id names a cell of the table and every th applies to a data cell",
            observed,
        )
        .limit(LIMIT)
    };
    let role = v
        .attr(table, "role")
        .and_then(|r| r.split_ascii_whitespace().next());
    if matches!(role, Some("presentation" | "none")) {
        return Some(make(
            State::NotTested,
            json!({ "reason": "table has role presentation or none" }),
        ));
    }

    let mut problems: Vec<String> = Vec::new();
    for &cell in &cells {
        let Some(list) = v.attr(cell, "headers") else {
            continue;
        };
        for token in list.split_ascii_whitespace() {
            let names_cell = v
                .by_id(token)
                .iter()
                .any(|t| *t != cell && matches!(v.tag(*t), "td" | "th") && in_table(*t));
            if !names_cell {
                problems.push(format!(
                    "headers id `{}` on {} does not name another cell of the same table",
                    clip(token, 40),
                    v.selector(cell)
                ));
            }
        }
    }

    let Some(placed) = place(v, table) else {
        return Some(make(
            State::NeedsReview,
            json!({ "reason": "table is too large to model", "problems": problems }),
        ));
    };
    let data: Vec<&Placed> = placed.iter().filter(|p| v.tag(p.cell) == "td").collect();
    let headers: Vec<&Placed> = placed.iter().filter(|p| v.tag(p.cell) == "th").collect();
    if headers.len().saturating_mul(data.len()) > MAX_PAIRS {
        return Some(make(
            State::NeedsReview,
            json!({ "reason": "table is too large to model", "problems": problems }),
        ));
    }
    for th in &headers {
        let scope = v
            .attr(th.cell, "scope")
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_default();
        let by_column = matches!(scope.as_str(), "col" | "colgroup")
            || !matches!(scope.as_str(), "row" | "rowgroup");
        let by_row = matches!(scope.as_str(), "row" | "rowgroup")
            || !matches!(scope.as_str(), "col" | "colgroup");
        let corner = th.row == 0
            && th.col == 0
            && !data.is_empty()
            && headers.iter().any(|h| h.row == 0 && h.col > 0)
            && headers.iter().any(|h| h.col == 0 && h.row > 0);
        let th_id = v.attr(th.cell, "id").filter(|i| !i.is_empty());
        let applies = data.iter().any(|d| {
            let below =
                d.col < th.col + th.cols && th.col < d.col + d.cols && d.row >= th.row + th.rows;
            let right =
                d.row < th.row + th.rows && th.row < d.row + d.rows && d.col >= th.col + th.cols;
            let referenced = th_id.is_some_and(|id| {
                v.attr(d.cell, "headers")
                    .is_some_and(|h| h.split_ascii_whitespace().any(|t| t == id))
            });
            // The corner header of a table with both column and row headers labels the grid.
            let edge = scope.is_empty() && corner;
            (by_column && below) || (by_row && right) || referenced || edge
        });
        if !applies {
            problems.push(format!(
                "th {} (`{}`) has no data cell it applies to",
                v.selector(th.cell),
                clip(&v.visible_text(th.cell), 40)
            ));
        }
    }
    let state = if problems.is_empty() {
        State::Pass
    } else {
        State::Fail
    };
    Some(make(
        state,
        json!({
            "header_cells": headers.len(),
            "data_cells": data.len(),
            "problems": problems,
        }),
    ))
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    v.elements()
        .filter(|el| v.tag(*el) == "table")
        .filter_map(|el| evaluate(v, el))
        .collect()
}
