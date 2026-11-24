//! This server's key on Elo Pros, and the signed calls it makes with it.
//!
//! A game's servers sign their writes to Elo Pros (stats, achievements and boards today) with an
//! Ed25519 key whose public half its developer named: `server_keys` in the game's `elo.json`, or
//! the dev desk's `serverkeys` act (`meter/serverkeys.py` in AnthonE/scry-forge). The private key
//! stays on the game box, in the PKCS#8 PEM file `--elo-key` names (`openssl genpkey -algorithm
//! ed25519`). It's never logged, never sent and never in the repo, and neither is a signature:
//! [`ServerKey`]'s `Debug` shows who it signs as and its public half only.
//!
//! What a request carries, as the platform's SDKs build it (`sdk/rust/src/server.rs` there;
//! `tests/data/server_texts.json` here pins it byte for byte): `X-Elo-Server: <slug>/<key id>`,
//! `X-Elo-Time: <unix seconds>`, `X-Elo-Nonce` (fresh each request) and `X-Elo-Signature`, the
//! standard base64 of the signature over [`server_text`]: `elo server`, the method, the path
//! without `/api`, `<slug>/<key id>`, the time, the nonce and the body's SHA-256 in hex, one a line.
//!
//! [`Signed`] posts with it, off the hot path (the session tasks' side). A report isn't idempotent
//! on Elo Pros (each try is a fresh nonce, so a new call, and a `sum` stat adds again), so only what
//! surely never landed is tried again: no connection made, a 429, a 502 or a 503 (Elo Pros' own
//! "nothing was recorded"), five times in all, a second and then doubling. A timeout once the
//! request went out, a 500 or a 504 may have landed, and is let go; a refusal is final. Whatever
//! becomes of a call is counted (`/status`'s `net.elo_*`), and none of it is ever fatal.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use base64::Engine;
use ring::signature::{Ed25519KeyPair, KeyPair};

use crate::net::NetStats;

/// PKCS#8 v1 for an Ed25519 key is this prefix and the 32-byte seed (what `openssl genpkey
/// -algorithm ed25519` writes, inside its PEM armour).
const PKCS8_ED25519: [u8; 16] =
    [0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
const NONCE_ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Tries a signed POST gets, how long one may take, and how long making its connection may.
pub const TRIES: u32 = 5;
pub const POST_TIMEOUT: Duration = Duration::from_secs(10);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether `id` is a key id the platform takes: lowercase letters, digits and `-`, up to 31
/// characters, not starting with `-`.
pub fn key_id_ok(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 31
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// One of this game's server keys.
pub struct ServerKey {
    game: String,
    id: String,
    pair: Ed25519KeyPair,
}

impl fmt::Debug for ServerKey {
    /// Who it signs as and its public half: never the private key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerKey").field("who", &self.who()).field("public", &self.public_b64()).finish()
    }
}

impl ServerKey {
    /// From the 32-byte private seed.
    pub fn from_seed(game: &str, id: &str, seed: &[u8]) -> Result<Self, String> {
        if !key_id_ok(id) {
            return Err("a key id is lowercase letters, digits and '-', up to 31 characters".into());
        }
        let pair = Ed25519KeyPair::from_seed_unchecked(seed)
            .map_err(|_| "an Ed25519 private key is a 32-byte seed".to_string())?;
        Ok(Self { game: game.to_string(), id: id.to_string(), pair })
    }

    /// From a PKCS#8 PEM (`openssl genpkey -algorithm ed25519`). The refusal never quotes the file.
    pub fn from_pem(game: &str, id: &str, pem: &str) -> Result<Self, String> {
        let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(body.trim())
            .map_err(|_| "the key file isn't a PEM".to_string())?;
        if der.len() != 48 || der[..16] != PKCS8_ED25519 {
            return Err("the key file isn't an Ed25519 private key (PKCS#8)".into());
        }
        Self::from_seed(game, id, &der[16..])
    }

    /// `<slug>/<key id>`, as a request names its signer.
    pub fn who(&self) -> String {
        format!("{}/{}", self.game, self.id)
    }

    /// The public half, standard base64: what the developer names (`server_keys`' `pub`).
    pub fn public_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.pair.public_key().as_ref())
    }

    /// The signature of `text`, standard base64.
    fn sign(&self, text: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.pair.sign(text.as_bytes()).as_ref())
    }

    /// The four headers of one request: `path` is the route without `/api` (with `?query`), `at`
    /// the unix seconds signed, `nonce` fresh for each request ([`nonce`]).
    pub fn headers(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        at: u64,
        nonce: &str,
    ) -> [(&'static str, String); 4] {
        let who = self.who();
        let text = server_text(method, path, &who, at, body, Some(nonce));
        let sig = self.sign(&text);
        [
            ("X-Elo-Server", who),
            ("X-Elo-Time", at.to_string()),
            ("X-Elo-Nonce", nonce.to_string()),
            ("X-Elo-Signature", sig),
        ]
    }
}

