//! The client's model of the sector, built from snapshots.

use std::collections::{HashMap, VecDeque};

use bc_proto::events::{BurstCause, Event};
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{
    BodyRef, CHUNK_BITS, ChunkDesc, EntityState, Faction, FrameId, MAX_ENTITIES, MISSILE_BITS, MissileState,
    NO_CHUNK, ObjectState, OwnState, Part, PilotKind, RockState, Segment, WeaponKind, ZeroInfo,
};
use bc_sim::TICK_HZ;
use bc_sim::bodies::{Bodies, Body};
use bc_sim::chunks::{self, held_pose, segment_pos, segment_rot};
use bc_sim::colony::hall::{Drill, DrillEvent};
use bc_sim::content::{SpecialKind, WeaponClass, frame, frame_name, weapon};
use bc_sim::flight::FlightState;
use bc_sim::ground::{Anchor, derive};
use bc_sim::perception::{Contact, KitView, Perception, SelfView};
use bc_sim::zero::N_HYP;
use bc_sim::zero::hypotheses::{self, Maneuver};
use bc_sim::zero::rollout::{STEPS, rollout};
use glam::{Quat, Vec3};

use crate::interp::{EntityTrack, Pose};
use crate::predict::Predictor;
use crate::surface::BodySet;

const HZ: f64 = TICK_HZ as f64;
/// Missiles not listed for this long are dropped: out of range, or lost with their burst (ticks).
const MISSILE_STALE_TICKS: u32 = 15;

/// A missile in flight, as the client last heard of it.
#[derive(Clone, Copy, Debug)]
pub struct MissileTrack {
    /// The newest record, and the tick it came in...
    pub latest: MissileState,
    pub latest_tick: u32,
    /// ...and the one before it.
    pub prev: Option<(u32, MissileState)>,
}

impl MissileTrack {
    /// Where the missile is at tick `t`: between the two newest records, or extrapolated (a few
    /// ticks at most: missiles steer) from the newest.
    pub fn pos_at(&self, t: f64) -> Vec3 {
        let latest = f64::from(self.latest_tick);
        if let Some((pt, p)) = self.prev
            && t < latest
            && t >= f64::from(pt)
            && self.latest_tick > pt
        {
            let u = ((t - f64::from(pt)) / (latest - f64::from(pt))) as f32;
            return p.pos.lerp(self.latest.pos, u);
        }
        let dt = ((t - latest).clamp(-8.0, 8.0) / HZ) as f32;
        self.latest.pos + self.latest.vel * dt
    }
}

/// A missile's burst, for the effects.
#[derive(Clone, Copy, Debug)]
pub struct MissileBurstMark {
    pub tick: u32,
    /// The missile's pool id.
    pub id: u16,
    pub pos: Vec3,
    pub cause: BurstCause,
    /// What kind of missile it was, if the client saw it fly.
    pub kind: Option<WeaponKind>,
}

/// A beam in flight (straight line, constant velocity).
#[derive(Clone, Debug)]
pub struct Beam {
    pub id: u16,
    pub shooter: u16,
    pub weapon: WeaponKind,
    pub origin: Vec3,
    pub velocity: Vec3,
    /// Tick it left the muzzle (fractional for client-predicted shots).
    pub spawn_tick: f64,
    pub ttl_ticks: f64,
    /// Drawn by the shooter's own client before the server confirmed it.
    pub predicted: bool,
    pub shot_seq: u8,
}

impl Beam {
    pub fn pos_at(&self, t: f64) -> Vec3 {
        self.origin + self.velocity * ((t - self.spawn_tick) / HZ) as f32
    }
    pub fn alive_at(&self, t: f64) -> bool {
        t >= self.spawn_tick - 1.0 && t <= self.spawn_tick + self.ttl_ticks
    }
}

/// A training round scoring on one of the Blast Hall's targets (`bc_sim::colony::hall`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetMark {
    pub tick: u32,
    pub target: u8,
    pub by_me: bool,
}

/// A hit, for sparks and hit markers.
#[derive(Clone, Debug)]
pub struct HitMark {
    pub pos: Vec3,
    pub tick: u32,
    pub target: u16,
    pub part: Part,
    pub weapon: WeaponKind,
    pub by_me: bool,
    pub on_me: bool,
    pub shooter: u16,
    /// The shot's direction, when it was a beam this client saw.
    pub dir: Option<Vec3>,
}

impl HitMark {
    /// Where the shot met the hit part's armour on a target of `frame` posed at `pos`/`rot`, and
    /// the surface normal there. Without the shot's line, it comes from `from` (the shooter).
    pub fn impact(&self, frame: FrameId, pos: Vec3, rot: Quat, from: Option<Vec3>) -> (Vec3, Vec3) {
        let cap = bc_sim::content::frame(frame).capsules[self.part as usize];
        let (a, b, r) = bc_sim::collide::capsule_world(&cap, pos, rot);
        let dir =
            self.dir.or_else(|| from.map(|f| (pos - f).normalize_or(Vec3::NEG_Z))).unwrap_or(Vec3::NEG_Z);
        bc_sim::collide::capsule_impact((a + b) * 0.5, dir, a, b, r)
    }
}

/// Kill feed and other notices.
#[derive(Clone, Debug)]
pub enum FeedLine {
    Kill {
        tick: u32,
        victim: u16,
        killer: u16,
    },
    Seizure {
        tick: u32,
        pilot: u16,
        active: bool,
    },
    Clash {
        tick: u32,
        a: u16,
        b: u16,
    },
    /// A pilot ejected from their suit.
    Eject {
        tick: u32,
        pilot: u16,
    },
    /// A pilot blew up their doomed suit.
    Blast {
        tick: u32,
        pilot: u16,
    },
}

