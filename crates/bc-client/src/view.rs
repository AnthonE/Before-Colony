//! The visual layer's inputs, whatever produces them: the network (`net_view`) or a scripted
//! showcase (`showcase`). Suits, effects and the chase camera read only these, never `GameClient`.

use std::collections::HashMap;

use bc_client_core::BodySet;
use bc_client_core::interp::GroundPose;
use bc_proto::{Faction, FrameId, Part, WeaponKind};
use bc_sim::bodies::{Body, BodyPose, Shape};
use bevy::prelude::*;

/// The clock every visual system animates with, in seconds. Game mode: the page's clock.
/// Showcase: a fixed 60 Hz step from `?t=`, so screenshots are reproducible on any machine.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct VisTime {
    pub now: f64,
    pub dt: f32,
}

/// What one suit visual shows this frame. Lives on the suit's root entity.
#[derive(Component, Clone, Debug)]
pub struct SuitDrive {
    pub slot: u16,
    pub frame: FrameId,
    pub faction: Faction,
    pub generation: u8,
    /// The pilot's own suit (no Orrery shell; the HUD and post effects show the Orrery instead).
    pub own: bool,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// World-space aim direction.
    pub aim: Vec3,
    /// `bc_proto::snapshot::ent_flags` bits (the own suit's state is mapped onto them).
    pub flags: u16,
    /// Thrust in the suit's frame, each axis -1..1 (x right, y up, z forward): what the own suit's
    /// thrusters are doing, estimated from acceleration for everyone else.
    pub thrust: Vec3,
    /// Armour per part in eighths (0 destroyed .. 7 pristine), by `Part`.
    pub parts: [u8; Part::COUNT],
    /// Holding a chunk: in the right hand (true) or the left.
    pub holding: Option<bool>,
    /// The gun in its hand, if what it holds is one: it fires on the secondary's trigger.
    pub held_gun: Option<WeaponKind>,
    /// On a body: standing on it, or in the air in its grip.
    pub ground: Option<SuitGround>,
    /// How weathered its paint is, 0 (factory fresh) to 7 (a veteran's): earned by use, and on
    /// the roster with its pilot (`bc_econ::weathering`).
    pub weathering: u8,
    /// Doomed: its torso breached and its reactor going (`Event::Doomed`), it burns.
    pub doomed: bool,
    /// Staggered (`bc_sim::sim::stagger`): its attitude jets fire every which way, and it sparks.
    pub staggered: bool,
}

impl SuitDrive {
    /// The pilot's suit standing in its bay's gantry (`hangar::BAY_SLOT`), drawn there while the
    /// pilot's in the bay: it keeps its drive (`own`, 20 km off up at the bay) once the bay's out
    /// of sight, so the suit the pilot flies is an own one that isn't this, where there is one
    /// (`own(…).min_by_key(|d| d.in_bay())`).
    pub fn in_bay(&self) -> bool {
        self.slot == crate::hangar::BAY_SLOT
    }
}

/// How a suit on a body is drawn on it: what its walk, its stance and its shadow need.
#[derive(Clone, Copy, Debug)]
pub struct SuitGround {
    pub body: Body,
    /// The body as drawn this frame, and its shape (in its frame).
    pub pose: BodyPose,
    pub shape: Shape,
    /// In the air in the body's grip, rather than standing on it.
    pub aloft: bool,
    /// The surface's outward normal under the suit (world).
    pub up: Vec3,
    /// The suit's velocity relative to the body (world), m/s.
    pub rel_vel: Vec3,
    /// How high its origin is over the surface, m: its stance, when standing.
    pub height: f32,
}

impl SuitGround {
    /// `g`, on its body as `bodies` has it drawn; none if the body isn't one of theirs.
    pub fn of(g: &GroundPose, bodies: &DrawnBodies) -> Option<Self> {
        Some(Self {
            body: g.body,
            pose: bodies.pose(g.body)?,
            shape: bodies.set.shape(g.body)?,
            aloft: g.aloft,
            up: g.up,
            rel_vel: g.rel_vel,
            height: g.height,
        })
    }
}

/// The sector's bodies as drawn this frame, and the view clock's time they're drawn at (ticks).
/// Every body is drawn on the view clock, as remote suits are, so a suit standing on one is drawn
/// on it. Game mode: the sector's, at the render tick. Showcase: every landmark, at its clock.
#[derive(Resource, Clone, Default)]
pub struct DrawnBodies {
    pub set: BodySet,
    pub t: f64,
}

