//! Client → server input.
//!
//! [`InputCmd`] is the one control interface in the game. Humans, external agents and server-side
//! Mobile Dolls all drive their suits by producing it, so nobody gets a privileged API.

use glam::Vec3;

use crate::quant::{self, quantize_dir};
use crate::{BitReader, BitWriter, DecodeError, NO_SLOT, PACKET_KIND_BITS, PacketKind, SLOT_BITS};

/// Button and state bits. Toggles (flight assist, ZERO, the frame's mode, a grab, the grip) are
/// sent as **states**, never as presses, so a lost or duplicated packet can't flip them twice. Bit
/// 15 is free.
pub mod buttons {
    /// Left mouse: primary weapon (beam rifle / Twin Buster Rifle).
    pub const FIRE_PRIMARY: u16 = 1 << 0;
    /// Right mouse: secondary weapon (machine cannon).
    pub const FIRE_SECONDARY: u16 = 1 << 1;
    /// F: beam saber.
    pub const MELEE: u16 = 1 << 2;
    /// Shift: main-thruster overdrive (more thrust, more propellant, more G).
    pub const BOOST: u16 = 1 << 3;
    /// X: retro-burn toward zero velocity even with flight assist off.
    pub const BRAKE: u16 = 1 << 4;
    /// State: flight assist (velocity hold) engaged.
    pub const FLIGHT_ASSIST: u16 = 1 << 5;
    /// State: ZERO System engaged.
    pub const ZERO: u16 = 1 << 6;
    /// Hold: turn with RCS thrusters (fast, burns propellant) instead of AMBAC alone.
    pub const RCS_SHARP: u16 = 1 << 7;
    /// State: the free hand closes on the nearest chunk in reach and holds it while set.
    pub const GRAB: u16 = 1 << 8;
    /// Press: put what's in hand into the hold.
    pub const STOW: u16 = 1 << 9;
    /// Press: fling what's in hand along the aim (and get pushed back).
    pub const THROW: u16 = 1 << 10;
    /// Press: dump the hold's contents.
    pub const JETTISON: u16 = 1 << 11;
    /// State: the frame's mode is engaged (Wing Zero's Neo-Bird form, Deathscythe's Hyper Jammer).
    pub const MODE: u16 = 1 << 12;
    /// Press: the frame's special attack (Heavyarms' Full Open Attack, Sandrock's Cross Crusher).
    pub const SPECIAL: u16 = 1 << 13;
    /// State (L): the grip is armed. Coming in slow and close to a surface lands the suit on it, and
    /// it holds on while this is set: clearing it lets go. Being a state, a client that stalls
    /// never drops its suit off a body.
    pub const GRIP: u16 = 1 << 14;
    /// Press: the burst step, along the stick (a double-tapped direction). A step starts on the
    /// press (`bc_sim::flight::Burst`), so a silent client's repeat can't step again.
    pub const BURST: u16 = 1 << 15;

    /// Actions a silent client's repeated command must not keep performing.
    pub const FIRE_MASK: u16 = FIRE_PRIMARY | FIRE_SECONDARY | MELEE | SPECIAL;
    /// States that persist while a client is silent (see [`InputCmd::neutral`](super::InputCmd::neutral)).
    pub const STATES: u16 = FLIGHT_ASSIST | ZERO | GRAB | MODE | GRIP;
    pub const BITS: u32 = 16;
}

