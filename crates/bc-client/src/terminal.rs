//! The hangar's terminals (survival rules), on the page: the fabricator, the stores, the suit's
//! maintenance console and the Colony Exchange are HTML panels (`web/ui.js`), opened by walking up
//! to them in the bay (`onfoot.rs`). This sends them what they show (`bc_client_core::terminal`
//! builds it): the catalogue once, then the pilot's hangar whenever the server's word on it, or
//! the log of what it said, changes.
//!
//! What the pilot asks for comes back as `UiCmd::Hangar` (a `bc_econ::Request`), and `onfoot.rs`
//! sends it on. The server decides everything; the page only shows its word.

use std::collections::VecDeque;

use bc_client_core::terminal::{catalogue, hangar_json};
use bevy::prelude::*;
use wasm_bindgen::JsValue;

use crate::net::GameClient;
use crate::page::{call_ui, get};

/// What the server said, oldest first (the terminals show the last few).
#[derive(Resource, Default)]
pub struct TerminalLog {
    pub lines: VecDeque<(String, bool)>,
    /// Bumped by every line.
    pub seq: u64,
}

impl TerminalLog {
    const KEEP: usize = 12;

    pub fn push(&mut self, text: String, ok: bool) {
        if self.lines.len() == Self::KEEP {
            self.lines.pop_front();
        }
        self.lines.push_back((text, ok));
        self.seq += 1;
    }
}

/// What the page was last sent: the hangar's version, the log's, and the home whose door or panel
/// is open (and whether the pilot's in it).
type Sent = (u64, u64, Option<(u32, bool)>);

/// Last thing each frame (survival rules): the catalogue once the page can take it, then the
/// hangar whenever the server's word on it, or the log, moved.
pub fn publish_terminal(
    game: NonSend<GameClient>,
    log: Res<TerminalLog>,
    ui: Res<crate::page::Ui>,
    me: Res<crate::onfoot::OnFoot>,
    mut catalogued: Local<bool>,
    mut sent: Local<Option<Sent>>,
) {
    let g = game.borrow();
    let h = &g.core.hangar;
    if h.place.is_none() {
        return;
    }
    let Some(w) = web_sys::window() else { return };
    if get(&get(&w, "bcUi"), "hangar").is_undefined() {
        return; // the page script isn't in yet: next frame
    }
    if !*catalogued {
        *catalogued = true;
        call_ui("catalogue", &JsValue::from_str(&catalogue().to_string()));
    }
    // A home's door or panel: which, and whether the pilot's in the home.
    let home = match ui.panel {
        crate::page::Panel::Home(id) => {
            let inside = me.city.as_ref().is_some_and(|c| {
                let at = c.feet();
                bc_sim::colony::homes::home_at(at.strip, at.s, at.x, at.h + 1.0)
                    == Some(bc_sim::colony::homes::HomeId(id))
            });
            Some((id, inside))
        }
        _ => None,
    };
    let now = (h.version, log.seq, home);
    if *sent == Some(now) {
        return;
    }
    *sent = Some(now);
    let colony = g.core.welcome.is_some_and(|w| w.colony);
    call_ui("hangar", &JsValue::from_str(&hangar_json(h, &log.lines, colony, home).to_string()));
}
