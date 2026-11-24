//! The frame a player's report carries (F7: the page's composer, `web/ui.js`, "Reports"). The page
//! asks for it as the composer opens (`UiCmd::Report { shot: true }`); this takes it with Bevy's
//! `Screenshot` (the world as drawn, never the page's HTML over it), halves it until it is at
//! most [`MAX_WIDTH`] wide, and hands it to the page as RGBA (`window.bcReportShot(width, height,
//! rgba)`). The browser encodes the PNG, so the client carries no encoder of its own. A frame that
//! never comes (the page waits a few frames' time, two seconds at least) is a report without a
//! picture.

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use wasm_bindgen::{JsCast, JsValue};

use crate::page::{UiCmd, UiCmds, get};

/// The widest picture sent (a 1080p frame halves once).
pub const MAX_WIDTH: u32 = 960;

/// The page asked for the frame: take it.
pub fn shoot(cmds: Res<UiCmds>, mut commands: Commands) {
    if cmds.0.iter().any(|c| matches!(c, UiCmd::Report { shot: true, .. })) {
        commands.spawn(Screenshot::primary_window()).observe(hand_over);
    }
}

fn hand_over(captured: On<ScreenshotCaptured>) {
    let image = &captured.image;
    let Some((rgba, w, h)) = rgba8(image) else {
        warn!("report: the frame came in a format the page can't take");
        return;
    };
    let (rgba, w, h) = shrink(rgba, w, h, MAX_WIDTH);
    let Some(window) = web_sys::window() else { return };
    let Ok(f) = get(&window, "bcReportShot").dyn_into::<js_sys::Function>() else { return };
    let pixels = js_sys::Uint8ClampedArray::from(&rgba[..]);
    let _ = f.call3(&window, &JsValue::from(w), &JsValue::from(h), &pixels);
}

/// The captured frame as tightly packed RGBA, opaque (with HDR on, alpha carries brightness, not
/// coverage), or None in a format other than the swapchain's 8-bit ones.
fn rgba8(image: &Image) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = (image.width(), image.height());
    let data = image.data.as_ref()?;
    if data.len() != (w * h * 4) as usize {
        return None;
    }
    let bgra = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => false,
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => true,
        _ => return None,
    };
    let mut px = data.clone();
    for p in px.as_chunks_mut::<4>().0 {
        if bgra {
            p.swap(0, 2);
        }
        p[3] = 255;
    }
    Some((px, w, h))
}

/// Halve an RGBA frame (a 2×2 box) until it is at most `max` wide.
fn shrink(mut px: Vec<u8>, mut w: u32, mut h: u32, max: u32) -> (Vec<u8>, u32, u32) {
    while w > max && w >= 2 && h >= 2 {
        let (nw, nh) = (w / 2, h / 2);
        let mut out = vec![0u8; (nw * nh * 4) as usize];
        let at = |x: usize, y: usize, c: usize| u32::from(px[(y * w as usize + x) * 4 + c]);
        for y in 0..nh as usize {
            for x in 0..nw as usize {
                for c in 0..4 {
                    let sum = at(2 * x, 2 * y, c)
                        + at(2 * x + 1, 2 * y, c)
                        + at(2 * x, 2 * y + 1, c)
                        + at(2 * x + 1, 2 * y + 1, c);
                    out[(y * nw as usize + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        (px, w, h) = (out, nw, nh);
    }
    (px, w, h)
}