/// Bits per axis for the aim direction (octahedral): ~0.005° precision.
pub const AIM_BITS: u32 = 16;
/// A lock-on's reference velocity ([`LockOn::ref_vel`]): 3 × this many bits on the centred grid over
/// ±[`LOCKON_VEL_MAX`] m/s (0.25 m/s steps, and a target at rest is exactly at rest).
pub const LOCKON_VEL_BITS: u32 = 14;
pub const LOCKON_VEL_MAX: f32 = 2_048.0;
/// Bits per axis for a lock-on's up (octahedral): ~0.2°.
pub const LOCKON_UP_BITS: u32 = 10;
/// How long a silent client's suit keeps flying locked on once it's gone hands-off, in ticks past
/// [`NEUTRAL_AFTER`]: about a second, then flight assist holds still in the sector's frame again.
pub const LOCKON_KEEP: u32 = 30;
/// How far behind its own tick a command may place its lag-compensation view, in 1/16 ticks: up
/// to 15.9 ticks, twice what lag compensation reaches back (8 ticks). An older view saturates, and
/// the server treats it as 8 ticks old either way.
pub const VIEW_DELTA_BITS: u32 = 8;
pub const MAX_CMDS: usize = 4;
/// A client silent for more than this many ticks goes hands-off (see [`InputCmd::stand_in`]).
pub const NEUTRAL_AFTER: u32 = 8;

/// Flying locked on to a target: flight assist holds the suit's velocity relative to `ref_vel` (the
/// target's, as its pilot sees it) instead of to the sector, and reads the stick in axes levelled
/// to `up`: the stick's up is `up`, its forward the aim laid flat, and the suit rolls level with
/// it. The pilot's client works both out, so the server and the owner's prediction fly exactly
/// the same command; neither is checked against the target, since all they ask for is a velocity a
/// pilot could fly by hand (the simulation caps `ref_vel` at the frame's boosted cruise).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LockOn {
    /// What flight assist holds the suit's velocity relative to, sector frame, m/s.
    pub ref_vel: Vec3,
    /// The fight's up (unit, sector frame): its "ground" is the plane square to it.
    pub up: Vec3,
}

impl LockOn {
    /// After a round trip through the wire.
    pub fn quantized(&self) -> Self {
        let q = |v: f32| {
            quant::dequantize_centered(
                quant::quantize_centered(v, LOCKON_VEL_MAX, LOCKON_VEL_BITS),
                LOCKON_VEL_MAX,
                LOCKON_VEL_BITS,
            )
        };
        Self {
            ref_vel: Vec3::new(q(self.ref_vel.x), q(self.ref_vel.y), q(self.ref_vel.z)),
            up: quantize_dir(self.up, LOCKON_UP_BITS),
        }
    }

    fn write(&self, w: &mut BitWriter<'_>) {
        quant::write_vec_centered(w, self.ref_vel, LOCKON_VEL_MAX, LOCKON_VEL_BITS);
        quant::write_dir(w, self.up, LOCKON_UP_BITS);
    }

    fn read(r: &mut BitReader<'_>) -> Self {
        let ref_vel = quant::read_vec_centered(r, LOCKON_VEL_MAX, LOCKON_VEL_BITS);
        Self { ref_vel, up: quant::read_dir(r, LOCKON_UP_BITS) }
    }
}

/// One tick of control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputCmd {
    /// Simulation tick this command is for.
    pub tick: u32,
    /// The client's interpolated view time when it issued the command, in 1/16 ticks (`tick << 4`
    /// means "no lag compensation"). Shots are resolved against the world as the pilot saw it.
    pub view_tick_q4: u32,
    /// Unit aim direction, world space.
    pub aim: Vec3,
    /// Thrust demand in the suit's local frame: x right, y up, z forward. -127..=127. On its feet
    /// on a body x and z walk instead, and y sets the stance and keeps it: -64 or less crouches, 32
    /// or more stands, 100 or more (standing) hops, and anything between holds the stance it has.
    pub thrust: [i8; 3],
    /// Roll rate demand, -127..=127 (positive = clockwise when viewed from behind).
    pub roll: i8,
    /// [`buttons`] bitset.
    pub buttons: u16,
    /// Suit the pilot designates as its target ([`NO_SLOT`] = none): missile locks, lock-on
    /// warnings and replication priority. The server checks it before using it.
    pub lock_target: u16,
    /// Increments on every shot fired, so the shooter can match its predicted beams to the server's.
    pub shot_seq: u8,
    /// Flying locked on to a target ([`LockOn`]), or not.
    pub lockon: Option<LockOn>,
}

