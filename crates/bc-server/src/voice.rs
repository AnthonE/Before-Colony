//! Proximity voice: the relay (`bc_proto::voice`). The owner's call (`docs/DECISIONS.md`): on foot a
//! voice carries 30 m through the air; in a suit it goes out over the radio, to the sensors' range.
//!
//! Off the sector's tick, in each talker's own session task, beside the plaza: a talker's frame is
//! checked (one 20 ms Opus frame, within the talker's own bucket, apart from input's) and copied to
//! each listener the server picked. The cull is the server's: a client never receives a frame from
//! beyond the reach, so nobody can listen past it (zero bytes, not quieter bytes).
//!
//! - **On foot** (a city strip, the base's deck, the ring's town): the people on the talker's floor
//!   within [`FOOT_M`] ([`crate::plaza::Plaza::near`]), through the air. The talker is named by their
//!   plaza id (their session's), which the people near already know them by.
//! - **In space**: the suits within the talker's sensor range (as tuned: its sensors' upgrades and
//!   damage, `bc_sim::tuning`) whose pilots are at the controls, over the radio. The talker is
//!   named by their suit's entity slot, as the roster does, never by a session's id; no position
//!   goes with it. Nothing goes out from, or to, a wreck, an ejected pilot or a suit asleep.
//! - In the hangar bay, and flying inside the colony: nobody is heard, and nobody hears (not yet).
//!
//! Each listener hears at most [`LANES`] talkers at once; a lane is a talker's until their last
//! frame or [`LANE_QUIET`] of quiet, and only a copy sent holds it. Agents hear nobody unless the
//! server says they may (people's voices aren't an agent's to hear by default). A copy that would
//! crowd the listener's connection (more than [`QUEUED`] waiting in its datagram buffer) isn't sent:
//! under pressure the game's own datagrams go first. Payloads are never decoded, kept or logged,
//! and neither is who heard whom: only counts (`/status`'s `voice`), and no refusal ends a session.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use bc_proto::PilotKind;
use bc_proto::control::roster_flags;
use bc_proto::voice::{VOICE_DOWN_MAX, Via, VoiceFrame, is_one_frame};
use bc_sector::Metrics;
use wtransport::Connection;

use crate::net::NetStats;
use crate::net::game::RosterEntry;
use crate::net::sessions::NO_SESSION;

/// How far a voice carries through the air on foot, m (the owner's default).
pub const FOOT_M: f32 = 30.0;
/// At most this many talkers heard at once by one listener.
pub const LANES: usize = 8;
/// A listener's lane is a talker's until their last frame, or this long quiet.
pub const LANE_QUIET: Duration = Duration::from_millis(350);
/// A talker's listeners are worked out again this often while they talk (the plaza's cadence).
pub const REFRESH: Duration = Duration::from_millis(100);
/// Frames a second a talker may send (one each 20 ms), and how many at once.
pub const RATE: f64 = 50.0;
pub const BURST: f64 = 10.0;
/// A copy isn't sent to a listener whose connection has more than this waiting to go out in its
/// datagram buffer, bytes: a few datagrams' worth (a tick's snapshot is one or a few). Past that
/// the link is behind and voice gives way, so it never queues ahead of a snapshot (quinn's buffer,
/// 1 MiB, would hold seconds of them before it dropped one).
pub const QUEUED: usize = 4 * bc_proto::MAX_DATAGRAM;

/// Whether a copy gives way to a connection whose datagram buffer holds `size` bytes with `room`
/// still free.
pub fn gives_way(size: usize, room: usize) -> bool {
    size.saturating_sub(room) > QUEUED
}

/// Whether a suit at `listener` hears one at `talker` whose radio reaches `range` m.
pub fn radio_reach(talker: glam::Vec3, range: f32, listener: glam::Vec3) -> bool {
    talker.distance_squared(listener) <= range * range
}

/// The suit in flight slot `flight` (its entity slot), if there's one and it's alive: a wreck's
/// radio is dead, and so is an ejected pilot's.
pub fn live_suit(metrics: &Metrics, flight: u16) -> Option<u16> {
    let p = metrics.pilots.get(flight as usize)?;
    let suit = Metrics::load(&p.suit).checked_sub(1)?;
    (Metrics::load(&p.alive) != 0).then_some(suit as u16)
}

