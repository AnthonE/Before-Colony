//! The page (`web/ui.js`) and the game (`bc_client_core::page`, `terminal`) agree on what crosses
//! between them: every field the page reads is one the game sends, every field of the view is
//! read, and every command the page (or a test, or a playtest's hook) sends is one the game takes.
//! A field renamed on one side fails here, not in a playtest where a panel quietly shows nothing.
//!
//! The page is plain JavaScript with no build step, so this reads its source: the names after
//! `v.` in the functions handed the view, after `view.` anywhere, and so on, each checked against
//! the JSON the game would send (the gallery's, which fills every field).

use std::collections::BTreeSet;
use std::path::Path;

use bc_client_core::gallery::gallery;
use bc_client_core::page::{Cmd, MapAt, View};
use serde_json::Value;

fn repo(path: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn ui_js() -> String {
    repo("web/ui.js")
}

/// The body of the function declared by `head` (`function update(v)`).
fn body<'a>(js: &'a str, head: &str) -> &'a str {
    let start = js.find(head).unwrap_or_else(|| panic!("web/ui.js has no `{head}`"));
    let open = start + js[start..].find('{').expect("a body");
    let mut depth = 0;
    for (i, c) in js[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &js[open..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("`{head}`'s body never closes");
}

/// The fields read off `var` in `js` (`var.name` and `var?.name`, `var` not itself a field).
fn reads(js: &str, var: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = js.as_bytes();
    let mut at = 0;
    while let Some(i) = js[at..].find(var) {
        let i = at + i;
        at = i + var.len();
        let before = if i == 0 { b' ' } else { b[i - 1] };
        if before.is_ascii_alphanumeric() || before == b'_' || before == b'.' || before == b'$' {
            continue;
        }
        let rest = &js[at..];
        let rest = rest.strip_prefix("?.").or_else(|| rest.strip_prefix('.'));
        let Some(rest) = rest else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object().unwrap_or_else(|| panic!("not an object: {v}")).keys().cloned().collect()
}

/// Every key any of `objects` has.
fn keys_of<'a>(objects: impl IntoIterator<Item = &'a Value>) -> BTreeSet<String> {
    objects.into_iter().flat_map(keys).collect()
}

/// Read but not sent: what the page would find undefined.
fn assert_sent(what: &str, read: &BTreeSet<String>, sent: &BTreeSet<String>) {
    let missing: Vec<_> = read.difference(sent).collect();
    assert!(
        missing.is_empty(),
        "web/ui.js reads {missing:?} off {what}, which the game doesn't send (it sends {sent:?})"
    );
}

/// A view with every field set, the way the game sends it.
fn full_view() -> Value {
    let v = View { map: Some(MapAt::default()), map_goal: Some("exchange"), ..View::default() };
    serde_json::to_value(v).unwrap()
}

#[test]
fn the_page_reads_only_what_the_view_sends_and_reads_all_of_it() {
    let js = ui_js();
    let mut read = reads(body(&js, "function update(v)"), "v");
    read.extend(reads(body(&js, "function renderRadio(v)"), "v"));
    read.extend(reads(&js, "view"));
    let sent = keys(&full_view());
    assert_sent("the view", &read, &sent);
    let unread: Vec<_> = sent.difference(&read).collect();
    assert!(unread.is_empty(), "the game sends {unread:?} in the view, and web/ui.js never reads it");

    let at = reads(body(&js, "function drawMap(at, goal, found)"), "at");
    assert_sent("the map's `at`", &at, &keys(&full_view()["map"]));
}

#[test]
fn the_page_reads_only_what_init_sends() {
    let js = ui_js();
    let g = serde_json::to_value(gallery()).unwrap();
    let init = &g["init"];
    assert_sent("init", &reads(body(&js, "function init(data)"), "data"), &keys(init));
    let frames = init["frames"].as_array().unwrap();
    assert_sent("a frame", &reads(body(&js, "function renderFrames()"), "f"), &keys_of(frames));
    let groups = init["controls"].as_array().unwrap();
    assert_sent(
        "a controls group",
        &reads(body(&js, "function renderControls(groups)"), "g"),
        &keys_of(groups),
    );

    let city = &init["city"];
    assert_sent("the city", &reads(&js, "city"), &keys(city));
    let strips = city["strips"].as_array().unwrap();
    let map = body(&js, "function drawMap(at, goal, found)");
    assert_sent("a strip", &reads(map, "strip"), &keys_of(strips));
    let districts = strips.iter().flat_map(|s| s["districts"].as_array().unwrap());
    assert_sent("a district", &reads(map, "d"), &keys_of(districts));
    // Places and sights are both marked as `p`.
    let points = strips
        .iter()
        .flat_map(|s| s["places"].as_array().unwrap().iter().chain(s["sights"].as_array().unwrap()));
    assert_sent("a place or a sight", &reads(map, "p"), &keys_of(points));
}

#[test]
fn the_page_reads_only_what_the_settings_send() {
    let js = ui_js();
    let g = serde_json::to_value(gallery()).unwrap();
    let settings = body(&js, "function renderSettings(data)");
    assert_sent("the settings", &reads(settings, "data"), &keys(&g["settings"]));
    let rows = g["settings"]["rows"].as_array().unwrap();
    assert_sent("a settings row", &reads(settings, "r"), &keys_of(rows));
}

#[test]
fn the_page_reads_only_what_the_terminals_send() {
    let js = ui_js();
    let g = serde_json::to_value(gallery()).unwrap();
    assert_sent("the catalogue", &reads(&js, "catalogue"), &keys(&g["catalogue"]));
    let hangars = g["scenes"].as_array().unwrap().iter().filter_map(|s| s.get("hangar"));
    assert_sent("the hangar", &reads(&js, "hangar"), &keys_of(hangars));
    // The suit's console, and what its refit says of a slot.
    let consoles: Vec<&Value> = g["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.get("hangar")?.get("console").filter(|c| c.is_object()))
        .collect();
    let suit = body(&js, "function renderSuit()");
    let cards = body(&js, "function suitCards(slot, suit, con, line)");
    let mut read = reads(suit, "con");
    read.extend(reads(cards, "con"));
    assert_sent("the suit's console", &read, &keys_of(consoles.iter().copied()));
    let slots = consoles.iter().flat_map(|c| c["refit"].as_object().into_iter().flat_map(|m| m.values()));
    assert_sent("a slot of the refit", &reads(cards, "r"), &keys_of(slots));
}

/// Command names sent by `pattern`'s matches: the word in quotes right after it.
fn sent_after(text: &str, pattern: &str) -> BTreeSet<String> {
    text.match_indices(pattern)
        .filter_map(|(i, _)| {
            let rest = &text[i + pattern.len()..];
            let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            let name: String = rest[1..].chars().take_while(|c| *c != quote).collect();
            Some(name)
        })
        .collect()
}

fn takes(name: &str) -> bool {
    // A field it needs, so the name is all that's in question.
    let json = serde_json::json!({ "cmd": name, "req": { "t": "sign" } });
    match serde_json::from_value::<Cmd>(json) {
        Ok(_) => true,
        Err(e) => !e.to_string().contains("unknown variant"),
    }
}

#[test]
fn every_command_sent_is_one_the_game_takes() {
    let js = ui_js();
    let html = repo("web/index.html");
    let mut sent = sent_after(&js, "send(");
    // The menu's buttons send their `data-cmd`, but for those the page turns into another
    // command first (`cmd === "controls"`: `help`).
    let renamed = sent_after(&js, "cmd === ");
    sent.extend(sent_after(&html, "data-cmd=").difference(&renamed).cloned());
    // The e2e suites and the playtests push commands of their own (dev hooks).
    for dir in ["e2e/tests", "scripts", ".claude/skills"] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(dir);
        for path in walk(&root) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for pattern in ["cmd: ", "\"cmd\": ", "\"cmd\":", "cmd:"] {
                sent.extend(sent_after(&text, pattern));
            }
        }
    }
    assert!(sent.len() > 15, "found only {sent:?}: has the page changed how it sends?");
    let unknown: Vec<_> = sent.iter().filter(|n| !takes(n)).collect();
    assert!(unknown.is_empty(), "commands sent that bc_client_core::page::Cmd doesn't take: {unknown:?}");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "node_modules" || n == "artifacts") {
                continue;
            }
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| matches!(x.to_str(), Some("ts" | "js" | "md" | "sh" | "py")))
            || p.extension().is_none()
        {
            out.push(p);
        }
    }
    out
}

#[test]
fn every_scene_in_the_gallery_is_named_once_and_shows_something() {
    let g = gallery();
    let mut names = BTreeSet::new();
    for s in &g.scenes {
        assert!(names.insert(s.name), "two scenes are called {}", s.name);
        assert!(s.expect.starts_with('#'), "{}: expect an element by id, not {}", s.name, s.expect);
        let html = repo("web/index.html");
        assert!(
            html.contains(&format!("id=\"{}\"", &s.expect[1..])),
            "{}: no {} on the page",
            s.name,
            s.expect
        );
        if s.view.terminal.is_empty() {
            assert!(s.hangar.is_none(), "{}: a hangar without a terminal open", s.name);
        } else {
            let h = s.hangar.as_ref().unwrap_or_else(|| panic!("{}: a terminal with no hangar", s.name));
            assert!(h["view"].is_object(), "{}: the hangar has no view", s.name);
        }
        if s.view.terminal == "home" {
            assert!(s.hangar.as_ref().is_some_and(|h| h["home"].is_object()), "{}: no home", s.name);
        }
        if s.view.terminal == "menu" {
            assert!(s.hangar.as_ref().is_some_and(|h| h["body"].is_object()), "{}: no body", s.name);
        }
    }
}
