//! Elo Pros, for the sign-in: the nonces a pass must carry (`/nonce`), and the read of a wallet's
//! items. Off the hot path: sessions use them as they come in, the HTTP side hands nonces out.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use bc_proto::auth::PlayerId;

use crate::pilots::unix_now;

/// How long a nonce is good for: long enough to sit on the title page until a pass (ten minutes of
/// its own) is used.
pub const NONCE_TTL: Duration = Duration::from_secs(15 * 60);
const NONCE_CAP: usize = 100_000;

/// The nonces this server hands the page, each good once.
///
/// A nonce carries its own proof: `hex(issued unix seconds, 8 bytes)`, `hex(8 random bytes)`, then
/// the first 16 bytes of their HMAC-SHA256 under this process's key, in hex: 64 characters, inside
/// Elo Pros' `[0-9A-Za-z]{8,64}`. So it needs no list of what was issued (oneshot's, whose page signs
/// in the same way).
pub struct Nonces {
    key: ring::hmac::Key,
    used: Mutex<BTreeMap<String, u64>>,
}

impl Default for Nonces {
    fn default() -> Self {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).expect("the system's randomness");
        Self { key: ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &k), used: Mutex::new(BTreeMap::new()) }
    }
}

impl Nonces {
    fn mac(&self, head: &str) -> String {
        hex::encode(&ring::hmac::sign(&self.key, head.as_bytes()).as_ref()[..16])
    }

    pub fn issue(&self) -> String {
        self.issue_at(unix_now())
    }

    pub fn issue_at(&self, now: u64) -> String {
        let mut r = [0u8; 8];
        getrandom::fill(&mut r).expect("the system's randomness");
        let head = format!("{}{}", hex::encode(now.to_be_bytes()), hex::encode(r));
        let mac = self.mac(&head);
        head + &mac
    }

    /// Whether `n` was issued here in the last [`NONCE_TTL`] and hasn't been taken. Takes it.
    pub fn take(&self, n: &str) -> bool {
        self.take_at(n, unix_now())
    }

    pub fn take_at(&self, n: &str, now: u64) -> bool {
        if n.len() != 64 || !n.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return false;
        }
        let (head, mac) = n.split_at(32);
        // Compared whole, in constant time.
        let want = self.mac(head);
        if want.bytes().zip(mac.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) != 0 {
            return false;
        }
        let Ok(at) = u64::from_str_radix(&head[..16], 16) else { return false };
        if at > now + 5 || now.saturating_sub(at) >= NONCE_TTL.as_secs() {
            return false;
        }
        let Ok(mut used) = self.used.lock() else { return false };
        used.retain(|_, t| now.saturating_sub(*t) < NONCE_TTL.as_secs());
        if used.contains_key(n) || used.len() >= NONCE_CAP {
            return false;
        }
        used.insert(n.to_string(), at);
        true
    }
}

/// Elo Pros' account key from its standard base64 (`GET /api/account/pubkey`).
pub fn parse_pubkey(text: &str) -> Option<[u8; 32]> {
    base64::engine::general_purpose::STANDARD.decode(text.trim()).ok()?.try_into().ok()
}

/// How long a session waits for a wallet's items as it comes in.
pub const READ_WAIT: Duration = Duration::from_secs(2);

/// The free Pill, this title's first cosmetic, by its key in the items' record.
pub const PILL: &str = "pill";

/// A wallet's items on Elo Pros (`GET /api/items/sol/of/<wallet>`; an `0x` wallet is read at the
/// Solana wallet linked to it).
pub struct Items {
    http: reqwest::Client,
    origin: String,
}

impl Items {
    pub fn new(origin: String) -> Option<Self> {
        let http = reqwest::Client::builder().timeout(READ_WAIT).build().ok()?;
        Some(Self { http, origin })
    }

    /// Whether `wallet` holds the Pill. No when Elo Pros can't be asked: it's cosmetic, so not
    /// knowing costs the look, never the game.
    pub async fn holds_pill(&self, wallet: &PlayerId) -> bool {
        let url = format!("{}/api/items/sol/of/{wallet}", self.origin);
        let read = async {
            let res = self.http.get(&url).send().await.ok()?.error_for_status().ok()?;
            res.json::<serde_json::Value>().await.ok()
        };
        read.await.is_some_and(|doc| holds(&doc, bc_auth::GAME, PILL))
    }
}

/// Whether a `/api/items/sol/of` answer lists `item` from title `slug`.
pub fn holds(doc: &serde_json::Value, slug: &str, item: &str) -> bool {
    doc["items"].as_array().is_some_and(|rows| rows.iter().any(|r| r["title"] == slug && r["item"] == item))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonces_are_good_once_and_for_a_while() {
        let n = Nonces::default();
        let a = n.issue_at(1000);
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_alphanumeric()), "Elo Pros' nonce shape");
        assert!(n.take_at(&a, 1001));
        assert!(!n.take_at(&a, 1002), "once");
        assert!(!n.take_at(&"0".repeat(64), 1001));
        assert!(!Nonces::default().take_at(&n.issue_at(1000), 1001), "another key");
        let ttl = NONCE_TTL.as_secs();
        assert!(!n.take_at(&n.issue_at(1000), 1000 + ttl));
        assert!(n.take_at(&n.issue_at(1000), 1000 + ttl - 1));
        assert!(!n.take_at(&n.issue_at(2000), 1000), "not from the future");
        let mut bad = n.issue_at(1000).into_bytes();
        bad[40] = if bad[40] == b'0' { b'1' } else { b'0' };
        assert!(!n.take_at(std::str::from_utf8(&bad).unwrap(), 1001));
    }

    #[test]
    fn the_account_key_is_base64_of_32_bytes() {
        assert_eq!(parse_pubkey("H0SHD2PwtN2Lb2tZhyrcrQCHGFIs+qy9VvR4aISFwlE=").map(|k| k[0]), Some(0x1f));
        assert_eq!(parse_pubkey("AAAA"), None);
        assert_eq!(parse_pubkey("not base64!"), None);
    }

    /// The shape `meter/solana_items.py` answers with, whichever wallet was asked.
    #[test]
    fn the_pill_is_read_by_title_and_item() {
        let doc = serde_json::json!({
            "reachable": true, "wallet": "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB", "via": null, "count": 2,
            "items": [
                {"title": "gates", "item": "pill", "name": "The Pill"},
                {"title": "before-colony", "item": "pill", "name": "The Pill", "chain_label": "Solana"},
            ],
        });
        assert!(holds(&doc, "before-colony", "pill"));
        assert!(!holds(&doc, "oneshot", "pill"));
        assert!(!holds(&doc, "before-colony", "other"));
        // Another title's Pill, an EVM wallet with no link, a read that failed, an item not in the record.
        assert!(!holds(
            &serde_json::json!({"items": [{"title": "gates", "item": "pill"}]}),
            "before-colony",
            "pill"
        ));
        assert!(!holds(
            &serde_json::json!({"reachable": true, "solana": null, "items": [], "count": 0}),
            "before-colony",
            "pill"
        ));
        assert!(!holds(&serde_json::json!({"reachable": false, "items": []}), "before-colony", "pill"));
        assert!(!holds(
            &serde_json::json!({"items": [{"title": "before-colony", "item": null}]}),
            "before-colony",
            "pill"
        ));
        assert!(!holds(&serde_json::json!({}), "before-colony", "pill"));
    }
}