/// A pilot's capsule thrown clear of their suit (`Event::Eject`): it coasts from `pos` at `vel`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EjectMark {
    pub tick: u32,
    pub suit: u16,
    pub pos: Vec3,
    pub vel: Vec3,
}

impl World {
    /// How the pilot's own suit was last lost: they ejected from it, or blew it up.
    pub fn my_loss(&self) -> (bool, bool) {
        let Some((at, me)) = self.my_loss_tick.zip(self.own_slot()) else { return (false, false) };
        let ejected = self.ejections.iter().any(|e| e.suit == me && e.tick == at);
        let blown = self.blasts.iter().any(|b| b.1 == me && b.0 == at);
        (ejected, blown)
    }
}

impl EjectMark {
    /// Where the capsule is at tick `t` (plus a fraction): it coasts.
    pub fn pos_at(&self, t: f64) -> Vec3 {
        self.pos + self.vel * ((t - f64::from(self.tick)) as f32 * bc_sim::config::DT)
    }
}

/// How a chunk moves, as the client knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectMotion {
    /// Drifting on a segment (from `seg.t0`).
    Free(Segment),
    /// In `holder`'s hand since tick `since`.
    Held { holder: u16, right: bool, rot: Quat, since: u32 },
}

impl ObjectMotion {
    /// The tick this motion began.
    pub fn starts(&self) -> u32 {
        match *self {
            ObjectMotion::Free(s) => s.t0,
            ObjectMotion::Held { since, .. } => since,
        }
    }
}

/// A salvage chunk the client knows of.
#[derive(Clone, Copy, Debug)]
pub struct ObjectTrack {
    pub generation: u8,
    pub desc: ChunkDesc,
    /// Its newest motion...
    pub motion: ObjectMotion,
    /// ...and the one before it, still in force until the newest begins (news of a bounce or a
    /// grab arrives before the moment the client draws).
    pub prev: Option<ObjectMotion>,
}

impl ObjectTrack {
    /// The motion in force at tick `t`.
    pub fn motion_at(&self, t: f64) -> ObjectMotion {
        match self.prev {
            Some(p) if f64::from(self.motion.starts()) > t => p,
            _ => self.motion,
        }
    }
}

/// One threat's predicted futures, for the ZERO overlay.
#[derive(Clone, Debug)]
pub struct Ghost {
    pub slot: u16,
    pub probs: [f32; N_HYP],
    pub paths: [[Vec3; STEPS]; N_HYP],
}

/// What the pilot's kit has done and what the client has seen of the Gundams' mechanics, counted
/// as snapshots arrive (so the counts don't depend on how fast the page renders).
#[derive(Clone, Copy, Debug, Default)]
pub struct KitStats {
    /// The pilot's own hits, by `WeaponClass`.
    pub hits_by_class: [u32; 5],
    /// Times the own suit's special engaged: a change of form, the jammer, Full Open, the Cross
    /// Crusher.
    pub specials: u32,
    /// Changes of the own suit's form that completed (Wing Zero ↔ Neo-Bird).
    pub transforms: u32,
    /// Missile locks the own suit acquired.
    pub locks: u32,
    /// Distinct missiles seen in flight, and missile bursts.
    pub missiles_seen: u32,
    pub bursts_seen: u32,
    /// Snapshots in which some suit (the own included) was seen jamming, burning with a
    /// flamethrower, or striking with the Dragon Fang.
    pub jamming: u32,
    pub flame: u32,
    pub fang: u32,
}

/// What the world has had to throw away.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorldStats {
    /// Records of suits on a body this client doesn't know (a rock past the field, a landmark
    /// the sector hasn't got): dropped, never drawn. The server never sends one.
    pub unresolved_bodies: u64,
}

/// Lock assist picks a target within this angle of the reticle (rad)...
pub const LOCK_PICK: f32 = 0.175; // 10°
/// ...and keeps it while it stays within this.
pub const LOCK_KEEP: f32 = 0.262; // 15°

pub struct World {
    /// Newest snapshot tick.
    pub tick: u32,
    pub entities: Vec<Option<EntityTrack>>,
    /// The own suit's newest state, in the sector's frame: one on a body (sent in the body's
    /// frame) is put where the body carries it at the snapshot's tick, and its `surface` still
    /// says which body and how it stands on it.
    pub own: Option<OwnState>,
    /// The own state as it was sent (on a body, in the body's frame).
    own_sent: Option<OwnState>,
    pub zero: Option<ZeroInfo>,
    pub beams: Vec<Beam>,
    pub hits: Vec<HitMark>,
    pub feed: VecDeque<FeedLine>,
    pub roster: HashMap<u16, (String, PilotKind)>,
    /// `bc_proto::control::roster_flags` by entity slot (signed in, asleep).
    pub roster_flags: HashMap<u16, u8>,
    /// Rocks whose state differs from the generated field's (mined, shattered), by id.
    pub rocks: HashMap<u16, RockState>,
    /// Salvage chunks in range, by id.
    pub objects: Vec<Option<ObjectTrack>>,
    /// The hulk each destroyed suit became, by slot (from the kill events).
    pub hulks: HashMap<u16, u16>,
    /// Rocks that shattered, newest last: (tick, rock).
    pub rock_breaks: VecDeque<(u32, u16)>,
    /// Blows that got through to a suit's systems, newest last: (tick, suit, system, level).
    pub system_hits: VecDeque<(u32, u16, u8, u8)>,
    /// Missiles in flight nearby, by pool id.
    pub missiles: Vec<Option<MissileTrack>>,
    /// Missiles that burst, newest last.
    pub missile_bursts: VecDeque<MissileBurstMark>,
    /// The Blast Hall's targets struck, newest last, and how many by this pilot's rounds.
    pub target_hits: VecDeque<TargetMark>,
    pub my_target_hits: u32,
    /// Suits doomed (their torsos breached, their reactors going), by slot: the tick it began.
    pub doomed: HashMap<u16, u32>,
    /// The tick the pilot's own suit was last lost (its `Kill`): an ejection or a blast of theirs
    /// that tick is how.
    pub my_loss_tick: Option<u32>,
    /// Pilots' capsules thrown clear, newest last.
    pub ejections: VecDeque<EjectMark>,
    /// Reactors blown by their pilots, newest last: (tick, suit, where).
    pub blasts: VecDeque<(u32, u16, Vec3)>,
    /// The pilot's drill in the Blast Hall (`bc_sim::colony::hall::Drill`), fed their own rounds'
    /// strikes and each snapshot's tick, as the server's sector feeds its own; and what it did,
    /// oldest first (the HUD takes them).
    pub drill: Drill,
    pub drill_news: Vec<DrillEvent>,
    seen: VecDeque<u16>,
    pub faction: Faction,
    pub my_hits: u32,
    pub my_kills: u32,
    /// Of them, Mobile Dolls (as last seen).
    pub doll_kills: u32,
    pub my_deaths: u32,
    pub hits_taken: u32,
    pub kit: KitStats,
    /// The sector's bodies, which riders are drawn on (from the Welcome).
    pub bodies: BodySet,
    pub stats: WorldStats,
}

