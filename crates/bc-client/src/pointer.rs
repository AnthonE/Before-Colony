//! Keeps the cursor's lock in step with what the pilot is doing (see
//! `bc_client_core::pointer`): asks the browser each frame whether the pointer is really locked,
//! locks it on a click into the world or Resume, frees it for menus and the chart, and opens the
//! pause menu when the browser takes it back (Esc, alt-tab).

use bc_client_core::pointer::{Grab, Pointer, PointerIn};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::net::{GameClient, now_s};
use crate::page::{Ui, UiCmd, UiCmds};

#[derive(Resource, Default)]
pub struct PointerRes(pub Pointer);

fn browser_locked() -> bool {
    web_sys::window().and_then(|w| w.document()).and_then(|d| d.pointer_lock_element()).is_some()
}

pub fn update_pointer(
    mut pointer: ResMut<PointerRes>,
    mut ui: ResMut<Ui>,
    cmds: Res<UiCmds>,
    map: Res<crate::map::MapOpen>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    game: NonSend<GameClient>,
) {
    let autopilot = game.borrow().autopilot;
    let playing = ui.playing() && !autopilot;
    let out = pointer.0.step(PointerIn {
        now: now_s(),
        playing,
        // The chart wants the cursor, as a panel does (the pilot stays engaged: it locks again
        // when the chart closes).
        panel_open: ui.panel_open() || map.0,
        clicked: mouse.just_pressed(MouseButton::Left),
        resume: cmds.has(&UiCmd::Resume),
        asked: cursor.grab_mode == CursorGrabMode::Locked,
        browser_locked: browser_locked(),
    });
    match out.grab {
        Grab::Lock => {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
        Grab::Release => {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
        Grab::Leave => {}
    }
    if out.open_pause {
        ui.open_pause();
    }
    if out.refused {
        ui.refused = true;
    }
    let flying = pointer.0.flying();
    if flying {
        ui.refused = false;
    }
    ui.click_to_fly = playing && !ui.panel_open() && !map.0 && !flying;
}
