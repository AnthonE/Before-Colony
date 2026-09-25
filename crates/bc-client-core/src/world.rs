//! The client's model of the sector, built from snapshots.

use std::collections::{HashMap, VecDeque};

use bc_proto::events::Event;
use bc_proto::snapshot::{ent_flags, own_flags};
use bc_proto::{
    CHUNK_BITS, ChunkDesc, EntityState, Faction, FrameId, MAX_ENTITIES, NO_CHUNK, ObjectState, OwnState,
    Part, PilotKind, RockState, Segment, WeaponKind, ZeroInfo,
};
use bc_sim::TICK_HZ;
use bc_sim::chunks::{self, held_pose, segment_pos, segment_rot};
use bc_sim::content::{frame, frame_name, weapon};
use bc_sim::perception::{Contact, Perception, SelfView};
use bc_sim::zero::N_HYP;
use bc_sim::zero::hypotheses::{self, Maneuver};
use bc_sim::zero::rollout::{STEPS, rollout};
use glam::{Quat, Vec3};

use crate::interp::{EntityTrack, Pose};
use crate::predict::Predictor;

const HZ: f64 = TICK_HZ as f64;
/// Tracks not refreshed for this long are dropped (ticks).
const STALE_TICKS: u32 = 90;

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
    Kill { tick: u32, victim: u16, killer: u16 },
    Seizure { tick: u32, pilot: u16, active: bool },
    Clash { tick: u32, a: u16, b: u16 },
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

pub struct World {
    /// Newest snapshot tick.
    pub tick: u32,
    pub entities: Vec<Option<EntityTrack>>,
    pub own: Option<OwnState>,
    pub zero: Option<ZeroInfo>,
    pub beams: Vec<Beam>,
    pub hits: Vec<HitMark>,
    pub feed: VecDeque<FeedLine>,
    pub roster: HashMap<u16, (String, PilotKind)>,
    /// Rocks whose state differs from the generated field's (mined, shattered), by id.
    pub rocks: HashMap<u16, RockState>,
    /// Salvage chunks in range, by id.
    pub objects: Vec<Option<ObjectTrack>>,
    /// The hulk each destroyed suit became, by slot (from the kill events).
    pub hulks: HashMap<u16, u16>,
    seen: VecDeque<u16>,
    pub faction: Faction,
    pub my_hits: u32,
    pub my_kills: u32,
    pub my_deaths: u32,
    pub hits_taken: u32,
}

impl World {
    pub fn new(faction: Faction) -> Self {
        Self {
            tick: 0,
            entities: vec![None; MAX_ENTITIES],
            own: None,
            zero: None,
            beams: Vec::new(),
            hits: Vec::new(),
            feed: VecDeque::new(),
            roster: HashMap::new(),
            rocks: HashMap::new(),
            objects: vec![None; 1 << CHUNK_BITS],
            hulks: HashMap::new(),
            seen: VecDeque::new(),
            faction,
            my_hits: 0,
            my_kills: 0,
            my_deaths: 0,
            hits_taken: 0,
        }
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

    /// Applies a decoded snapshot.
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
        let prev_own = self.own;
        self.tick = tick;
        self.own = own;
        self.zero = zero;
        let me = own.map(|o| o.slot);
        if let (Some(now), Some(before)) = (own, prev_own)
            && before.alive
            && !now.alive
        {
            // Our own death is counted from the Kill event below; nothing else to do here.
        }
        for e in ents {
            if Some(e.slot) == me {
                continue;
            }
            let slot = e.slot as usize;
            match &mut self.entities[slot] {
                Some(track) if track.latest.generation == e.generation => track.push(tick, *e),
                other => *other = Some(EntityTrack::new(tick, *e)),
            }
        }
        for ev in events {
            self.apply_event(ev, me);
        }
        // Forget tracks the server stopped updating without telling us (lost Leave).
        for slot in self.entities.iter_mut() {
            if slot.as_ref().is_some_and(|t| tick.saturating_sub(t.latest_tick) > STALE_TICKS) {
                *slot = None;
            }
        }
    }

    /// Applies a snapshot's rock and object lists (each record is the thing's whole current state).
    pub fn apply_salvage(&mut self, rocks: &[RockState], objects: &[ObjectState]) {
        for r in rocks {
            self.rocks.insert(r.id, *r);
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
    /// rides the predicted suit.
    pub fn object_pose(&self, id: u16, t: f64, predict: &Predictor) -> Option<(Vec3, Quat)> {
        let track = self.objects.get(id as usize)?.as_ref()?;
        match track.motion_at(t) {
            ObjectMotion::Free(seg) => Some((segment_pos(&seg, t), segment_rot(&seg, t))),
            ObjectMotion::Held { holder, right, rot, .. } => {
                let (pos, turn) = if Some(holder) == self.own_slot() {
                    (predict.render_pos(), predict.state.rot)
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
                    && let Some(b) = self.beams.iter_mut().find(|b| b.predicted && b.shot_seq == shot_seq)
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
                }
                if on_me {
                    self.hits_taken += 1;
                }
                // The beam that hit stops being drawn (its direction places the sparks).
                let beam = if w.is_beam() {
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
                if Some(killer) == me && killer != victim {
                    self.my_kills += 1;
                }
                if Some(victim) == me {
                    self.my_deaths += 1;
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
            Event::Detach { .. } | Event::RockBreak { .. } => {}
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
        self.entity(slot).map(|e| e.sample(t))
    }

    /// Builds the same [`Perception`] the server's Mobile Dolls use, from this client's view.
    pub fn perception(&self, t: f64, predict: &Predictor) -> Option<Perception> {
        let own = self.own?;
        let spec = frame(own.frame);
        let s = &predict.state;
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
            propellant: own.propellant / spec.propellant_cap,
            g_strain: s.g_strain,
            ready: [own.weapon_ready & 1 != 0, own.weapon_ready & 2 != 0, own.weapon_ready & 4 != 0],
            overheated: own.flags & own_flags::OVERHEAT != 0,
        };
        let mut p = Perception::default();
        p.reset(me);
        for (slot, track) in self.entities.iter().enumerate() {
            let Some(track) = track else { continue };
            let e = &track.latest;
            if e.flags & ent_flags::WRECK != 0 {
                continue;
            }
            let pose = track.sample(t);
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
                accel: track.accel_estimate(),
                hull: f32::from(e.parts[Part::Torso as usize]) / 7.0,
                dist: (pose.pos - s.pos).length(),
                firing: e.flags & (ent_flags::FIRING_PRIMARY | ent_flags::FIRING_SECONDARY) != 0,
                aiming_at_me: pose.aim.dot(to_me) > 0.9986,
                locked_on_me: e.flags & ent_flags::LOCKED_ON_YOU != 0,
                hostile: e.faction != self.faction,
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
            let pose = track.sample(t);
            let spec = frame(track.latest.frame);
            let mut paths = [[Vec3::ZERO; STEPS]; N_HYP];
            for (k, path) in paths.iter_mut().enumerate() {
                let a = hypotheses::accel(spec, pose.rot, Maneuver::from_index(k));
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