impl World {
    pub fn new(faction: Faction) -> Self {
        Self {
            tick: 0,
            entities: vec![None; MAX_ENTITIES],
            own: None,
            own_sent: None,
            zero: None,
            beams: Vec::new(),
            hits: Vec::new(),
            feed: VecDeque::new(),
            roster: HashMap::new(),
            roster_flags: HashMap::new(),
            rocks: HashMap::new(),
            objects: vec![None; 1 << CHUNK_BITS],
            hulks: HashMap::new(),
            rock_breaks: VecDeque::new(),
            system_hits: VecDeque::new(),
            missiles: vec![None; 1 << MISSILE_BITS],
            missile_bursts: VecDeque::new(),
            target_hits: VecDeque::new(),
            my_target_hits: 0,
            doomed: HashMap::new(),
            my_loss_tick: None,
            ejections: VecDeque::new(),
            blasts: VecDeque::new(),
            drill: Drill::default(),
            drill_news: Vec::new(),
            seen: VecDeque::new(),
            faction,
            my_hits: 0,
            my_kills: 0,
            doll_kills: 0,
            my_deaths: 0,
            hits_taken: 0,
            kit: KitStats::default(),
            bodies: BodySet::default(),
            stats: WorldStats::default(),
        }
    }

    /// The own state as the server sent it: on a body, in the body's frame.
    pub fn own_sent(&self) -> Option<&OwnState> {
        self.own_sent.as_ref()
    }

    pub fn own_slot(&self) -> Option<u16> {
        self.own.map(|o| o.slot)
    }

    fn first_time(&mut self, id: u16) -> bool {
        if self.seen.contains(&id) {
            return false;
        }
        self.seen.push_back(id);
        if self.seen.len() > 512 {
            self.seen.pop_front();
        }
        true
    }

    /// Display name for an entity slot: the pilot's name, or a Mobile Doll callsign.
    pub fn name_of(&self, slot: u16) -> String {
        if let Some((name, _)) = self.roster.get(&slot) {
            return name.clone();
        }
        match self.entity(slot) {
            Some(t) => format!("{}-{:02}", frame_name(t.latest.frame).to_uppercase(), slot % 100),
            None => format!("UNIT-{slot:02}"),
        }
    }

    pub fn entity(&self, slot: u16) -> Option<&EntityTrack> {
        self.entities.get(slot as usize).and_then(Option::as_ref)
    }

    /// Lock assist: the hostile the pilot is pointing at, seen from `from` along `aim` at time `t`
    /// (ticks): the one nearest the reticle within [`LOCK_PICK`], kept while within [`LOCK_KEEP`].
    pub fn lock_assist(&self, from: Vec3, aim: Vec3, current: Option<u16>, t: f64) -> Option<u16> {
        let off = |slot: u16| {
            let track = self.entity(slot)?;
            let e = &track.latest;
            if e.faction == self.faction || e.flags & ent_flags::WRECK != 0 {
                return None;
            }
            let to = track.sample(t, &self.bodies).pos - from;
            Some(aim.angle_between(to))
        };
        if let Some(slot) = current
            && off(slot).is_some_and(|a| a <= LOCK_KEEP)
        {
            return Some(slot);
        }
        (0..self.entities.len() as u16)
            .filter_map(|slot| off(slot).filter(|a| *a <= LOCK_PICK).map(|a| (a, slot)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, slot)| slot)
    }

    /// Applies a snapshot's missile list (before its events, whose bursts end missiles).
    pub fn apply_missiles(&mut self, tick: u32, list: &[MissileState]) {
        if tick < self.tick {
            return;
        }
        for m in list {
            let slot = &mut self.missiles[usize::from(m.id) & ((1 << MISSILE_BITS) - 1)];
            match slot {
                Some(track) if track.latest.generation == m.generation => {
                    if tick > track.latest_tick {
                        track.prev = Some((track.latest_tick, track.latest));
                        track.latest = *m;
                        track.latest_tick = tick;
                    }
                }
                other => {
                    *other = Some(MissileTrack { latest: *m, latest_tick: tick, prev: None });
                    self.kit.missiles_seen += 1;
                }
            }
        }
        for slot in self.missiles.iter_mut() {
            if slot.as_ref().is_some_and(|m| tick.saturating_sub(m.latest_tick) > MISSILE_STALE_TICKS) {
                *slot = None;
            }
        }
    }

