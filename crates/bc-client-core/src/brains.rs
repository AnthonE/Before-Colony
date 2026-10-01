//! Ready-made agent brains.

use crate::InputContext;
use crate::salvage::route;
use crate::world::World;
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, GRAB, GRIP, MELEE, STOW};
use bc_proto::{ChunkKind, FrameId, InputCmd, Part};
use bc_sim::TICK_HZ;
use bc_sim::ai::{self, AiState, DollProfile, PILOT};
use bc_sim::bodies::{Body, BodyPose};
use bc_sim::content::salvage::{DOCK_CENTER, stowable};
use bc_sim::content::{ArmSlot, frame, weapon};
use bc_sim::field::SUIT_CLEARANCE;
use bc_sim::ground::{Footing, STANCE, WALK_SPEED, place};
use bc_sim::math::quat_axis_angle;
use glam::Vec3;

/// Where the fighting is (the Mobile Doll patrol ring around the colony).
pub const COMBAT_ZONE: Vec3 = Vec3::new(0.0, 900.0, 0.0);

/// The Mobile Doll brain from `bc-sim`, running on the agent's own sensor view, flying the frame's
/// whole kit (`bc_sim::ai::kit`): it picks targets and maneuvers by utility, leads them linearly,
/// and uses every weapon and the frame's special. With nothing on sensors it heads for the combat
/// zone.
pub struct DollBrain {
    ai: AiState,
}

impl DollBrain {
    pub fn new(seed: u32) -> Self {
        Self { ai: AiState { rng: seed | 1, anchor: COMBAT_ZONE, ..AiState::default() } }
    }

    pub fn decide(&mut self, ctx: &InputContext) -> InputCmd {
        // See targets where the server will judge the shots, not where the screen shows them.
        let Some(p) = ctx.world.perception(ctx.resolve_tick, ctx.predict) else {
            return InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() };
        };
        let spec = frame(p.me.frame);
        let range = spec.loadout[0].map_or(3_000.0, |m| weapon(m.weapon).range);
        // Fights at the frame's preferred distance.
        let profile = match spec.ai.engage_range {
            r if r > 0.0 => DollProfile { preferred_range: r, ..PILOT },
            _ => PILOT,
        };
        if ctx.tick >= self.ai.think_at {
            ai::think(&p, &mut self.ai, ctx.tick, &profile, range);
            self.ai.think_at = ctx.tick + 3;
        }
        let target = p.get(self.ai.target).copied();
        ai::drive(&p.me, target.as_ref(), &mut self.ai, ctx.tick, &profile, spec)
    }

    /// Current target slot, if any.
    pub fn target(&self) -> Option<u16> {
        (self.ai.target != bc_proto::NO_SLOT).then_some(self.ai.target)
    }
}

/// How fast a miner cruises (m/s), and the braking it plans on (m/s²: a laden Leo's retro-thrust
/// manages more).
const CRUISE: f32 = 150.0;
const BRAKING: f32 = 6.0;
/// How far a miner goes for loose ore (m).
const FETCH_RANGE: f32 = 300.0;

/// A miner. It works the nearest standing rock with its saber (its rifle, if the saber arm has been
/// shot off); its free hand takes each chip of ore knocked off, and stows it. When the rock
/// shatters it gathers up the ore, and when the hold is full (or it has something in hand that
/// won't fit) it takes it to the dock, round the colony: under arcade rules the dock buys it;
/// under survival rules its pilot docks ([`MinerBrain::ready_to_dock`]) and sells it on the
/// exchange. It flies with flight assist.
pub struct MinerBrain {
    /// How full the hold gets (0..1) before it goes to sell.
    pub sell_at: f32,
    /// Rocks bigger than this (m) take too long to break: it leaves them alone.
    pub max_rock_radius: f32,
    hauling: bool,
    rock: Option<usize>,
}

impl Default for MinerBrain {
    fn default() -> Self {
        Self { sell_at: 0.8, max_rock_radius: 30.0, hauling: false, rock: None }
    }
}

