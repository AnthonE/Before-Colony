//! The page around the game, as data: what it shows ([`View`], [`Init`], [`SettingsView`]) and
//! what the player asks of it ([`Cmd`]). Whoever draws it (the browser's `web/ui.js` today, a
//! desktop build's renderer tomorrow) draws these and sends these back; the game decides
//! everything else.
//!
//! The browser gets each as JSON in the field names below (`camelCase` for the view, `snake_case`
//! commands tagged by `cmd`), and `tests/page_contract.rs` reads `web/ui.js` to check that every
//! field the page reads is one this sends and every command it sends is one this takes: a field
//! renamed on one side fails the tests, not a playtest. `examples/page_gallery.rs` fills each
//! screen with made-up data for the page's gallery (`?gallery`), where every panel can be looked at
//! without playing to it.

use bc_sim::content::{PLAYABLE_ORDER, SpecialKind, frame, frame_designation, frame_name, weapon_name};
use serde::{Deserialize, Serialize};

use crate::controls::{BINDINGS, Group};
use crate::settings::{KNOBS, Kind, Settings};

/// Which screen the page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Screen {
    /// Callsign, frame, Launch.
    #[default]
    Title,
    /// The first dial and handshake.
    Connecting,
    /// In the world.
    Playing,
    /// The link dropped and is being redialed (the banner).
    Reconnecting,
    /// Down, waiting for the player (the title, with the reason).
    Failed,
}

/// Which modal panel is up, as the page draws it (the game's own `Panel` says which terminal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    #[default]
    None,
    Pause,
    Settings,
    /// A hangar terminal, or a desk's in the city (`View::terminal` says which tab).
    Terminal,
}

/// Where the pilot stands on the colony's map (M): the strip, `x` along it, `s` across it, and
/// their heading in the map's terms (radians from +x towards −s).
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize)]
pub struct MapAt {
    pub strip: f32,
    pub x: f32,
    pub s: f32,
    pub heading: f32,
}

/// What the page draws, sent whenever it changes.
#[derive(Clone, Debug, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub screen: Screen,
    pub panel: PanelKind,
    /// The controls sheet (F1 or `?`) is up. It isn't modal: flying goes on under it.
    pub help: bool,
    /// Flying needs a click (the pointer isn't locked)...
    pub click_to_fly: bool,
    /// ...and the browser refused the last one.
    pub refused: bool,
    /// What the link screens say.
    pub message: String,
    pub retryable: bool,
    pub reload: bool,
    pub attempt: u32,
    /// Seconds to the next redial.
    pub retry_in: u32,
    /// A short message at the top of the screen, shown when its number moves.
    pub toast_seq: u32,
    pub toast: String,
    /// A first-flight hint (empty: none).
    pub hint: String,
    /// The wallet is being asked to sign the pilot in.
    pub signing: bool,
    pub signed_in: bool,
    /// At rest on or against a body: leaving now parks the suit there.
    pub parkable: bool,
    /// In the air over a body, in its grip.
    pub aloft: bool,
    /// The landmark hide spot the suit is in (empty: none).
    pub hide_spot: &'static str,
    pub survival: bool,
    /// The terminals' tab to open on (empty: no terminal is open).
    pub terminal: &'static str,
    /// Survival rules: `hangar`, `space`, `city`, or empty.
    pub place: &'static str,
    pub on_foot: bool,
    /// At the wheel in the city: `car`, `scooter` or `markab` (empty: not).
    pub driving: &'static str,
    /// Flying inside the colony, and in a trainer from the Blast Hall's gantry.
    pub inside: bool,
    pub trainer: bool,
    /// A launch or homecoming is playing.
    pub sequence: bool,
    /// On foot: what the pilot can use where they stand, and their bay's line.
    pub prompt: String,
    pub bay_line: String,
    /// The colony's map, while it's up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapAt>,
    /// The objective's place on the map, by slug (its door gets the ◆).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_goal: Option<&'static str>,
    /// The sights found, a bit each (`sights`).
    pub map_found: u32,
    /// A sortie's news, shown large when its number moves (`news_bad`: in red).
    pub news_seq: u32,
    pub news: String,
    pub news_bad: bool,
    /// A sortie's payout sheet (`bc_econ::debrief`): its lines, what and credits, and its net.
    pub debrief_seq: u32,
    pub debrief: Vec<(String, i64)>,
    pub debrief_net: i64,
    /// The colony radio's line is open: the pilot is typing.
    pub chat: bool,
    /// The radio's latest lines, who and what, and how many have been heard.
    pub radio_seq: u32,
    pub radio: Vec<(String, String)>,
}