impl DrawnBodies {
    /// Where `b` is drawn this frame.
    pub fn pose(&self, b: Body) -> Option<BodyPose> {
        self.set.pose_at(b, self.t)
    }
}

/// Suit visual roots by entity slot.
#[derive(Resource, Default)]
pub struct SuitIndex(pub HashMap<u16, Entity>);

/// A beam as drawn this frame; the producer has already evaluated it on the right clock.
#[derive(Clone, Copy, Debug)]
pub struct BeamView {
    pub head: Vec3,
    pub dir: Vec3,
    /// Distance flown so far (the streak never reaches back past the muzzle).
    pub travelled: f32,
    pub weapon: WeaponKind,
}

/// Every beam in flight this frame.
#[derive(Resource, Default)]
pub struct BeamFeed(pub Vec<BeamView>);

/// A missile as drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct MissileView {
    pub pos: Vec3,
    pub vel: Vec3,
    pub kind: WeaponKind,
    /// It is tracking the pilot's own suit.
    pub targets_you: bool,
}

/// Every missile in flight this frame.
#[derive(Resource, Default)]
pub struct MissileFeed(pub Vec<MissileView>);

/// A pilot's capsule thrown clear of their suit, as drawn this frame (`pods`).
#[derive(Clone, Copy, Debug)]
pub struct PodView {
    pub pos: Vec3,
    pub vel: Vec3,
    /// The suit's velocity as it left (what the capsule was thrown clear of).
    pub suit_vel: Vec3,
    /// Seconds since it left.
    pub age: f32,
    /// The pilot's own.
    pub own: bool,
}

/// Every capsule worth drawing this frame.
#[derive(Resource, Default)]
pub struct PodFeed(pub Vec<PodView>);

/// One-shot visual events. Producers push each event exactly once; the effects drain them.
#[derive(Clone, Copy, Debug)]
pub enum FxEvent {
    /// A shot striking something: where, the surface's normal if known, and the suit and part
    /// it hit (none for the colony's hull).
    Hit { pos: Vec3, weapon: WeaponKind, normal: Option<Vec3>, target: Option<(u16, Part)> },
    /// A suit destroyed: where, how fast its wreck is going (the blast goes with it), and whose
    /// it was (for the colour of its armour flying off).
    Kill { pos: Vec3, vel: Vec3, victim: Option<u16> },
    /// A beam leaving the muzzle (`shooter`: the suit's slot, if it's known, so the flash can be
    /// drawn at its weapon).
    Muzzle { pos: Vec3, dir: Vec3, vel: Vec3, weapon: WeaponKind, shooter: Option<u16> },
    /// A beam's whole line, from the gun to what stopped it, flashing as it ends: a shot too fast
    /// to see in flight across a room (the Blast Hall's are gone within a frame) is still seen.
    BeamTrace { from: Vec3, to: Vec3, weapon: WeaponKind },
    /// Beam sabers meeting, and how fast the pair of them are going.
    Clash { pos: Vec3, vel: Vec3 },
    /// A rock shattering: where, how big it was, and the colour of its ore.
    RockBreak { pos: Vec3, radius: f32, ore: Vec3 },
    /// The pilot's own suit taking a hit (as well as its [`FxEvent::Hit`]).
    Struck { weapon: WeaponKind },
    /// A suit changing form (Altair ↔ Aquila), the moment it does.
    Transform { pos: Vec3, vel: Vec3, rot: Quat },
    /// A missile leaving its launcher (`own`: the pilot's).
    MissileLaunch { pos: Vec3, own: bool },
    /// A missile's warhead going off: against a suit (`struck`), or at the end of its flight or
    /// against rock or hull.
    MissileBurst { pos: Vec3, kind: WeaponKind, struck: bool },
    /// A suit landing on a body: where its feet came down, how the surface moves there and which
    /// way it faces, how hard it landed (m/s), and whether it's rock (dust flies) or hull.
    Touchdown { pos: Vec3, vel: Vec3, normal: Vec3, speed: f32, rock: bool },
    /// A pilot's capsule blown clear of their suit: where, and how fast it goes (`own`: the
    /// pilot's).
    Eject { pos: Vec3, vel: Vec3, own: bool },
    /// A doomed suit's reactor blown by its pilot: the self-destruct's blast.
    Blast { pos: Vec3 },
    /// A suit staggered, the moment it's heard of (`own`: the pilot's).
    Stagger { pos: Vec3, vel: Vec3, own: bool },
}

