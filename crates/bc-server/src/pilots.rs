//! Pilots who sign in with a wallet: their records, which session each is flying in, and the
//! resume tokens that let them reconnect without signing again.
//!
//! Records go through a [`PilotStore`]. It's in memory for now ([`MemoryStore`]); a Redis or
//! Mongo store implements the same two methods, and a [`PilotRecord`] is plain serde data so it
//! can be kept as-is. None of this is on the sector's hot path: sessions call it around the
//! handshake and teardown.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bc_proto::auth::{Address, TOKEN_BYTES};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, oneshot};

/// The lowercase `0x…` form a pilot is keyed by.
pub fn key(address: &Address) -> String {
    String::from_utf8_lossy(&bc_auth::lower_hex(address)).into_owned()
}

/// `0x1234…abcd`, for logs and `/status`.
pub fn short(address: &Address) -> String {
    let k = key(address);
    format!("{}…{}", &k[..6], &k[k.len() - 4..])
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Where a pilot's suit sleeps while they're away: its entity slot and generation in this server
/// run's sector (a restarted server's suits are gone).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sleeper {
    /// The server run it belongs to.
    pub run: u64,
    pub suit: u16,
    pub generation: u16,
    pub since_unix: u64,
}

/// What happened to a sleeping suit, to tell its pilot when they're back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fate {
    Destroyed { by: String },
    Lost,
}

/// Everything kept about a signed-in pilot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PilotRecord {
    /// Lowercase `0x…` wallet address: the key.
    pub address: String,
    pub name: String,
    /// The frame last flown (a slug).
    pub frame: String,
    /// Credits from salvage, kept across sessions.
    pub credits: u32,
    /// The suit asleep in the sector, if any.
    pub sleeper: Option<Sleeper>,
    /// What became of it, if the pilot hasn't been told yet.
    pub fate: Option<Fate>,
    pub created_unix: u64,
    pub seen_unix: u64,
}

impl PilotRecord {
    pub fn new(address: &Address) -> Self {
        let now = unix_now();
        Self {
            address: key(address),
            name: String::new(),
            frame: String::new(),
            credits: 0,
            sleeper: None,
            fate: None,
            created_unix: now,
            seen_unix: now,
        }
    }
}

/// Where pilot records live.
pub trait PilotStore: Send + Sync {
    fn load(&self, address: &str) -> BoxFuture<'_, anyhow::Result<Option<PilotRecord>>>;
    fn save(&self, record: PilotRecord) -> BoxFuture<'_, anyhow::Result<()>>;
}

/// Records in memory: gone when the server stops.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, PilotRecord>>);

impl PilotStore for MemoryStore {
    fn load(&self, address: &str) -> BoxFuture<'_, anyhow::Result<Option<PilotRecord>>> {
        let found = self.0.lock().ok().and_then(|m| m.get(address).cloned());
        Box::pin(async move { Ok(found) })
    }

    fn save(&self, record: PilotRecord) -> BoxFuture<'_, anyhow::Result<()>> {
        if let Ok(mut m) = self.0.lock() {
            m.insert(record.address.clone(), record);
        }
        Box::pin(async { Ok(()) })
    }
}

/// A pilot's live session.
struct Online {
    session: u64,
    /// Tells the session another one took over.
    kick: Option<oneshot::Sender<()>>,
    /// Signalled when the session has torn down.
    gone: Arc<Notify>,
}

struct Token {
    address: Address,
    /// `None` while its session is live; once it ends, the token lasts [`Pilots::ttl`].
    expires: Option<Instant>,
}

/// The server's pilots.
pub struct Pilots {
    pub store: Arc<dyn PilotStore>,
    /// Identifies this server run (a sleeper from another run is gone).
    pub run: u64,
    online: Mutex<HashMap<Address, Online>>,
    tokens: Mutex<HashMap<[u8; TOKEN_BYTES], Token>>,
    next_session: AtomicU64,
    ttl: Duration,
}

/// A claimed pilot: this session is theirs until [`Pilots::release`].
pub struct Claim {
    pub address: Address,
    pub session: u64,
    /// Fires when a newer session takes the pilot over.
    pub kicked: oneshot::Receiver<()>,
}

/// How long a session being taken over gets to tear down.
const TAKEOVER_WAIT: Duration = Duration::from_secs(5);

impl Pilots {
    pub fn new(store: Arc<dyn PilotStore>, ttl: Duration) -> Self {
        let mut run = [0u8; 8];
        let _ = getrandom::fill(&mut run);
        Self {
            store,
            run: u64::from_le_bytes(run),
            online: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            next_session: AtomicU64::new(1),
            ttl,
        }
    }

