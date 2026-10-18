//! Canonical node ids.
//!
//! Chrome numbers backend DOM nodes and synthetic accessibility nodes from a
//! per-process counter that drifts between runs. Left alone, the same page
//! hashes differently every time. [`CanonicalIds`] renumbers those ids so that
//! captures are content-addressed in a useful way, while keeping the joins
//! between the DOM snapshot, accessibility tree and focus walk intact.
//!
//! Rules, applied per viewport (one page load):
//!
//! - A DOM node gets `index + 1` of the first DOM snapshot taken at that viewport.
//! - Any other non-negative id (a node only the accessibility tree knows, or
//!   the style element of the `text-spacing` mode) gets the next free number
//!   after the DOM range, in order of first use.
//! - Negative accessibility ids are synthetic. Within one tree they become
//!   `-1`, `-2`, ... in order of first appearance, node id before children,
//!   nodes in tree order.

use crate::browser::{AxTree, DomSnapshot, FocusWalk};
use std::collections::HashMap;

/// Renumbering state for one page load.
pub(crate) struct CanonicalIds {
    map: HashMap<i64, i64>,
    next: i64,
}

impl CanonicalIds {
    /// Build the DOM-order mapping from the first snapshot of a page load.
    pub(crate) fn from_dom(dom: &DomSnapshot) -> Self {
        let mut map = HashMap::new();
        for (index, node) in dom.nodes.iter().enumerate() {
            if let Some(id) = node.backend_node_id {
                map.entry(id).or_insert(index as i64 + 1);
            }
        }
        CanonicalIds {
            map,
            next: dom.nodes.len() as i64 + 1,
        }
    }

    fn id(&mut self, old: i64) -> i64 {
        if let Some(&new) = self.map.get(&old) {
            return new;
        }
        let new = self.next;
        self.next += 1;
        self.map.insert(old, new);
        new
    }

    /// Renumber `backend_node_id` of every DOM node.
    pub(crate) fn dom(&mut self, dom: &mut DomSnapshot) {
        for node in &mut dom.nodes {
            if let Some(id) = node.backend_node_id.as_mut() {
                *id = self.id(*id);
            }
        }
    }

    /// Renumber accessibility node ids, child references and DOM backend ids.
    pub(crate) fn ax(&mut self, tree: &mut AxTree) {
        let mut synthetic: HashMap<i64, i64> = HashMap::new();
        let mut convert = |ids: &mut Self, text: &str| -> String {
            match text.parse::<i64>() {
                Ok(old) if old < 0 => {
                    let fresh = -(synthetic.len() as i64) - 1;
                    synthetic.entry(old).or_insert(fresh).to_string()
                }
                Ok(old) => ids.id(old).to_string(),
                Err(_) => text.to_string(),
            }
        };
        for node in &mut tree.nodes {
            node.node_id = convert(self, &node.node_id);
            for child in &mut node.children {
                *child = convert(self, child);
            }
            if let Some(id) = node.backend_dom_node_id.as_mut() {
                *id = self.id(*id);
            }
        }
    }

    /// Renumber `backend_node_id` of every focus stop.
    pub(crate) fn focus(&mut self, walk: &mut FocusWalk) {
        for stop in &mut walk.stops {
            stop.backend_node_id = self.id(stop.backend_node_id);
        }
    }
}
