//! Survival: suits their pilots built, launched from the colony's docking hub and brought home
//! again. (Under arcade rules, [`Sim::join`](super::Sim::join) hands out any frame at its
//! faction's spawn point instead.)
//!
//! - [`Sim::launch`](super::Sim::launch) puts a [`Loadout`] in the sector in its pilot's bay
//!   ([`LaunchAt::Bay`]): the parts its pilot fitted (as worn as they are), the weapons fitted on
//!   its mounts, the rounds loaded and the propellant in the tank. It rides the bay's catapult
//!   cradle in the door, turning with the colony, until its pilot lets go of the grip, and is
//!   thrown out of the door (`colony::hub`).
//! - [`Sim::dock`](super::Sim::dock) takes a suit that has come to rest in the dock out of the
//!   sector again, and says what it brings home ([`Homecoming`]): what's left of it, its hold,
//!   whatever it has in hand, and the bounties it earned.
//! - Inside the colony a suit can also come in at the Blast Hall's gantry
//!   ([`Sim::launch_at`](super::Sim::launch_at), [`LaunchAt::Gantry`]): one of the Charter Board's
//!   trainers, standing on the gantry's pad (`colony::hall`). It docks back there.
//! - A suit its pilot left parked in a landmark's hide spot outlives the server:
//!   [`Sim::park_record`](super::Sim::park_record) says where it stands and what it carries
//!   ([`ParkRecord`]), and [`Sim::restore_sleeper`](super::Sim::restore_sleeper) puts it back
//!   there, asleep, when the server starts again.

use bc_proto::buttons::{FLIGHT_ASSIST, GRIP};
use bc_proto::{CARGO_KINDS, ChunkDesc, Faction, FrameId, InputCmd, NO_CHUNK, Part, PilotKind};
use glam::{Quat, Vec3};

use super::Sim;
use crate::bodies::{Bodies, Body, Shape};
use crate::colony::hub::{BAY_RIDE_LOCAL, BAYS, bay_pose, bay_ride_rot, is_bay};
use crate::content::{Kits, Modules, Systems, frame, weapon};
use crate::ground::{self, Anchor, CROUCH_STANCE, Footing, STANCE};
use crate::handle::SuitId;
use crate::math::{cos, floor, look_rotation, quat_normalize, sin};
use crate::suits::{ALL_MOUNTS, NO_SPOT, Usage};

/// Launches into the colony are spread round the inner gate, a place apart each.
const LAUNCH_PLACES: u32 = 12;

/// Where the `n`th suit comes into the colony from the bays: round the inner gate, nose down the
/// colony, its head towards the axis (up, in there).
fn inner_launch_pose(n: u32) -> (Vec3, Quat) {
    use crate::colony::interior::INNER_GATE;
    let a = (n % LAUNCH_PLACES) as f32 * core::f32::consts::TAU / LAUNCH_PLACES as f32;
    let pos = INNER_GATE + Vec3::new(0.0, cos(a) * 40.0, sin(a) * 40.0);
    (pos, look_rotation(Vec3::X, crate::colony::frame::up_at(pos)))
}

/// Where on the Blast Hall's gantry a trainer stands: its origin, a stance over the pad's middle,
/// facing in toward the targets, upright.
fn gantry_pose() -> (Vec3, Quat) {
    use crate::colony::{frame::up_at, hall};
    let up = up_at(hall::gantry());
    let (pos, _, _) = ground::place(&Shape::city(), hall::gantry() + up * STANCE, STANCE);
    (pos, look_rotation(hall::gantry_facing(), up))
}

/// Where a suit launched into the sector comes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchAt {
    /// From its pilot's bay, by number (`colony::hub::bay_of_slot`): in space, riding the bay's
    /// catapult cradle in its door until its pilot lets go; inside the colony, by the inner gate.
    Bay(u8),
    /// Inside the colony, at the Blast Hall's gantry (`colony::hall`): one of the Charter Board's
    /// trainers, standing on the gantry's pad facing in, gripping until its pilot is heard from.
    /// It docks back there (`hall::in_gantry`), not at the inner gate.
    Gantry,
}

