//! Launch configuration injected by `web/loader.js` as `window.BC_CONFIG`.

use bc_client_core::brains::Plan;
use bc_sim::bodies::Body;
use glam::Vec3;
use wasm_bindgen::JsValue;

#[derive(Clone, Debug, Default)]
pub struct LaunchConfig {
    /// WebTransport URL, e.g. `https://127.0.0.1:4433/bc`.
    pub wt_url: String,
    /// SHA-256 of the dev server's self-signed certificate (hex), if any.
    pub cert_hash: Option<Vec<u8>>,
    /// `?autopilot=`: who flies this pilot (the E2E tests'), if anyone: `1`, the Mobile Doll AI;
    /// `lander`, a lander that walks MO-II; `lander:walk:<landmark>`, one that walks a landmark;
    /// `lander:hide:<landmark>:<spot>`, one that hides in a landmark's hide spot.
    pub autopilot: Option<Autopilot>,
    /// `?autoplay=1` (implied by `?autopilot=`, unless `?autoplay=0`): skip the title screen and
    /// launch straight away.
    pub autoplay: bool,
    /// The browser has no WebTransport (the page still loads, to say so).
    pub no_web_transport: bool,
    /// `?name=` pilot name.
    pub name: String,
    /// `?frame=`: a playable frame's slug (`leo`, `wingzero`, `heavyarms`, `deathscythe`,
    /// `sandrock`, `shenlong`).
    pub frame: String,
    /// `?mode=echo`: transport smoke test only.
    pub echo: bool,
    /// `?quality=low`: no HDR/bloom/post effects (software rendering, old GPUs). Kept for old links;
    /// `quality` below supersedes it.
    pub low_quality: bool,
    /// `?quality=low|medium|high|ultra`, already resolved by the loader when it was `auto`.
    pub quality: String,
    /// The loader's pick for this GPU, whatever the URL said.
    pub quality_auto: String,
    /// `?quality=` as given (empty when absent): it overrides the saved setting for this visit.
    pub quality_param: String,
    /// `?showcase=<scene>`: an offline, scripted scene for building and reviewing visuals.
    pub showcase: Option<String>,
    /// `?t=`: showcase start time, seconds.
    pub showcase_t: f64,
    /// `?cam=`: showcase camera preset (1-9).
    pub showcase_cam: u32,
    /// `?realtime=1`: run the showcase on the wall clock instead of a fixed 60 Hz step.
    pub showcase_realtime: bool,
    /// `?hold=N`: stop the showcase clock after N frames (0: never), for exact screenshots.
    pub showcase_hold: u64,
    /// `?perf=1`: frame-time overlay.
    pub perf: bool,
    /// `?calm=1`, or the browser's reduced-motion setting: camera shake, kicks and warps turned down.
    pub calm: bool,
    /// `?tonemap=tony|agx|aces`: the tonemapper, for comparing them (default TonyMcMapface).
    pub tonemap: String,
    /// `?look=0`: the plain look (no grade, vignette or lit smoke), for comparing against.
    pub look: bool,
    /// `?life=0`: no traffic or people in the city (`life.rs`), for comparing against and measuring.
    pub life: bool,
    /// `?hz=N`: the showcase's fixed clock rate (default 60), to see effects at a low frame rate.
    pub showcase_hz: f64,
}

fn get(obj: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

pub(crate) fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

impl LaunchConfig {
    pub fn from_window() -> Self {
        let Some(window) = web_sys::window() else { return Self::default() };
        let cfg = get(&window, "BC_CONFIG");
        let string = |k: &str| get(&cfg, k).as_string().unwrap_or_default();
        let flag = |k: &str| get(&cfg, k).as_bool().unwrap_or(false);
        let number = |k: &str| get(&cfg, k).as_f64();
        let wt_url = string("wtUrl");
        let cert_hash = get(&cfg, "certHash").as_string().and_then(|h| decode_hex(&h));
        let name = string("name");
        let frame = string("frame");
        let autopilot = Autopilot::parse(&string("autopilot"));
        let autoplay = string("autoplay");
        Self {
            wt_url,
            cert_hash,
            autopilot,
            autoplay: autoplay == "1" || (autopilot.is_some() && autoplay != "0"),
            no_web_transport: flag("noWebTransport"),
            name,
            frame,
            echo: flag("echo"),
            low_quality: flag("lowQuality"),
            quality: string("quality"),
            quality_auto: string("qualityAuto"),
            quality_param: string("qualityParam"),
            showcase: Some(string("showcase")).filter(|s| !s.is_empty()),
            showcase_t: number("t").unwrap_or(0.0),
            showcase_cam: number("cam").map_or(1, |c| c.clamp(1.0, 9.0) as u32),
            showcase_realtime: flag("realtime"),
            showcase_hold: number("hold").map_or(0, |n| n.max(0.0) as u64),
            perf: flag("perf"),
            calm: flag("calm"),
            tonemap: string("tonemap"),
            look: get(&cfg, "look").as_bool().unwrap_or(true),
            life: get(&cfg, "life").as_bool().unwrap_or(true),
            showcase_hz: number("hz").filter(|h| *h >= 1.0).unwrap_or(60.0).min(240.0),
        }
    }
}

/// Who flies the pilot's suit for them (`?autopilot=`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Autopilot {
    /// The Mobile Doll brain, with ZERO engaged.
    Doll,
    /// A lander: it flies to a body, lands on it with its grip, and walks or hides there.
    Lander(Plan),
}

impl Autopilot {
    /// `1`, `lander`, `lander:walk:<landmark>` or `lander:hide:<landmark>:<spot>` (a number left
    /// out is 0); anything else (or nothing): none.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        let num = |i: usize| parts.get(i).map_or(Some(0), |p| p.parse::<u8>().ok());
        // A landmark's top, at an angle to its axis (a station's pylons stand along the others).
        let walk = |landmark: u8| Plan::Walk {
            body: Body::Landmark(landmark),
            dir_local: Vec3::new(1.0, 0.45, 0.45),
        };
        match parts.as_slice() {
            ["1"] => Some(Self::Doll),
            ["lander"] => Some(Self::Lander(walk(0))),
            ["lander", "walk", ..] => Some(Self::Lander(walk(num(2)?))),
            ["lander", "hide", ..] => Some(Self::Lander(Plan::Hide { landmark: num(2)?, spot: num(3)? })),
            _ => None,
        }
    }
}

/// A frame pilots may fly, by its slug (`leo`, `wingzero`, `heavyarms`…); anything else: None.
pub fn parse_frame(s: &str) -> Option<bc_proto::FrameId> {
    bc_proto::FrameId::from_slug(s).filter(|f| bc_sim::content::playable(*f))
}