    /// Missiles in flight that the client knows of.
    pub fn missiles(&self) -> impl Iterator<Item = &MissileTrack> {
        self.missiles.iter().flatten()
    }

    /// Applies a decoded snapshot. A suit on a body is kept in the body's frame (it is drawn there,
    /// [`EntityTrack::sample`]); a record naming a body this sector doesn't have is dropped and
    /// counted ([`WorldStats::unresolved_bodies`]).
    pub fn apply(
        &mut self,
        tick: u32,
        own: Option<OwnState>,
        zero: Option<ZeroInfo>,
        events: &[Event],
        ents: &[EntityState],
    ) {
        if tick < self.tick {
            return;
        }
        if own.and_then(|o| o.surface).is_some_and(|on| !self.bodies.knows(on.body)) {
            self.stats.unresolved_bodies += 1;
        }
        let at = self.bodies.at(tick);
        let own_sent = own;
        let own = own.map(|o| own_in_sector(&o, &at));
        let prev_own = self.own;
        self.tick = tick;
        self.own = own;
        self.own_sent = own_sent;
        self.zero = zero;
        let me = own.map(|o| o.slot);
        if let Some(now) = own {
            self.count_own(now, prev_own);
        }
        let mut seen = (false, false, false);
        for e in ents {
            if Some(e.slot) == me {
                continue;
            }
            if e.on.is_some_and(|on| !self.bodies.knows(on.body)) {
                self.stats.unresolved_bodies += 1;
                continue;
            }
            let e = *e;
            let (jamming, flame, fang) = kit_seen(&e);
            seen = (seen.0 | jamming, seen.1 | flame, seen.2 | fang);
            let slot = e.slot as usize;
            match &mut self.entities[slot] {
                Some(track) if track.latest.generation == e.generation => track.push(tick, e),
                other => *other = Some(EntityTrack::new(tick, e)),
            }
        }
        let own_jamming = own.is_some_and(|o| {
            o.alive
                && o.flags & own_flags::SPECIAL_ACTIVE != 0
                && matches!(frame(o.frame).special, SpecialKind::HyperJammer { .. })
        });
        self.kit.jamming += u32::from(seen.0 || own_jamming);
        self.kit.flame += u32::from(seen.1);
        self.kit.fang += u32::from(seen.2);
        for ev in events {
            self.apply_event(ev, me);
        }
        // The drill's clock at this snapshot's tick: the pilot's strikes up to it came with it or
        // before (but for one held over for room in a crowded datagram, when the HUD may call time
        // early: the board goes by the server's clock).
        if let Some(e) = self.drill.tick(f64::from(tick)) {
            self.drill_said(e);
        }
        // Forget tracks the server stopped updating without telling us (lost Leave).
        for slot in self.entities.iter_mut() {
            if slot.as_ref().is_some_and(|t| tick.saturating_sub(t.latest_tick) > t.stale_ticks()) {
                *slot = None;
            }
        }
    }

    /// Counts the own suit's special engaging, changes of form, and locks acquired.
    fn count_own(&mut self, now: OwnState, before: Option<OwnState>) {
        let Some(before) = before.filter(|b| b.alive && now.alive && b.generation == now.generation) else {
            return;
        };
        let rose = |f: u16| now.flags & f != 0 && before.flags & f == 0;
        if rose(own_flags::SPECIAL_ACTIVE) || rose(own_flags::TRANSFORMING) {
            self.kit.specials += 1;
        }
        if rose(own_flags::LOCK_ACQUIRED) {
            self.kit.locks += 1;
        }
        if now.frame != before.frame {
            self.kit.transforms += 1;
        }
    }

    /// Applies a snapshot's rock and object lists (each record is the thing's whole current state).
    /// A shattered rock is gone from the bodies the view sweeps, too.
    pub fn apply_salvage(&mut self, rocks: &[RockState], objects: &[ObjectState]) {
        for r in rocks {
            self.rocks.insert(r.id, *r);
            self.bodies.set_rock_dead(usize::from(r.id), r.destroyed);
        }
        for o in objects {
            let (generation, desc, motion) = match *o {
                ObjectState::Gone { id } => {
                    if let Some(slot) = self.objects.get_mut(id as usize) {
                        *slot = None;
                    }
                    continue;
                }
                ObjectState::Free { generation, desc, seg, .. } => {
                    (generation, desc, ObjectMotion::Free(seg))
                }
                ObjectState::Held { generation, desc, holder, right, rot, since, .. } => {
                    (generation, desc, ObjectMotion::Held { holder, right, rot, since })
                }
            };
            let Some(slot) = self.objects.get_mut(o.id() as usize) else { continue };
            match slot {
                // The same chunk (a hulk's parts may have changed): keep what it did before.
                Some(t)
                    if t.generation == generation
                        && core::mem::discriminant(&t.desc.kind) == core::mem::discriminant(&desc.kind) =>
                {
                    if t.motion != motion {
                        t.prev = Some(t.motion);
                        t.motion = motion;
                    }
                    t.desc = desc;
                }
                _ => *slot = Some(ObjectTrack { generation, desc, motion, prev: None }),
            }
        }
    }

    /// Where chunk `id` is at tick `t`, and how it's turned. A chunk in the own suit's hand
    /// rides the own suit as drawn (`own`: its place and turn).
    pub fn object_pose(&self, id: u16, t: f64, own: Option<(Vec3, Quat)>) -> Option<(Vec3, Quat)> {
        let track = self.objects.get(id as usize)?.as_ref()?;
        match track.motion_at(t) {
            ObjectMotion::Free(seg) => Some((segment_pos(&seg, t), segment_rot(&seg, t))),
            ObjectMotion::Held { holder, right, rot, .. } => {
                let (pos, turn) = if Some(holder) == self.own_slot() {
                    own?
                } else {
                    let p = self.pose(holder, t)?;
                    (p.pos, p.rot)
                };
                Some(held_pose(pos, turn, right, rot, chunks::radius(&track.desc)))
            }
        }
    }