impl Default for InputCmd {
    fn default() -> Self {
        Self {
            tick: 0,
            view_tick_q4: 0,
            aim: Vec3::Z,
            thrust: [0; 3],
            roll: 0,
            buttons: 0,
            lock_target: NO_SLOT,
            shot_seq: 0,
            lockon: None,
        }
    }
}

impl InputCmd {
    /// A "hands off" command for `tick` that keeps the given aim and states (flight assist, ZERO,
    /// the frame's mode, a grip on whatever is in hand, and the grip on a surface).
    pub fn neutral(tick: u32, aim: Vec3, keep_buttons: u16) -> Self {
        Self {
            tick,
            view_tick_q4: tick << 4,
            aim,
            buttons: keep_buttons & buttons::STATES,
            ..Self::default()
        }
    }

    /// What the server flies for `tick` when a client's command for it never arrived, `missing`
    /// ticks (1, 2, …) after `last`, the last one that did: `last` again without firing, on the same
    /// view delay, then hands-off ([`InputCmd::neutral`]) once the client has been silent for more
    /// than [`NEUTRAL_AFTER`] ticks, still locked on for [`LOCKON_KEEP`] ticks more (so a stalled
    /// pilot keeps station on its target rather than braking hard to the sector's rest). The owner's
    /// prediction flies the same through gaps in what it sent.
    pub fn stand_in(last: &InputCmd, tick: u32, missing: u32) -> Self {
        if missing > NEUTRAL_AFTER {
            let lockon = last.lockon.filter(|_| missing <= NEUTRAL_AFTER + LOCKON_KEEP);
            return Self { lockon, ..Self::neutral(tick, last.aim, last.buttons) };
        }
        let delta = (last.tick << 4).saturating_sub(last.view_tick_q4);
        Self {
            tick,
            view_tick_q4: (tick << 4).saturating_sub(delta),
            buttons: last.buttons & !buttons::FIRE_MASK,
            ..*last
        }
    }

    #[inline]
    pub fn pressed(&self, b: u16) -> bool {
        self.buttons & b != 0
    }

    /// Thrust demand as floats in [-1, 1].
    #[inline]
    pub fn thrust_vec(&self) -> Vec3 {
        Vec3::new(self.thrust[0] as f32, self.thrust[1] as f32, self.thrust[2] as f32) / 127.0
    }

    #[inline]
    pub fn roll_f32(&self) -> f32 {
        self.roll as f32 / 127.0
    }

    /// The same command after a round trip through the wire format. Clients predict with this so
    /// their simulation sees exactly what the server decodes.
    pub fn quantized(&self) -> Self {
        let mut c = *self;
        c.aim = quantize_dir(self.aim, AIM_BITS);
        let delta = ((self.tick << 4).saturating_sub(self.view_tick_q4)).min((1 << VIEW_DELTA_BITS) - 1);
        c.view_tick_q4 = (self.tick << 4) - delta;
        c.lock_target = self.lock_target.min(NO_SLOT);
        c.buttons = (u32::from(self.buttons) & ((1 << buttons::BITS) - 1)) as u16;
        c.lockon = self.lockon.map(|l| l.quantized());
        c
    }

    fn write_body(&self, w: &mut BitWriter<'_>) {
        let delta = ((self.tick << 4).saturating_sub(self.view_tick_q4)).min((1 << VIEW_DELTA_BITS) - 1);
        w.write_bits(delta, VIEW_DELTA_BITS);
        quant::write_dir(w, self.aim, AIM_BITS);
        for t in self.thrust {
            w.write_bits(u32::from(t as u8), 8);
        }
        w.write_bits(u32::from(self.roll as u8), 8);
        w.write_bits(u32::from(self.buttons), buttons::BITS);
        w.write_bits(u32::from(self.lock_target.min(NO_SLOT)), SLOT_BITS);
        w.write_u8(self.shot_seq);
        w.write_bool(self.lockon.is_some());
        if let Some(l) = &self.lockon {
            l.write(w);
        }
    }