/// The body's SHA-256, lowercase hex.
pub fn sha256_hex(body: &[u8]) -> String {
    hex::encode(ring::digest::digest(&ring::digest::SHA256, body).as_ref())
}

/// The exact text a server signs for one request (`serverkeys.message`): no trailing newline, and
/// no nonce line for a request that sends none.
pub fn server_text(method: &str, path: &str, who: &str, at: u64, body: &[u8], nonce: Option<&str>) -> String {
    let tail = nonce.map(|n| format!("{n}\n")).unwrap_or_default();
    format!("elo server\n{}\n{path}\n{who}\n{at}\n{tail}{}", method.to_ascii_uppercase(), sha256_hex(body))
}

/// A fresh request nonce: 22 of `A-Z a-z 0-9 - _` (the platform takes 8 to 64).
pub fn nonce() -> String {
    let mut b = [0u8; 22];
    getrandom::fill(&mut b).expect("the system's randomness");
    b.iter().map(|x| NONCE_ABC[usize::from(*x & 63)] as char).collect()
}

/// How one signed POST went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Posted {
    /// Taken: the answer's body.
    Taken(String),
    /// Refused for good (a 4xx but 429): `"<code>: <text>"`.
    Refused(String),
    /// Surely not landed, and may pass: no connection made, a 429, a 502 or a 503. Tried again.
    Passing(String),
    /// May have landed (a timeout once sent, a 500, a 504): never tried again, since a report
    /// sent twice counts twice.
    Unknown(String),
}

/// What an answer with status `code` (not a success) means for the call: tried again only when
/// nothing was recorded (a 429; a 502, the proxy finding nobody behind it; a 503, Elo Pros' own
/// "nothing was recorded, send it again"), let go when it may have been (other 5xx), final when
/// refused (other 4xx).
fn answered(code: u16, text: String) -> Posted {
    match code {
        429 | 502 | 503 => Posted::Passing(text),
        500..=599 => Posted::Unknown(text),
        _ => Posted::Refused(text),
    }
}

/// What became of the signed POSTs since start (in `NetStats`, for `/status`).
pub struct Counts<'a> {
    pub sent: &'a AtomicU64,
    pub refused: &'a AtomicU64,
    pub failed: &'a AtomicU64,
}

/// An answer's text, cut short for a log line (it never holds a signature: the platform's
/// refusals say what's wrong in words).
fn clip(text: &str) -> String {
    let t = text.trim();
    match t.char_indices().nth(200) {
        Some((at, _)) => format!("{}…", &t[..at]),
        None => t.to_string(),
    }
}

/// Signed calls to Elo Pros with this server's key.
pub struct Signed {
    http: reqwest::Client,
    origin: String,
    key: Arc<ServerKey>,
    stats: Arc<NetStats>,
}

impl Signed {
    /// Calls to `origin` (`https://elopros.com`, no trailing slash needed), signed with `key`.
    pub fn new(origin: &str, key: Arc<ServerKey>, stats: Arc<NetStats>) -> Option<Self> {
        let http =
            reqwest::Client::builder().timeout(POST_TIMEOUT).connect_timeout(CONNECT_TIMEOUT).build().ok()?;
        let origin = origin.trim_end_matches('/').to_string();
        Some(Self { http, origin, key, stats })
    }

    pub fn who(&self) -> String {
        self.key.who()
    }