impl MinerBrain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether it's on its way to sell.
    pub fn hauling(&self) -> bool {
        self.hauling
    }

    /// The rock it's working, by index in the field.
    pub fn rock(&self) -> Option<usize> {
        self.rock
    }

    /// Survival rules: it has brought its haul to rest in the dock, and its pilot should dock.
    pub fn ready_to_dock(&self, world: &World) -> bool {
        self.hauling && world.own.is_some_and(|o| o.alive) && world.salvage_view().is_some_and(|v| v.docked)
    }

    /// The haul is home (docked and unloaded): back to work.
    pub fn unloaded(&mut self) {
        self.hauling = false;
        self.rock = None;
    }

    pub fn decide(&mut self, ctx: &InputContext) -> InputCmd {
        let world = ctx.world;
        let idle = InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() };
        let (Some(own), Some(view)) = (world.own, world.salvage_view()) else { return idle };
        if !own.alive {
            // The hold spilled where it died.
            self.hauling = false;
            self.rock = None;
            return idle;
        }
        let s = &ctx.predict.state;
        let t = f64::from(ctx.tick);
        let cargo = view.cargo_total_kg();
        // The saber, and the hand that grabs, are on the left arm, unless it's been shot off.
        let left_arm = own.parts[Part::ArmL as usize] > 0.0;
        // The hand keeps hold (letting go of GRAB drops what's in it), and takes what comes in reach.
        let mut buttons = FLIGHT_ASSIST | GRAB;
        if let Some(k) = view.held {
            match world.objects.get(usize::from(k)).and_then(Option::as_ref).map(|o| o.desc) {
                // Not heard of yet.
                None => {}
                Some(d) if stowable(&d) && cargo + d.mass_kg <= view.capacity_kg => {
                    // A press is an edge: every other tick.
                    if ctx.tick.is_multiple_of(2) {
                        buttons |= STOW;
                    }
                }
                Some(_) => self.hauling = true,
            }
        }
        if cargo as f32 >= view.capacity_kg as f32 * self.sell_at {
            self.hauling = true;
        }
        if self.hauling {
            if !(view.docked && cargo == 0 && view.held.is_none()) {
                return fly(ctx, own.frame, DOCK_CENTER, Vec3::ZERO, None, buttons);
            }
            self.hauling = false; // sold
        }
        // Loose ore close by that will fit: fetch the nearest, bringing the free hand to it.
        if view.held.is_none() {
            let room = view.capacity_kg.saturating_sub(cargo);
            let ore = world
                .loose_chunks(s.pos, FETCH_RANGE, t)
                .filter(|c| {
                    matches!(c.desc.kind, ChunkKind::Ore { .. })
                        && stowable(&c.desc)
                        && c.desc.mass_kg <= room
                })
                .min_by(|a, b| a.pos.distance_squared(s.pos).total_cmp(&b.pos.distance_squared(s.pos)));
            if let Some(c) = ore {
                let hand = s.rot * if left_arm { ArmSlot::Left } else { ArmSlot::Right }.muzzle();
                return fly(ctx, own.frame, c.pos - hand, c.vel, Some(c.pos), buttons);
            }
        }
        // Otherwise work a rock: hold station off its face, and cut.
        let field = &ctx.predict.field;
        let standing = |i: usize| !field.is_dead(i) && field.rocks()[i].radius <= self.max_rock_radius;
        self.rock = self.rock.filter(|&i| standing(i)).or_else(|| {
            (0..field.len()).filter(|&i| standing(i)).min_by(|&a, &b| {
                let d = |i: usize| field.rocks()[i].pos.distance_squared(s.pos);
                d(a).total_cmp(&d(b))
            })
        });
        let Some(i) = self.rock else { return fly(ctx, own.frame, COMBAT_ZONE, Vec3::ZERO, None, buttons) };
        let rock = field.rocks()[i];
        let stand = rock.surface(s.pos, SUIT_CLEARANCE + 1.0);
        if s.pos.distance(stand) < 6.0 && view.held.is_none() {
            if !left_arm {
                // No saber: shoot it apart (beams waste most of the ore).
                buttons |= FIRE_PRIMARY;
            } else if own.weapon_ready & 4 != 0 && ctx.tick.is_multiple_of(2) {
                // A swing starts on a press (an edge), once the saber is ready.
                buttons |= MELEE;
            }
        }
        fly(ctx, own.frame, stand, Vec3::ZERO, Some(rock.pos), buttons)
    }
}

