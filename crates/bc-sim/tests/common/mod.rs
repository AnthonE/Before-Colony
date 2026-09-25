//! Shared scenario builders for the bc-sim integration tests.
#![allow(dead_code, clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_proto::buttons::{
    BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRAB, JETTISON, MELEE, RCS_SHARP, STOW, THROW, ZERO,
};
use bc_proto::{Faction, FrameId, InputCmd, NO_SLOT, PilotKind};
use bc_sim::math::{hash01, look_rotation};
use bc_sim::{Sim, SimConfig, SuitId};
use glam::Vec3;

/// A busy sector: `humans` player-controlled suits (half Wing Zero with ZERO engaged) and `dolls`
/// Mobile Dolls, spread over a few km so everyone has contacts.
pub fn arena(humans: usize, dolls: usize, seed: u64) -> (Sim, Vec<SuitId>) {
    let cfg = SimConfig { target_dolls: 0, seed, max_suits: 512, ..SimConfig::default() };
    let mut sim = Sim::new(cfg);
    let mut players = Vec::new();
    for k in 0..humans {
        let frame = if k % 2 == 0 { FrameId::WingZero } else { FrameId::Leo };
        let a = k as f32 * 0.37;
        let pos = Vec3::new(a.cos() * 1_800.0, 800.0 + (k % 5) as f32 * 60.0, a.sin() * 1_800.0);
        let id = sim
            .spawn_at(frame, Faction::Colonies, PilotKind::Human, pos, look_rotation(-pos, Vec3::Y))
            .expect("slot");
        players.push(id);
    }
    for k in 0..dolls {
        let a = k as f32 * 0.61;
        let r = 1_000.0 + (k % 7) as f32 * 150.0;
        let pos = Vec3::new(a.cos() * r, 1_200.0 + (k % 3) as f32 * 80.0, a.sin() * r);
        let frame = if k % 4 == 3 { FrameId::Virgo } else { FrameId::Taurus };
        sim.spawn_at(frame, Faction::Oz, PilotKind::MobileDoll, pos, look_rotation(-pos, Vec3::Y))
            .expect("slot");
    }
    (sim, players)
}

/// Deterministic "human-like" input: weaving thrust, aim sweeping toward the centre, bursts of fire,
/// ZERO engaged on Wing Zeros, occasional boosts and saber swings.
pub fn scripted(sim: &Sim, id: SuitId, tick: u32) -> InputCmd {
    let i = id.idx() as u32;
    let pos = sim.suits.flight[id.idx()].pos;
    let wobble = Vec3::new(
        hash01(tick / 20, i) - 0.5,
        hash01(tick / 20, i + 99) - 0.5,
        hash01(tick / 20, i + 7) - 0.5,
    );
    let aim = (Vec3::new(0.0, 1_100.0, 0.0) - pos).normalize_or(Vec3::Z) + wobble * 0.3;
    let mut buttons = FLIGHT_ASSIST | RCS_SHARP;
    if !(tick / 15 + i).is_multiple_of(3) {
        buttons |= FIRE_PRIMARY;
    }
    if (tick / 7 + i).is_multiple_of(4) {
        buttons |= FIRE_SECONDARY;
    }
    if (tick / 45 + i).is_multiple_of(5) {
        buttons |= BOOST;
    }
    if (tick + i).is_multiple_of(97) {
        buttons |= MELEE;
    }
    if sim.suits.frame[id.idx()] == FrameId::WingZero {
        buttons |= ZERO;
    }
    // Salvage: the free hand reaches for wreckage in stretches; now and then stow, throw, or dump
    // the hold.
    if (tick / 60 + i).is_multiple_of(3) {
        buttons |= GRAB;
    }
    if (tick + i * 7).is_multiple_of(53) {
        buttons |= STOW;
    }
    if (tick + i * 5).is_multiple_of(89) {
        buttons |= THROW;
    }
    if (tick + i * 3).is_multiple_of(149) {
        buttons |= JETTISON;
    }
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    InputCmd {
        tick,
        view_tick_q4: (tick << 4).saturating_sub(40 + (i % 50)),
        aim: aim.normalize_or(Vec3::Z),
        thrust: [q(wobble.x * 2.0), q(wobble.y * 2.0), q(0.3 + wobble.z)],
        roll: 0,
        buttons,
        lock_target: NO_SLOT,
        shot_seq: (tick / 10) as u8,
    }
    .quantized()
}

