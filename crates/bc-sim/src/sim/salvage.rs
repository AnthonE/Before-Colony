//! The salvage loop: grab a chunk with a free hand, stow it in the hold, tow what's too big,
//! throw it, jettison the hold, and sell everything at the colony's dock.

use bc_proto::buttons::{GRAB, JETTISON, STOW, THROW};
use bc_proto::objects::{SPIN_MAX, quantize_held_rot};
use bc_proto::{CARGO_KINDS, ChunkDesc, ChunkKind, NO_CHUNK, Segment};
use glam::Vec3;

use super::Sim;
use crate::chunks::{self, Motion, segment_pos, segment_rot};
use crate::content::salvage::{
    CATCH_SPEED, DOCK_CENTER, DOCK_RADIUS, DOCK_SPEED, JETTISON_SPEED, PRICE, REACH, THROW_IMPULSE,
    THROW_SPEED_MAX, hold_kg, material, ore_ttl, stowable, wreck_ttl,
};
use crate::content::{ArmSlot, frame};
use crate::math::{hash01, normalize_or};

impl Sim {
    /// The chunk suit `i` holds, if it still holds it.
    pub fn held_chunk(&self, i: usize) -> Option<usize> {
        let (k, g, _) = self.suits.held[i];
        (k != NO_CHUNK
            && self.chunks.is_alive(k)
            && self.chunks.generation[k as usize] == g
            && matches!(self.chunks.motion[k as usize], Motion::Held { holder, .. } if holder as usize == i))
        .then_some(k as usize)
    }

    /// Whether suit `i` is in the dock, slow enough to sell.
    pub fn docked(&self, i: usize) -> bool {
        let f = &self.suits.flight[i];
        (f.pos - DOCK_CENTER).length_squared() < DOCK_RADIUS * DOCK_RADIUS
            && f.vel.length_squared() < DOCK_SPEED * DOCK_SPEED
    }

    pub(super) fn salvage_step(&mut self, t: u32) {
        let mut alive = core::mem::take(&mut self.iter_bits);
        alive.copy_from(&self.suits.alive);
        for i in alive.iter() {
            let cmd = self.suits.input[i];
            let prev = self.suits.prev_buttons[i];
            let pressed = |b: u16| cmd.buttons & b != 0 && prev & b == 0;
            match self.held_chunk(i) {
                Some(k) => {
                    // Let go when told to, or when the hand holding it is gone.
                    let right = self.suits.held[i].2;
                    let arm = if right { ArmSlot::Right } else { ArmSlot::Left };
                    if !cmd.pressed(GRAB) || self.suits.part_hp[i][arm.part() as usize] <= 0.0 {
                        self.release(i, k, Vec3::ZERO, t);
                    } else if pressed(STOW) {
                        self.stow(i, k);
                    } else if pressed(THROW) {
                        self.throw(i, k, cmd.aim, t);
                    }
                }
                None => {
                    self.suits.held[i] = (NO_CHUNK, 0, false);
                    if cmd.pressed(GRAB) {
                        self.grab(i, t);
                    }
                }
            }
            if pressed(JETTISON) {
                self.spill(i, t, false);
            }
            if self.docked(i) {
                self.sell(i);
                self.refuel(i);
            }
        }
        self.iter_bits = alive;
    }

    /// Closes suit `i`'s free hand on the nearest free chunk in reach that isn't moving too fast.
    fn grab(&mut self, i: usize, t: u32) {
        let Some(right) = self.suits.grab_hand(i) else { return };
        let f = self.suits.flight[i];
        let hand = f.pos + f.rot * if right { ArmSlot::Right } else { ArmSlot::Left }.muzzle();
        let tt = f64::from(t);
        let mut best: Option<(f32, usize)> = None;
        for k in self.chunks.alive.iter() {
            let Motion::Free(seg) = self.chunks.motion[k] else { continue };
            let gap = (segment_pos(&seg, tt) - hand).length() - chunks::radius(&self.chunks.desc[k]);
            if gap <= REACH
                && (seg.vel - f.vel).length_squared() <= CATCH_SPEED * CATCH_SPEED
                && best.is_none_or(|(g, _)| gap < g)
            {
                best = Some((gap, k));
            }
        }
        let Some((_, k)) = best else { return };
        let Motion::Free(seg) = self.chunks.motion[k] else { return };
        let rot = quantize_held_rot(f.rot.conjugate() * segment_rot(&seg, tt));
        self.chunks.set_motion(k, Motion::Held { holder: i as u16, right, rot, since: t });
        self.suits.held[i] = (k as u16, self.chunks.generation[k], right);
    }