    /// Whether hulk `id` is still on show as its suit's wreck (so it isn't drawn twice).
    pub fn wreck_on_show(&self, id: u16) -> bool {
        self.hulks.iter().any(|(&slot, &hulk)| {
            hulk == id
                && if Some(slot) == self.own_slot() {
                    self.own.is_some_and(|o| !o.alive)
                } else {
                    self.entity(slot).is_some_and(|e| e.latest.flags & ent_flags::WRECK != 0)
                }
        })
    }

    /// What the drill did, for the HUD to take (a client that never does keeps the newest few).
    fn drill_said(&mut self, e: DrillEvent) {
        if self.drill_news.len() >= 32 {
            self.drill_news.remove(0);
        }
        self.drill_news.push(e);
    }

    fn apply_event(&mut self, ev: &Event, me: Option<u16>) {
        match *ev {
            Event::Leave { tick, slot } => {
                if let Some(t) = self.entities.get_mut(slot as usize)
                    && t.as_ref().is_some_and(|t| t.latest_tick <= tick)
                {
                    *t = None;
                }
            }
            Event::BeamSpawn { id, tick, shooter, weapon: w, shot_seq, origin, velocity } => {
                if !self.first_time(id) {
                    return;
                }
                let ttl = f64::from(weapon(w).ttl_ticks());
                if Some(shooter) == me
                    && let Some(b) =
                        self.beams.iter_mut().find(|b| b.predicted && b.shot_seq == shot_seq && b.weapon == w)
                {
                    // Our own predicted beam: keep drawing it, now confirmed.
                    b.predicted = false;
                    b.id = id;
                    return;
                }
                self.beams.push(Beam {
                    id,
                    shooter,
                    weapon: w,
                    origin,
                    velocity,
                    spawn_tick: f64::from(tick),
                    ttl_ticks: ttl,
                    predicted: false,
                    shot_seq,
                });
            }
            Event::Hit { id, tick, target, part, shooter, weapon: w, .. } => {
                if !self.first_time(id) {
                    return;
                }
                let pos = if Some(target) == me {
                    self.own.map_or(Vec3::ZERO, |o| o.pos)
                } else {
                    self.entity(target).map_or(Vec3::ZERO, |t| t.latest.pos)
                };
                let by_me = Some(shooter) == me;
                let on_me = Some(target) == me;
                if by_me {
                    self.my_hits += 1;
                    self.kit.hits_by_class[weapon(w).class as usize] += 1;
                }
                if on_me {
                    self.hits_taken += 1;
                }
                // The beam that hit stops being drawn (its direction places the sparks).
                let beam = if weapon(w).replication == bc_sim::content::Replication::PerShot {
                    self.beams.iter().position(|b| b.shooter == shooter && b.alive_at(f64::from(tick)))
                } else {
                    None
                };
                let dir = beam.map(|k| self.beams[k].velocity.normalize_or(Vec3::NEG_Z));
                self.hits.push(HitMark { pos, tick, target, part, weapon: w, by_me, on_me, shooter, dir });
                if let Some(k) = beam {
                    self.beams.remove(k);
                }
            }
            Event::Kill { id, tick, victim, killer, hulk } => {
                if !self.first_time(id) {
                    return;
                }
                if hulk == NO_CHUNK {
                    self.hulks.remove(&victim);
                } else {
                    self.hulks.insert(victim, hulk);
                }
                self.doomed.remove(&victim);
                if Some(killer) == me && killer != victim {
                    self.my_kills += 1;
                    if self.entity(victim).is_some_and(|tr| tr.latest.pilot == PilotKind::MobileDoll) {
                        self.doll_kills += 1;
                    }
                }
                if Some(victim) == me {
                    self.my_deaths += 1;
                    self.my_loss_tick = Some(tick);
                }
                self.push_feed(FeedLine::Kill { tick, victim, killer });
            }
            Event::Clash { id, tick, a, b } => {
                if self.first_time(id) {
                    self.push_feed(FeedLine::Clash { tick, a, b });
                }
            }
            Event::Seizure { id, tick, pilot, active } => {
                if self.first_time(id) {
                    self.push_feed(FeedLine::Seizure { tick, pilot, active });
                }
            }
            // The chunks a limb or a shattered rock leaves arrive in the objects list.
            Event::Detach { .. } => {}
            Event::RockBreak { id, tick, rock, .. } => {
                if self.first_time(id) {
                    self.rock_breaks.push_back((tick, rock));
                    while self.rock_breaks.len() > 32 {
                        self.rock_breaks.pop_front();
                    }
                }
            }
            Event::SystemHit { id, tick, target, system, level } => {
                if self.first_time(id) {
                    self.system_hits.push_back((tick, target, system, level));
                    while self.system_hits.len() > 32 {
                        self.system_hits.pop_front();
                    }
                }
            }
            Event::Doomed { id, tick, suit } => {
                if self.first_time(id) {
                    self.doomed.insert(suit, tick);
                }
            }
            Event::Eject { id, tick, suit, pos, vel } => {
                if self.first_time(id) {
                    self.ejections.push_back(EjectMark { tick, suit, pos, vel });
                    while self.ejections.len() > 16 {
                        self.ejections.pop_front();
                    }
                    self.push_feed(FeedLine::Eject { tick, pilot: suit });
                }
            }
            Event::Blast { id, tick, suit, pos } => {
                if self.first_time(id) {
                    self.blasts.push_back((tick, suit, pos));
                    while self.blasts.len() > 16 {
                        self.blasts.pop_front();
                    }
                    self.push_feed(FeedLine::Blast { tick, pilot: suit });
                }
            }
            Event::TargetHit { id, tick, target, shooter } => {
                if self.first_time(id) {
                    let by_me = Some(shooter) == me;
                    if by_me {
                        self.my_target_hits += 1;
                        if let Some(e) = self.drill.strike(target, tick) {
                            self.drill_said(e);
                        }
                    }
                    self.target_hits.push_back(TargetMark { tick, target, by_me });
                    while self.target_hits.len() > 32 {
                        self.target_hits.pop_front();
                    }
                }
            }
            Event::MissileBurst { id, tick, missile, pos, cause } => {
                if self.first_time(id) {
                    let slot = self.missiles.get_mut(usize::from(missile));
                    let kind = slot.as_ref().and_then(|m| m.map(|m| m.latest.kind));
                    if let Some(slot) = slot
                        && slot.is_some_and(|m| m.latest_tick <= tick)
                    {
                        *slot = None;
                    }
                    self.missile_bursts.push_back(MissileBurstMark { tick, id: missile, pos, cause, kind });
                    self.kit.bursts_seen += 1;
                    while self.missile_bursts.len() > 64 {
                        self.missile_bursts.pop_front();
                    }
                }
            }
        }
    }