/// A suit as its pilot built it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loadout {
    /// Armour per part, as a fraction of the frame's (0: not fitted). The torso is the suit, so
    /// it must be fitted.
    pub parts: [f32; Part::COUNT],
    /// A bit per loadout slot (primary, secondary, melee): its weapon is fitted.
    pub mounts: u8,
    /// Rounds loaded, per loadout slot.
    pub ammo: [u16; 3],
    /// In the tank, kg.
    pub propellant: f32,
    /// What's damaged or failed inside the parts.
    pub systems: Systems,
    /// The equipment on the parts.
    pub modules: Modules,
    /// The consumables in its rack.
    pub kits: Kits,
}

impl Loadout {
    /// Everything fitted, new, loaded and full (tests, and arcade frames).
    pub fn full(frame_id: FrameId) -> Self {
        let spec = crate::content::frame(frame_id);
        let mut ammo = [0; 3];
        for (a, m) in ammo.iter_mut().zip(spec.loadout.iter()) {
            if let Some(m) = m {
                *a = crate::content::weapon(m.weapon).ammo;
            }
        }
        Self {
            parts: [1.0; Part::COUNT],
            mounts: 0b111,
            ammo,
            propellant: spec.propellant_cap,
            systems: Systems::OK,
            modules: Modules::NONE,
            kits: Kits::NONE,
        }
    }
}

/// What a suit brings home when it docks, or what its pilot gets when it's lost (the bounty).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homecoming {
    /// The frame line (Neo-Bird comes home as Wing Zero).
    pub frame: FrameId,
    /// Armour per part as a fraction of the frame's; 0: gone (shot off, or never fitted).
    pub parts: [f32; Part::COUNT],
    pub mounts: u8,
    pub ammo: [u16; 3],
    pub propellant: f32,
    /// What's damaged or failed inside the parts still on (a part shot off takes its own).
    pub systems: Systems,
    /// The equipment on the parts still on (a part shot off took its own).
    pub modules: Modules,
    /// The consumables it didn't use.
    pub kits: Kits,
    /// What it has been through out there (thrusters, guns, the reactor).
    pub usage: Usage,
    /// The hold, kg per cargo kind.
    pub cargo_kg: [u16; CARGO_KINDS],
    /// Whatever it had in hand (a hulk it towed in, a limb, ore).
    pub held: Option<ChunkDesc>,
    /// Credits the colony pays for the Mobile Dolls it destroyed.
    pub bounty: u32,
}

/// A suit parked in a landmark's hide spot as its pilot left (survival): where it stands on the
/// landmark, and everything it carries. Enough to put it back as it was when the server starts
/// again ([`Sim::restore_sleeper`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParkRecord {
    /// The landmark it stands on (an index into [`Sim::landmarks`]).
    pub landmark: u8,
    /// Its origin and attitude in the landmark's frame.
    pub local: Vec3,
    pub rot: Quat,
    /// How high its origin stands over the ground, m: on its feet, or crouched.
    pub stance: f32,
    pub frame: FrameId,
    pub faction: Faction,
    pub pilot: PilotKind,
    /// What's left of it, its hold and its bounty (nothing in hand: a sleeper lets go).
    pub home: Homecoming,
}

impl Sim {
    /// Launches `loadout` from a bay, flown by `pilot` (a bay of its own each, round the ring, for
    /// tests and scenarios: the sector launches each pilot from theirs, [`launch_at`]). `None` if
    /// the sector is full, the frame isn't one pilots fly, or there's no torso.
    ///
    /// [`launch_at`]: Self::launch_at
    pub fn launch(
        &mut self,
        frame_id: FrameId,
        faction: Faction,
        pilot: PilotKind,
        loadout: &Loadout,
    ) -> Option<SuitId> {
        let bay = (self.spawn_counter % BAYS + 1) as u8;
        self.launch_at(frame_id, faction, pilot, loadout, LaunchAt::Bay(bay))
    }

