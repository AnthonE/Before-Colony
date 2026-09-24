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
}

fn get(obj: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
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
        let wt_url = string("wtUrl");
        let cert_hash = get(&cfg, "certHash").as_string().and_then(|h| decode_hex(&h));
        let name = Some(string("name")).filter(|s| !s.is_empty()).unwrap_or_else(|| "Pilot".into());
        let frame = Some(string("frame")).filter(|s| !s.is_empty()).unwrap_or_else(|| "wingzero".into());
        Self { wt_url, cert_hash, autopilot: flag("autopilot"), name, frame, echo: flag("echo") }
    }
}