/// Where a lander comes in from: this far over its landing spot, m...
const APPROACH_HEIGHT: f32 = 50.0;
/// ...and once it's this close to there (m), this slow relative to the body (m/s)...
const APPROACH_NEAR: f32 = 30.0;
const APPROACH_SLOW: f32 = 3.0;
/// ...it arms its grip and comes down this fast (m/s), to this high over the spot (its feet, m):
/// inside the catch, slow enough to be caught.
const DESCENT: f32 = 5.0;
const DESCEND_TO: f32 = 12.0;
/// Down, a walker paces a square this big (m), and hops this often (ticks).
const SQUARE: f32 = 30.0;
const HOP_EVERY: u32 = 90;

/// What a [`LanderBrain`] is to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Plan {
    /// Land on `body` where its surface is straight out from its middle along `dir_local` (its
    /// frame), then walk a 30 m square, hopping every 3 s.
    Walk { body: Body, dir_local: Vec3 },
    /// Land in hide spot `spot` of landmark `landmark`, crouch, and keep still.
    Hide { landmark: u8, spot: u8 },
}

/// Where a lander is in its plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Flying to a spot over where it lands.
    Approach,
    /// Coming down on it, grip armed.
    Descend,
}

/// A lander: it flies to a body, comes in slow and close with its grip armed, lets the catch and
/// the grip land it, then walks about on it ([`Plan::Walk`]) or hides in a hide spot
/// ([`Plan::Hide`]). For tests, the browser's autopilot and bots. A body that moves is flown to
/// where it is now, at the speed of its surface there.
pub struct LanderBrain {
    pub plan: Plan,
    stage: Stage,
    /// When it first stood on the body.
    landed: Option<u32>,
}

impl LanderBrain {
    pub fn new(plan: Plan) -> Self {
        Self { plan, stage: Stage::Approach, landed: None }
    }

    /// The body it lands on.
    pub fn body(&self) -> Body {
        match self.plan {
            Plan::Walk { body, .. } => body,
            Plan::Hide { landmark, .. } => Body::Landmark(landmark),
        }
    }

    /// Where it lands on its body, and the surface's normal there (the body's frame).
    fn spot(&self, ctx: &InputContext) -> Option<(Vec3, Vec3)> {
        let bodies = ctx.predict.bodies(ctx.tick);
        match self.plan {
            Plan::Walk { body, dir_local } => bodies.surface_along(body, dir_local),
            Plan::Hide { landmark, spot } => {
                let def = bodies.landmarks.get(usize::from(landmark))?;
                let c = def.hides.get(usize::from(spot))?.center;
                Some((c, def.shape.probe(c).normal))
            }
        }
    }