    fn push_feed(&mut self, line: FeedLine) {
        self.feed.push_back(line);
        while self.feed.len() > 8 {
            self.feed.pop_front();
        }
    }

    /// Adds the local pilot's own shot immediately (confirmed later by the server's event).
    pub fn predict_beam(
        &mut self,
        shooter: u16,
        w: WeaponKind,
        shot_seq: u8,
        origin: Vec3,
        velocity: Vec3,
        tick: f64,
    ) {
        self.beams.push(Beam {
            id: 0,
            shooter,
            weapon: w,
            origin,
            velocity,
            spawn_tick: tick,
            ttl_ticks: f64::from(weapon(w).ttl_ticks()),
            predicted: true,
            shot_seq,
        });
    }

    /// Drops finished beams and old hit marks.
    pub fn prune(&mut self, t: f64) {
        self.beams.retain(|b| t <= b.spawn_tick + b.ttl_ticks && !(b.predicted && t > b.spawn_tick + 45.0));
        let keep_after = self.tick.saturating_sub(60);
        self.hits.retain(|h| h.tick >= keep_after);
    }

    /// Interpolated pose of entity `slot` at time `t`.
    pub fn pose(&self, slot: u16, t: f64) -> Option<Pose> {
        self.entity(slot).map(|e| e.sample(t, &self.bodies))
    }

    /// Builds the same [`Perception`] the server's Mobile Dolls use, from this client's view.
    pub fn perception(&self, t: f64, predict: &Predictor) -> Option<Perception> {
        let own = self.own?;
        let spec = frame(own.frame);
        let s = &predict.state;
        let tuning = bc_sim::tuning::own_tuning(&own);
        let me = SelfView {
            slot: own.slot,
            frame: own.frame,
            faction: self.faction,
            pos: s.pos,
            vel: s.vel,
            rot: s.rot,
            aim: s.rot * Vec3::Z,
            parts: own.parts,
            heat: own.heat,
            energy: own.energy,
            propellant: own.propellant / bc_sim::tuning::tank_cap(spec, &tuning),
            g_strain: s.g_strain,
            ready: [own.weapon_ready & 1 != 0, own.weapon_ready & 2 != 0, own.weapon_ready & 4 != 0],
            overheated: own.flags & own_flags::OVERHEAT != 0,
            kit: KitView {
                lock_acquired: own.flags & own_flags::LOCK_ACQUIRED != 0,
                missile_incoming: own.flags & own_flags::MISSILE_INCOMING != 0,
                special_ready: own.weapon_ready & 8 != 0,
                special_active: own.flags & own_flags::SPECIAL_ACTIVE != 0,
                transforming: own.flags & own_flags::TRANSFORMING != 0,
            },
            surface_n: predict.surface_n(),
            tuning,
        };
        let mut p = Perception::default();
        p.reset(me);
        for (slot, track) in self.entities.iter().enumerate() {
            let Some(track) = track else { continue };
            let e = &track.latest;
            if e.flags & ent_flags::WRECK != 0 {
                continue;
            }
            let pose = track.sample(t, &self.bodies);
            let to_me = (s.pos - pose.pos).normalize_or(Vec3::Z);
            p.offer(Contact {
                slot: slot as u16,
                frame: e.frame,
                faction: e.faction,
                pilot: e.pilot,
                pos: pose.pos,
                vel: pose.vel,
                rot: pose.rot,
                aim: pose.aim,
                accel: track.accel_estimate(&self.bodies),
                hull: f32::from(e.parts[Part::Torso as usize]) / 7.0,
                dist: (pose.pos - s.pos).length(),
                firing: e.flags & (ent_flags::FIRING_PRIMARY | ent_flags::FIRING_SECONDARY) != 0,
                aiming_at_me: pose.aim.dot(to_me) > 0.9986,
                locked_on_me: e.flags & ent_flags::LOCKED_ON_YOU != 0,
                hostile: e.faction != self.faction,
                // Standing on a body (not in the air over one): which way is up off it.
                surface_n: pose.ground.filter(|g| !g.aloft).map_or(Vec3::ZERO, |g| g.up),
            });
        }
        Some(p)
    }