    /// Launches `loadout` [`at`](LaunchAt) its pilot's bay (in space, riding its cradle; inside the
    /// colony, by the inner gate), or inside the colony at the Blast Hall's gantry as a trainer.
    /// `None` as [`launch`](Self::launch), for a gantry outside the colony or a frame without legs
    /// to stand on it, and for a bay the ring doesn't have.
    pub fn launch_at(
        &mut self,
        frame_id: FrameId,
        faction: Faction,
        pilot: PilotKind,
        loadout: &Loadout,
        at: LaunchAt,
    ) -> Option<SuitId> {
        let spec = frame(frame_id);
        let gantry = at == LaunchAt::Gantry;
        if !spec.playable
            || loadout.parts[Part::Torso as usize] <= 0.0
            || (gantry && (!self.interior() || !spec.has_legs()))
            || matches!(at, LaunchAt::Bay(n) if !is_bay(n))
        {
            return None;
        }
        let id = self.suits.allocate(frame_id, faction, pilot)?;
        self.spawn_counter += 1;
        // In its bay's cradle, as the bay stands this tick.
        let cradle = match at {
            LaunchAt::Bay(n) if !self.interior() => Some((n, bay_pose(n, self.tick, 0.0))),
            _ => None,
        };
        let (pos, rot) = match (at, cradle) {
            (LaunchAt::Gantry, _) => gantry_pose(),
            (_, Some((_, pose))) => (pose.to_world(BAY_RIDE_LOCAL), pose.rot * bay_ride_rot()),
            (LaunchAt::Bay(_), None) => inner_launch_pose(self.spawn_counter),
        };
        let i = id.idx();
        self.suits.place(i, frame_id, pos, rot, self.tick);
        for (hp, (max, f)) in self.suits.part_hp[i].iter_mut().zip(spec.part_hp.iter().zip(loadout.parts)) {
            *hp = max * f.clamp(0.0, 1.0);
        }
        self.suits.mounts[i] = loadout.mounts & ALL_MOUNTS;
        self.suits.systems[i] = loadout.systems.clean();
        self.suits.modules[i] = loadout.modules.clean();
        self.suits.kits[i] = loadout.kits;
        self.suits.retune(i);
        // Charged full, a capacitor bank's worth included.
        self.suits.energy[i] = spec.energy_cap * self.suits.tuning[i].energy_cap;
        for (slot, ws) in self.suits.weapons[i].iter_mut().enumerate() {
            if let Some(m) = spec.loadout[slot] {
                ws.ammo = loadout.ammo[slot].min(weapon(m.weapon).ammo);
            }
        }
        let tank = crate::tuning::tank_cap(spec, &self.suits.tuning[i]);
        let f = &mut self.suits.flight[i];
        f.propellant = loadout.propellant.clamp(0.0, tank);
        f.vel = rot * Vec3::Z * crate::colony::interior::INNER_LAUNCH_SPEED;
        if let Some((n, pose)) = cradle {
            // Standing in its bay's door, carried round with it, its grip held: it rides there
            // until its pilot lets go, and is thrown out of the door.
            let a = Anchor {
                body: Body::Bay(n),
                local: BAY_RIDE_LOCAL,
                rot: bay_ride_rot(),
                stance: STANCE,
                ..Anchor::default()
            };
            ground::derive(&pose, &a, f);
            (self.suits.footing[i], self.suits.anchor[i]) = (Footing::Grounded, a);
            self.suits.input[i] =
                InputCmd { aim: rot * Vec3::Z, buttons: FLIGHT_ASSIST | GRIP, ..InputCmd::default() };
        } else if gantry {
            // On its feet on the pad, at rest, its grip held: it stands there until its pilot is
            // first heard from.
            f.vel = Vec3::ZERO;
            let a = Anchor { body: Body::City, local: pos, rot, stance: STANCE, ..Anchor::default() };
            (self.suits.footing[i], self.suits.anchor[i]) = (Footing::Grounded, a);
            self.suits.trainer.set(i, true);
            self.suits.input[i] =
                InputCmd { aim: rot * Vec3::Z, buttons: FLIGHT_ASSIST | GRIP, ..InputCmd::default() };
        } else if self.interior() {
            // Until its pilot is first heard from, flight assist holds it at the gate: the colony's
            // pull would otherwise take it down to the floor while their page catches up.
            self.suits.input[i] =
                InputCmd { aim: rot * Vec3::Z, buttons: FLIGHT_ASSIST, ..InputCmd::default() };
        }
        Some(id)
    }