    fn read_body(r: &mut BitReader<'_>, tick: u32) -> Self {
        let delta = r.read_bits(VIEW_DELTA_BITS);
        let aim = quant::read_dir(r, AIM_BITS);
        let thrust = [r.read_u8() as i8, r.read_u8() as i8, r.read_u8() as i8];
        let roll = r.read_u8() as i8;
        let buttons = r.read_bits(buttons::BITS) as u16;
        let lock_target = r.read_bits(SLOT_BITS) as u16;
        let shot_seq = r.read_u8();
        let lockon = r.read_bool().then(|| LockOn::read(r));
        Self {
            tick,
            view_tick_q4: (tick << 4).saturating_sub(delta),
            aim,
            thrust,
            roll,
            buttons,
            lock_target,
            shot_seq,
            lockon,
        }
    }
}

/// Client → server datagram: the newest commands, newest first, with consecutive ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputPacket {
    /// Newest snapshot tick the client has received (drives event redundancy and RTT).
    pub ack_snapshot: u32,
    /// Client clock in milliseconds (mod 2¹⁶), echoed back in snapshots for RTT measurement.
    pub client_time_ms: u16,
    pub count: u8,
    /// `cmds[0]` is the newest; `cmds[i].tick == cmds[0].tick - i`.
    pub cmds: [InputCmd; MAX_CMDS],
}

impl Default for InputPacket {
    fn default() -> Self {
        Self { ack_snapshot: 0, client_time_ms: 0, count: 0, cmds: [InputCmd::default(); MAX_CMDS] }
    }
}

