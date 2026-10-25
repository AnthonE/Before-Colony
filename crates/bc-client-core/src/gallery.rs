//! The page's gallery (`?gallery` on the page, `web/gallery.js`): every screen and panel the page
//! draws, filled with made-up data that looks like the game's, so a change to the page's look can
//! be checked on all of it at once without playing to each one. `examples/page_gallery.rs` writes
//! it out as JSON (`scripts/build-web.sh` puts it in `web/dist/gallery.json`), the `gallery` e2e
//! suite photographs each scene, and `tests/page_contract.rs` checks every scene's view against
//! what the page reads.
//!
//! A scene is a [`View`] (what the game would send), with the hangar's word for the terminals' tabs.
//! A new screen or tab gets a scene here.

use bc_econ::exchange::Exchange;
use bc_econ::proving::{Board, Feat};
use bc_econ::wire::{HangarView, MarketView, Place};
use bc_econ::{Hangar, Item, Material, Ore, Side};
use serde::Serialize;
use serde_json::Value;

use crate::hangar::HangarState;
use crate::page::{Init, MapAt, PanelKind, Screen, SettingsView, View};
use crate::settings::Settings;

/// One screen, as the game would have it.
#[derive(Clone, Debug, Serialize)]
pub struct Scene {
    /// Its name in the gallery (and its screenshot's file name).
    pub name: &'static str,
    /// What it is, in a line.
    pub about: &'static str,
    /// The page's element it shows (`#id`): the e2e suite checks it's visible.
    pub expect: &'static str,
    pub view: View,
    /// The hangar's word, for a terminal's tab (`terminal::hangar_json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hangar: Option<Value>,
}

/// Everything the gallery shows.
#[derive(Clone, Debug, Serialize)]
pub struct Gallery {
    pub init: Init,
    pub settings: SettingsView,
    /// The terminals' catalogue (`terminal::catalogue`).
    pub catalogue: Value,
    pub scenes: Vec<Scene>,
}

/// A unix time to make the boards' clocks from.
const NOW: u64 = 1_790_000_000;
const ME: &str = "0x5e1f00000000000000000000000000000000c0de";

fn playing() -> View {
    View { screen: Screen::Playing, ..View::default() }
}

fn survival(place: &'static str) -> View {
    View {
        survival: true,
        signed_in: true,
        place,
        bay_line: "BAY 07 · 12,480 CR · PECKISH".into(),
        ..playing()
    }
}

fn scene(name: &'static str, about: &'static str, expect: &'static str, view: View) -> Scene {
    Scene { name, about, expect, view, hangar: None }
}

/// A pilot's hangar a few sorties in: the starter bay with ore and steel in its stores, a job at the
/// fabricator, the exchange with the colony's quotes and an order of theirs resting, the Charter
/// Board's contracts and the Proving Ground's board.
fn hangar_state(place: Place) -> HangarState {
    let mut h = Hangar::starter();
    h.credits = 12_480;
    h.stores.add(Item::Ore(Ore::NickelIron), 18_400);
    h.stores.add(Item::Material(Material::Steel), 2_250);
    let mut ex = Exchange::new();
    ex.seed_missing();
    let _ = ex.place(ME, Item::Ore(Ore::NickelIron), Side::Sell, 1_200, 4_000, 0, true);
    let watched = Item::Ore(Ore::NickelIron);
    let mut board = Board::default();
    for (feat, name, ms) in
        [(Feat::Course, "VEGA-7", 71_420), (Feat::Course, "KESTREL", 74_900), (Feat::Drill, "VEGA-7", 38_250)]
    {
        board.record(feat, name, name, ms, NOW);
    }
    HangarState {
        place: Some(place),
        bay: 7,
        view: Some(HangarView::of(&h, NOW)),
        market: Some(MarketView::of(&ex, &ME.to_string())),
        charter: Some(bc_econ::charter::Board::new().view(ME, NOW)),
        proving: Some(board.view(ME, NOW)),
        book: Some((ex.depth(watched, 8), ex.history(watched))),
        // Forty minutes of food left in them: time for The Arrival.
        body: Some(bc_econ::body::BodyView {
            fed: bc_sim::content::body::Fed::Peckish,
            food_s: 40 * 60,
            well_fed_s: 0,
        }),
        ..HangarState::default()
    }
}

