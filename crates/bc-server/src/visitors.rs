//! The store's traders on the Exchange floor (`docs/FLOOR.md`): everyone signed in on the Elo Pros
//! trade page stands in the room, by the name the store shows, where pilots walking in see them.
//! The store lists them (`GET /api/floor/before-colony/visitors` in AnthonE/scry-forge,
//! `meter/floor.py`: `{at, visitors: [{key, name}]}`, the key an opaque stand-in for the account),
//! and this reads the list every [`EVERY`] and hands it to the plaza ([`Plaza::set_visitors`]).
//!
//! A list it couldn't read leaves the room as it was, until [`STALE`] has passed without a good one:
//! then the room empties, rather than keeping people the store may long since have lost. Off the
//! hot path: a tokio task, like the oracle's.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::plaza::{Plaza, VISITOR_BASE, VISITORS};

/// How often the list is read.
pub const EVERY: Duration = Duration::from_secs(10);
/// How long the room keeps the last good list.
pub const STALE: Duration = Duration::from_secs(60);
/// The most bytes of a list read.
const MAX_BYTES: usize = 64 * 1024;
/// Longest name shown, in characters.
const NAME_MAX: usize = 24;

/// A list as the store sends it, made safe: at most [`VISITORS`] rows, each a key of `[0-9a-z_-]`
/// (up to 64) and a printable name (trimmed to [`NAME_MAX`]); a row that doesn't fit is dropped, and
/// a key twice keeps its first. `None`: not a list at all.
pub fn parse(doc: &serde_json::Value) -> Option<Vec<(String, String)>> {
    let rows = doc.get("visitors")?.as_array()?;
    let mut out: Vec<(String, String)> = Vec::new();
    for row in rows {
        let key = row.get("key").and_then(|k| k.as_str()).unwrap_or_default();
        let ok_key = !key.is_empty()
            && key.len() <= 64
            && key.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'_' || b == b'-');
        let name: String = row
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .trim()
            .chars()
            .take(NAME_MAX)
            .collect();
        if ok_key && !name.is_empty() && !out.iter().any(|(k, _)| k == key) {
            out.push((key.to_string(), name));
        }
        if out.len() == VISITORS as usize {
            break;
        }
    }
    Some(out)
}

async fn read(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<(String, String)>> {
    let res = client.get(url).send().await?.error_for_status()?;
    if res.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
        anyhow::bail!("the list is too big");
    }
    let bytes = res.bytes().await?;
    if bytes.len() > MAX_BYTES {
        anyhow::bail!("the list is too big");
    }
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    parse(&doc).ok_or_else(|| anyhow::anyhow!("not a list of visitors"))
}