impl InputPacket {
    /// Encodes into `buf`, returning the byte length: at most 65 for four commands (86 + 4 × 107
    /// bits), and 96 if all four fly locked on (86 + 4 × 169). Each command is 8 + 32 + 24 + 8 + 16 +
    /// 10 + 8 + 1 bits: view delta, aim, thrust, roll, buttons, lock target, shot sequence, and
    /// whether a lock-on ([`LockOn`]: 42 + 20 bits) follows.
    pub fn encode(&self, buf: &mut [u8]) -> Option<usize> {
        let count = self.count.clamp(1, MAX_CMDS as u8);
        let mut w = BitWriter::new(buf);
        w.write_bits(PacketKind::Input as u32, PACKET_KIND_BITS);
        w.write_u32(self.ack_snapshot);
        w.write_u16(self.client_time_ms);
        w.write_bits(u32::from(count - 1), 2);
        w.write_u32(self.cmds[0].tick);
        for cmd in &self.cmds[..count as usize] {
            cmd.write_body(&mut w);
        }
        (!w.overflowed()).then(|| w.bytes_written())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = BitReader::new(bytes);
        if r.read_bits(PACKET_KIND_BITS) != PacketKind::Input as u32 {
            return Err(DecodeError::WrongKind);
        }
        let mut p = InputPacket {
            ack_snapshot: r.read_u32(),
            client_time_ms: r.read_u16(),
            count: r.read_bits(2) as u8 + 1,
            ..Default::default()
        };
        let newest = r.read_u32();
        for i in 0..p.count as usize {
            p.cmds[i] = InputCmd::read_body(&mut r, newest.wrapping_sub(i as u32));
        }
        if r.overflowed() {
            return Err(DecodeError::Truncated);
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_round_trip() {
        let mut p = InputPacket { ack_snapshot: 991, client_time_ms: 4242, count: 3, ..Default::default() };
        for i in 0..3 {
            p.cmds[i] = InputCmd {
                tick: 1000 - i as u32,
                view_tick_q4: ((1000 - i as u32) << 4) - 37,
                aim: Vec3::new(0.2, -0.3, 0.93).normalize(),
                thrust: [-127, 5, 127],
                roll: -3,
                buttons: buttons::FIRE_PRIMARY | buttons::ZERO | buttons::JETTISON,
                lock_target: 17,
                shot_seq: 250,
                // One of them locked on, its target at rest.
                lockon: (i == 1).then_some(LockOn { ref_vel: Vec3::new(-412.3, 0.0, 77.7), up: Vec3::Y }),
            }
            .quantized();
        }
        let mut buf = [0u8; 128];
        let n = p.encode(&mut buf).unwrap();
        assert!(n <= 72, "{n}");
        let back = InputPacket::decode(&buf[..n]).unwrap();
        assert_eq!(back.count, 3);
        for i in 0..3 {
            assert_eq!(back.cmds[i], p.cmds[i]);
        }
        assert_eq!(back.ack_snapshot, 991);
        assert_eq!(back.client_time_ms, 4242);
    }

    #[test]
    fn four_commands_fit_in_65_bytes_or_96_locked_on() {
        let mut p =
            InputPacket { ack_snapshot: u32::MAX, client_time_ms: u16::MAX, count: 4, ..Default::default() };
        for i in 0..4 {
            p.cmds[i] = InputCmd {
                tick: u32::MAX - 8 - i as u32,
                view_tick_q4: 0,
                buttons: u16::MAX,
                lock_target: u16::MAX,
                shot_seq: u8::MAX,
                ..InputCmd::default()
            };
        }
        let mut buf = [0u8; 128];
        assert_eq!(p.encode(&mut buf), Some(65));
        for c in &mut p.cmds {
            c.lockon = Some(LockOn { ref_vel: Vec3::splat(-5_000.0), up: Vec3::NEG_Z });
        }
        assert_eq!(p.encode(&mut buf), Some(96));
    }

    #[test]
    fn a_lock_on_round_trips_and_a_still_target_stays_still() {
        let l = LockOn { ref_vel: Vec3::new(0.0, -0.1, 1_999.9), up: Vec3::new(0.3, 0.9, -0.2).normalize() };
        let q = l.quantized();
        // The centred grid: zero is zero, and a tenth of a m/s is under half a step.
        assert_eq!(q.ref_vel.x.to_bits(), 0.0f32.to_bits());
        assert_eq!(q.ref_vel.y, 0.0);
        assert!((q.ref_vel.z - 1_999.9).abs() <= 0.13, "{}", q.ref_vel.z);
        assert!(q.up.angle_between(l.up) < 0.004, "{}", q.up.angle_between(l.up));
        assert!((q.up.length() - 1.0).abs() < 1e-6);
        // Quantizing again changes nothing: the client predicts with what the server decodes.
        assert_eq!(q.quantized(), q);
        // Out of range clamps to the grid's ends.
        let far = LockOn { ref_vel: Vec3::new(9_000.0, -9_000.0, 0.0), up: Vec3::Y }.quantized();
        assert_eq!((far.ref_vel.x, far.ref_vel.y), (LOCKON_VEL_MAX, -LOCKON_VEL_MAX));
    }

    #[test]
    fn a_silent_client_stays_locked_on_for_a_while() {
        let lockon = Some(LockOn { ref_vel: Vec3::new(120.0, 0.0, -40.0), up: Vec3::Y });
        let last = InputCmd {
            tick: 50,
            thrust: [0, 0, 127],
            buttons: buttons::FLIGHT_ASSIST | buttons::FIRE_PRIMARY,
            lock_target: 9,
            lockon,
            ..InputCmd::default()
        };
        // Repeated as it was...
        assert_eq!(InputCmd::stand_in(&last, 53, 3).lockon, lockon);
        // ...then hands off, still keeping station on the target...
        let n = InputCmd::stand_in(&last, 50 + NEUTRAL_AFTER + 1, NEUTRAL_AFTER + 1);
        assert_eq!((n.thrust, n.lockon, n.buttons), ([0; 3], lockon, buttons::FLIGHT_ASSIST));
        let n = InputCmd::stand_in(&last, 50, NEUTRAL_AFTER + LOCKON_KEEP);
        assert_eq!(n.lockon, lockon);
        // ...until it lets that go too.
        assert_eq!(InputCmd::stand_in(&last, 50, NEUTRAL_AFTER + LOCKON_KEEP + 1).lockon, None);
        // Handing a suit to nobody (asleep, launched) is never locked on.
        assert_eq!(InputCmd::neutral(9, Vec3::X, u16::MAX).lockon, None);
    }

    #[test]
    fn a_silent_client_keeps_its_states() {
        let n = InputCmd::neutral(9, Vec3::X, u16::MAX);
        assert_eq!(
            n.buttons,
            buttons::FLIGHT_ASSIST | buttons::ZERO | buttons::GRAB | buttons::MODE | buttons::GRIP
        );
        // A client that stalls never lets go of the surface it stands on.
        let last =
            InputCmd { tick: 9, buttons: buttons::GRIP | buttons::FIRE_PRIMARY, ..InputCmd::default() };
        assert_eq!(InputCmd::stand_in(&last, 11, 2).buttons, buttons::GRIP);
        assert_eq!(InputCmd::stand_in(&last, 30, NEUTRAL_AFTER + 1).buttons, buttons::GRIP);
        // Presses (fire, the special) are never repeated for a silent client.
        const { assert!(buttons::STATES & buttons::FIRE_MASK == 0) };
        const { assert!(buttons::FIRE_MASK & buttons::SPECIAL != 0) };
    }

    #[test]
    fn a_missing_command_repeats_the_last_then_lets_go() {
        let last = InputCmd {
            tick: 100,
            view_tick_q4: (100 << 4) - 37,
            aim: Vec3::X,
            thrust: [12, -40, 127],
            roll: 9,
            buttons: buttons::FIRE_PRIMARY | buttons::BOOST | buttons::FLIGHT_ASSIST | buttons::MELEE,
            lock_target: 7,
            shot_seq: 3,
            lockon: None,
        };
        // Repeated without firing, on the same view delay.
        let r = InputCmd::stand_in(&last, 103, 3);
        assert_eq!(r.tick, 103);
        assert_eq!(r.view_tick_q4, (103 << 4) - 37);
        assert_eq!(r.thrust, last.thrust);
        assert_eq!(r.buttons, buttons::BOOST | buttons::FLIGHT_ASSIST);
        assert_eq!(InputCmd::stand_in(&last, 108, NEUTRAL_AFTER).thrust, last.thrust);
        // Silent for longer: hands off, keeping the aim and the states.
        let n = InputCmd::stand_in(&last, 109, NEUTRAL_AFTER + 1);
        assert_eq!(n, InputCmd::neutral(109, Vec3::X, last.buttons));
        assert_eq!(n.thrust, [0; 3]);
    }

    #[test]
    fn an_old_view_saturates() {
        // A view 20 ticks old is further back than lag compensation reaches: it goes out as the
        // oldest the field can say (15.9 ticks), which the server treats as 8 ticks old.
        let cmd = InputCmd { tick: 500, view_tick_q4: (500 - 20) << 4, ..InputCmd::default() };
        let q = cmd.quantized();
        assert_eq!(q.view_tick_q4, (500 << 4) - 255);
        let mut p = InputPacket { count: 1, ..Default::default() };
        p.cmds[0] = q;
        let mut buf = [0u8; 64];
        let n = p.encode(&mut buf).unwrap();
        assert_eq!(InputPacket::decode(&buf[..n]).unwrap().cmds[0], q);
        // A recent view is exact.
        let fresh = InputCmd { tick: 500, view_tick_q4: (500 << 4) - 37, ..InputCmd::default() };
        assert_eq!(fresh.quantized().view_tick_q4, (500 << 4) - 37);
    }
}
