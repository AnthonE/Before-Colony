//! A suit's arms tick by tick: whether a blade is striking (and lunging), and how lately a weapon
//! fired. Busy arms take AMBAC's limbs and a blade's lunge drives the suit, so the flight model
//! needs both every tick.
//!
//! The server's melee step starts and advances strikes with [`strike_slot`] and [`next_phase`].
//! The owner's client rolls an [`ArmsClock`] forward from each snapshot with the same functions,
//! so its prediction lunges, and turns with busy arms, exactly when the server's suit does.

use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, MELEE, SPECIAL};
use bc_proto::snapshot::{OwnArms, own_flags};
use bc_proto::{InputCmd, OwnState};

use crate::content::{ArmSlot, FrameSpec, MeleeSpec, SPECIAL_MOUNT, SpecialKind, WeaponClass, frame, weapon};
use crate::suits::MeleePhase;

/// After a weapon fires (or a strike begins), the arms are busy for this many ticks.
pub const BUSY_FIRE_TICKS: u32 = 6;
/// AMBAC's authority while the arms are busy, of what it has with them idle.
pub const BUSY_AMBAC: f32 = 0.6;

/// AMBAC's authority with busy arms, from its authority with them idle (as sent to the owner).
pub fn busy_ambac(idle: f32) -> f32 {
    idle * BUSY_AMBAC
}

/// The mount a command strikes with, if one it asks for is ready: in priority order, the special's
/// press (a melee move), the melee press, then fire held on a blade in a gun slot (the Dragon
/// Fang). `ready(slot)`: whether that mount could start a strike now.
pub fn strike_slot(cmd: &InputCmd, prev_buttons: u16, ready: impl Fn(u8) -> bool) -> Option<u8> {
    let edge = |b: u16| cmd.pressed(b) && prev_buttons & b == 0;
    [
        (edge(SPECIAL), SPECIAL_MOUNT),
        (edge(MELEE), 2),
        (cmd.pressed(FIRE_PRIMARY), 0),
        (cmd.pressed(FIRE_SECONDARY), 1),
    ]
    .into_iter()
    .find(|&(want, slot)| want && ready(slot))
    .map(|(_, slot)| slot)
}

/// A strike's phase and timer a tick on: the timer counts each phase down, then the next begins.
pub fn next_phase(phase: MeleePhase, timer: u8, m: &MeleeSpec) -> (MeleePhase, u8) {
    let timer = timer.saturating_sub(1);
    if timer > 0 {
        return (phase, timer);
    }
    match phase {
        MeleePhase::Windup => (MeleePhase::Active, m.active),
        MeleePhase::Active => (MeleePhase::Recovery, m.recovery),
        _ => (MeleePhase::Idle, 0),
    }
}

/// A strike's phase as sent in [`OwnArms::phase`].
pub fn phase_to_wire(phase: MeleePhase) -> u8 {
    match phase {
        MeleePhase::Idle => 0,
        MeleePhase::Windup => 1,
        MeleePhase::Active => 2,
        MeleePhase::Recovery => 3,
    }
}

fn phase_from_wire(p: u8) -> MeleePhase {
    match p {
        1 => MeleePhase::Windup,
        2 => MeleePhase::Active,
        3 => MeleePhase::Recovery,
        _ => MeleePhase::Idle,
    }
}

fn run_down(wait: &mut u8) {
    if *wait != OwnArms::NEVER {
        *wait = wait.saturating_sub(1);
    }
}

/// The arms as the owner's client rolls them on from a snapshot. What it can't foresee (a clash, a
/// limb shot off, running out of energy or ammunition, heat building to an overheat or cooling
/// off) waits for the next snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmsClock {
    pub phase: MeleePhase,
    pub timer: u8,
    /// The mount striking: a loadout slot, or [`SPECIAL_MOUNT`].
    pub slot: u8,
    /// The tick a weapon last fired or a strike began.
    pub fired_at: u32,
    /// Per mount (the loadout's three, then the special's): ticks until it could strike or fire,
    /// or [`OwnArms::NEVER`].
    pub wait: [u8; 4],
    /// Each gun slot's missile salvo under way: rounds still to launch, and ticks until the next.
    pub salvo: [u8; 2],
    pub salvo_gap: [u8; 2],
    /// Ticks the primary has charged (the Twin Buster Rifle fires when it's full).
    pub charge: u8,
    /// Ticks of Full Open left.
    pub full_open: u8,
    /// Overheated: guns and blades wait (Full Open fires through it).
    pub overheated: bool,
    /// The buttons of the command flown last (a press is a button that wasn't down then).
    pub prev_buttons: u16,
}