    /// Makes this session the pilot's, taking over (and waiting out) any older one.
    pub async fn claim(&self, address: Address) -> Claim {
        loop {
            let waiting = {
                let Ok(mut online) = self.online.lock() else { break };
                match online.get_mut(&address) {
                    Some(o) => {
                        if let Some(kick) = o.kick.take() {
                            let _ = kick.send(());
                        }
                        Some(o.gone.clone())
                    }
                    None => None,
                }
            };
            let Some(gone) = waiting else { break };
            if tokio::time::timeout(TAKEOVER_WAIT, gone.notified()).await.is_err() {
                // It didn't go in time: it's forgotten either way.
                if let Ok(mut online) = self.online.lock() {
                    online.remove(&address);
                }
            }
        }
        let (tx, rx) = oneshot::channel();
        let session = self.next_session.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut online) = self.online.lock() {
            online.insert(address, Online { session, kick: Some(tx), gone: Arc::new(Notify::new()) });
        }
        Claim { address, session, kicked: rx }
    }

    /// The session ended: the pilot is free, and their token starts to age.
    pub fn release(&self, claim_address: Address, session: u64) {
        if let Ok(mut online) = self.online.lock()
            && online.get(&claim_address).is_some_and(|o| o.session == session)
            && let Some(o) = online.remove(&claim_address)
        {
            o.gone.notify_one();
        }
        if let Ok(mut tokens) = self.tokens.lock() {
            let expires = Some(Instant::now() + self.ttl);
            for t in tokens.values_mut().filter(|t| t.address == claim_address && t.expires.is_none()) {
                t.expires = expires;
            }
        }
    }

    /// Whether the pilot is flying right now.
    pub fn is_online(&self, address: &Address) -> bool {
        self.online.lock().is_ok_and(|o| o.contains_key(address))
    }

    /// A new resume token for a signed-in session (the pilot's older ones stop working).
    pub fn issue_token(&self, address: Address) -> [u8; TOKEN_BYTES] {
        let mut token = [0u8; TOKEN_BYTES];
        let _ = getrandom::fill(&mut token);
        if let Ok(mut tokens) = self.tokens.lock() {
            let now = Instant::now();
            tokens.retain(|_, t| t.address != address && t.expires.is_none_or(|e| e > now));
            tokens.insert(token, Token { address, expires: None });
        }
        token
    }

    /// The pilot a resume token belongs to, if it's still good. Used once: the session issues a
    /// new one.
    pub fn redeem(&self, token: &[u8; TOKEN_BYTES]) -> Option<Address> {
        let mut tokens = self.tokens.lock().ok()?;
        let t = tokens.remove(token)?;
        t.expires.is_none_or(|e| e > Instant::now()).then_some(t.address)
    }

    pub async fn load_or_new(&self, address: &Address) -> PilotRecord {
        match self.store.load(&key(address)).await {
            Ok(Some(r)) => r,
            Ok(None) => PilotRecord::new(address),
            Err(e) => {
                tracing::warn!("pilot store: {e}");
                PilotRecord::new(address)
            }
        }
    }

    pub async fn save(&self, record: PilotRecord) {
        if let Err(e) = self.store.save(record).await {
            tracing::warn!("pilot store: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pilots() -> Pilots {
        Pilots::new(Arc::new(MemoryStore::default()), Duration::from_millis(200))
    }

    #[tokio::test]
    async fn a_second_session_takes_over_the_first() {
        let p = Arc::new(pilots());
        let a = Address([1; 20]);
        let first = p.claim(a).await;
        let p2 = p.clone();
        // The first session tears down when kicked.
        let watcher = tokio::spawn(async move {
            let _ = first.kicked.await;
            p2.release(a, first.session);
        });
        let second = p.claim(a).await;
        watcher.await.unwrap();
        assert!(p.is_online(&a));
        assert_ne!(second.session, 1);
        p.release(a, second.session);
        assert!(!p.is_online(&a));
    }

    #[tokio::test]
    async fn a_session_that_will_not_go_is_forgotten() {
        let p = pilots();
        let a = Address([2; 20]);
        let _stuck = p.claim(a).await;
        let started = Instant::now();
        let _second = p.claim(a).await;
        assert!(started.elapsed() >= TAKEOVER_WAIT - Duration::from_millis(50));
    }

    #[tokio::test]
    async fn tokens_work_once_and_age_after_the_session() {
        let p = pilots();
        let a = Address([3; 20]);
        let claim = p.claim(a).await;
        let t1 = p.issue_token(a);
        let t2 = p.issue_token(a);
        assert_eq!(p.redeem(&t1), None, "a newer token replaced it");
        p.release(a, claim.session);
        assert_eq!(p.redeem(&t2), Some(a));
        assert_eq!(p.redeem(&t2), None, "used once");
        let claim = p.claim(a).await;
        let t3 = p.issue_token(a);
        p.release(a, claim.session);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(p.redeem(&t3), None, "expired");
    }

    #[tokio::test]
    async fn records_round_trip_through_the_store() {
        let p = pilots();
        let a = Address([4; 20]);
        let mut r = p.load_or_new(&a).await;
        assert_eq!(r.address, format!("0x{}", "04".repeat(20)));
        r.credits = 1_200;
        r.sleeper = Some(Sleeper { run: p.run, suit: 12, generation: 3, since_unix: 5 });
        p.save(r.clone()).await;
        assert_eq!(p.load_or_new(&a).await, r);
        // Plain serde data: what Redis or Mongo would keep.
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<PilotRecord>(&json).unwrap(), r);
    }
}