/// Steps the arena `ticks` times with scripted input for every player.
pub fn run(sim: &mut Sim, players: &[SuitId], ticks: u32) {
    for _ in 0..ticks {
        let t = sim.next_tick();
        for &id in players {
            let cmd = scripted(sim, id, t);
            sim.set_input(id, cmd);
        }
        sim.step();
    }
}

/// The Gundams' arena: every playable-to-be frame in pairs (one per side) 12 m apart, spread along
/// x, among `dolls` Mobile Dolls. Returns the sim and the pilots, partners adjacent.
pub fn gundam_arena(dolls: usize, seed: u64) -> (Sim, Vec<SuitId>) {
    let (mut sim, _) = arena(0, dolls, seed);
    let frames = [
        FrameId::WingZero,
        FrameId::Heavyarms,
        FrameId::Deathscythe,
        FrameId::Sandrock,
        FrameId::Shenlong,
        FrameId::Leo,
    ];
    let mut pilots = Vec::new();
    for (k, f) in frames.iter().enumerate() {
        for (side, faction) in [Faction::Colonies, Faction::Oz].into_iter().enumerate() {
            let pos = Vec3::new(k as f32 * 400.0 - 1_000.0, 1_100.0, side as f32 * 12.0);
            let facing = if side == 0 { Vec3::Z } else { -Vec3::Z };
            let id = sim.spawn_at(*f, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y));
            pilots.push(id.expect("slot"));
        }
    }
    (sim, pilots)
}

/// Deterministic input for a Gundam pilot duelling `foe`: aim at it, close in, strike with every
/// blade and the special, fire in bursts.
pub fn duel_scripted(sim: &Sim, id: SuitId, foe: SuitId, tick: u32) -> InputCmd {
    let i = id.idx() as u32;
    let me = sim.suits.flight[id.idx()].pos;
    let wobble =
        Vec3::new(hash01(tick / 15, i) - 0.5, hash01(tick / 15, i + 5) - 0.5, hash01(tick / 15, i + 9) - 0.5);
    let to = sim.suits.flight[foe.idx()].pos - me;
    let aim = to.normalize_or(Vec3::Z) + wobble * 0.1;
    let mut buttons = FLIGHT_ASSIST;
    // Blades and the special when the foe is near and ahead, now and then.
    let ahead = (sim.suits.flight[id.idx()].rot * Vec3::Z).dot(to.normalize_or(Vec3::Z)) > 0.8;
    if ahead && to.length() < 16.0 && (tick + i).is_multiple_of(7) {
        buttons |= MELEE;
    }
    if ahead && to.length() < 14.0 && (tick + 3 * i).is_multiple_of(11) {
        buttons |= bc_proto::buttons::SPECIAL;
    }
    // The frame's mode (the Hyper Jammer) for stretches of three seconds.
    if (tick / 90 + i).is_multiple_of(2) {
        buttons |= bc_proto::buttons::MODE;
    }
    if (tick / 10 + i).is_multiple_of(3) {
        buttons |= FIRE_PRIMARY;
    }
    if (tick / 7 + i).is_multiple_of(4) {
        buttons |= FIRE_SECONDARY;
    }
    if to.length() > 60.0 && (tick / 30 + i).is_multiple_of(2) {
        buttons |= BOOST;
    }
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0) as i8;
    // Flight assist holds the velocity the stick asks for (suit frame): toward the foe, slower as
    // it closes, and a little weave.
    let local = sim.suits.flight[id.idx()].rot.inverse() * to.normalize_or(Vec3::Z);
    let pull = ((to.length() - 6.0) / 300.0).clamp(0.0, 0.5);
    let weave = wobble * 0.05;
    InputCmd {
        tick,
        view_tick_q4: (tick << 4).saturating_sub(40 + (i % 50)),
        aim: aim.normalize_or(Vec3::Z),
        thrust: [q(local.x * pull + weave.x), q(local.y * pull + weave.y), q(local.z * pull + weave.z)],
        roll: 0,
        buttons,
        lock_target: NO_SLOT,
        shot_seq: (tick / 10) as u8,
    }
    .quantized()
}