/// A frame to launch in, on the title screen.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrameCard {
    pub slug: &'static str,
    pub name: &'static str,
    pub designation: &'static str,
    pub weapons: Vec<&'static str>,
    /// The special on the H key, in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special: Option<&'static str>,
    pub orrery: bool,
}

/// A section of the controls sheet: its title and its rows, keys and what they do.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ControlGroup {
    pub title: &'static str,
    /// Only in the colony's city (hidden while its inside is closed).
    pub inside: bool,
    pub rows: Vec<(&'static str, &'static str)>,
}

/// A named stretch of a strip, `x` to `x1` along it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct District {
    pub name: &'static str,
    pub s: f32,
    pub x: f32,
    pub x1: f32,
}

/// A key place's door.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MapPlace {
    pub name: &'static str,
    pub slug: &'static str,
    pub s: f32,
    pub x: f32,
}

/// A sight, and its bit on the found-list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MapSight {
    pub name: &'static str,
    pub i: u32,
    pub s: f32,
    pub x: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CityStrip {
    pub name: &'static str,
    pub districts: Vec<District>,
    pub places: Vec<MapPlace>,
    pub sights: Vec<MapSight>,
}

/// The colony's city for the page's map (M): each strip's districts along it, its key places'
/// doors and its sights, and the grid's lines the map draws them on (`bc_sim::colony::city`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CityMap {
    pub strips: Vec<CityStrip>,
    pub width: f32,
    pub x0: f32,
    pub x1: f32,
    pub block: f32,
    pub grid_x0: f32,
    pub avenue: f32,
    pub rows: i32,
    /// The canal's edges across the strip.
    pub canal: [f32; 2],
    /// Hub Gate's square: (s0, s1, x0, x1).
    pub square: [f32; 4],
}

/// Sent once: the frames to choose from, the controls sheet, the city's map, and the pilot to
/// prefill.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Init {
    pub frames: Vec<FrameCard>,
    pub controls: Vec<ControlGroup>,
    pub city: CityMap,
    pub name: String,
    pub frame: String,
    pub autoplay: bool,
}

impl Init {
    pub fn new(name: String, frame: String, autoplay: bool) -> Self {
        Init { frames: frame_cards(), controls: control_groups(), city: city_map(), name, frame, autoplay }
    }
}

/// The special on the H key, by name.
pub fn special_name(kind: SpecialKind) -> Option<&'static str> {
    match kind {
        SpecialKind::None => None,
        SpecialKind::Transform { .. } => Some("Aquila: changes into a fast cruiser form"),
        SpecialKind::Eclipse { .. } => Some("Eclipse: hides it from sensors"),
        SpecialKind::FullSalvo { .. } => Some("Full Salvo: every gun at once"),
        SpecialKind::MeleeMove { .. } => Some("Pincer Strike: the shield's claws"),
        SpecialKind::Ward { .. } => Some("Ward: a field no beam crosses"),
    }
}

pub fn frame_cards() -> Vec<FrameCard> {
    PLAYABLE_ORDER
        .iter()
        .map(|&f| {
            let spec = frame(f);
            FrameCard {
                slug: f.slug(),
                name: frame_name(f),
                designation: frame_designation(f),
                weapons: spec.loadout.iter().flatten().map(|m| weapon_name(m.weapon)).collect(),
                special: special_name(spec.special),
                orrery: spec.orrery,
            }
        })
        .collect()
}

pub fn control_groups() -> Vec<ControlGroup> {
    Group::ALL
        .iter()
        .map(|&g| ControlGroup {
            title: g.title(),
            inside: g.inside(),
            rows: BINDINGS.iter().filter(|b| b.group == g).map(|b| (b.keys, b.action)).collect(),
        })
        .collect()
}