/// The terminal's word, with a line or two in its log.
fn hangar(place: Place, colony: bool, home: Option<(u32, bool)>) -> Value {
    let log =
        [("MADE 2 × STEEL".to_string(), true), ("not enough nickel-iron for 6 batches".to_string(), false)]
            .into_iter()
            .collect();
    crate::terminal::hangar_json(&hangar_state(place), &log, colony, home)
}

/// A flat to let on strip 0 (the first building of flats with one on its ground floor).
fn a_home() -> u32 {
    use bc_sim::colony::city::{CITY, ROWS, Stage, block, lots};
    use bc_sim::colony::homes::{Kind, plan};
    (CITY.0..CITY.1)
        .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
        .find_map(|b| {
            lots(&b).as_slice().iter().find_map(|bd| {
                let p = plan(&b, bd).filter(|p| p.kind == Kind::Flats)?;
                p.home(0, 0)
            })
        })
        .map_or(0, |h| h.id.0)
}

fn terminal(
    name: &'static str,
    about: &'static str,
    place: &'static str,
    tab: &'static str,
    home: Option<(u32, bool)>,
) -> Scene {
    let at = if place == "city" { Place::City } else { Place::Hangar };
    let view = View { panel: PanelKind::Terminal, terminal: tab, on_foot: true, ..survival(place) };
    Scene { name, about, expect: "#terminal", view, hangar: Some(hangar(at, true, home)) }
}