#[derive(Resource, Default)]
pub struct FxEvents(pub Vec<FxEvent>);

/// Shots landing, by when they were seen (the page's clock, s): each of the Blast Hall's targets
/// struck (by anyone; it flashes), and the pilot's own landing on a suit or a target (the
/// reticle's hit marker).
#[derive(Resource, Clone, Copy, Debug)]
pub struct Strikes {
    pub target_at: [f64; bc_sim::colony::hall::TARGETS],
    pub marker_at: f64,
    /// The pilot's hits counted so far (`World::my_hits`): a new one marks the reticle.
    pub hits_seen: u32,
    /// Counts for the browser tests (`window.__bc`): the pilot's own shots seen leaving the gun,
    /// their lines flashed as they ended, and strikes on the hall's targets shown.
    pub own_muzzles: u32,
    pub own_traces: u32,
    pub target_flashes: u32,
}

impl Default for Strikes {
    fn default() -> Self {
        Self {
            target_at: [f64::NEG_INFINITY; bc_sim::colony::hall::TARGETS],
            marker_at: f64::NEG_INFINITY,
            hits_seen: 0,
            own_muzzles: 0,
            own_traces: 0,
            target_flashes: 0,
        }
    }
}

impl Strikes {
    /// Target `k` struck at `now`; `mine`: by the pilot's own round.
    pub fn strike(&mut self, k: u8, now: f64, mine: bool) {
        if let Some(at) = self.target_at.get_mut(usize::from(k)) {
            // (A round predicted to strike and the server's news of it are one strike.)
            if now - *at > 0.3 {
                self.target_flashes += 1;
            }
            *at = now;
        }
        if mine {
            self.marker_at = now;
        }
    }
}

/// How the pilot likes the view (their settings in game mode; the defaults, or `?calm=1`, in the
/// showcase).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ViewPrefs {
    /// Field of view cruising, degrees (boost widens it).
    pub fov: f32,
    /// How much of the camera shake, kicks and warps to keep, 0..1.
    pub shake: f32,
    /// Flying from the cockpit (first person) rather than the chase camera.
    pub cockpit: bool,
    /// The cockpit see-through all round (the holo cockpit) rather than its framed monitors.
    pub holo: bool,
    /// Full-screen flicker and flashes are allowed (off for photosensitive pilots).
    pub flashing: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self { fov: 70.0, shake: 1.0, cockpit: true, holo: false, flashing: true }
    }
}

/// What the chase camera follows; `None` before spawning (an establishing shot instead).
#[derive(Resource, Default)]
pub struct CameraTarget(pub Option<ChaseTarget>);

#[derive(Clone, Copy, Debug, Default)]
pub struct ChaseTarget {
    pub pos: Vec3,
    /// Looking round from the cockpit: the view's yaw (right) and pitch (up) off the aim, radians.
    pub look: Vec2,
    /// How fast the suit moves as drawn (what the camera keeps pace with).
    pub vel: Vec3,
    pub up: Vec3,
    pub aim: Vec3,
    /// The suit jumped (spawn, respawn, a relocation): cut rather than chase.
    pub cut: bool,
    /// On a body (standing on it, or in the air in its grip): the camera comes in closer and
    /// higher.
    pub ground: bool,
    /// Boosting: the field of view widens.
    pub boost: bool,
    /// Pilot G-strain, 0..1: greys the view out, then closes it to a tunnel.
    pub g_strain: f32,
    /// The pilot has blacked out (G-LOC).
    pub blackout: bool,
    /// The Orrery is engaged, and how close it is to seizing control (0..1).
    pub orrery: bool,
    pub orrery_strain: f32,
    /// An Orrery seizure has the controls.
    pub seized: bool,
}

/// System sets for one rendered frame, in order: produce the view model, pose the suits, move the
/// camera, then effects and HUD (which need the camera's transform).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Vis {
    Drive,
    Suits,
    Camera,
    /// Sound: after the camera (the listener) moves, before the effects drain the events.
    Audio,
    Fx,
    Hud,
}