pub fn city_map() -> CityMap {
    use bc_sim::colony::city::{
        AVENUE, BLOCK, CANAL_ROW, CITY, DISTRICT_BLOCKS, HUB_GATE, ROWS, SITE, SQUARE_ROWS, block_rect,
        channel, grid_x, place_door, row_span,
    };
    use bc_sim::colony::frame::{STRIP_WIDTH, STRIPS};
    use bc_sim::content::city::{DISTRICT_NAMES, PLACES, SIGHTS, STRIP_NAMES};
    let strips = (0..STRIPS)
        .map(|k| {
            let mut districts: Vec<District> = DISTRICT_NAMES[k]
                .iter()
                .enumerate()
                .map(|(d, &name)| {
                    let d = d as i32;
                    let b0 = if d == 0 { HUB_GATE.0 } else { CITY.0 + DISTRICT_BLOCKS * d };
                    let b1 = (CITY.0 + DISTRICT_BLOCKS * (d + 1)).min(CITY.1 + 1);
                    District { name, s: 0.0, x: grid_x(b0), x1: grid_x(b1) }
                })
                .collect();
            districts.push(District {
                name: "THE BUILDING SITE",
                s: 0.0,
                x: grid_x(SITE.0),
                x1: grid_x(SITE.1 + 1),
            });
            let places = PLACES
                .iter()
                .filter(|p| p.strip as usize == k)
                .map(|p| {
                    let ((s, x), _) = place_door(p);
                    MapPlace { name: p.name, slug: p.slug, s, x }
                })
                .collect();
            let sights = (0..SIGHTS.len())
                .filter(|&i| SIGHTS[i].0 as usize == k)
                .map(|i| {
                    let (_, s, x) = crate::sights::sight_at(i);
                    MapSight { name: SIGHTS[i].3, i: i as u32, s, x }
                })
                .collect();
            CityStrip { name: STRIP_NAMES[k], districts, places, sights }
        })
        .collect();
    let canal = channel(&block_rect(CITY.0, CANAL_ROW));
    let (sq0, _) = row_span(-SQUARE_ROWS);
    let (_, sq1) = row_span(SQUARE_ROWS);
    CityMap {
        strips,
        width: STRIP_WIDTH,
        x0: grid_x(HUB_GATE.0),
        x1: grid_x(SITE.1 + 1),
        block: BLOCK,
        grid_x0: grid_x(0),
        avenue: AVENUE,
        rows: ROWS,
        canal: [canal.s0, canal.s1],
        square: [sq0, sq1, grid_x(HUB_GATE.0), grid_x(HUB_GATE.1 + 1)],
    }
}

/// How a setting is changed on the panel.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingKind {
    Range { min: f32, max: f32, step: f32 },
    Toggle,
    Choice { choices: &'static [&'static str] },
}

/// One row of the settings panel, and its value as text.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingRow {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    #[serde(flatten)]
    pub kind: SettingKind,
    pub value: String,
}

/// The settings panel: its rows (`settings::KNOBS`) and their values.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsView {
    pub rows: Vec<SettingRow>,
}

impl SettingsView {
    pub fn of(s: &Settings) -> Self {
        let rows = KNOBS
            .iter()
            .map(|k| SettingRow {
                key: k.key,
                label: k.label,
                group: k.group,
                kind: match k.kind {
                    Kind::Range { min, max, step } => SettingKind::Range { min, max, step },
                    Kind::Toggle => SettingKind::Toggle,
                    Kind::Choice(choices) => SettingKind::Choice { choices },
                },
                value: s.get(k.key).unwrap_or_default(),
            })
            .collect();
        SettingsView { rows }
    }
}

const fn yes() -> bool {
    true
}

/// A command from the page (or from keys it forwards, or from a test's or a playtest's hook), as
/// `{cmd: "<name>", ...fields}`. A field left out takes its default.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Cmd {
    /// Launch with this callsign and frame slug, signed in with the wallet at `address` (`0x…`)
    /// or as a guest (empty).
    Play {
        #[serde(default)]
        name: String,
        #[serde(default)]
        frame: String,
        #[serde(default)]
        address: String,
    },
    /// Dial again (after a failure, or before the next scheduled redial).
    Retry,
    /// Stop connecting and go back to the title.
    Cancel,
    /// Close the pause menu and fly.
    Resume,
    /// Open the pause menu.
    Pause,
    /// Leave the world for the title screen.
    Disconnect,
    /// Esc: close whatever is on top, or open the pause menu.
    Back,
    /// F1 or `?`: show or hide the controls sheet (`None`: toggle).
    Help {
        #[serde(default)]
        show: Option<bool>,
    },
    /// Open (`true`, the default) or close the settings panel.
    Settings {
        #[serde(default = "yes")]
        show: bool,
    },
    /// A setting changed on the panel: its key and new value, as text.
    Set {
        #[serde(default)]
        key: String,
        #[serde(default)]
        value: String,
    },
    /// A menu sound: `click` or `confirm`.
    Sfx {
        #[serde(default)]
        cue: String,
    },
    /// Dev hook: drop the link as if the network had failed (tests the reconnect path).
    DropLink,
    /// Something asked of the hangar from one of its terminals (survival rules).
    Hangar { req: bc_econ::Request },
    /// A line for the colony's radio, typed on the page.
    Say {
        #[serde(default)]
        text: String,
    },
    /// Open (`true`) or close the radio's line.
    Chat {
        #[serde(default)]
        open: bool,
    },
    /// Dev hook, on foot: walk to a place in the bay or the city (by its slug).
    WalkTo {
        #[serde(default)]
        spot: String,
    },
    /// Dev hook, flying: keep the aim on the nearest hostile (headless browsers can't lock the
    /// pointer to aim with), or stop.
    AimHostile {
        #[serde(default)]
        on: bool,
    },
    /// Dev hook, flying inside the colony: fly the suit (flight assist) to `up` metres over a
    /// place's door (by its slug), `ahead` metres out from it (or to the inner gate,
    /// `inner_gate`), or stop (an empty `spot`).
    FlyTo {
        #[serde(default)]
        spot: String,
        #[serde(default)]
        up: f32,
        #[serde(default)]
        ahead: f32,
    },
    /// Dev hook (the agents' playtests, `scripts/pt turn`): turn the eyes as the mouse would with
    /// the pointer locked, in degrees, yaw to the right and pitch up.
    Look {
        #[serde(default)]
        yaw: f32,
        #[serde(default)]
        pitch: f32,
    },
    /// Dev hook: use what's in view.
    Use,
    /// Dev hook: skip a launch or homecoming sequence.
    Skip,
    /// Dev hook, on foot at a home building's lift: ride it to storey `storey` (0 the ground).
    Lift {
        #[serde(default)]
        storey: u32,
    },
    /// From a home's panel: put down a piece of furniture (its slug), in build mode.
    Place {
        #[serde(default)]
        furniture: String,
    },
    /// The suit's console: its tab open (`on`), and the view of the suit in its gantry the page's
    /// drag and wheel ask for (`bay::Refit`: `yaw` in radians round it, `zoom` 1 the whole suit).
    SuitView {
        #[serde(default)]
        on: bool,
        #[serde(default)]
        yaw: f32,
        #[serde(default)]
        zoom: f32,
    },
}