impl Default for ArmsClock {
    fn default() -> Self {
        Self {
            phase: MeleePhase::Idle,
            timer: 0,
            slot: 0,
            fired_at: 0,
            wait: [OwnArms::NEVER; 4],
            salvo: [0; 2],
            salvo_gap: [0; 2],
            charge: 0,
            full_open: 0,
            overheated: false,
            prev_buttons: 0,
        }
    }
}

impl ArmsClock {
    /// The arms as of snapshot tick `t`, whose command had `buttons` down.
    pub fn from_own(own: &OwnState, t: u32, buttons: u16) -> Self {
        let spec = frame(own.frame);
        let a = &own.arms;
        // Sent as a fraction of the charge, which comes back to the tick (see the content tests).
        let charge_ticks = spec.loadout[0].map_or(0, |m| weapon(m.weapon).charge_ticks);
        let full_open = matches!(spec.special, SpecialKind::FullOpen { .. })
            && own.flags & own_flags::SPECIAL_ACTIVE != 0;
        Self {
            phase: phase_from_wire(a.phase),
            timer: a.timer,
            slot: a.slot,
            fired_at: t.saturating_sub(u32::from(a.fired_ago)),
            wait: a.wait,
            salvo: a.salvo,
            salvo_gap: a.salvo_gap,
            charge: (own.charge * f32::from(charge_ticks) + 0.5) as u8,
            full_open: if full_open { own.special_timer } else { 0 },
            overheated: own.flags & own_flags::OVERHEAT != 0,
            prev_buttons: buttons,
        }
    }

    /// Whether the arms are busy for the flight of tick `t` (the clock as it stood after tick
    /// `t − 1`): a strike under way, or a weapon fired within [`BUSY_FIRE_TICKS`].
    pub fn busy(&self, t: u32) -> bool {
        self.phase != MeleePhase::Idle || t.saturating_sub(self.fired_at) < BUSY_FIRE_TICKS
    }

    /// The mount of a strike in its windup or stroke.
    pub fn striking(&self) -> Option<u8> {
        matches!(self.phase, MeleePhase::Windup | MeleePhase::Active).then_some(self.slot)
    }

    /// Whether the strike under way drives the suit forward (a blade's windup and stroke).
    pub fn lunging(&self, spec: &FrameSpec) -> bool {
        self.striking()
            .and_then(|slot| spec.melee_mount(slot))
            .and_then(|m| weapon(m.weapon).melee)
            .is_some_and(|m| m.lunge)
    }

    /// Whether the strike under way takes `arm` with it (the Dragon Fang), so its guns wait.
    fn blocks(&self, spec: &FrameSpec, arm: ArmSlot) -> bool {
        self.phase != MeleePhase::Idle
            && spec
                .melee_mount(self.slot)
                .is_some_and(|m| weapon(m.weapon).blocks_arm && m.arm.part() == arm.part())
    }

    /// A change of form began: any strike under way is dropped, and so is a charge.
    pub fn drop_strike(&mut self) {
        (self.phase, self.timer, self.charge) = (MeleePhase::Idle, 0, 0);
    }

    /// Rolls the arms through tick `t` under `cmd`, after its flight, as the server's specials,
    /// weapons and melee steps do. `changing`: the suit is changing form.
    pub fn tick(&mut self, spec: &FrameSpec, cmd: &InputCmd, changing: bool, t: u32) {
        self.roll(spec, cmd, changing, t);
        self.prev_buttons = cmd.buttons;
    }

