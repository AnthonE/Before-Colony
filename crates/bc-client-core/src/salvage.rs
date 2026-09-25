//! What a miner or salvager sees, kept apart from [`Perception`](bc_sim::perception::Perception)
//! (which the Mobile Doll AI reads every tick): its hold and what's in its hand, the loose chunks
//! near it, and a way round the colony to the dock.

use bc_proto::snapshot::own_flags;
use bc_proto::{CARGO_KINDS, ChunkDesc, NO_CHUNK};
use bc_sim::chunks::segment_pos;
use bc_sim::content::salvage::hold_kg;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use glam::Vec3;

use crate::world::{ObjectMotion, World};

/// The own suit's hold, what's in its hand, and its credits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SalvageView {
    /// The hold's contents per material, kg.
    pub cargo_kg: [u16; CARGO_KINDS],
    /// What the hold takes in all, kg.
    pub capacity_kg: u32,
    /// The chunk in hand.
    pub held: Option<u16>,
    pub credits: u32,
    /// At the dock, slow enough to trade.
    pub docked: bool,
}

impl SalvageView {
    /// Everything in the hold, kg.
    pub fn cargo_total_kg(&self) -> u32 {
        self.cargo_kg.iter().map(|&kg| u32::from(kg)).sum()
    }
}

/// A chunk nobody is holding, as it is at some moment.
#[derive(Clone, Copy, Debug)]
pub struct LooseChunk {
    pub id: u16,
    pub desc: ChunkDesc,
    pub pos: Vec3,
    pub vel: Vec3,
}

impl World {
    /// The own suit's hold and hand, once it has a suit.
    pub fn salvage_view(&self) -> Option<SalvageView> {
        let own = self.own?;
        Some(SalvageView {
            cargo_kg: own.cargo_kg,
            capacity_kg: hold_kg(own.frame),
            held: (own.held != NO_CHUNK).then_some(own.held),
            credits: own.credits,
            docked: own.flags & own_flags::DOCKED != 0,
        })
    }

    /// The chunks nobody is holding within `radius` of `pos`, as they are at tick `t`.
    pub fn loose_chunks(&self, pos: Vec3, radius: f32, t: f64) -> impl Iterator<Item = LooseChunk> + '_ {
        self.objects.iter().enumerate().filter_map(move |(id, track)| {
            let track = track.as_ref()?;
            let ObjectMotion::Free(seg) = track.motion_at(t) else { return None };
            let at = segment_pos(&seg, t);
            (at.distance_squared(pos) <= radius * radius).then_some(LooseChunk {
                id: id as u16,
                desc: track.desc,
                pos: at,
                vel: seg.vel,
            })
        })
    }
}

/// Where to head next from `from` on the way to `to`: straight there, unless the colony is in the
/// way (the dock is on its axis, past the −X end cap). Then over the end cap nearer the two, level
/// with the field above the colony.
pub fn route(from: Vec3, to: Vec3) -> Vec3 {
    if clear_of_colony(from, to) {
        return to;
    }
    let end = if from.x + to.x < 0.0 { -1.0 } else { 1.0 };
    let over = Vec3::new(
        COLONY_CENTER.x + end * (COLONY_HALF_LENGTH + 700.0),
        COLONY_CENTER.y + COLONY_RADIUS + 1_500.0,
        COLONY_CENTER.z,
    );
    if from.distance(over) < 300.0 { to } else { over }
}

/// Whether the straight line from `a` to `b` keeps 100 m off the colony (checked every 1/64 of it).
fn clear_of_colony(a: Vec3, b: Vec3) -> bool {
    (0..=64).all(|k| {
        let p = a.lerp(b, k as f32 / 64.0) - COLONY_CENTER;
        p.x.abs() > COLONY_HALF_LENGTH + 150.0 || p.y.hypot(p.z) > COLONY_RADIUS + 100.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::content::salvage::DOCK_CENTER;

    #[test]
    fn the_way_to_the_dock_goes_round_the_colony() {
        let field = Vec3::new(-4_000.0, 800.0, -3_000.0);
        let over = route(field, DOCK_CENTER);
        assert!(over != DOCK_CENTER, "straight through the colony");
        assert!(clear_of_colony(field, over) && clear_of_colony(over, DOCK_CENTER));
        assert_eq!(route(over, DOCK_CENTER), DOCK_CENTER);
        // And back.
        let back = route(DOCK_CENTER, field);
        assert!(clear_of_colony(DOCK_CENTER, back) && clear_of_colony(back, field));
        // Nothing in the way in the field.
        assert_eq!(route(field, field + Vec3::X * 2_000.0), field + Vec3::X * 2_000.0);
    }
}