/// Who might hear the pilot flying flight slot `talker` over the radio: the other flight slots with
/// a live suit within its radio's range (its sensors' tuned reach, as the sector last published it)
/// whose roster entry has a session at the controls (not asleep), as
/// `(session, flight slot)`; and the talker's suit's entity slot (`None`: no live suit in that
/// slot). Flight slots are the sector's (`metrics.pilots`) and suits the roster's (entity slots):
/// `suit - 1` joins them.
pub fn radio_listeners(
    metrics: &Metrics,
    roster: &HashMap<u16, RosterEntry>,
    talker: u16,
    out: &mut Vec<(u16, Option<u16>)>,
) -> Option<u16> {
    out.clear();
    let suit = live_suit(metrics, talker)?;
    let me = &metrics.pilots[talker as usize];
    let (at, range) = (me.pos(), me.reach());
    for (s, p) in metrics.pilots.iter().enumerate() {
        let Some(theirs) = live_suit(metrics, s as u16) else { continue };
        if s == talker as usize || !radio_reach(at, range, p.pos()) {
            continue;
        }
        match roster.get(&theirs) {
            Some(e) if e.session != NO_SESSION && e.flags & roster_flags::ASLEEP == 0 => {
                out.push((e.session, Some(s as u16)));
            }
            _ => {}
        }
    }
    Some(suit)
}

/// Where a talker is, for the relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    /// On foot: their plaza id (their session's).
    Foot(u16),
    /// In space, at the controls of a live suit through flight slot `flight`.
    Radio { flight: u16 },
    /// Nowhere a voice carries.
    Nowhere,
}

