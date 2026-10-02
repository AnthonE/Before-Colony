//! The pilot's settings: what they are, their ranges, and the text they're kept as (the browser
//! keeps it in localStorage under `bc.settings`).
//!
//! One implementation, here: the page's settings panel is drawn from [`KNOBS`] and every change it
//! makes comes back through [`Settings::set`], which parses and clamps. The text is `key = value`
//! lines a person can read; a key this build doesn't know is kept as it is (a newer build wrote
//! it), and a known key with a bad value falls back to its default.

/// The version this build writes.
pub const SETTINGS_VERSION: u32 = 1;

/// The graphics tier to use: `Auto` lets the page pick from the GPU it finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxChoice {
    Auto,
    Low,
    Medium,
    High,
    Ultra,
}

impl GfxChoice {
    pub const ALL: [GfxChoice; 5] =
        [GfxChoice::Auto, GfxChoice::Low, GfxChoice::Medium, GfxChoice::High, GfxChoice::Ultra];
    pub const NAMES: [&'static str; 5] = ["auto", "low", "medium", "high", "ultra"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::NAMES.iter().position(|n| *n == s).map(|i| Self::ALL[i])
    }
}

/// Where the camera sits in flight: behind the suit, or in its cockpit (first person, through the
/// head's cameras).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraView {
    #[default]
    Chase,
    Cockpit,
}

impl CameraView {
    pub const ALL: [CameraView; 2] = [CameraView::Chase, CameraView::Cockpit];
    pub const NAMES: [&'static str; 2] = ["chase", "cockpit"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::NAMES.iter().position(|n| *n == s).map(|i| Self::ALL[i])
    }

    /// The other view (the key that switches them).
    pub fn toggled(self) -> Self {
        match self {
            CameraView::Chase => CameraView::Cockpit,
            CameraView::Cockpit => CameraView::Chase,
        }
    }
}

/// How a knob is edited.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Range { min: f32, max: f32, step: f32 },
    Toggle,
    Choice(&'static [&'static str]),
}

/// One row of the settings panel.
#[derive(Clone, Copy, Debug)]
pub struct Knob {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub kind: Kind,
}

const fn range(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    min: f32,
    max: f32,
    step: f32,
) -> Knob {
    Knob { key, label, group, kind: Kind::Range { min, max, step } }
}

const fn toggle(key: &'static str, label: &'static str, group: &'static str) -> Knob {
    Knob { key, label, group, kind: Kind::Toggle }
}

/// Everything the settings panel shows, in order.
pub const KNOBS: &[Knob] = &[
    range("sensitivity", "Mouse sensitivity", "CONTROLS", 0.25, 3.0, 0.05),
    toggle("invert_y", "Invert mouse Y", "CONTROLS"),
    Knob { key: "camera", label: "Flight camera", group: "VIEW", kind: Kind::Choice(&CameraView::NAMES) },
    range("fov", "Field of view", "VIEW", 60.0, 100.0, 1.0),
    range("shake", "Camera shake and screen effects", "VIEW", 0.0, 1.0, 0.05),
    toggle("flashing", "Flashing effects", "VIEW"),
    toggle("hints", "Hints for new pilots", "VIEW"),
    toggle("objectives", "Objectives and their waypoint", "VIEW"),
    toggle("lead", "Lead marker when locked on", "VIEW"),
    Knob { key: "gfx", label: "Graphics quality", group: "GRAPHICS", kind: Kind::Choice(&GfxChoice::NAMES) },
    range("vol_master", "Master volume", "SOUND", 0.0, 1.0, 0.05),
    range("vol_effects", "Weapons and impacts", "SOUND", 0.0, 1.0, 0.05),
    range("vol_cockpit", "Cockpit, alarms and menus", "SOUND", 0.0, 1.0, 0.05),
    range("vol_music", "Music", "SOUND", 0.0, 1.0, 0.05),
];