    /// Takes suit `id` into the hangar: it must be alive, awake and at rest in the dock (a trainer,
    /// on the Blast Hall's gantry). What it brings home; it's gone from the sector.
    pub fn dock(&mut self, id: SuitId) -> Option<Homecoming> {
        if !self.suits.valid(id) {
            return None;
        }
        let i = id.idx();
        if !self.suits.alive.get(i) || self.suits.sleeping.get(i) || !self.docked(i) {
            return None;
        }
        let held = self.held_chunk(i);
        let home = Homecoming { held: held.map(|k| self.chunks.desc[k]), ..self.homecoming(i) };
        if let Some(k) = held {
            self.chunks.kill(k);
        }
        self.suits.held[i] = (NO_CHUNK, 0, false);
        self.suits.cargo_kg[i] = [0; CARGO_KINDS];
        self.suits.release(i);
        Some(home)
    }

    /// What suit `i` would bring home, but for whatever it has in hand.
    pub fn homecoming(&self, i: usize) -> Homecoming {
        let s = &self.suits;
        Homecoming {
            frame: match s.frame[i] {
                FrameId::WingZeroBird => FrameId::WingZero,
                f => f,
            },
            parts: s.part_fractions(i),
            mounts: s.mounts[i],
            ammo: [s.weapons[i][0].ammo, s.weapons[i][1].ammo, s.weapons[i][2].ammo],
            propellant: s.flight[i].propellant,
            systems: s.systems[i],
            modules: s.modules[i].without(s.gone_mask(i)),
            kits: s.kits[i],
            usage: s.usage[i],
            cargo_kg: s.cargo_kg[i],
            held: None,
            bounty: s.credits[i],
        }
    }

    /// Suit `i`, if it's asleep on its feet (or knees) in one of a landmark's hide spots: where,
    /// and what it carries. Nothing for a suit parked anywhere else.
    pub fn park_record(&self, i: usize) -> Option<ParkRecord> {
        if !self.is_parked(i) || !self.suits.alive.get(i) {
            return None;
        }
        let s = &self.suits;
        let (Footing::Grounded, Body::Landmark(landmark), false) =
            (s.footing[i], s.anchor[i].body, s.hide_spot[i] == NO_SPOT)
        else {
            return None;
        };
        let a = s.anchor[i];
        Some(ParkRecord {
            landmark,
            local: a.local,
            rot: a.rot,
            stance: a.stance,
            frame: s.frame[i],
            faction: s.faction[i],
            pilot: s.pilot[i],
            home: self.homecoming(i),
        })
    }

    /// Suits asleep in a hide spot that were hit this tick, each (entity slot, generation) as
    /// [`park_record`](Self::park_record) finds it now: what a later server run puts back is
    /// what's left of it, not what its pilot left.
    pub fn hidden_hit(&self, mut f: impl FnMut(u16, u16, ParkRecord)) {
        for i in self.suits.sleeping.iter() {
            if self.suits.last_hit[i] == self.tick
                && let Some(rec) = self.park_record(i)
            {
                f(i as u16, self.suits.generation[i], rec);
            }
        }
    }

