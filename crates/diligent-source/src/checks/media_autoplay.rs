//! `SRC-MEDIA-AUTOPLAY` (ACT 80f0bf, informs A11Y-1.4.2).
//!
//! Decides: an `audio` or `video` element with `autoplay` and without `muted`
//! has `controls`. With controls it is a `pass`; without, a `fail`. Elements
//! without `autoplay` or with `muted` are not applicable.
//!
//! Does not decide: whether the media has an audio track, whether it plays
//! for 3 seconds or less (both cases make ACT 80f0bf pass), or playback
//! started by script.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-MEDIA-AUTOPLAY",
    act: &["80f0bf"],
    informs: &["A11Y-1.4.2"],
    summary: "Autoplaying unmuted media offers controls",
    absent_reason: "No visible audio or video element autoplays with sound.",
};

const EXPECTED: &str = "audio/video with autoplay and no muted attribute has controls";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        if !(dom::is_html(el) && matches!(dom::tag(el), "audio" | "video")) {
            continue;
        }
        if !dom::has_attr(el, "autoplay") || dom::has_attr(el, "muted") || dom::is_hidden(el) {
            continue;
        }
        let controls = dom::has_attr(el, "controls");
        let state = if controls { State::Pass } else { State::Fail };
        let o = Outcome::new(
            state,
            EXPECTED,
            json!({ "tag": dom::tag(el), "autoplay": true, "muted": false, "controls": controls }),
        )
        .limit("Audio track presence and playback length are not known from the markup.");
        em.emit(&INFO, el, o);
    }
}
