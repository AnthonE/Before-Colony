//! `elo.json` at the repo root: this game's manifest for Elo Pros (`meter/gamerepo.py` in
//! AnthonE/scry-forge reads it from the repo, once the listing takes its commits). It parses, it is
//! this game's at this version, it declares exactly the stats, achievements and boards the server
//! reports (`bc_server::gamestats`), and it names no server keys until the operator's real one
//! exists: the block replaces the whole set, and an empty one revokes every key.

fn manifest() -> serde_json::Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../elo.json");
    let text = std::fs::read_to_string(path).expect("elo.json at the repo root");
    serde_json::from_str(&text).expect("elo.json parses")
}

#[test]
fn the_manifest_is_this_games() {
    let m = manifest();
    assert_eq!(m["elo"], 1, "the standard Elo Pros speaks");
    assert_eq!(m["game"], bc_auth::GAME);
    assert_eq!(m["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(m["multiplayer"], "persistent");
    // The community terms' version it accepts, a date (`openlane.TERMS_VERSION`).
    let terms = m["terms"].as_str().expect("terms");
    assert!(terms.len() == 10 && terms.bytes().all(|c| c.is_ascii_digit() || c == b'-'), "{terms}");
    // Only what a manifest carries (`gamerepo.parse`), and notes of ours (`_…`, ignored there).
    let known = ["elo", "game", "version", "terms", "multiplayer", "stats", "server_keys"];
    for k in m.as_object().expect("an object").keys() {
        assert!(k.starts_with('_') || known.contains(&k.as_str()), "{k}: not a field this manifest writes");
    }
}

#[test]
fn it_names_server_keys_only_once_there_is_one() {
    let m = manifest();
    let Some(keys) = m.get("server_keys") else { return };
    let keys = keys.as_array().expect("server_keys is a list");
    assert!(!keys.is_empty(), "an empty server_keys revokes every key: leave the block out instead");
    for k in keys {
        let id = k["id"].as_str().expect("a key's id");
        assert!(bc_server::serverkey::key_id_ok(id), "{id}");
        let public = k["pub"].as_str().expect("a key's public half");
        assert_eq!(bc_server::elo::parse_pubkey(public).map(|p| p.len()), Some(32), "{public}");
    }
}

#[test]
fn it_declares_what_the_server_reports() {
    assert_eq!(manifest()["stats"], bc_server::gamestats::declaration());
}
