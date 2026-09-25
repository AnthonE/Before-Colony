//! The visual layer's inputs, whatever produces them: the network (`net_view`) or a scripted
//! showcase (`showcase`). Suits, effects and the chase camera read only these, never `GameClient`.

use std::collections::HashMap;

use bc_proto::{Faction, FrameId, WeaponKind};
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
    /// The pilot's own suit (no ZERO shell; the HUD and post effects show ZERO instead).
    pub own: bool,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// World-space aim direction.
    pub aim: Vec3,
    /// `bc_proto::snapshot::ent_flags` bits (the own suit's state is mapped onto them).
    pub flags: u16,
    /// Thrust demand in the suit's frame, each axis -1..1 (x right, y up, z forward): the pilot's
    /// for the own suit, estimated from acceleration for everyone else.
    pub thrust: Vec3,
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

/// One-shot visual events. Producers push each event exactly once; the effects drain them.
#[derive(Clone, Copy, Debug)]
pub enum FxEvent {
    Hit {
        pos: Vec3,
        weapon: WeaponKind,
    },
    Kill {
        pos: Vec3,
    },
    /// A beam leaving the muzzle.
    Muzzle {
        pos: Vec3,
        dir: Vec3,
        vel: Vec3,
        weapon: WeaponKind,
    },
    /// Beam sabers meeting.
    Clash {
        pos: Vec3,
    },
    /// The pilot's own suit taking a hit (as well as its [`FxEvent::Hit`]).
    Struck {
        weapon: WeaponKind,
    },
}

#[derive(Resource, Default)]
pub struct FxEvents(pub Vec<FxEvent>);

/// What the chase camera follows; `None` before spawning (an establishing shot instead).
#[derive(Resource, Default)]
pub struct CameraTarget(pub Option<ChaseTarget>);

#[derive(Clone, Copy, Debug, Default)]
pub struct ChaseTarget {
    pub pos: Vec3,
    pub vel: Vec3,
    pub up: Vec3,
    pub aim: Vec3,
    /// Boosting: the field of view widens.
    pub boost: bool,
    /// Pilot G-strain, 0..1: greys the view out, then closes it to a tunnel.
    pub g_strain: f32,
    /// The pilot has blacked out (G-LOC).
    pub blackout: bool,
    /// The ZERO System is engaged, and how close it is to seizing control (0..1).
    pub zero: bool,
    pub zero_strain: f32,
    /// A ZERO seizure has the controls.
    pub seized: bool,
}

/// System sets for one rendered frame, in order: produce the view model, pose the suits, move the
/// camera, then effects and HUD (which need the camera's transform).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Vis {
    Drive,
    Suits,
    Camera,
    Fx,
    Hud,
}
