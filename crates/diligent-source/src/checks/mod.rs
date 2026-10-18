//! The static checks. Each module decides one narrow, document-level question
//! and states in its own doc comment what it does not decide.

use crate::emit::Emitter;
use crate::model::CheckInfo;

pub mod aria_ref;
pub mod autocomplete;
pub mod autofill;
pub mod banner;
pub mod button_name;
pub mod control_name;
pub mod dup_id;
pub mod heading_order;
pub mod html_lang;
pub mod identifier_links;
pub mod iframe_name;
pub mod img_alt;
pub mod link_name;
pub mod media_autoplay;
pub mod meta_description;
pub mod meta_refresh;
pub mod svg_name;
pub mod tabindex_positive;
pub mod table_headers;
pub mod title;
pub mod viewport_zoom;

/// A registered check: its description and its runner.
pub struct Check {
    /// Static description.
    pub info: &'static CheckInfo,
    /// Runs the check against one document.
    pub run: for<'a> fn(&mut Emitter<'a>),
}

/// Every check, in the order they appear in the work order table.
pub fn all() -> Vec<Check> {
    vec![
        Check {
            info: &img_alt::INFO,
            run: img_alt::run,
        },
        Check {
            info: &svg_name::INFO,
            run: svg_name::run,
        },
        Check {
            info: &control_name::INFO,
            run: control_name::run,
        },
        Check {
            info: &button_name::INFO,
            run: button_name::run,
        },
        Check {
            info: &link_name::INFO,
            run: link_name::run,
        },
        Check {
            info: &html_lang::INFO,
            run: html_lang::run,
        },
        Check {
            info: &title::INFO,
            run: title::run,
        },
        Check {
            info: &meta_description::INFO,
            run: meta_description::run,
        },
        Check {
            info: &viewport_zoom::INFO,
            run: viewport_zoom::run,
        },
        Check {
            info: &meta_refresh::INFO,
            run: meta_refresh::run,
        },
        Check {
            info: &iframe_name::INFO,
            run: iframe_name::run,
        },
        Check {
            info: &dup_id::INFO,
            run: dup_id::run,
        },
        Check {
            info: &aria_ref::INFO,
            run: aria_ref::run,
        },
        Check {
            info: &autocomplete::INFO,
            run: autocomplete::run,
        },
        Check {
            info: &heading_order::INFO,
            run: heading_order::run,
        },
        Check {
            info: &tabindex_positive::INFO,
            run: tabindex_positive::run,
        },
        Check {
            info: &table_headers::INFO,
            run: table_headers::run,
        },
        Check {
            info: &media_autoplay::INFO,
            run: media_autoplay::run,
        },
        Check {
            info: &banner::INFO,
            run: banner::run,
        },
        Check {
            info: &identifier_links::INFO,
            run: identifier_links::run,
        },
    ]
}
