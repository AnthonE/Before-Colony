//! The colony's radio in the browser (`bc_econ::wire::Request::Say`): `/` opens the page's line
//! (Enter too, on foot, where Enter docks nothing), Enter sends what's typed, Esc closes it. While
//! it's open no keys reach the suit (`Ui::panel_open`). What's heard goes to the page, which shows
//! the latest lines for a while.

use bevy::prelude::*;

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
    let open = keys.just_pressed(KeyCode::Slash)
        || keys.just_pressed(KeyCode::NumpadDivide)
        || (ui.on_foot && keys.just_pressed(KeyCode::Enter));
    if open && ui.panel == crate::page::Panel::None {
        ui.chat = true;
    }
}
