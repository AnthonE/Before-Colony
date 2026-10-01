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
//! (destroyed, or cleared for room), kept here until its pilot is back. Under survival rules, one
//! left in a landmark's hide spot outlives the run: the record keeps where it is and what it
//! carries ([`ParkedSuit`]), and the next run puts it back there before anyone connects.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bc_econ::Hangar;
use bc_proto::auth::{Address, TOKEN_BYTES};
use bc_proto::{CARGO_KINDS, Faction, FrameId, Part, PilotKind};
use bc_sim::content::landmarks::LANDMARKS_VERSION;
use bc_sim::sim::{Homecoming, ParkRecord};
use futures::future::BoxFuture;
use glam::{Quat, Vec3};
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

/// A suit its pilot left asleep in a landmark's hide spot (survival): where it is and what it
/// carries, so that the next server run can put it back there
/// ([`GameShared::restore_parked`](crate::net::game::GameShared::restore_parked)).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkedSuit {
    /// The landmarks it was left on (`LANDMARKS_VERSION`): if they've changed since, the colony's
    /// tugs bring the suit in instead.
    pub landmarks_version: u16,
    pub landmark: u8,
    /// Its origin and attitude in the landmark's frame, and how high it stands (m: on its feet,
    /// or crouched).
    pub local: [f32; 3],
    pub rot: [f32; 4],
    pub stance: f32,
    /// The frame (a slug), the side it flies for, and who flies it (`human` or `agent`).
    pub frame: String,
    pub faction: String,
    #[serde(default)]
    pub pilot: String,
    /// Armour per part as a fraction of the frame's, the weapons fitted (a bit per mount), the
    /// rounds loaded, the tank (kg), the hold (kg per cargo kind) and the bounties earned.
    pub parts: [f32; Part::COUNT],
    pub mounts: u8,
    pub ammo: [u16; 3],
    pub propellant: f32,
    pub cargo_kg: [u16; CARGO_KINDS],
    pub bounty: u32,
    pub since_unix: u64,
}

impl ParkedSuit {
    /// The sector's record of a suit left in a hide spot, as it's kept.
    pub fn new(rec: &ParkRecord, since_unix: u64) -> Self {
        let h = &rec.home;
        Self {
            landmarks_version: LANDMARKS_VERSION,
            landmark: rec.landmark,
            local: rec.local.to_array(),
            rot: rec.rot.to_array(),
            stance: rec.stance,
            frame: rec.frame.slug().to_string(),
            faction: faction_slug(rec.faction).to_string(),
            pilot: if rec.pilot == PilotKind::Agent { "agent" } else { "human" }.to_string(),
            parts: h.parts,
            mounts: h.mounts,
            ammo: h.ammo,
            propellant: h.propellant,
            cargo_kg: h.cargo_kg,
            bounty: h.bounty,
            since_unix,
        }
    }

    /// The record to give the sector, if it names a frame and a side there are.
    pub fn record(&self) -> Option<ParkRecord> {
        let frame = FrameId::from_slug(&self.frame)?;
        let faction = match self.faction.as_str() {
            "oz" => Faction::Oz,
            "colonies" => Faction::Colonies,
            "alliance" => Faction::Alliance,
            _ => return None,
        };
        Some(ParkRecord {
            landmark: self.landmark,
            local: Vec3::from_array(self.local),
            rot: Quat::from_array(self.rot),
            stance: self.stance,
            frame,
            faction,
            pilot: if self.pilot == "agent" { PilotKind::Agent } else { PilotKind::Human },
            home: Homecoming {
                frame,
                parts: self.parts,
                mounts: self.mounts,
                ammo: self.ammo,
                propellant: self.propellant,
                cargo_kg: self.cargo_kg,
                held: None,
                bounty: self.bounty,
            },
        })
    }
}

fn faction_slug(f: Faction) -> &'static str {
    match f {
        Faction::Oz => "oz",
        Faction::Colonies => "colonies",
        Faction::Alliance => "alliance",
    }
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
    /// Survival: the suit asleep in a landmark's hide spot, kept for the next server run.
    #[serde(default)]
    pub parked: Option<ParkedSuit>,
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
            parked: None,
        }
    }
}

/// Where pilot records live.
pub trait PilotStore: Send + Sync {
    fn load(&self, address: &str) -> BoxFuture<'_, anyhow::Result<Option<PilotRecord>>>;
    fn save(&self, record: PilotRecord) -> BoxFuture<'_, anyhow::Result<()>>;
    /// Every record (at boot: for the suits left out there).
    fn all(&self) -> BoxFuture<'_, anyhow::Result<Vec<PilotRecord>>>;
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

    fn all(&self) -> BoxFuture<'_, anyhow::Result<Vec<PilotRecord>>> {
        let all = self.0.lock().map(|m| m.values().cloned().collect()).unwrap_or_default();
        Box::pin(async move { Ok(all) })
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
        anyhow::ensure!(is_key(address), "not a pilot key: {address:?}");
        Ok(self.dir.join(format!("{address}.json")))
    }
}