    /// Puts a suit back as [`park_record`](Self::park_record) found it, asleep: kneeling (or
    /// standing) where it was in its hide spot, with its armour, weapons, rounds, tank, hold and
    /// bounty, and gripping, so its pilot wakes there. It powers down as any suit just parked
    /// does, and nobody has fought it. `None` if the sector is full, or the record doesn't name a
    /// hide spot this sector has, a frame pilots fly on legs, or a torso.
    pub fn restore_sleeper(&mut self, rec: &ParkRecord) -> Option<SuitId> {
        let spec = frame(rec.frame);
        if !spec.playable
            || !spec.has_legs()
            || rec.home.parts[Part::Torso as usize] <= 0.0
            || !rec.stance.is_finite()
            || !rec.rot.is_finite()
            || rec.rot.length_squared() < 0.25
        {
            return None;
        }
        let t = self.tick;
        let body = Body::Landmark(rec.landmark);
        // On the stance grid, between a crouch and full height.
        let stance = (floor(rec.stance * 16.0 + 0.5) / 16.0).clamp(CROUCH_STANCE, STANCE);
        let (pose, local, spot) = {
            let bodies = Bodies::at(&self.field, self.landmarks(), t);
            let (pose, shape) = (bodies.pose(body)?, bodies.shape(body)?);
            // Where it stood, its feet on the ground: a record saved by `park_record` has them
            // there already, and is kept to the bit.
            let (local, _) = ground::settle(&shape, rec.local, Vec3::Y, stance);
            (pose, local, bodies.hide_spot_of(body, local)?)
        };
        self.make_room_for_sleeper();
        let id = self.suits.allocate(rec.frame, rec.faction, rec.pilot)?;
        let i = id.idx();
        // Unit length as saved (kept to the bit), or made so.
        let rot =
            if (rec.rot.length_squared() - 1.0).abs() < 1e-4 { rec.rot } else { quat_normalize(rec.rot) };
        let a = Anchor { body, local, rot, stance, ..Anchor::default() };
        let home = &rec.home;
        self.suits.place(i, rec.frame, pose.to_world(local), pose.rot * a.rot, t);
        let s = &mut self.suits;
        for (hp, (max, f)) in s.part_hp[i].iter_mut().zip(spec.part_hp.iter().zip(home.parts)) {
            *hp = max * f.clamp(0.0, 1.0);
        }
        s.mounts[i] = home.mounts & ALL_MOUNTS;
        // As worn inside as it was left, its equipment and rack with it.
        s.systems[i] = home.systems.clean();
        s.modules[i] = home.modules.clean();
        s.kits[i] = home.kits;
        s.usage[i] = home.usage;
        s.retune(i);
        for (slot, ws) in s.weapons[i].iter_mut().enumerate() {
            if let Some(m) = spec.loadout[slot] {
                ws.ammo = home.ammo[slot].min(weapon(m.weapon).ammo);
            }
        }
        s.flight[i].propellant = home.propellant.clamp(0.0, crate::tuning::tank_cap(spec, &s.tuning[i]));
        s.cargo_kg[i] = home.cargo_kg;
        s.credits[i] = home.bounty;
        (s.footing[i], s.anchor[i]) = (Footing::Grounded, a);
        super::sleep::hold(&pose, &a, &mut s.flight[i]);
        // Asleep since now: lying still, never fought (so it goes dark as a suit just parked
        // does), and still gripping.
        let aim = s.flight[i].rot * Vec3::Z;
        s.aim[i] = aim;
        s.input[i] = InputCmd::neutral(t, aim, GRIP);
        s.sleeping.set(i, true);
        s.slept_at[i] = t;
        s.last_fired[i] = 0;
        s.last_hit[i] = 0;
        s.still_since[i] = t;
        s.hide_spot[i] = spot;
        Some(id)
    }
}