    /// The ZERO System's predicted futures for each tracked threat, drawn from this client's view
    /// with the same rollout code the server uses.
    pub fn zero_ghosts(&self, t: f64) -> Vec<Ghost> {
        let Some(z) = &self.zero else { return Vec::new() };
        let mut out = Vec::new();
        for th in &z.threats[..z.threat_count as usize] {
            let Some(track) = self.entity(th.slot) else { continue };
            let pose = track.sample(t, &self.bodies);
            let spec = frame(track.latest.frame);
            let surface_n = pose.ground.filter(|g| !g.aloft).map_or(Vec3::ZERO, |g| g.up);
            let mut paths = [[Vec3::ZERO; STEPS]; N_HYP];
            for (k, path) in paths.iter_mut().enumerate() {
                let a = hypotheses::accel_on(spec, pose.rot, surface_n, Maneuver::from_index(k));
                *path = rollout(pose.pos, pose.vel, a);
            }
            out.push(Ghost { slot: th.slot, probs: th.probs, paths });
        }
        out
    }

    /// Frame of an entity (or our own suit).
    pub fn frame_of(&self, slot: u16) -> Option<FrameId> {
        if self.own_slot() == Some(slot) {
            return self.own.map(|o| o.frame);
        }
        self.entity(slot).map(|t| t.latest.frame)
    }
}

/// A suit on `body`, in the sector's frame with the body where `bodies` has it: carried, turned and
/// moved by the body, exactly as the server derives it (`bc_sim::ground::derive`).
fn in_sector(
    bodies: &Bodies,
    body: BodyRef,
    local: Vec3,
    rot: Quat,
    vel: Vec3,
    ang_vel: Vec3,
) -> Option<FlightState> {
    let body = Body::from(body);
    let pose = bodies.pose(body)?;
    let mut f = FlightState::default();
    derive(&pose, &Anchor { body, local, rot, vel, ang_vel, stance: 0.0 }, &mut f);
    Some(f)
}

/// The own suit's state in the sector's frame (it's sent in its body's while on one), still
/// saying what it stands on. One naming a body this sector doesn't have is taken as it came.
fn own_in_sector(own: &OwnState, bodies: &Bodies) -> OwnState {
    let Some(on) = own.surface else { return *own };
    match in_sector(bodies, on.body, own.pos, own.rot, own.vel, own.ang_vel) {
        Some(f) => OwnState { pos: f.pos, vel: f.vel, rot: f.rot, ang_vel: f.ang_vel, ..*own },
        None => *own,
    }
}

