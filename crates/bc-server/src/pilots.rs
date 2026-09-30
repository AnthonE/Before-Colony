//! Pilots who sign in with a wallet: their records, which session each is flying in, and the
//! resume tokens that let them reconnect without signing again.
//!
//! Records go through a [`PilotStore`]: in memory ([`MemoryStore`]), or a JSON file per pilot in
//! the server's data directory ([`FileStore`], `--data-dir`); a Redis or Mongo store implements
//! the same two methods, and a [`PilotRecord`] is plain serde data so it can be kept as-is. None
//! of this is on the sector's hot path: sessions call it around the handshake and teardown, and
//! when their hangar changes.
//!
//! A signed-in pilot who leaves stays in the sector, asleep in the cockpit: the record keeps which
//! suit. Sleepers live as long as this server run, and so does the news of what became of one
//! (destroyed, or cleared for room), kept here until its pilot is back.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bc_econ::Hangar;
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fate {
    /// Shot down (`by`: who, if they were a pilot).
    Destroyed { by: String },
    /// Cleared to make room, or the server restarted.
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
    pub created_unix: u64,
    pub seen_unix: u64,
    /// Survival: the pilot's hangar bay (credits, stores, the suit, the stations' queues). None
    /// until they first come in, when they get the starter kit.
    #[serde(default)]
    pub hangar: Option<Hangar>,
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
            created_unix: now,
            seen_unix: now,
            hangar: None,
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

/// Records as JSON files, one per pilot, in a directory: they outlive the server.
pub struct FileStore {
    dir: std::path::PathBuf,
}

impl FileStore {
    /// Keeps records in `dir` (made if it isn't there).
    pub fn new(dir: impl Into<std::path::PathBuf>) -> anyhow::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, address: &str) -> anyhow::Result<std::path::PathBuf> {
        // Keys are `0x` and 40 hex digits: nothing else becomes a file name.
        let ok = address.len() == 42
            && address.starts_with("0x")
            && address[2..].bytes().all(|b| b.is_ascii_hexdigit());
        anyhow::ensure!(ok, "not a pilot key: {address:?}");
        Ok(self.dir.join(format!("{address}.json")))
    }
}

impl PilotStore for FileStore {
    fn load(&self, address: &str) -> BoxFuture<'_, anyhow::Result<Option<PilotRecord>>> {
        let path = self.path(address);
        Box::pin(async move {
            let path = path?;
            tokio::task::spawn_blocking(move || match std::fs::read(&path) {
                Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            })
            .await?
        })
    }

    fn save(&self, record: PilotRecord) -> BoxFuture<'_, anyhow::Result<()>> {
        let path = self.path(&record.address);
        Box::pin(async move {
            let path = path?;
            let bytes = serde_json::to_vec_pretty(&record)?;
            tokio::task::spawn_blocking(move || write_atomically(&path, &bytes)).await?
        })
    }
}

/// Writes `bytes` to `path` so that a reader sees the old file or the new one, never half of it.
pub fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
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
    /// Signed-in pilots' suits, flying or asleep, by (entity slot, generation).
    suits: Mutex<HashMap<(u16, u16), Address>>,
    /// What became of pilots' sleepers while they were away, until they're back.
    news: Mutex<HashMap<Address, Fate>>,
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
            suits: Mutex::new(HashMap::new()),
            news: Mutex::new(HashMap::new()),
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

    /// The pilot flies (or sleeps in) this suit.
    pub fn bind_suit(&self, suit: (u16, u16), address: Address) {
        if let Ok(mut suits) = self.suits.lock() {
            suits.insert(suit, address);
        }
    }

    /// The suit is gone: whose it was, if anyone's.
    pub fn suit_gone(&self, suit: (u16, u16)) -> Option<Address> {
        self.suits.lock().ok()?.remove(&suit)
    }

    /// News for a pilot who's away (the latest replaces any before it).
    pub fn tell(&self, address: Address, fate: Fate) {
        if let Ok(mut news) = self.news.lock() {
            news.insert(address, fate);
        }
    }

    /// The news waiting for a pilot, taken.
    pub fn take_news(&self, address: &Address) -> Option<Fate> {
        self.news.lock().ok()?.remove(address)
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

    #[test]
    fn a_sleepers_end_is_news_for_its_pilot() {
        let p = pilots();
        let a = Address([5; 20]);
        p.bind_suit((12, 3), a);
        assert_eq!(p.suit_gone((12, 2)), None, "another occupant of the slot");
        assert_eq!(p.suit_gone((12, 3)), Some(a));
        assert_eq!(p.suit_gone((12, 3)), None, "once");
        p.tell(a, Fate::Destroyed { by: "Zechs".into() });
        assert_eq!(p.take_news(&a), Some(Fate::Destroyed { by: "Zechs".into() }));
        assert_eq!(p.take_news(&a), None);
    }

    #[tokio::test]
    async fn records_survive_the_server_in_files() {
        let dir = std::env::temp_dir().join(format!("bc-pilots-{}", std::process::id()));
        let store = FileStore::new(&dir).unwrap();
        let a = Address([6; 20]);
        let mut r = PilotRecord::new(&a);
        r.hangar = Some(Hangar::starter());
        store.save(r.clone()).await.unwrap();
        // Another server run, the same directory.
        let again = FileStore::new(&dir).unwrap();
        assert_eq!(again.load(&key(&a)).await.unwrap(), Some(r));
        assert_eq!(again.load(&key(&Address([7; 20]))).await.unwrap(), None);
        assert!(again.load("../etc/passwd").await.is_err());
        let _ = std::fs::remove_dir_all(dir);
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
