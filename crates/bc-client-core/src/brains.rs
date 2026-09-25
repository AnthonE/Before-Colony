//! Ready-made agent brains.

use crate::InputContext;
use crate::salvage::route;
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, GRAB, MELEE, STOW};
use bc_proto::{ChunkKind, FrameId, InputCmd, Part};
use bc_sim::ai::{self, AiState, DOLL};
use bc_sim::content::salvage::{DOCK_CENTER, stowable};
use bc_sim::content::{ArmSlot, frame, weapon};
use bc_sim::field::SUIT_CLEARANCE;
use glam::Vec3;

/// Where the fighting is (the Mobile Doll patrol ring around the colony).
pub const COMBAT_ZONE: Vec3 = Vec3::new(0.0, 900.0, 0.0);

/// The Mobile Doll brain from `bc-sim`, running on the agent's own sensor view. It picks targets
/// and maneuvers by utility and leads them linearly. With nothing on sensors it heads for the
/// combat zone.
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
        if ctx.tick >= self.ai.think_at {
            ai::think(&p, &mut self.ai, ctx.tick, &DOLL, range);
            self.ai.think_at = ctx.tick + 3;
        }
        let target = p.get(self.ai.target).copied();
        ai::drive(&p.me, target.as_ref(), &mut self.ai, ctx.tick, &DOLL, spec)
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
/// shot off); its free hand takes each chip of ore knocked off, and stows it. When the rock shatters it gathers up the ore, and when the hold
/// is full (or it has something in hand that won't fit) it takes it to the dock to sell, round
/// the colony. It flies with flight assist.
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
    let s = &ctx.predict.state;
    let next = route(s.pos, to);
    // Brake for the end of the whole way, not the turn.
    let (left, end_vel) = if next == to {
        (s.pos.distance(to), vel)
    } else {
        (s.pos.distance(next) + next.distance(to), Vec3::ZERO)
    };
    let speed = (2.0 * BRAKING * left).sqrt().min(left * 0.8).min(CRUISE);
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
