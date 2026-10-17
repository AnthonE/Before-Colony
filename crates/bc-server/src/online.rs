//! Who is on, for the Elo Pros store (`/online.json`): each signed-in session's wallet, its
//! callsign, whether an agent flies it, and since when. The platform's playtime polls it once a
//! minute and counts hours per wallet (`meter/playtime.py` in AnthonE/scry-forge), in the shape
//! Gates' shard serves (`crates/server/src/online.rs` there). Guests have no wallet and aren't in
//! it. Off the hot path: sessions enter and leave it, the HTTP side reads it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bc_proto::auth::Address;

use crate::pilots::unix_now;

struct Entry {
    name: String,
    agent: bool,
    since: u64,
    session: u64,
}

/// The wallets on now, by address (so the document reads in a stable order).
#[derive(Default)]
pub struct Online(Mutex<BTreeMap<[u8; 20], Entry>>);

impl Online {
    /// `address` is on in `session` (the one `Pilots::claim` gave it) until the guard drops. A newer
    /// session for the same wallet takes the entry over, and the older one's guard then leaves it.
    pub fn enter(self: &Arc<Self>, address: Address, session: u64, name: &str, agent: bool) -> Here {
        if let Ok(mut on) = self.0.lock() {
            on.insert(address.0, Entry { name: name.to_string(), agent, since: unix_now(), session });
        }
        Here { online: self.clone(), address, session }
    }

    /// `{"at": unix, "online": [{"wallet", "name", "agent", "since"}]}`, wallets lowercase.
    pub fn json(&self) -> serde_json::Value {
        let online: Vec<serde_json::Value> = self.0.lock().map_or_else(
            |_| Vec::new(),
            |on| {
                on.iter()
                    .map(|(a, e)| {
                        let wallet = bc_auth::lower_hex(&Address(*a));
                        serde_json::json!({
                            "wallet": core::str::from_utf8(&wallet).unwrap_or_default(),
                            "name": e.name,
                            "agent": e.agent,
                            "since": e.since,
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
    address: Address,
    session: u64,
}

impl Drop for Here {
    fn drop(&mut self) {
        if let Ok(mut on) = self.online.0.lock()
            && on.get(&self.address.0).is_some_and(|e| e.session == self.session)
        {
            on.remove(&self.address.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_takeover_keeps_the_newer_session() {
        let online = Arc::new(Online::default());
        let a = Address([0xAB; 20]);
        let first = online.enter(a, 1, "Ace", false);
        let second = online.enter(a, 2, "Ace", false);
        drop(first);
        let doc = online.json();
        assert_eq!(doc["online"][0]["wallet"], format!("0x{}", "ab".repeat(20)));
        assert_eq!(doc["online"][0]["name"], "Ace");
        drop(second);
        assert_eq!(online.json()["online"], serde_json::json!([]));
    }
}