    pub fn decide(&mut self, ctx: &InputContext) -> InputCmd {
        let idle = InputCmd { buttons: FLIGHT_ASSIST, ..InputCmd::default() };
        let (Some((at, n)), Some(own)) = (self.spot(ctx), ctx.world.own.filter(|o| o.alive)) else {
            return idle;
        };
        let bodies = ctx.predict.bodies(ctx.tick);
        let body = self.body();
        let (Some(pose), Some(shape)) = (bodies.pose(body), bodies.shape(body)) else { return idle };
        let m = ctx.predict.mover();
        // A way along the surface (it looks along it, so an armed grip rolls its feet down).
        let along =
            |n: Vec3| (n.cross(if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })).normalize_or(Vec3::X);
        if m.footing != Footing::Free {
            let a = m.anchor;
            let up = place(&shape, a.local, a.stance).1;
            let mut cmd = InputCmd { buttons: FLIGHT_ASSIST | GRIP, ..InputCmd::default() };
            if m.footing == Footing::Aloft {
                // In the grip's hold: let it bring the suit down.
                cmd.aim = pose.rot * along(up);
                return cmd;
            }
            let landed = *self.landed.get_or_insert(ctx.tick);
            let down = ctx.tick - landed;
            match self.plan {
                Plan::Walk { .. } => {
                    // A side of the square at a time, turning a quarter each.
                    let side = (SQUARE / WALK_SPEED * TICK_HZ as f32) as u32;
                    let turn = (down / side % 4) as f32 * core::f32::consts::FRAC_PI_2;
                    cmd.aim = pose.rot * (quat_axis_angle(up, turn) * along(up));
                    cmd.thrust = [0, 0, 127];
                    if down > 0 && down.is_multiple_of(HOP_EVERY) {
                        cmd.thrust[1] = 127;
                    }
                }
                Plan::Hide { .. } => {
                    cmd.aim = pose.rot * along(up);
                    cmd.thrust = [0, -127, 0];
                }
            }
            return cmd;
        }
        self.landed = None;
        let (hi, lo) =
            (pose.to_world(at + n * APPROACH_HEIGHT), pose.to_world(at + n * (STANCE + DESCEND_TO)));
        let s = &ctx.predict.state;
        let rel = |p: Vec3, pose: &BodyPose| s.vel - pose.point_vel(p);
        if self.stage == Stage::Approach
            && s.pos.distance(hi) < APPROACH_NEAR
            && rel(s.pos, &pose).length() < APPROACH_SLOW
        {
            self.stage = Stage::Descend;
        }
        if self.stage == Stage::Descend && s.pos.distance(hi) > APPROACH_HEIGHT * 2.0 {
            // Thrown off (or never caught): round again.
            self.stage = Stage::Approach;
        }
        let look = Some(s.pos + pose.rot * along(n) * 1_000.0);
        match self.stage {
            Stage::Approach => fly(ctx, own.frame, hi, pose.point_vel(hi), look, FLIGHT_ASSIST),
            Stage::Descend => {
                let mut cmd =
                    fly_at(ctx, own.frame, lo, pose.point_vel(lo), look, FLIGHT_ASSIST | GRIP, DESCENT);
                // The catch wants nothing pushing it away from the surface.
                cmd.thrust[1] = cmd.thrust[1].min(0);
                cmd
            }
        }
    }
}

/// Flies with flight assist toward `to` (moving at `vel`), round the colony if it's in the way,
/// braking to arrive; looks at `look`, or where it's going.
fn fly(
    ctx: &InputContext,
    frame_id: FrameId,
    to: Vec3,
    vel: Vec3,
    look: Option<Vec3>,
    buttons: u16,
) -> InputCmd {
    fly_at(ctx, frame_id, to, vel, look, buttons, CRUISE)
}