/// What a contact shows of the Gundams' mechanics: (jamming, a flamethrower burning, the Dragon
/// Fang striking).
fn kit_seen(e: &EntityState) -> (bool, bool, bool) {
    if e.flags & ent_flags::WRECK != 0 {
        return (false, false, false);
    }
    let spec = frame(e.frame);
    let jamming =
        e.flags & ent_flags::SPECIAL != 0 && matches!(spec.special, SpecialKind::HyperJammer { .. });
    let burning = |slot: usize, bit: u16| {
        e.flags & bit != 0 && spec.loadout[slot].is_some_and(|m| weapon(m.weapon).class == WeaponClass::Cone)
    };
    let flame = burning(0, ent_flags::FIRING_PRIMARY) || burning(1, ent_flags::FIRING_SECONDARY);
    let fang = e.flags & (ent_flags::SABER | ent_flags::MELEE_ALT) == ent_flags::SABER | ent_flags::MELEE_ALT;
    (jamming, flame, fang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interp::EntityTrack;

    fn hostile_at(world: &mut World, slot: u16, pos: Vec3) {
        let e = EntityState { slot, faction: Faction::Oz, pos, ..EntityState::default() };
        world.entities[usize::from(slot)] = Some(EntityTrack::new(10, e));
    }

    /// A rider at `local` on `body`, walking over it at `vel` (its frame).
    fn rider(slot: u16, body: BodyRef, local: Vec3, vel: Vec3) -> EntityState {
        use bc_proto::RiderOn;
        EntityState {
            slot,
            faction: Faction::Oz,
            on: Some(RiderOn { body, aloft: false }),
            pos: local,
            vel,
            aim: Vec3::Z,
            ..EntityState::default()
        }
    }

    #[test]
    fn unresolved_bodies_are_dropped() {
        use bc_sim::field::Field;
        use std::sync::Arc;

        let mut world = World::new(Faction::Colonies);
        world.bodies = BodySet::new(Arc::new(Field::generate(0xDEB12, 160)), 1);
        let t = 5_000;
        let local = Vec3::new(0.0, 69.125, 0.0);
        let ents = [
            rider(3, BodyRef::Landmark(0), local, Vec3::Z * 8.0),
            // A landmark the sector hasn't got, a rock its field hasn't: nowhere to draw them.
            rider(4, BodyRef::Landmark(1), local, Vec3::ZERO),
            rider(5, BodyRef::Rock(160), local, Vec3::ZERO),
            rider(6, BodyRef::Rock(159), Vec3::Y * 40.0, Vec3::ZERO),
            EntityState { slot: 7, pos: Vec3::splat(100.0), ..EntityState::default() },
        ];
        world.apply(t, None, None, &[], &ents);
        assert!(world.entity(4).is_none() && world.entity(5).is_none());
        assert_eq!(world.stats.unresolved_bodies, 2);
        // The rest are kept as sent, and drawn on their bodies.
        let on_deck = world.entity(3).expect("on MO-II").latest;
        assert_eq!(on_deck.pos, local);
        let deck = world.bodies.pose_at(Body::Landmark(0), f64::from(t)).unwrap();
        let drawn = world.pose(3, f64::from(t)).unwrap();
        assert!(drawn.pos.distance(deck.to_world(local)) < 1e-3);
        assert!(drawn.vel.distance(deck.point_vel(drawn.pos) + deck.rot * Vec3::Z * 8.0) < 1e-3);
        let rock = world.bodies.field.rocks()[159];
        assert!(
            world.pose(6, f64::from(t)).unwrap().pos.distance(rock.pos + rock.rot * (Vec3::Y * 40.0)) < 1e-3
        );
        assert_eq!(world.pose(7, f64::from(t)).map(|p| p.pos), Some(Vec3::splat(100.0)), "flying free");
        // An own state on a body it doesn't know is taken as it came, and counted.
        let own = OwnState {
            alive: true,
            surface: Some(bc_proto::OwnSurface {
                footing: bc_proto::snapshot::footing::GROUNDED,
                body: BodyRef::Landmark(3),
                stance_q: 146,
            }),
            pos: local,
            ..OwnState::default()
        };
        world.apply(t + 1, Some(own), None, &[], &[]);
        assert_eq!(world.stats.unresolved_bodies, 3);
        assert_eq!(world.own.map(|o| o.pos), Some(local));
    }

    #[test]
    fn the_own_state_is_kept_in_the_sectors_frame_and_as_sent() {
        let mut world = World::new(Faction::Colonies);
        let t = 7_000;
        let local = Vec3::new(-230.0, 0.0, 89.125);
        let own = OwnState {
            alive: true,
            surface: Some(bc_proto::OwnSurface {
                footing: bc_proto::snapshot::footing::GROUNDED,
                body: BodyRef::Landmark(0),
                stance_q: 146,
            }),
            pos: local,
            vel: Vec3::X,
            ..OwnState::default()
        };
        world.apply(t, Some(own), None, &[], &[]);
        let deck = world.bodies.at(t).pose(Body::Landmark(0)).unwrap();
        let o = world.own.unwrap();
        assert_eq!(o.pos, deck.to_world(local));
        assert_eq!(o.vel, deck.point_vel(o.pos) + deck.rot * Vec3::X);
        assert_eq!(o.surface, own.surface, "still saying what it stands on");
        assert_eq!(world.own_sent().map(|o| o.pos), Some(local));
    }

    #[test]
    fn still_rider_tracks_live_300_ticks() {
        let mut world = World::new(Faction::Colonies);
        let local = Vec3::new(0.0, 69.125, 0.0);
        let ents = [
            rider(3, BodyRef::Landmark(0), local, Vec3::ZERO),
            rider(4, BodyRef::Landmark(0), local, Vec3::Z * 3.0),
            EntityState { slot: 5, pos: Vec3::splat(100.0), ..EntityState::default() },
        ];
        world.apply(100, None, None, &[], &ents);
        // Snapshots with nothing of them: a moving suit is given up after 90 ticks, a still rider
        // (refreshed a tenth as often) after 300.
        for t in 101..=191 {
            world.apply(t, None, None, &[], &[]);
        }
        assert!(world.entity(3).is_some());
        assert!(world.entity(4).is_none() && world.entity(5).is_none());
        for t in 192..=400 {
            world.apply(t, None, None, &[], &[]);
        }
        assert!(world.entity(3).is_some(), "lost at 400");
        world.apply(401, None, None, &[], &[]);
        assert!(world.entity(3).is_none(), "kept past 300 ticks");
    }

    #[test]
    fn perception_fills_surface_n_for_grounded_contacts() {
        use bc_proto::RiderOn;

        let mut world = World::new(Faction::Colonies);
        let t = 3_000;
        let standing = rider(3, BodyRef::Landmark(0), Vec3::new(0.0, 69.125, 0.0), Vec3::ZERO);
        let aloft = EntityState {
            on: Some(RiderOn { body: BodyRef::Landmark(0), aloft: true }),
            ..rider(4, BodyRef::Landmark(0), Vec3::new(0.0, 0.0, 100.0), Vec3::ZERO)
        };
        let free = EntityState {
            slot: 5,
            faction: Faction::Oz,
            pos: Vec3::splat(-17_000.0),
            ..EntityState::default()
        };
        world.apply(t, None, None, &[], &[standing, aloft, free]);
        world.own = Some(OwnState { alive: true, slot: 1, ..OwnState::default() });
        let p = world.perception(f64::from(t), &Predictor::default()).expect("a view");
        let deck = world.bodies.at(t).pose(Body::Landmark(0)).unwrap();
        let n = |slot| p.get(slot).map(|c| c.surface_n).unwrap();
        assert!(n(3).distance(deck.rot * Vec3::Y) < 1e-4, "up off the pylon: {:?}", n(3));
        assert_eq!(n(4), Vec3::ZERO, "aloft: not on the surface");
        assert_eq!(n(5), Vec3::ZERO, "flying free");
        assert_eq!(p.me.surface_n, Vec3::ZERO);
        // Standing on the rolling deck it isn't accelerating.
        assert!(p.get(3).unwrap().accel.length() < 1e-3);
    }

    #[test]
    fn lock_assist_picks_what_the_reticle_is_on_and_keeps_it() {
        let mut world = World::new(Faction::Colonies);
        let deg = |a: f32| Vec3::new(a.to_radians().sin(), 0.0, a.to_radians().cos()) * 1_000.0;
        hostile_at(&mut world, 3, deg(6.0));
        hostile_at(&mut world, 4, deg(-3.0));
        hostile_at(&mut world, 5, deg(20.0));
        // The nearest the reticle.
        assert_eq!(world.lock_assist(Vec3::ZERO, Vec3::Z, None, 10.0), Some(4));
        // A lock is kept while it's within 15°, even with another closer to the reticle.
        let aim = deg(12.0).normalize();
        assert_eq!(world.lock_assist(Vec3::ZERO, aim, Some(4), 10.0), Some(4));
        assert_eq!(world.lock_assist(Vec3::ZERO, aim, None, 10.0), Some(3));
        // Nothing within 10°: no lock.
        assert_eq!(world.lock_assist(Vec3::ZERO, -Vec3::Z, None, 10.0), None);
        // Friends and wrecks aren't locked.
        world.faction = Faction::Oz;
        assert_eq!(world.lock_assist(Vec3::ZERO, Vec3::Z, None, 10.0), None);
    }
}