pub fn gallery() -> Gallery {
    let paused = |v: View| View { panel: PanelKind::Pause, ..v };
    let home = a_home();
    let scenes = vec![
        scene("title", "The title: a callsign, the frames, Launch.", "#launch", View::default()),
        scene(
            "connecting",
            "The first dial.",
            "#link",
            View { screen: Screen::Connecting, ..View::default() },
        ),
        scene(
            "signing",
            "Asking the wallet to sign the pilot in.",
            "#link",
            View { screen: Screen::Connecting, signing: true, ..View::default() },
        ),
        scene(
            "failed-retry",
            "The dial failed, and can be tried again.",
            "#link",
            View {
                screen: Screen::Failed,
                message: "Can't reach the server: no route to the sector.".into(),
                retryable: true,
                ..View::default()
            },
        ),
        scene(
            "failed-reload",
            "The page is older than the server: it has to be reloaded.",
            "#reload",
            View {
                screen: Screen::Failed,
                message: "The server speaks a newer protocol than this page. Reload it to fly.".into(),
                reload: true,
                ..View::default()
            },
        ),
        scene(
            "reconnecting",
            "The link dropped mid-flight: the banner, redialing.",
            "#banner",
            View { screen: Screen::Reconnecting, attempt: 2, retry_in: 3, ..playing() },
        ),
        scene(
            "click-to-fly",
            "In the world, the pointer not locked yet.",
            "#prompt",
            View { click_to_fly: true, ..playing() },
        ),
        scene(
            "hint",
            "A first-flight hint.",
            "#hint",
            View { hint: "W A S D and Space / C thrust. The mouse aims; Q / E roll.".into(), ..playing() },
        ),
        scene(
            "toast",
            "A short message at the top.",
            "#toast",
            View { toast_seq: 1, toast: "COURSE SET: FORNAX · AFT WELL".into(), ..playing() },
        ),
        scene(
            "news-debrief",
            "A sortie's news and its payout sheet.",
            "#debrief",
            View {
                news_seq: 1,
                news: "BACK IN THE BAY · 3 KILLS · 1.8 T OF SALVAGE".into(),
                debrief_seq: 1,
                debrief: vec![
                    ("Kills (3)".into(), 1_500),
                    ("Salvage sold to the colony (1.8 t)".into(), 2_340),
                    ("Rounds and munitions".into(), -420),
                    ("Repairs".into(), -610),
                ],
                debrief_net: 2_810,
                ..survival("hangar")
            },
        ),
        scene(
            "news-lost",
            "A suit lost: the news in red.",
            "#news",
            View {
                news_seq: 1,
                news: "SUIT LOST · ITS WRECK IS OUT THERE".into(),
                news_bad: true,
                ..playing()
            },
        ),
        scene("pause-guest", "The menu, flying as a guest.", "#pause", paused(playing())),
        scene(
            "pause-parked",
            "The menu, signed in and at rest on a body.",
            "#pause",
            paused(View { signed_in: true, parkable: true, ..playing() }),
        ),
        scene(
            "pause-hidden",
            "The menu, signed in and in a hide spot.",
            "#pause",
            paused(View {
                signed_in: true,
                hide_spot: "THE AFT WELL",
                survival: true,
                place: "space",
                ..playing()
            }),
        ),
        scene(
            "pause-city",
            "The menu, on foot in the city.",
            "#pause",
            paused(View { on_foot: true, ..survival("city") }),
        ),
        scene("help", "The controls sheet, flying.", "#help", View { help: true, ..playing() }),
        scene(
            "settings",
            "The settings panel, from the menu.",
            "#settings",
            View { panel: PanelKind::Settings, ..playing() },
        ),
        scene(
            "radio",
            "The colony's radio, its line open.",
            "#chat",
            View {
                chat: true,
                radio_seq: 3,
                radio: vec![
                    ("KESTREL".into(), "anyone selling steel? paying 2k a tonne".into()),
                    ("VEGA-7".into(), "on the book at 1,950".into()),
                    ("COLONY".into(), "THE CANAL STRIP'S CAP LIFT IS RUNNING AGAIN".into()),
                ],
                ..survival("city")
            },
        ),
        scene(
            "on-foot",
            "On foot in the bay, at a terminal.",
            "#use",
            View { on_foot: true, prompt: "E  FABRICATOR".into(), ..survival("hangar") },
        ),
        scene(
            "map",
            "The colony's map (M), with an objective.",
            "#map",
            View {
                on_foot: true,
                map: Some(MapAt { strip: 0.0, x: 1_400.0, s: 700.0, heading: 0.6 }),
                map_goal: Some("exchange"),
                map_found: 0b101,
                ..survival("city")
            },
        ),
        terminal("fabricator", "The bay's fabricator.", "hangar", "fabricator", None),
        terminal("stores", "The bay's stores.", "hangar", "stores", None),
        terminal("suit", "The suit's maintenance console.", "hangar", "suit", None),
        terminal("exchange-bay", "The Colony Exchange, from the bay's terminal.", "hangar", "exchange", None),
        terminal("exchange-desk", "The Exchange floor's counter, in the city.", "city", "exchange", None),
        terminal("charter", "The Charter Board's desk.", "city", "charter", None),
        terminal("proving", "The Proving Ground's desk, at the Blast Hall.", "city", "proving", None),
        terminal("wanted", "The Most Wanted, behind The Arrival's bar.", "city", "wanted", None),
        terminal(
            "menu",
            "The Arrival's menu: the staff's kitchen, and how fed the pilot is.",
            "city",
            "menu",
            None,
        ),
        terminal("home-door", "A free home's door.", "city", "home", Some((home, false))),
        terminal("home-inside", "Inside a home.", "city", "home", Some((home, true))),
    ];
    Gallery {
        init: Init::new("VEGA-7".into(), String::new(), false),
        settings: SettingsView::of(&Settings::default()),
        catalogue: crate::terminal::catalogue(),
        scenes,
    }
}