/// [`fly`], no faster than `cruise` m/s (relative to `vel`).
fn fly_at(
    ctx: &InputContext,
    frame_id: FrameId,
    to: Vec3,
    vel: Vec3,
    look: Option<Vec3>,
    buttons: u16,
    cruise: f32,
) -> InputCmd {
    let s = &ctx.predict.state;
    let next = route(s.pos, to);
    // Brake for the end of the whole way, not the turn.
    let (left, end_vel) = if next == to {
        (s.pos.distance(to), vel)
    } else {
        (s.pos.distance(next) + next.distance(to), Vec3::ZERO)
    };
    let speed = (2.0 * BRAKING * left).sqrt().min(left * 0.8).min(cruise);
    let want = end_vel + (next - s.pos).normalize_or_zero() * speed;
    let fwd = s.rot * Vec3::Z;
    let aim = match look {
        Some(p) => (p - s.pos).normalize_or(fwd),
        None if want.length_squared() > 100.0 => want.normalize(),
        None => fwd,
    };
    // The stick asks for a velocity in the suit's own frame, as a share of its cruise speed.
    let stick = s.rot.conjugate() * want / frame(frame_id).fa_speed;
    let q = |x: f32| (x.clamp(-1.0, 1.0) * 127.0).round() as i8;
    InputCmd { aim, thrust: [q(stick.x), q(stick.y), q(stick.z)], buttons, ..InputCmd::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputHistory, Predictor};
    use bc_proto::snapshot::cover;
    use bc_proto::{Faction, OwnState, PilotKind, SnapshotHeader, SnapshotReader, SnapshotWriter};
    use bc_sim::bodies::Bodies;
    use bc_sim::math::look_rotation;
    use bc_sim::{Sim, SimConfig};

    /// The own state as the client decodes it.
    fn over_the_wire(own: &OwnState) -> OwnState {
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, 256);
        w.header(&SnapshotHeader::default());
        w.own(Some(own));
        let n = w.finish().unwrap();
        SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
    }

    /// A Leo `height` m over where `plan` lands, flown by a lander seeing what its client would,
    /// for `ticks`; `each` sees the server after every tick.
    fn land(plan: Plan, height: f32, ticks: u32, mut each: impl FnMut(&Sim, usize)) {
        let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
        let mut brain = LanderBrain::new(plan);
        let bodies = Bodies::at(&sim.field, sim.landmarks(), 0);
        let body = brain.body();
        let pose = bodies.pose(body).unwrap();
        let (at, n) = match plan {
            Plan::Walk { body, dir_local } => bodies.surface_along(body, dir_local).unwrap(),
            Plan::Hide { landmark, spot } => {
                let def = &sim.landmarks()[usize::from(landmark)];
                let c = def.hides[usize::from(spot)].center;
                (c, def.shape.probe(c).normal)
            }
        };
        let start = pose.to_world(at + n * height);
        let id = sim
            .spawn_at(
                FrameId::Leo,
                Faction::Colonies,
                PilotKind::Human,
                start,
                look_rotation(Vec3::Z, Vec3::Y),
            )
            .unwrap();
        let i = id.idx();
        let (mut world, mut predict, mut history) =
            (World::new(Faction::Colonies), Predictor::default(), InputHistory::default());
        for t in 1..=ticks {
            let own = over_the_wire(&sim.own_state(i));
            world.apply(t - 1, Some(own), None, &[], &[]);
            predict.reconcile(t - 1, &own, &history);
            let ctx = InputContext {
                tick: t,
                view_tick: f64::from(t),
                resolve_tick: f64::from(t),
                now: 0.0,
                world: &world,
                predict: &predict,
            };
            let cmd = InputCmd { tick: t, view_tick_q4: t << 4, ..brain.decide(&ctx) }.quantized();
            history.push(cmd);
            sim.set_input(id, cmd);
            sim.step();
            each(&sim, i);
        }
    }

    #[test]
    fn lander_lands_walks_and_hides() {
        // Into THE DEEP on Hermit: down, crouched, and hidden.
        let (mut grounded, mut hidden) = (None, None);
        land(Plan::Hide { landmark: 1, spot: 0 }, 400.0, 2_400, |sim, i| {
            let t = sim.tick();
            if grounded.is_none() && sim.footing(i) == Footing::Grounded {
                grounded = Some(t);
            }
            if grounded.is_some() && hidden.is_none() && sim.cover_code(i) == cover::HIDDEN {
                hidden = Some(t);
            }
        });
        let (Some(down), Some(dark)) = (grounded, hidden) else {
            panic!("grounded {grounded:?}, hidden {hidden:?}")
        };
        assert!(dark > down, "down at {down}, hidden at {dark}");
        // On MO-II as it rolls: down, then about on it, hopping, never letting go.
        let (mut down, mut hops, mut free_after, mut on_ground) = (None, 0, 0, 0);
        let mut was = Footing::Free;
        let (mut first, mut furthest) = (None, 0.0f32);
        land(
            Plan::Walk { body: Body::Landmark(0), dir_local: Vec3::new(1.0, 0.6, 0.6) },
            300.0,
            2_400,
            |sim, i| {
                let f = sim.footing(i);
                if down.is_none() && f == Footing::Grounded {
                    down = Some(sim.tick());
                }
                if down.is_some() {
                    hops += u32::from(was == Footing::Grounded && f == Footing::Aloft);
                    free_after += u32::from(f == Footing::Free);
                    on_ground += u32::from(f == Footing::Grounded);
                    let local = sim.suits.anchor[i].local;
                    let from = *first.get_or_insert(local);
                    furthest = furthest.max(local.distance(from));
                }
                was = f;
            },
        );
        assert!(down.is_some_and(|t| t < 1_500), "landed at {down:?}");
        assert_eq!(free_after, 0, "let go");
        assert!(hops >= 5 && on_ground > 500, "{hops} hops, {on_ground} ticks down");
        assert!(furthest > 15.0, "walked {furthest} m");
    }
}
