//! Before Colony browser client (Bevy 0.19 → wasm32).

#[cfg(target_arch = "wasm32")]
mod ambience;
#[cfg(target_arch = "wasm32")]
mod anim;
#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod assets;
#[cfg(target_arch = "wasm32")]
mod beams;
#[cfg(target_arch = "wasm32")]
mod blast;
#[cfg(target_arch = "wasm32")]
mod camera;
#[cfg(target_arch = "wasm32")]
mod colony;
#[cfg(target_arch = "wasm32")]
mod config;
#[cfg(target_arch = "wasm32")]
mod damage;
#[cfg(target_arch = "wasm32")]
mod dev_hooks;
#[cfg(target_arch = "wasm32")]
mod echo;
#[cfg(target_arch = "wasm32")]
mod fx;
#[cfg(target_arch = "wasm32")]
mod gfx;
#[cfg(target_arch = "wasm32")]
mod hud;
#[cfg(target_arch = "wasm32")]
mod input;
#[cfg(target_arch = "wasm32")]
mod materials;
#[cfg(target_arch = "wasm32")]
mod missiles_vis;
#[cfg(target_arch = "wasm32")]
mod model;
#[cfg(target_arch = "wasm32")]
mod net;
#[cfg(target_arch = "wasm32")]
mod net_view;
#[cfg(target_arch = "wasm32")]
mod noise;
#[cfg(target_arch = "wasm32")]
mod particles;
#[cfg(target_arch = "wasm32")]
mod perf;
#[cfg(target_arch = "wasm32")]
mod rocks;
#[cfg(target_arch = "wasm32")]
mod salvage_vis;
#[cfg(target_arch = "wasm32")]
mod showcase;
#[cfg(target_arch = "wasm32")]
mod sky;
#[cfg(target_arch = "wasm32")]
mod suits_vis;
#[cfg(target_arch = "wasm32")]
mod transport;
#[cfg(target_arch = "wasm32")]
mod view;
#[cfg(target_arch = "wasm32")]
mod zero_overlay;
#[cfg(target_arch = "wasm32")]
mod zero_vision;

#[cfg(target_arch = "wasm32")]
fn main() {
    app::run();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("bc-client runs in the browser: build it with scripts/build-web.sh");
}