/// Keys are `0x` and 40 hex digits: nothing else becomes a file name.
fn is_key(address: &str) -> bool {
    address.len() == 42 && address.starts_with("0x") && address[2..].bytes().all(|b| b.is_ascii_hexdigit())
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

    fn all(&self) -> BoxFuture<'_, anyhow::Result<Vec<PilotRecord>>> {
        let dir = self.dir.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let mut all = Vec::new();
                for entry in std::fs::read_dir(&dir)? {
                    let path = entry?.path();
                    let key = path.file_stem().and_then(|k| k.to_str()).filter(|k| is_key(k));
                    let key = key.and_then(bc_auth::parse_address);
                    let Some(key) = key.filter(|_| path.extension().is_some_and(|e| e == "json")) else {
                        continue;
                    };
                    // One unreadable record keeps nobody else's suit from coming back.
                    match std::fs::read(&path).map_err(anyhow::Error::from).and_then(|bytes| {
                        serde_json::from_slice::<PilotRecord>(&bytes).map_err(anyhow::Error::from)
                    }) {
                        Ok(r) => all.push(r),
                        Err(e) => tracing::warn!(address = %short(&key), "pilot store: {e}"),
                    }
                }
                Ok(all)
            })
            .await?
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

    /// The pilot's sleeper `suit` is gone (destroyed, or cleared for room): if it was left in a
    /// hide spot, the next server run mustn't put it back. A pilot flying again is left to their
    /// session, which forgets the sleeper (and its spot) as they come in.
    pub async fn clear_parked(&self, address: Address, suit: (u16, u16)) {
        if self.is_online(&address) {
            return;
        }
        let mut r = match self.store.load(&key(&address)).await {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("pilot store: {e}");
                return;
            }
        };
        let theirs = r.sleeper.is_some_and(|s| s.run == self.run && (s.suit, s.generation) == suit);
        if r.parked.is_none() || !theirs || self.is_online(&address) {
            return;
        }
        r.parked = None;
        tracing::info!(address = %short(&address), suit = suit.0, "hidden suit gone: not kept");
        self.save(r).await;
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
    async fn every_record_is_listed_but_what_isnt_one() {
        let dir = std::env::temp_dir().join(format!("bc-pilots-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileStore::new(&dir).unwrap();
        let (a, b) = (PilotRecord::new(&Address([8; 20])), PilotRecord::new(&Address([9; 20])));
        store.save(a.clone()).await.unwrap();
        store.save(b.clone()).await.unwrap();
        // A torn file and strangers in the directory hold up nobody else's.
        std::fs::write(dir.join(format!("0x{}.json", "0a".repeat(20))), b"{ \"address\": ").unwrap();
        std::fs::write(dir.join("notes.json"), b"{}").unwrap();
        std::fs::write(dir.join(format!("0x{}.tmp", "0b".repeat(20))), b"{}").unwrap();
        let mut all = store.all().await.unwrap();
        all.sort_by(|x, y| x.address.cmp(&y.address));
        assert_eq!(all, [a.clone(), b.clone()]);
        let memory = MemoryStore::default();
        memory.save(a.clone()).await.unwrap();
        assert_eq!(memory.all().await.unwrap(), [a]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_parked_suit_is_kept_as_the_sector_recorded_it() {
        let rec = ParkRecord {
            landmark: 1,
            local: Vec3::new(0.1, 596.0, -0.3),
            rot: Quat::from_xyzw(0.1, 0.7, -0.1, 0.7).normalize(),
            stance: 6.0,
            frame: FrameId::Heavyarms,
            faction: Faction::Alliance,
            pilot: PilotKind::Agent,
            home: Homecoming {
                frame: FrameId::Heavyarms,
                parts: [1.0, 0.8, 0.0, 0.5, 1.0, 0.25],
                mounts: 0b101,
                ammo: [3, 0, 77],
                propellant: 412.5,
                cargo_kg: [1, 2, 3, 4],
                held: None,
                bounty: 900,
            },
        };
        let mut r = PilotRecord::new(&Address([10; 20]));
        r.parked = Some(ParkedSuit::new(&rec, 77));
        let again: PilotRecord = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(again, r);
        assert_eq!(again.parked.and_then(|p| p.record()), Some(rec));
        // Records from before suits were kept in hide spots read as keeping none.
        let mut old = serde_json::to_value(PilotRecord::new(&Address([10; 20]))).unwrap();
        old.as_object_mut().unwrap().remove("parked");
        assert_eq!(serde_json::from_value::<PilotRecord>(old).unwrap().parked, None);
        // A frame or a side there isn't: nothing to put back.
        let odd = ParkedSuit { faction: "zeon".into(), ..ParkedSuit::new(&rec, 77) };
        assert_eq!(odd.record(), None);
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