impl Cmd {
    /// A command as the page wrote it, as JSON (`None`: not one this takes).
    pub fn parse(json: &str) -> Option<Cmd> {
        serde_json::from_str(json).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_as_the_page_writes_them() {
        assert_eq!(Cmd::parse(r#"{"cmd":"resume"}"#), Some(Cmd::Resume));
        // Fields the command doesn't take are no reason to drop it.
        assert_eq!(Cmd::parse(r#"{"cmd":"use","x":1}"#), Some(Cmd::Use));
        assert_eq!(Cmd::parse(r#"{"cmd":"help"}"#), Some(Cmd::Help { show: None }));
        assert_eq!(Cmd::parse(r#"{"cmd":"help","show":false}"#), Some(Cmd::Help { show: Some(false) }));
        assert_eq!(Cmd::parse(r#"{"cmd":"settings"}"#), Some(Cmd::Settings { show: true }));
        assert_eq!(
            Cmd::parse(r#"{"cmd":"fly_to","spot":"exchange","up":40}"#),
            Some(Cmd::FlyTo { spot: "exchange".into(), up: 40.0, ahead: 0.0 })
        );
        assert_eq!(Cmd::parse(r#"{"cmd":"lift","storey":3}"#), Some(Cmd::Lift { storey: 3 }));
        assert_eq!(
            Cmd::parse(r#"{"cmd":"suit_view","on":true,"yaw":0.5,"zoom":1.2}"#),
            Some(Cmd::SuitView { on: true, yaw: 0.5, zoom: 1.2 })
        );
        assert!(matches!(Cmd::parse(r#"{"cmd":"hangar","req":{"t":"sign"}}"#), Some(Cmd::Hangar { .. })));
        assert_eq!(Cmd::parse(r#"{"cmd":"warp"}"#), None);
        assert_eq!(Cmd::parse("not json"), None);
    }

    #[test]
    fn the_view_is_sent_in_the_pages_names() {
        let v = View {
            screen: Screen::Playing,
            click_to_fly: true,
            map: Some(MapAt::default()),
            ..View::default()
        };
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["screen"], "playing");
        assert_eq!(json["panel"], "none");
        assert_eq!(json["clickToFly"], true);
        assert!(json["map"].is_object());
        // Nothing to say is said by leaving it out.
        assert!(json.get("mapGoal").is_none());
    }

    #[test]
    fn a_settings_row_says_its_kind_inline() {
        let v = SettingsView::of(&Settings::default());
        let json = serde_json::to_value(&v).unwrap();
        let rows = json["rows"].as_array().unwrap();
        assert_eq!(rows.len(), KNOBS.len());
        for r in rows {
            let kind = r["kind"].as_str().unwrap();
            assert!(matches!(kind, "range" | "toggle" | "choice"), "{r}");
            if kind == "range" {
                assert!(r["min"].is_number() && r["max"].is_number() && r["step"].is_number(), "{r}");
            }
            if kind == "choice" {
                assert!(r["choices"].is_array(), "{r}");
            }
            assert!(r["value"].is_string(), "{r}");
        }
    }
}
