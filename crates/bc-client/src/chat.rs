//! The colony's radio in the browser (`bc_econ::wire::Request::Say`): `/` opens the page's line,
//! and so does Enter wherever it docks nothing: on foot, and in a suit away from where it docks
//! (`bc_client_core::dock::enter_docks`, which the docking in `onfoot.rs` reads too; on the chart
//! Enter sets a course). Enter sends what's typed, Esc closes it. While it's open no keys reach
//! the suit (`Ui::panel_open`). What's heard goes to the page, which shows the latest lines for a
//! while.

use bc_client_core::dock;
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

pub fn radio(
    keys: Res<ButtonInput<KeyCode>>,
    cmds: Res<UiCmds>,
    mut ui: ResMut<Ui>,
    mut spoken: ResMut<Spoken>,
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    chart: Res<MapOpen>,
) {
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
    // Enter: on foot; in a suit (out of the bay's sequences, off the chart), unless it docks. This
    // runs after the docking (`onfoot::drive_onfoot`), on the same frame's state, so one Enter
    // never does both, nor neither.
    let enter = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let in_suit = !ui.on_foot && !ui.sequence && !chart.0;
    let open = keys.just_pressed(KeyCode::Slash)
        || keys.just_pressed(KeyCode::NumpadDivide)
        || (enter && (ui.on_foot || (in_suit && !dock::enter_docks(&g.core))));
    if open && ui.panel == crate::page::Panel::None {
        ui.chat = true;
    }
}
