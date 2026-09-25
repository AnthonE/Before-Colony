//! Mining: shots and sabers wear rocks down (sabers chip ore off as they go), a rock with no
//! structure left shatters into ore, and grows back once nobody is near. A saber also cuts limbs
//! off hulks.

use bc_proto::events::Event;
use bc_proto::{ChunkDesc, ChunkKind, NO_CHUNK, Part, Segment, WeaponKind};
use glam::Vec3;

use super::Sim;
use crate::chunks::{self, Motion};
use crate::config::secs;
use crate::content::frame;
use crate::content::salvage::{
    BEAM_WASTE_KG, CHIP_KG, REGROW_CLEAR, ore_ttl, part_mass_kg, rock_multiplier, wreck_ttl,
};
use crate::math::{hash01, normalize_or};
use crate::rocks::{max_hp, max_ore_kg};

/// Ticks a shattered rock takes to grow back, and between retries while someone is near.
fn regrow_after() -> u32 {
    secs(600.0)
}

impl Sim {
    /// Rock `i` takes `amount` of `kind`'s damage at `at`, struck along `dir`, by suit `by`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn rock_hit(
        &mut self,
        i: usize,
        amount: f32,
        kind: WeaponKind,
        at: Vec3,
        dir: Vec3,
        by: usize,
        t: u32,
    ) {
        if self.rocks.destroyed.get(i) {
            return;
        }
        let before = self.rock_state(i);
        let r = &mut self.rocks;
        r.hp[i] -= amount * rock_multiplier(kind);
        // Beams boil off some of the ore they strike.
        if kind.is_beam() {
            r.ore_kg[i] = r.ore_kg[i].saturating_sub((amount * BEAM_WASTE_KG) as u32);
        }
        // A saber chips a piece off, ore and all.
        if kind == WeaponKind::BeamSaber {
            let chip = r.ore_kg[i].min(CHIP_KG) / 10 * 10;
            if chip > 0 {
                r.ore_kg[i] -= chip;
                let rock = self.field.rocks()[i];
                let out = rock.normal(at, 0.0);
                let seg = Segment {
                    t0: t,
                    pos: at + out * 1.5,
                    vel: out * 3.0 + dir,
                    rot: rock.rot,
                    spin: normalize_or(out.cross(dir), Vec3::Y) * 0.8,
                }
                .quantized();
                let desc = ChunkDesc {
                    kind: ChunkKind::Ore { ore: rock.ore },
                    seed: (t % 251) as u8,
                    mass_kg: chip,
                };
                self.chunks.spawn(desc, Motion::Free(seg), t + ore_ttl(), t);
            }
        }
        if self.rocks.hp[i] <= 0.0 {
            self.shatter(i, by, t);
        } else if self.rock_state(i) != before {
            self.rocks.touch(i);
        }
    }

    /// Rock `i` comes apart: its ore flies off as chunks, and it's gone until it grows back.
    fn shatter(&mut self, i: usize, by: usize, t: u32) {
        let rock = self.field.rocks()[i];
        let ore = self.rocks.ore_kg[i];
        let n = (ore / 400).clamp(2, 8);
        let each = ore / n / 10 * 10;
        if each > 0 {
            for k in 0..n {
                let salt = i as u32 * 8 + k;
                let dir = normalize_or(
                    Vec3::new(
                        hash01(t, salt) - 0.5,
                        hash01(t ^ 0x5A, salt) - 0.5,
                        hash01(t ^ 0xA5, salt) - 0.5,
                    ),
                    Vec3::Y,
                );
                let seg = Segment {
                    t0: t,
                    pos: rock.pos + dir * rock.radius * 0.5,
                    vel: dir * (2.0 + 4.0 * hash01(t ^ 0x77, salt)),
                    rot: rock.rot,
                    spin: dir.cross(Vec3::Y) * 0.6,
                }
                .quantized();
                let desc =
                    ChunkDesc { kind: ChunkKind::Ore { ore: rock.ore }, seed: salt as u8, mass_kg: each };
                self.chunks.spawn(desc, Motion::Free(seg), t + ore_ttl(), t);
            }
        }
        let r = &mut self.rocks;
        r.hp[i] = 0.0;
        r.ore_kg[i] = 0;
        r.destroyed.set(i, true);
        r.regrow_at[i] = t + regrow_after();
        r.touch(i);
        self.field.set_dead(i, true);
        self.events.push(Event::RockBreak { id: 0, tick: t, rock: i as u16, by: by as u16 });
    }

    /// Shattered rocks grow back in time, once no suit is within `REGROW_CLEAR`.
    pub(super) fn field_step(&mut self, t: u32) {
        if !t.is_multiple_of(30) {
            return;
        }
        for i in 0..self.field.len() {
            if !self.rocks.destroyed.get(i) || t < self.rocks.regrow_at[i] {
                continue;
            }
            let rock = self.field.rocks()[i];
            let near = self.suits.alive.iter().any(|j| {
                (self.suits.flight[j].pos - rock.pos).length_squared() < REGROW_CLEAR * REGROW_CLEAR
            });
            let r = &mut self.rocks;
            if near {
                r.regrow_at[i] = t + secs(30.0);
                continue;
            }
            r.hp[i] = max_hp(&rock);
            r.ore_kg[i] = max_ore_kg(&rock);
            r.destroyed.set(i, false);
            r.touch(i);
            self.field.set_dead(i, false);
        }
    }

    /// A saber through hulk `k` cuts off the part nearest the blade (`hand` to `tip`), which
    /// drifts off as a limb.
    pub(super) fn cut_hulk(&mut self, k: usize, hand: Vec3, tip: Vec3, t: u32) {
        let ChunkKind::Hulk { frame: f, faction, parts } = self.chunks.desc[k].kind else { return };
        let (pos, rot, vel) = self.chunk_pose(k);
        let spec = frame(f);
        let mut best: Option<(f32, Part, Vec3)> = None;
        for part in [Part::Head, Part::ArmL, Part::ArmR, Part::Legs, Part::Backpack] {
            if parts & (1 << part as u8) == 0 {
                continue;
            }
            let c = spec.capsules[part as usize];
            let mid = pos + rot * ((c.a + c.b) * 0.5);
            let (_, _, d2) = crate::collide::segment_segment(hand, tip, mid, mid);
            if best.is_none_or(|(b, ..)| d2 < b) {
                best = Some((d2, part, mid));
            }
        }
        let Some((_, part, mid)) = best else { return };
        let out = normalize_or(mid - pos, Vec3::Y);
        let seg = Segment { t0: t, pos: mid, vel: vel + out * 3.0, rot, spin: out.cross(Vec3::Z) * 1.2 }
            .quantized();
        let desc = ChunkDesc {
            kind: ChunkKind::Limb { frame: f, faction, part },
            seed: (t % 253) as u8,
            mass_kg: part_mass_kg(f, part),
        };
        let chunk = self.chunks.spawn(desc, Motion::Free(seg), t + wreck_ttl(false), t).unwrap_or(NO_CHUNK);
        let hulk = &mut self.chunks.desc[k];
        hulk.kind = ChunkKind::Hulk { frame: f, faction, parts: parts & !(1 << part as u8) };
        hulk.mass_kg = hulk.mass_kg.saturating_sub(part_mass_kg(f, part));
        self.chunks.touch(k);
        self.events.push(Event::Detach { id: 0, tick: t, source: k as u16, from_hulk: true, part, chunk });
    }

    /// Hulks a saber blade (`hand` to `tip`, radius `r`) passes through.
    pub(super) fn hulk_in_blade(&self, hand: Vec3, tip: Vec3, r: f32) -> Option<usize> {
        self.chunks.alive.iter().find(|&k| {
            matches!(self.chunks.desc[k].kind, ChunkKind::Hulk { .. })
                && matches!(self.chunks.motion[k], Motion::Free(_))
                && {
                    let (pos, ..) = self.chunk_pose(k);
                    let reach = chunks::radius(&self.chunks.desc[k]) + r;
                    crate::collide::segment_near_point(hand, tip, pos, reach)
                }
        })
    }
}