/// The pilot's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The last callsign launched with.
    pub name: String,
    /// The last frame launched in (a slug).
    pub frame: String,
    /// A multiplier on the base mouse sensitivity.
    pub sensitivity: f32,
    pub invert_y: bool,
    /// Where the camera sits in flight.
    pub camera: CameraView,
    /// Field of view, degrees.
    pub fov: f32,
    /// How much of the camera shake, kicks and warps to keep, 0..1.
    pub shake: f32,
    /// Full-screen flicker and flashes (a ZERO seizure's); off for photosensitive pilots.
    pub flashing: bool,
    pub gfx: GfxChoice,
    /// Show the first-flight hints.
    pub hints: bool,
    /// The hints already shown (bits of `hints::Hint`).
    pub hints_seen: u32,
    /// Show the current objective and its waypoint.
    pub objectives: bool,
    /// The objectives done (bits of `objectives::Objective`), and the Mobile Dolls downed.
    pub objectives_done: u32,
    pub dolls_downed: u32,
    /// Locked on, the ◆ that shows where to lead the target.
    pub lead: bool,
    /// Volumes, 0..1.
    pub vol_master: f32,
    pub vol_effects: f32,
    pub vol_cockpit: f32,
    pub vol_music: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: String::new(),
            frame: String::new(),
            sensitivity: 1.0,
            invert_y: false,
            camera: CameraView::Chase,
            fov: 70.0,
            shake: 1.0,
            flashing: true,
            gfx: GfxChoice::Auto,
            hints: true,
            hints_seen: 0,
            objectives: true,
            objectives_done: 0,
            dolls_downed: 0,
            lead: true,
            vol_master: 0.8,
            vol_effects: 0.9,
            vol_cockpit: 0.8,
            vol_music: 0.5,
        }
    }
}

fn knob(key: &str) -> Option<&'static Knob> {
    KNOBS.iter().find(|k| k.key == key)
}

