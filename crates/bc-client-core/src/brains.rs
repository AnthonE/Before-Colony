//! Ready-made agent brains.

use crate::InputContext;
use bc_proto::InputCmd;
use bc_proto::buttons::FLIGHT_ASSIST;
use bc_sim::ai::{self, AiState, DOLL};
use bc_sim::content::{frame, weapon};
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
