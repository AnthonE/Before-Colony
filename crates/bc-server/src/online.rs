//! Who is on, for the Elo Pros store (`/online.json`): each signed-in session's wallet, its
//! callsign, whether an agent flies it, since when, and a line of what they're doing ([`Status`],
//! the store's rich presence). The platform's playtime polls it once a
//! minute and counts hours per wallet (`meter/playtime.py` in AnthonE/scry-forge), in the shape
//! Gates' shard serves (`crates/server/src/online.rs` there). `wallet` is the pilot's id as Elo Pros
//! writes it: `0x…` lowercase, base58 exactly as written, or an account's `acct_…`. Guests have none
//! and aren't in it. Off the hot path: sessions enter and leave it, the HTTP side reads it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bc_proto::auth::PlayerId;

use crate::pilots::unix_now;

struct Entry {
    name: String,
    agent: bool,
    since: u64,
    session: u64,
    status: Status,
}

/// What a pilot is doing, as the store shows it beside the game on their profile and friends list
/// (`status` in a row; `meter/playtime.py` `clean_roster`). Coarse on purpose: on foot it's open
/// season anywhere, so a line never says where in the colony someone is (their home least of all),
/// only what they're at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// On foot anywhere: the bay, Bay Row, the town, the city.
    #[default]
    InTheColony,
    /// In a suit's cockpit, out in space or inside the colony.
    Flying,
    /// On foot, at the Colony Exchange's books (its terminal, or its desk).
    AtTheExchange,
}

impl Status {
    /// The line the store prints.
    pub fn line(self) -> &'static str {
        match self {
            Status::InTheColony => "In the colony",
            Status::Flying => "Flying",
            Status::AtTheExchange => "At the Exchange",
        }
    }
}

/// The wallets on now, by address (so the document reads in a stable order).
#[derive(Default)]
pub struct Online(Mutex<BTreeMap<PlayerId, Entry>>);

impl Online {
    /// `address` is on in `session` (the one `Pilots::claim` gave it) until the guard drops. A newer
    /// session for the same wallet takes the entry over, and the older one's guard then leaves it.
    pub fn enter(self: &Arc<Self>, address: PlayerId, session: u64, name: &str, agent: bool) -> Here {
        if let Ok(mut on) = self.0.lock() {
            let status = Status::default();
            on.insert(address, Entry { name: name.to_string(), agent, since: unix_now(), session, status });
        }
        Here { online: self.clone(), address, session }
    }

    /// `{"at": unix, "online": [{"wallet", "name", "agent", "since", "status"}]}`.
    pub fn json(&self) -> serde_json::Value {
        let online: Vec<serde_json::Value> = self.0.lock().map_or_else(
            |_| Vec::new(),
            |on| {
                on.iter()
                    .map(|(a, e)| {
                        serde_json::json!({
                            "wallet": a.as_str(),
                            "name": e.name,
                            "agent": e.agent,
                            "since": e.since,
                            "status": e.status.line(),
                        })
                    })
                    .collect()
            },
        );
        serde_json::json!({ "at": unix_now(), "online": online })
    }
}

/// A session's place in [`Online`], given back when it ends.
pub struct Here {
    online: Arc<Online>,
    address: PlayerId,
    session: u64,
}

impl Here {
    /// The handle the session moves its [`Status`] with.
    pub fn presence(&self) -> Presence {
        Presence { online: self.online.clone(), address: self.address, session: self.session }
    }
}

/// Moves a session's [`Status`] in [`Online`] (only while that session holds the entry).
#[derive(Clone)]
pub struct Presence {
    online: Arc<Online>,
    address: PlayerId,
    session: u64,
}

impl Presence {
    pub fn set(&self, status: Status) {
        if let Ok(mut on) = self.online.0.lock()
            && let Some(e) = on.get_mut(&self.address).filter(|e| e.session == self.session)
        {
            e.status = status;
        }
    }
}

impl Drop for Here {
    fn drop(&mut self) {
        if let Ok(mut on) = self.online.0.lock()
            && on.get(&self.address).is_some_and(|e| e.session == self.session)
        {
            on.remove(&self.address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solana_wallet_and_an_account_are_listed_as_written() {
        let online = Arc::new(Online::default());
        let sol = "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
        let acct = "acct_0123456789abcdef0123";
        let _a = online.enter(PlayerId::parse(sol).unwrap(), 1, "Sol", false);
        let _b = online.enter(PlayerId::parse(acct).unwrap(), 2, "Acct", false);
        let doc = online.json();
        let wallets: Vec<&str> =
            doc["online"].as_array().unwrap().iter().map(|e| e["wallet"].as_str().unwrap()).collect();
        assert!(wallets.contains(&sol));
        assert!(wallets.contains(&acct));
    }

    #[test]
    fn a_takeover_keeps_the_newer_session() {
        let online = Arc::new(Online::default());
        let a = PlayerId::from(bc_proto::auth::Address([0xAB; 20]));
        let first = online.enter(a, 1, "Ace", false);
        let second = online.enter(a, 2, "Ace", false);
        drop(first);
        let doc = online.json();
        assert_eq!(doc["online"][0]["wallet"], format!("0x{}", "ab".repeat(20)));
        assert_eq!(doc["online"][0]["name"], "Ace");
        drop(second);
        assert_eq!(online.json()["online"], serde_json::json!([]));
    }

    #[test]
    fn the_status_line_is_coarse_and_the_holders_alone() {
        let online = Arc::new(Online::default());
        let a = PlayerId::from(bc_proto::auth::Address([0xCD; 20]));
        let first = online.enter(a, 1, "Ace", false);
        assert_eq!(online.json()["online"][0]["status"], "In the colony");
        first.presence().set(Status::Flying);
        assert_eq!(online.json()["online"][0]["status"], "Flying");
        // A newer session holds the entry: the older one's word doesn't move it.
        let second = online.enter(a, 2, "Ace", false);
        first.presence().set(Status::AtTheExchange);
        assert_eq!(online.json()["online"][0]["status"], "In the colony");
        second.presence().set(Status::AtTheExchange);
        assert_eq!(online.json()["online"][0]["status"], "At the Exchange");
        for s in [Status::InTheColony, Status::Flying, Status::AtTheExchange] {
            assert!(s.line().len() <= 64 && s.line().chars().all(|c| c.is_ascii_graphic() || c == ' '));
        }
        drop((first, second));
        assert_eq!(online.json()["online"], serde_json::json!([]));
    }
}
