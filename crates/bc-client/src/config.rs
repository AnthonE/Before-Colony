//! Launch configuration injected by `web/loader.js` as `window.BC_CONFIG`.

use wasm_bindgen::JsValue;

#[derive(Clone, Debug, Default)]
pub struct LaunchConfig {
    /// WebTransport URL, e.g. `https://127.0.0.1:4433/bc`.
    pub wt_url: String,
    /// SHA-256 of the dev server's self-signed certificate (hex), if any.
    pub cert_hash: Option<Vec<u8>>,
    /// `?autopilot=1`: the Mobile Doll AI flies this pilot (used by the E2E test).
    pub autopilot: bool,
    /// `?name=` pilot name.
    pub name: String,
    /// `?frame=wingzero|leo`.
    pub frame: String,
    /// `?mode=echo`: transport smoke test only.
    pub echo: bool,
    /// `?quality=low`: no HDR/bloom/post effects (software rendering, old GPUs). Kept for old links;
    /// `quality` below supersedes it.
    pub low_quality: bool,
    /// `?quality=low|medium|high|ultra`, already resolved by the loader when it was `auto`.
    pub quality: String,
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
}

fn get(obj: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
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
        let name = Some(string("name")).filter(|s| !s.is_empty()).unwrap_or_else(|| "Pilot".into());
        let frame = Some(string("frame")).filter(|s| !s.is_empty()).unwrap_or_else(|| "wingzero".into());
        Self {
            wt_url,
            cert_hash,
            autopilot: flag("autopilot"),
            name,
            frame,
            echo: flag("echo"),
            low_quality: flag("lowQuality"),
            quality: string("quality"),
            showcase: Some(string("showcase")).filter(|s| !s.is_empty()),
            showcase_t: number("t").unwrap_or(0.0),
            showcase_cam: number("cam").map_or(1, |c| c.clamp(1.0, 9.0) as u32),
            showcase_realtime: flag("realtime"),
            showcase_hold: number("hold").map_or(0, |n| n.max(0.0) as u64),
            perf: flag("perf"),
            calm: flag("calm"),
            tonemap: string("tonemap"),
        }
    }
}

/// A frame pilots may fly, by its slug (`leo`, `wingzero`, `heavyarms`…); anything else: None.
pub fn parse_frame(s: &str) -> Option<bc_proto::FrameId> {
    bc_proto::FrameId::from_slug(s).filter(|f| bc_sim::content::playable(*f))
}