    fn roll(&mut self, spec: &FrameSpec, cmd: &InputCmd, changing: bool, t: u32) {
        let prev_buttons = self.prev_buttons;
        let edge = |b: u16| cmd.pressed(b) && prev_buttons & b == 0;
        // The special: its cooldown runs down; Full Open runs its course, or opens on a press.
        run_down(&mut self.wait[3]);
        let mut opened_out = false;
        if let SpecialKind::FullOpen { ticks, cooldown, .. } = spec.special {
            if self.full_open > 0 {
                self.full_open -= 1;
                opened_out = self.full_open == 0;
            } else if edge(SPECIAL) && self.wait[3] == 0 && !self.overheated {
                self.full_open = ticks.min(255) as u8;
                self.wait[3] = OwnArms::wait(cooldown);
            }
        }
        // Guns, which are down while the suit changes form. Full Open fires them all, heat or not.
        let heedless = self.full_open > 0;
        if !changing {
            for (slot, button) in [(0usize, FIRE_PRIMARY), (1, FIRE_SECONDARY)] {
                let Some(mount) = spec.loadout[slot] else { continue };
                let w = weapon(mount.weapon);
                let blocked = self.blocks(spec, mount.arm);
                match w.class {
                    WeaponClass::Melee => continue,
                    // A flamethrower counts as firing every tick it's lit.
                    WeaponClass::Cone => {
                        if cmd.pressed(button)
                            && !blocked
                            && !self.overheated
                            && self.wait[slot] != OwnArms::NEVER
                        {
                            self.fired_at = t;
                        }
                        continue;
                    }
                    WeaponClass::Beam | WeaponClass::Ballistic | WeaponClass::Missile => {}
                }
                let wants = heedless || cmd.pressed(button);
                run_down(&mut self.wait[slot]);
                let ready = self.wait[slot] == 0 && !blocked && (heedless || !self.overheated);
                if w.class == WeaponClass::Missile {
                    // A pull starts a salvo; its rounds leave `salvo_gap` apart.
                    if wants && ready && self.salvo[slot] == 0 {
                        (self.salvo[slot], self.salvo_gap[slot]) = (w.salvo.max(1), 0);
                        self.wait[slot] = OwnArms::wait(w.cooldown);
                    }
                    if self.salvo[slot] > 0 {
                        if self.salvo_gap[slot] > 0 {
                            self.salvo_gap[slot] -= 1;
                        } else {
                            self.salvo[slot] -= 1;
                            self.salvo_gap[slot] = w.salvo_gap.saturating_sub(1);
                            self.fired_at = t;
                        }
                    }
                    continue;
                }
                let fire = if w.charge_ticks > 0 {
                    // Held, it charges and fires when full; let go, the charge is lost.
                    if wants && ready {
                        self.charge = self.charge.saturating_add(1);
                        let full = u16::from(self.charge) >= w.charge_ticks;
                        if full {
                            self.charge = 0;
                        }
                        full
                    } else {
                        self.charge = 0;
                        false
                    }
                } else {
                    wants && ready
                };
                if fire {
                    self.fired_at = t;
                    self.wait[slot] = OwnArms::wait(w.cooldown);
                }
            }
            // The special mounts fire all through Full Open.
            if heedless {
                self.fired_at = t;
            }
        }
        // Once Full Open is over, the suit is locked out as if overheated.
        if opened_out {
            self.overheated = true;
        }
        // Blades: their cooldowns run down, then a strike starts, or the one under way moves on.
        for slot in 0..3u8 {
            if spec.melee_mount(slot).is_some() {
                run_down(&mut self.wait[usize::from(slot)]);
            }
        }
        match self.phase {
            MeleePhase::Idle if changing => {}
            MeleePhase::Idle => {
                let ready = |slot: u8| {
                    self.wait[usize::from(slot)] == 0 && !self.overheated && spec.melee_mount(slot).is_some()
                };
                let Some(slot) = strike_slot(cmd, prev_buttons, ready) else { return };
                let Some(mount) = spec.melee_mount(slot) else { return };
                let w = weapon(mount.weapon);
                let Some(m) = w.melee else { return };
                (self.phase, self.timer, self.slot, self.fired_at) = (MeleePhase::Windup, m.windup, slot, t);
                self.wait[usize::from(slot)] = OwnArms::wait(match spec.special {
                    SpecialKind::MeleeMove { cooldown } if slot == SPECIAL_MOUNT => cooldown,
                    _ => w.cooldown + m.duration(),
                });
            }
            phase => match spec.melee_mount(self.slot).and_then(|m| weapon(m.weapon).melee) {
                Some(m) => (self.phase, self.timer) = next_phase(phase, self.timer, &m),
                None => (self.phase, self.timer) = (MeleePhase::Idle, 0),
            },
        }
    }
}
