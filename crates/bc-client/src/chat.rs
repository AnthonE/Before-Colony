//! The colony's radio in the browser (`bc_econ::wire::Request::Say`): `/` opens the page's line,
//! and so does Enter wherever it docks nothing: on foot, and in a suit away from where it docks
//! (`bc_client_core::dock::EnterKey`: this decides where each press goes, and the docking in
//! `onfoot.rs` and the docking hints read it; on the chart Enter sets a course). Enter sends what's
//! typed, Esc closes it. While it's open no keys reach the suit (`Ui::panel_open`). What's heard
//! goes to the page, which shows the latest lines for a while.

use bc_client_core::dock::{self, EnterKey};
use bevy::prelude::*;

use crate::map::MapOpen;
use crate::net::{GameClient, NetState};
use crate::page::{Ui, UiCmd, UiCmds};

/// The page keeps this many of the latest lines.
pub const SHOWN: usize = 6;

/// What each callsign last said, and when (the page's clock): in the city it's shown over their
/// head (`people::name_tags`).
#[derive(Resource, Default)]
pub struct Spoken(pub std::collections::HashMap<String, (String, f64)>);

/// Where this frame's press of Enter went, as [`radio`] decided it: the docking
/// (`onfoot::drive_onfoot`) and the docking hints (`settings::update_hints`) read it rather than
/// deciding again on a suit that has moved on since.
#[derive(Resource, Default)]
pub struct Enter(pub EnterKey);

/// Runs early in the frame, before the pointer's step (`pointer::update_pointer`): the frame that
/// opens the line lets the pointer go too, as the page moves the keyboard's focus to it.
#[allow(clippy::too_many_arguments)]
pub fn radio(
    keys: Res<ButtonInput<KeyCode>>,
    cmds: Res<UiCmds>,
    mut ui: ResMut<Ui>,
    mut spoken: ResMut<Spoken>,
    mut enter: ResMut<Enter>,
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    chart: Res<MapOpen>,
) {
    enter.0 = EnterKey::Unused;
    let mut g = game.borrow_mut();
    // What's been heard.
    let now = crate::net::now_s();
    for (from, text) in g.core.hangar.said.drain(..) {
        spoken.0.insert(from.clone(), (text.clone(), now));
        ui.radio.push((from, text));
        ui.radio_seq += 1;
    }
    let extra = ui.radio.len().saturating_sub(SHOWN);
    ui.radio.drain(..extra);
    if !ui.playing() {
        ui.chat = false;
        return;
    }
    for cmd in &cmds.0 {
        match cmd {
            UiCmd::Chat(open) => ui.chat = *open,
            UiCmd::Say(text) => {
                if let (Some(line), Some(t)) = (bc_econ::wire::clean_line(text), net.get()) {
                    t.send_control(g.core.request(&bc_econ::Request::Say { text: line }));
                }
            }
            _ => {}
        }
    }
    // Enter: on foot; in a suit (out of the bay's sequences, off the chart), unless it docks, which
    // is decided here once (the docking reads it).
    let pressed = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let in_suit = !ui.on_foot && !ui.sequence && !chart.0;
    let docks = pressed && in_suit && dock::enter_docks(&g.core);
    enter.0 = EnterKey::route(pressed, !ui.panel_open(), ui.on_foot, in_suit, docks);
    let open = keys.just_pressed(KeyCode::Slash) || keys.just_pressed(KeyCode::NumpadDivide);
    if (open && ui.panel == crate::page::Panel::None) || enter.0 == EnterKey::Radio {
        ui.chat = true;
    }
}