/// A number within a range knob's bounds, on its step.
fn clamp_to(key: &str, v: f32) -> f32 {
    match knob(key).map(|k| k.kind) {
        Some(Kind::Range { min, max, step }) => {
            let v = v.clamp(min, max);
            let stepped = min + ((v - min) / step).round() * step;
            // Two decimals: 0.05 steps don't come back as 0.30000001.
            ((stepped * 100.0).round() / 100.0).clamp(min, max)
        }
        _ => v,
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "on" | "yes" => Some(true),
        "false" | "0" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn parse_num(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Printable, one line, at most `max` characters.
fn clean(s: &str, max: usize) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Settings {
    /// The value of `key` as text (what the page shows).
    pub fn get(&self, key: &str) -> Option<String> {
        Some(match key {
            "name" => self.name.clone(),
            "frame" => self.frame.clone(),
            "sensitivity" => self.sensitivity.to_string(),
            "invert_y" => self.invert_y.to_string(),
            "camera" => self.camera.name().to_string(),
            "fov" => self.fov.to_string(),
            "shake" => self.shake.to_string(),
            "flashing" => self.flashing.to_string(),
            "gfx" => self.gfx.name().to_string(),
            "hints" => self.hints.to_string(),
            "hints_seen" => self.hints_seen.to_string(),
            "objectives" => self.objectives.to_string(),
            "objectives_done" => self.objectives_done.to_string(),
            "dolls_downed" => self.dolls_downed.to_string(),
            "lead" => self.lead.to_string(),
            "vol_master" => self.vol_master.to_string(),
            "vol_effects" => self.vol_effects.to_string(),
            "vol_cockpit" => self.vol_cockpit.to_string(),
            "vol_music" => self.vol_music.to_string(),
            _ => return None,
        })
    }

    /// Sets `key` from text, clamped to its range. `false` if the key is unknown or the value
    /// doesn't parse (nothing changes).
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match key {
            "name" => self.name = clean(value, 16),
            "frame" => self.frame = clean(value, 16),
            "sensitivity" | "fov" | "shake" | "vol_master" | "vol_effects" | "vol_cockpit" | "vol_music" => {
                let Some(v) = parse_num(value) else { return false };
                let v = clamp_to(key, v);
                match key {
                    "sensitivity" => self.sensitivity = v,
                    "fov" => self.fov = v,
                    "shake" => self.shake = v,
                    "vol_master" => self.vol_master = v,
                    "vol_effects" => self.vol_effects = v,
                    "vol_cockpit" => self.vol_cockpit = v,
                    _ => self.vol_music = v,
                }
            }
            "invert_y" | "hints" | "flashing" | "objectives" | "lead" => {
                let Some(b) = parse_bool(value) else { return false };
                match key {
                    "invert_y" => self.invert_y = b,
                    "hints" => self.hints = b,
                    "objectives" => self.objectives = b,
                    "lead" => self.lead = b,
                    _ => self.flashing = b,
                }
            }
            "gfx" => {
                let Some(g) = GfxChoice::parse(value) else { return false };
                self.gfx = g;
            }
            "camera" => {
                let Some(c) = CameraView::parse(value) else { return false };
                self.camera = c;
            }
            "hints_seen" | "objectives_done" | "dolls_downed" => {
                let Ok(n) = value.trim().parse() else { return false };
                match key {
                    "hints_seen" => self.hints_seen = n,
                    "objectives_done" => self.objectives_done = n,
                    _ => self.dolls_downed = n,
                }
            }
            _ => return false,
        }
        true
    }

    /// Every key, in the order the file lists them.
    const KEYS: [&'static str; 19] = [
        "name",
        "frame",
        "sensitivity",
        "invert_y",
        "camera",
        "fov",
        "shake",
        "flashing",
        "gfx",
        "hints",
        "hints_seen",
        "objectives",
        "objectives_done",
        "dolls_downed",
        "lead",
        "vol_master",
        "vol_effects",
        "vol_cockpit",
        "vol_music",
    ];
}

/// Settings read from text, with what the file had that this build doesn't know.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// The version the file was written by.
    pub version: u32,
    /// Lines with keys this build doesn't know, kept verbatim for the next save.
    pub unknown: Vec<String>,
}

fn key_shaped(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Reads settings text over `defaults`. Never fails: junk lines are dropped, and a known key with
/// a bad value keeps its default.
pub fn parse(text: &str, defaults: Settings) -> Loaded {
    let mut settings = defaults;
    let mut version = SETTINGS_VERSION;
    let mut unknown: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = value.trim();
        if key == "version" {
            if let Ok(v) = value.parse() {
                version = v;
            }
        } else if Settings::KEYS.contains(&key) {
            settings.set(key, value);
        } else if key_shaped(key) {
            // A newer build's knob: keep its last value.
            unknown.retain(|(k, _)| k != key);
            unknown.push((key.to_string(), format!("{key} = {value}")));
        }
    }
    Loaded { settings, version, unknown: unknown.into_iter().map(|(_, l)| l).collect() }
}

/// The text to keep. `version` is the loaded file's: a newer build's file stays marked as newer.
pub fn serialize(s: &Settings, version: u32, unknown: &[String]) -> String {
    let mut out = String::from("# Before Colony settings. Keys this build doesn't know are kept.\n");
    out.push_str(&format!("version = {}\n", version.max(SETTINGS_VERSION)));
    for key in Settings::KEYS {
        if let Some(v) = s.get(key) {
            out.push_str(&format!("{key} = {v}\n"));
        }
    }
    for line in unknown {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings {
            name: "Heero".into(),
            frame: "wingzero".into(),
            sensitivity: 1.35,
            invert_y: true,
            camera: CameraView::Cockpit,
            fov: 85.0,
            shake: 0.25,
            flashing: false,
            gfx: GfxChoice::Medium,
            hints: false,
            hints_seen: 0b1011,
            objectives: false,
            objectives_done: 0b101,
            dolls_downed: 3,
            lead: false,
            vol_master: 0.6,
            vol_effects: 1.0,
            vol_cockpit: 0.35,
            vol_music: 0.0,
        };
        let text = serialize(&s, SETTINGS_VERSION, &[]);
        let back = parse(&text, Settings::default());
        assert_eq!(back.settings, s);
        assert!(back.unknown.is_empty());
        assert_eq!(back.version, SETTINGS_VERSION);
    }

    #[test]
    fn a_newer_builds_keys_survive_in_order() {
        let text = "version = 7\nvoice_chat = 0.3\nfov = 90\nzoom_style = \"snappy\"\nvoice_chat = 0.4\n";
        let loaded = parse(text, Settings::default());
        assert_eq!(loaded.settings.fov, 90.0);
        assert_eq!(
            loaded.unknown,
            vec!["zoom_style = \"snappy\"".to_string(), "voice_chat = 0.4".to_string()]
        );
        let written = serialize(&loaded.settings, loaded.version, &loaded.unknown);
        assert!(written.contains("version = 7\n"), "a newer file stays marked newer");
        assert!(written.contains("zoom_style = \"snappy\"\n") && written.contains("voice_chat = 0.4\n"));
        assert!(!written.contains("voice_chat = 0.3"), "the last value wins");
    }

    #[test]
    fn junk_and_bad_values_fall_back() {
        let text = "fov = lots\nsensitivity = NaN\n=== \nnot a line\ninvert_y = maybe\n!!bad key = 3\n";
        let loaded = parse(text, Settings::default());
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.unknown.is_empty());
    }

    #[test]
    fn values_are_clamped_to_their_knobs() {
        let mut s = Settings::default();
        assert!(s.set("fov", "500"));
        assert_eq!(s.fov, 100.0);
        assert!(s.set("sensitivity", "0.01"));
        assert_eq!(s.sensitivity, 0.25);
        assert!(s.set("shake", "0.333"));
        assert_eq!(s.shake, 0.35);
        assert!(!s.set("gfx", "potato"));
        assert!(s.set("gfx", "ULTRA"));
        assert_eq!(s.gfx, GfxChoice::Ultra);
        assert!(!s.set("camera", "helicopter"));
        assert_eq!(s.camera, CameraView::Chase);
        assert!(s.set("camera", " Cockpit "));
        assert_eq!(s.camera, CameraView::Cockpit);
        assert!(!s.set("nope", "1"));
    }

    #[test]
    fn the_camera_toggles_between_its_two_views() {
        for c in CameraView::ALL {
            assert_ne!(c.toggled(), c);
            assert_eq!(c.toggled().toggled(), c);
            assert_eq!(CameraView::parse(c.name()), Some(c));
        }
        // An older build's file has no camera line: the chase camera, as before.
        let loaded = parse("version = 1\nfov = 80\n", Settings::default());
        assert_eq!(loaded.settings.camera, CameraView::Chase);
        // Nor a flashing line: whatever the defaults say (off for a pilot who asked for calm).
        let calm = Settings { flashing: false, ..Settings::default() };
        assert!(!parse("version = 1\n", calm).settings.flashing);
        assert!(parse("version = 1\n", Settings::default()).settings.flashing);
    }

    #[test]
    fn names_stay_one_short_line() {
        let mut s = Settings::default();
        s.set("name", "  Zechs\nMerquise the Lightning Count  ");
        assert_eq!(s.name, "ZechsMerquise th");
        let back = parse(&serialize(&s, 1, &[]), Settings::default());
        assert_eq!(back.settings.name, s.name);
    }

    #[test]
    fn every_knob_reads_and_writes() {
        let mut s = Settings::default();
        for k in KNOBS {
            let v = s.get(k.key).unwrap_or_else(|| panic!("{} has no value", k.key));
            assert!(s.set(k.key, &v), "{} doesn't take its own value", k.key);
        }
        assert_eq!(s, Settings::default());
    }
}
