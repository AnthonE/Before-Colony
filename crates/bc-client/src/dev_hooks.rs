//! `window.__bc`: a small status object the Playwright tests (and curious humans) can read.

use bevy::prelude::*;
use wasm_bindgen::JsValue;

/// Fields published to `window.__bc` a few times a second.
#[derive(Resource, Default)]
pub struct DevStatus {
    pub fields: Vec<(&'static str, JsValue)>,
}

impl DevStatus {
    pub fn set(&mut self, key: &'static str, value: impl Into<JsValue>) {
        let value = value.into();
        match self.fields.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.fields.push((key, value)),
        }
    }
}

pub struct DevHooksPlugin;

impl Plugin for DevHooksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DevStatus>().add_systems(Last, publish);
    }
}

fn publish(
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    mut frames: Local<u32>,
    mut fps_window: Local<(f32, u32)>,
    mut status: ResMut<DevStatus>,
) {
    *frames += 1;
    let now = time.elapsed_secs();
    if *frames == 1 {
        hide_boot_overlay();
    }
    if now - fps_window.0 >= 2.0 {
        let fps = (*frames - fps_window.1) as f32 / (now - fps_window.0);
        *fps_window = (now, *frames);
        status.set("fps", fps);
    }
    if now - *last < 0.25 {
        return;
    }
    *last = now;
    let frames = *frames;
    status.set("frames", frames);
    let Some(window) = web_sys::window() else { return };
    let obj = js_sys::Object::new();
    for (k, v) in &status.fields {
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), v);
    }
    let _ = js_sys::Reflect::set(&window, &JsValue::from_str("__bc"), &obj);
}

/// Fades out the HTML "loading" overlay once Bevy renders its first frame.
fn hide_boot_overlay() {
    let boot = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("boot"));
    if let Some(el) = boot {
        let _ = el.class_list().add_1("hidden");
    }
}