    /// Lets go of chunk `k` where it is, moving as the hand was plus `push`.
    fn release(&mut self, i: usize, k: usize, push: Vec3, t: u32) {
        let (pos, rot, vel) = self.chunk_pose(k);
        let lim = Vec3::splat(SPIN_MAX);
        let spin = self.suits.flight[i].ang_vel.clamp(-lim, lim);
        let seg = Segment { t0: t, pos, vel: vel + push, rot, spin }.quantized();
        self.chunks.set_motion(k, Motion::Free(seg));
        let life = match self.chunks.desc[k].kind {
            ChunkKind::Ore { .. } => ore_ttl(),
            _ => wreck_ttl(false),
        };
        self.chunks.expire[k] = t + life;
        self.suits.held[i] = (NO_CHUNK, 0, false);
    }

    /// Puts chunk `k` in suit `i`'s hold, if it fits.
    fn stow(&mut self, i: usize, k: usize) {
        let desc = self.chunks.desc[k];
        let room = hold_kg(self.suits.frame[i]).saturating_sub(self.suits.cargo_total_kg(i));
        if !stowable(&desc) || desc.mass_kg > room {
            return;
        }
        let bin = &mut self.suits.cargo_kg[i][material(desc.kind)];
        *bin = bin.saturating_add(desc.mass_kg as u16);
        self.chunks.kill(k);
        self.suits.held[i] = (NO_CHUNK, 0, false);
    }

    /// Flings chunk `k` along `aim`; the suit is pushed back just as hard.
    fn throw(&mut self, i: usize, k: usize, aim: Vec3, t: u32) {
        let f = self.suits.flight[i];
        let dir = normalize_or(aim, f.rot * Vec3::Z);
        let chunk_kg = self.chunks.desc[k].mass_kg.max(10) as f32;
        let impulse = THROW_IMPULSE.min(chunk_kg * THROW_SPEED_MAX);
        self.release(i, k, dir * (impulse / chunk_kg), t);
        // The push the chunk actually got (its velocity is on the wire's grid), back on the suit:
        // what's left of it, its propellant and its hold.
        let Motion::Free(seg) = self.chunks.motion[k] else { return };
        let mods = self.flight_mods(i);
        let suit_kg = frame(self.suits.frame[i]).mass(f.propellant) + mods.extra_mass_kg as f32;
        self.suits.flight[i].vel -= (seg.vel - f.vel) * (chunk_kg / suit_kg);
    }

    /// Empties suit `i`'s hold as loose ore: behind it (jettisoned), or all round (spilled as it
    /// dies). With `all`, what's in hand goes too.
    pub(super) fn spill(&mut self, i: usize, t: u32, all: bool) {
        if all && let Some(k) = self.held_chunk(i) {
            self.release(i, k, Vec3::ZERO, t);
        }
        let f = self.suits.flight[i];
        let back = f.rot * -Vec3::Z;
        for kind in 0..CARGO_KINDS {
            let kg = u32::from(self.suits.cargo_kg[i][kind]) / 10 * 10;
            self.suits.cargo_kg[i][kind] = 0;
            if kg == 0 {
                continue;
            }
            let salt = i as u32 * 4 + kind as u32;
            let scatter = normalize_or(
                Vec3::new(hash01(t, salt) - 0.5, hash01(t ^ 0x33, salt) - 0.5, hash01(t ^ 0xCC, salt) - 0.5),
                Vec3::Y,
            );
            let away = if all { scatter } else { normalize_or(back + scatter * 0.4, back) };
            let seg = Segment {
                t0: t,
                pos: f.pos + away * 12.0,
                vel: f.vel + away * JETTISON_SPEED,
                rot: f.rot,
                spin: scatter * 0.6,
            }
            .quantized();
            let desc =
                ChunkDesc { kind: ChunkKind::Ore { ore: kind as u8 }, seed: (salt * 37) as u8, mass_kg: kg };
            self.chunks.spawn(desc, Motion::Free(seg), t + ore_ttl(), t);
        }
    }

    /// Sells suit `i`'s hold, and whatever it has in hand, for credits.
    /// Tops up suit `i`'s propellant (at the dock).
    fn refuel(&mut self, i: usize) {
        self.suits.flight[i].propellant = frame(self.suits.frame[i]).propellant_cap;
    }

    fn sell(&mut self, i: usize) {
        let mut value: u32 = 0;
        for (kind, kg) in self.suits.cargo_kg[i].iter_mut().enumerate() {
            value = value.saturating_add(u32::from(*kg) * PRICE[kind]);
            *kg = 0;
        }
        if let Some(k) = self.held_chunk(i) {
            let desc = self.chunks.desc[k];
            value = value.saturating_add(desc.mass_kg * PRICE[material(desc.kind)]);
            self.chunks.kill(k);
            self.suits.held[i] = (NO_CHUNK, 0, false);
        }
        self.suits.credits[i] = self.suits.credits[i].saturating_add(value);
    }
}