/// Reads `url` every [`EVERY`] into `plaza`'s Exchange floor until `shutdown`. Never ends early: a
/// read that fails is logged (now and then) and tried again.
pub async fn run(
    url: String,
    plaza: Arc<Plaza>,
    max_clients: usize,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if max_clients > usize::from(VISITOR_BASE) {
        tracing::warn!(
            "the store's traders stay off the Exchange floor: --max-clients is over {VISITOR_BASE}"
        );
        return;
    }
    let Some(room) = crate::floor::exchange_room() else { return };
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(5)).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("the store's traders stay off the Exchange floor: {e}");
            return;
        }
    };
    tracing::info!(%url, "the store's traders stand on the Exchange floor");
    let mut good = Instant::now();
    let mut said: Option<Instant> = None;
    let mut tick = tokio::time::interval(EVERY);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = shutdown.changed() => return,
        }
        match read(&client, &url).await {
            Ok(list) => {
                good = Instant::now();
                plaza.set_visitors(&room, &list);
            }
            Err(e) => {
                if said.is_none_or(|t| t.elapsed() > Duration::from_secs(300)) {
                    said = Some(Instant::now());
                    tracing::warn!("the store's traders: {e}");
                }
                if good.elapsed() > STALE {
                    plaza.set_visitors(&room, &[]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_list_is_made_safe_row_by_row() {
        let doc = json!({ "at": 1, "visitors": [
            { "key": "a1b2c3", "name": "  Vega\u{7}  " },
            { "key": "A1B2", "name": "Shouting key" },
            { "key": "d4e5", "name": "" },
            { "key": "a1b2c3", "name": "Vega again" },
            { "key": "f6", "name": "A name far longer than any name on a tag should ever be" },
            { "name": "No key" },
        ]});
        let got = parse(&doc).unwrap();
        assert_eq!(
            got,
            [
                ("a1b2c3".to_string(), "Vega".to_string()),
                ("f6".to_string(), "A name far longer than a".to_string()),
            ]
        );
        assert!(parse(&json!({ "people": [] })).is_none());
        let many: Vec<_> = (0..100).map(|i| json!({ "key": format!("k{i}"), "name": "x" })).collect();
        assert_eq!(parse(&json!({ "visitors": many })).unwrap().len(), VISITORS as usize);
    }

    #[test]
    fn traders_stand_on_the_floor_keep_their_spots_and_go_when_unlisted() {
        let room = crate::floor::exchange_room().unwrap();
        let plaza = Plaza::default();
        let list = |names: &[&str]| -> Vec<(String, String)> {
            names.iter().map(|n| (n.to_lowercase(), n.to_string())).collect()
        };
        plaza.set_visitors(&room, &list(&["Vega", "Rook", "Sable"]));
        let here = plaza.in_room(&room, Instant::now());
        assert_eq!(here.len(), 3, "everyone listed stands in the room");
        assert!(here.iter().all(|(id, _, _, visitor)| *visitor && *id >= VISITOR_BASE));
        let vega = here.iter().find(|h| h.1 == "Vega").map(|h| (h.0, h.2)).unwrap();
        // A minute on, without a pose from anyone, they're still there.
        let later = Instant::now() + Duration::from_secs(60);
        assert_eq!(plaza.in_room(&room, later).len(), 3);
        // Rook goes; Vega keeps her slot and her spot.
        plaza.set_visitors(&room, &list(&["Vega", "Sable", "Mira"]));
        let here = plaza.in_room(&room, Instant::now());
        let names: Vec<&str> = here.iter().map(|h| h.1.as_str()).collect();
        assert!(names.contains(&"Mira") && !names.contains(&"Rook"), "{names:?}");
        let again = here.iter().find(|h| h.1 == "Vega").map(|h| (h.0, h.2)).unwrap();
        assert_eq!(again, vega);
        // They're not pilots in the city.
        assert_eq!(plaza.counts().0, 0);
        plaza.set_visitors(&room, &[]);
        assert!(plaza.in_room(&room, Instant::now()).is_empty());
    }

    #[test]
    fn a_full_floor_stands_everyone_apart_between_the_door_and_the_counter() {
        let room = crate::floor::exchange_room().unwrap();
        let plaza = Plaza::default();
        let list: Vec<(String, String)> =
            (0..VISITORS).map(|i| (format!("k{i}"), format!("Trader {i}"))).collect();
        plaza.set_visitors(&room, &list);
        let here = plaza.in_room(&room, Instant::now());
        assert_eq!(here.len(), VISITORS as usize);
        let near = room.depth() - bc_sim::colony::city::COUNTER_GAP - bc_sim::colony::city::COUNTER_DEPTH;
        for (i, a) in here.iter().enumerate() {
            let (u, v) = room.local(a.2.s, a.2.x);
            assert!(u.abs() < room.width() * 0.5 - 1.0 && v > 6.0 && v < near - 2.0, "({u}, {v})");
            for b in &here[i + 1..] {
                let d = (a.2.s - b.2.s).hypot(a.2.x - b.2.x);
                assert!(d > 1.0, "{} and {} {d} m apart", a.1, b.1);
            }
        }
    }
}