/// A token bucket: [`RATE`] a second, [`BURST`] at once.
#[derive(Clone, Copy, Debug)]
pub struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Bucket {
    pub fn new(now: Instant) -> Self {
        Self { tokens: BURST, last: now }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + dt * RATE).min(BURST);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// A listener's lanes: who they're hearing now.
#[derive(Clone, Copy, Debug)]
pub struct Lanes([(u32, Option<Instant>); LANES]);

impl Default for Lanes {
    fn default() -> Self {
        Self([(0, None); LANES])
    }
}

impl Lanes {
    /// The lane a frame from `key` (how it came, and who) would go out on at `now`: the one it holds,
    /// or a free one (`None`: all taken).
    pub fn lane(&self, key: u32, now: Instant) -> Option<usize> {
        let live = |l: &(u32, Option<Instant>)| l.1.is_some_and(|until| until > now);
        self.0.iter().position(|l| live(l) && l.0 == key).or_else(|| self.0.iter().position(|l| !live(l)))
    }

    /// A frame from `key` went out on lane `i` at `now`: the lane is theirs until [`LANE_QUIET`], or
    /// free again if it was their last (`end`).
    pub fn hold(&mut self, i: usize, key: u32, now: Instant, end: bool) {
        self.0[i] = (key, (!end).then(|| now + LANE_QUIET));
    }

    /// Whether a frame from `key` gets through at `now` (and goes out): [`Self::lane`], then
    /// [`Self::hold`].
    pub fn admit(&mut self, key: u32, now: Instant, end: bool) -> bool {
        let Some(i) = self.lane(key, now) else { return false };
        self.hold(i, key, now, end);
        true
    }
}

/// How a voice came, and who: a lane's key.
fn lane_key(talker: u16, via: Via) -> u32 {
    u32::from(via == Via::Radio) << 16 | u32::from(talker)
}

/// A session, as the relay knows it.
pub struct Ear {
    id: u16,
    conn: Connection,
    /// The room its connection's datagram buffer had when it joined (empty then): its size.
    buffer: usize,
    agent: bool,
    /// At the controls of a live suit in space: its flight slot + 1 (0: off the radio).
    radio: AtomicU32,
    lanes: Mutex<Lanes>,
}

#[derive(Default)]
struct Stats {
    /// Frames talkers sent; refused while voice is off, malformed (or not one 20 ms frame), or over
    /// a talker's bucket.
    frames_in: AtomicU64,
    off: AtomicU64,
    bad: AtomicU64,
    limited: AtomicU64,
    /// Frames from somewhere nobody hears (the bay), and from where nobody was near.
    nowhere: AtomicU64,
    unheard: AtomicU64,
    /// Copies sent; not sent for a listener's lanes all being taken, or their connection full.
    relayed: AtomicU64,
    lane_full: AtomicU64,
    gave_way: AtomicU64,
}

/// Every session's ear, the relay's settings, and its counts.
pub struct Relay {
    on: bool,
    agents_hear: bool,
    ears: Mutex<HashMap<u16, Arc<Ear>>>,
    stats: Stats,
}

impl Relay {
    /// `on`: voice is relayed at all (the server's `--no-voice` turns it off); `agents_hear`: agents'
    /// sessions are sent people's voices.
    pub fn new(on: bool, agents_hear: bool) -> Self {
        Self { on, agents_hear, ears: Mutex::default(), stats: Stats::default() }
    }

    /// Session `id` is on (its connection, who's at its controls): its mouth, which takes its ear
    /// away again when dropped.
    pub fn join(self: &Arc<Self>, id: u16, conn: Connection, pilot: PilotKind, now: Instant) -> Mouth {
        let buffer = conn.quic_connection().datagram_send_buffer_space();
        let ear = Arc::new(Ear {
            id,
            conn,
            buffer,
            agent: pilot == PilotKind::Agent,
            radio: AtomicU32::new(0),
            lanes: Mutex::default(),
        });
        if let Ok(mut ears) = self.ears.lock() {
            ears.insert(id, ear.clone());
        }
        Mouth {
            relay: self.clone(),
            ear,
            bucket: Bucket::new(now),
            heard: Vec::new(),
            of: None,
            ids: Vec::new(),
            candidates: Vec::new(),
        }
    }

    fn add(c: &AtomicU64) {
        c.fetch_add(1, Ordering::Relaxed);
    }

    /// `/status`'s `voice`: counts, never who.
    pub fn json(&self) -> serde_json::Value {
        let s = &self.stats;
        let r = |c: &AtomicU64| c.load(Ordering::Relaxed);
        serde_json::json!({
            "on": self.on,
            "agents_hear": self.agents_hear,
            "sessions": self.ears.lock().map_or(0, |e| e.len()),
            "frames_in": r(&s.frames_in),
            "off": r(&s.off),
            "bad": r(&s.bad),
            "limited": r(&s.limited),
            "nowhere": r(&s.nowhere),
            "unheard": r(&s.unheard),
            "relayed": r(&s.relayed),
            "lane_full": r(&s.lane_full),
            "gave_way": r(&s.gave_way),
        })
    }
}

/// A session's voice: its ear on the relay (taken away when this is dropped), the bucket its frames
/// come out of, and who heard it last (worked out again each [`REFRESH`] while it talks).
pub struct Mouth {
    relay: Arc<Relay>,
    ear: Arc<Ear>,
    bucket: Bucket,
    /// Weak: a listener gone (their session over) is gone from here too, and their connection with
    /// them, however long since this was worked out.
    heard: Vec<Weak<Ear>>,
    of: Option<(Speaker, Instant)>,
    /// Scratch for the plaza's and the radio's lists.
    pub ids: Vec<u16>,
    pub candidates: Vec<(u16, Option<u16>)>,
}

impl Drop for Mouth {
    fn drop(&mut self) {
        if let Ok(mut ears) = self.relay.ears.lock()
            && ears.get(&self.ear.id).is_some_and(|e| Arc::ptr_eq(e, &self.ear))
        {
            ears.remove(&self.ear.id);
        }
    }
}

impl Mouth {
    /// Whether the session is at the controls of a live suit in space, through flight slot `flight`
    /// (it hears the radio then).
    pub fn on_the_radio(&self, flight: Option<u16>) {
        self.ear.radio.store(flight.map_or(0, |f| u32::from(f) + 1), Ordering::Relaxed);
    }

    /// A datagram of the talker's, at `now`: the frame, if voice is on, it's one 20 ms frame and the
    /// talker isn't over their bucket. Every one is counted.
    pub fn take(&mut self, bytes: &[u8], now: Instant) -> Option<VoiceFrame> {
        let stats = &self.relay.stats;
        Relay::add(&stats.frames_in);
        if !self.relay.on {
            Relay::add(&stats.off);
            return None;
        }
        let frame = match VoiceFrame::decode_up(bytes) {
            Ok(f) if is_one_frame(f.payload()) => f,
            _ => {
                Relay::add(&stats.bad);
                return None;
            }
        };
        if !self.bucket.allow(now) {
            Relay::add(&stats.limited);
            return None;
        }
        Some(frame)
    }

    /// Whether who hears the talker should be worked out again: they've moved on (on foot, in
    /// space, nowhere), or it's been [`REFRESH`].
    pub fn stale(&self, speaker: Speaker, now: Instant) -> bool {
        self.of.is_none_or(|(was, at)| was != speaker || now.saturating_duration_since(at) >= REFRESH)
    }

    /// Who hears the talker now, from `candidates` (`(session, flight slot)`: on foot none, in space
    /// the flight slot the listener must be at the controls through): their ears, never the talker's
    /// own, nor an agent's unless agents hear.
    pub fn listen(&mut self, speaker: Speaker, now: Instant) {
        self.heard.clear();
        self.of = Some((speaker, now));
        let Ok(ears) = self.relay.ears.lock() else { return };
        for &(id, flight) in &self.candidates {
            let Some(ear) = ears.get(&id) else { continue };
            let radio_ok = flight.is_none_or(|f| ear.radio.load(Ordering::Relaxed) == u32::from(f) + 1);
            if id != self.ear.id && radio_ok && (!ear.agent || self.relay.agents_hear) {
                self.heard.push(Arc::downgrade(ear));
            }
        }
    }

    /// `frame` from `talker` (`via`), to each of the listeners last worked out, outside every lock
    /// but each one's own lanes' (held while its copy goes, so a lane is taken only by a copy that
    /// went). Counted in `net` as the datagrams out.
    pub fn relay(&self, frame: &VoiceFrame, talker: u16, via: Via, now: Instant, net: &NetStats) {
        let stats = &self.relay.stats;
        if self.heard.is_empty() {
            Relay::add(&stats.unheard);
            return;
        }
        let mut buf = [0u8; VOICE_DOWN_MAX];
        let Some(n) = frame.relayed(talker, via).encode_down(&mut buf) else {
            Relay::add(&stats.bad);
            return;
        };
        let key = lane_key(talker, via);
        for ear in self.heard.iter().filter_map(Weak::upgrade) {
            let Ok(mut lanes) = ear.lanes.lock() else { continue };
            let Some(lane) = lanes.lane(key, now) else {
                Relay::add(&stats.lane_full);
                continue;
            };
            if gives_way(ear.buffer, ear.conn.quic_connection().datagram_send_buffer_space()) {
                Relay::add(&stats.gave_way);
                continue;
            }
            if ear.conn.send_datagram(&buf[..n]).is_ok() {
                lanes.hold(lane, key, now, frame.end);
                Relay::add(&stats.relayed);
                NetStats::add(&net.datagrams_out, 1);
                NetStats::add(&net.bytes_out, n as u64);
            }
        }
    }

    /// A frame from where nobody hears (the bay, a wreck, inside the colony): counted, not sent.
    pub fn nowhere(&self) {
        Relay::add(&self.relay.stats.nowhere);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::FrameId;
    use glam::Vec3;

    #[test]
    fn the_bucket_lets_a_talker_through_at_fifty_a_second_ten_at_once() {
        let t0 = Instant::now();
        let mut b = Bucket::new(t0);
        assert_eq!((0..100).filter(|_| b.allow(t0)).count(), 10, "the burst");
        // A second of 20 ms frames, paced: every one.
        let paced = (1..=50).filter(|k| b.allow(t0 + Duration::from_millis(20 * k))).count();
        assert_eq!(paced, 50);
        // Faster than that is held to the rate.
        let t1 = t0 + Duration::from_secs(10);
        let fast = (0..200).filter(|k| b.allow(t1 + Duration::from_millis(5 * k))).count();
        assert!((55..=62).contains(&fast), "{fast} in a second at 200 a second");
    }

    #[test]
    fn a_listener_hears_eight_talkers_at_once_and_a_lane_frees_on_the_last_frame_or_quiet() {
        let t0 = Instant::now();
        let mut lanes = Lanes::default();
        for k in 0..8 {
            assert!(lanes.admit(k, t0, false));
        }
        assert!(!lanes.admit(8, t0, false), "a ninth waits");
        assert!(lanes.admit(3, t0 + Duration::from_millis(20), false), "one held keeps its lane");
        // Talker 3's last frame gets through and frees its lane for the ninth.
        assert!(lanes.admit(3, t0 + Duration::from_millis(40), true));
        assert!(lanes.admit(8, t0 + Duration::from_millis(40), false));
        assert!(!lanes.admit(9, t0 + Duration::from_millis(40), false));
        // Quiet for a while, the others' lanes are free again (talker 8's isn't: it spoke later).
        let later = t0 + LANE_QUIET + Duration::from_millis(1);
        for k in 9..16 {
            assert!(lanes.admit(k, later, false), "{k}");
        }
        assert!(!lanes.admit(16, later, false));
        // The air's talker 5 and the radio's suit 5 are two talkers.
        assert_ne!(lane_key(5, Via::Air), lane_key(5, Via::Radio));
        // A copy that didn't go out (it gave way, or the send failed) takes no lane.
        let mut lanes = Lanes::default();
        for k in 0..7 {
            assert!(lanes.admit(k, t0, false));
        }
        assert_eq!(lanes.lane(7, t0), Some(7), "one lane left");
        assert_eq!(lanes.lane(8, t0), Some(7), "and it's still free: lane() takes nothing");
        lanes.hold(7, 8, t0, false);
        assert_eq!(lanes.lane(7, t0), None);
    }

    #[test]
    fn voice_gives_way_once_a_few_datagrams_wait_not_half_a_mebibyte() {
        let size = 1 << 20;
        assert!(!gives_way(size, size), "an empty buffer");
        assert!(!gives_way(size, size - 3 * bc_proto::MAX_DATAGRAM), "a tick's snapshot waiting");
        assert!(!gives_way(size, size - QUEUED));
        assert!(gives_way(size, size - QUEUED - 1));
        assert!(gives_way(size, size - 64 * 1024), "a link this far behind is well past it");
        assert!(gives_way(size, 0));
        const { assert!(QUEUED <= 8 * 1024, "a few KiB, not a share of the buffer") };
    }

    #[test]
    fn a_suits_radio_reaches_to_its_range_and_no_further() {
        let at = Vec3::new(100.0, -40.0, 9_000.0);
        let range = 8_000.0;
        assert!(radio_reach(at, range, at));
        assert!(radio_reach(at, range, at + Vec3::new(0.0, 7_999.0, 0.0)));
        assert!(radio_reach(at, range, at + Vec3::new(8_000.0, 0.0, 0.0)), "at the edge");
        assert!(!radio_reach(at, range, at + Vec3::new(0.0, 0.0, -8_001.0)));
        assert!(!radio_reach(at, range, at + Vec3::new(5_700.0, 5_700.0, 0.0)));
    }

    #[test]
    fn the_radio_bridges_flight_slots_to_the_roster_and_skips_sleepers_and_empty_slots() {
        let m = Metrics::new(4);
        // A tuned reach (sensor upgrades, damage), not the frame's: the sector publishes it.
        let range = 0.75 * bc_sim::content::frame(FrameId::Argo).sensor_range;
        let seat = |s: usize, suit: u64, at: Vec3| {
            Metrics::set(&m.pilots[s].suit, suit);
            Metrics::set(&m.pilots[s].alive, 1);
            Metrics::set(&m.pilots[s].frame, FrameId::Argo as u64);
            m.pilots[s].set_pos(at);
            m.pilots[s].set_reach(range);
        };
        seat(0, 11, Vec3::ZERO); // the talker, in suit 10
        seat(1, 21, Vec3::X * (range - 10.0)); // suit 20, in range
        seat(2, 31, Vec3::Y * (range + 10.0)); // suit 30, out of range
        seat(3, 41, Vec3::Z * 50.0); // suit 40, asleep
        let entry = |session: u16, flags: u8| RosterEntry {
            name: "x".into(),
            pilot: PilotKind::Human,
            session,
            flags,
            address: None,
        };
        let roster: HashMap<u16, RosterEntry> = [
            (10, entry(1, 0)),
            (20, entry(2, 0)),
            (30, entry(3, 0)),
            (40, entry(NO_SESSION, roster_flags::ASLEEP)),
        ]
        .into();
        let mut out = Vec::new();
        assert_eq!(radio_listeners(&m, &roster, 0, &mut out), Some(10));
        assert_eq!(out, vec![(2, Some(1))]);
        // A wreck neither hears nor talks.
        Metrics::set(&m.pilots[1].alive, 0);
        assert_eq!(radio_listeners(&m, &roster, 0, &mut out), Some(10));
        assert!(out.is_empty());
        assert_eq!(radio_listeners(&m, &roster, 1, &mut out), None);
        assert_eq!(live_suit(&m, 1), None);
        // A slot with no suit in it says nothing.
        Metrics::set(&m.pilots[0].suit, 0);
        assert_eq!(radio_listeners(&m, &roster, 0, &mut out), None);
        assert_eq!(radio_listeners(&m, &roster, 9, &mut out), None);
    }
}