    fn counts(&self) -> Counts<'_> {
        Counts {
            sent: &self.stats.elo_sent,
            refused: &self.stats.elo_refused,
            failed: &self.stats.elo_failed,
        }
    }

    /// POSTs `body` (JSON) to `path` (without `/api`), signed afresh each try, trying again what
    /// surely never landed ([`TRIES`] in all). The answer, or why not (counted, never fatal).
    pub async fn post(&self, path: &str, body: &str) -> Result<String, String> {
        self.post_tries(path, body, TRIES).await
    }

    /// [`Signed::post`] with `tries` in all (at least one).
    pub async fn post_tries(&self, path: &str, body: &str, tries: u32) -> Result<String, String> {
        let counts = self.counts();
        let tries = tries.max(1);
        let mut wait = Duration::from_secs(1);
        let mut last = String::new();
        for attempt in 0..tries {
            match self.post_once(path, body).await {
                Posted::Taken(text) => {
                    counts.sent.fetch_add(1, Ordering::Relaxed);
                    return Ok(text);
                }
                Posted::Refused(why) => {
                    counts.refused.fetch_add(1, Ordering::Relaxed);
                    return Err(why);
                }
                Posted::Unknown(why) => {
                    counts.failed.fetch_add(1, Ordering::Relaxed);
                    return Err(format!("not tried again (it may have landed): {why}"));
                }
                Posted::Passing(why) => last = why,
            }
            if attempt + 1 < tries {
                tokio::time::sleep(wait).await;
                wait *= 2;
            }
        }
        counts.failed.fetch_add(1, Ordering::Relaxed);
        Err(format!("not taken after {tries} tries: {last}"))
    }

    /// One signed POST, no retry.
    pub async fn post_once(&self, path: &str, body: &str) -> Posted {
        let at = crate::pilots::unix_now();
        let mut req = self
            .http
            .post(format!("{}/api{path}", self.origin))
            .header("Content-Type", "application/json")
            .body(body.to_string());
        for (k, v) in self.key.headers("POST", path, body.as_bytes(), at, &nonce()) {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(res) if res.status().is_success() => Posted::Taken(res.text().await.unwrap_or_default()),
            Ok(res) => {
                let code = res.status();
                let text = format!("{code}: {}", clip(&res.text().await.unwrap_or_default()));
                answered(code.as_u16(), text)
            }
            // No connection was made: nothing went out.
            Err(e) if e.is_connect() => Posted::Passing(format!("no connection: {}", clip(&e.to_string()))),
            Err(e) => Posted::Unknown(format!("no answer: {}", clip(&e.to_string()))),
        }
    }

    /// Asks Elo Pros whether it takes this server's signature (`POST /api/serverkeys/check`), once,
    /// as the server starts: said in the log, so a key nobody named yet is seen at once.
    pub async fn check(&self) {
        match self.post_once("/serverkeys/check", "").await {
            Posted::Taken(_) => tracing::info!(who = %self.who(), "Elo Pros takes this server's signature"),
            Posted::Refused(why) | Posted::Passing(why) | Posted::Unknown(why) => {
                tracing::warn!(who = %self.who(), "Elo Pros didn't take this server's signature: {why}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_id_is_the_platforms_shape() {
        assert!(key_id_ok("sector-1") && key_id_ok("eu-1") && key_id_ok("0"));
        assert!(!key_id_ok("") && !key_id_ok("-a") && !key_id_ok("Eu") && !key_id_ok("a_b"));
        assert!(!key_id_ok(&"a".repeat(32)));
        assert!(ServerKey::from_seed("before-colony", "Bad Id", &[7; 32]).is_err());
    }

    #[test]
    fn its_debug_shows_who_and_the_public_half_only() {
        let key = ServerKey::from_seed("before-colony", "sector-1", &[7; 32]).unwrap();
        let shown = format!("{key:?}");
        assert!(shown.contains("before-colony/sector-1") && shown.contains(&key.public_b64()), "{shown}");
        assert!(!shown.contains("pair") && !shown.contains("seed"), "{shown}");
    }

    #[test]
    fn nonces_are_fresh_and_the_platforms_shape() {
        let (a, b) = (nonce(), nonce());
        assert_ne!(a, b);
        assert_eq!(a.len(), 22);
        assert!(a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }

    /// Only what surely never landed is sent again: a report isn't idempotent.
    #[test]
    fn only_what_never_landed_is_tried_again() {
        let kind = |code| match answered(code, String::new()) {
            Posted::Passing(_) => "again",
            Posted::Unknown(_) => "let go",
            Posted::Refused(_) => "final",
            Posted::Taken(_) => "taken",
        };
        assert_eq!([kind(429), kind(502), kind(503)], ["again"; 3]);
        assert_eq!([kind(500), kind(504)], ["let go"; 2]);
        assert_eq!([kind(400), kind(401), kind(409), kind(422)], ["final"; 4]);
    }

    #[test]
    fn a_long_answer_is_cut_for_the_log() {
        assert_eq!(clip("  short  "), "short");
        let long = "é".repeat(300);
        assert_eq!(clip(&long).chars().count(), 201);
    }
}
