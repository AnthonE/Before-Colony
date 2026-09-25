//! Client → server input.
//!
//! [`InputCmd`] is the one control interface in the game. Humans, external agents and server-side
//! Mobile Dolls all drive their suits by producing it, so nobody gets a privileged API.

use glam::Vec3;

use crate::quant::{self, quantize_dir};
use crate::{BitReader, BitWriter, DecodeError, NO_SLOT, PACKET_KIND_BITS, PacketKind, SLOT_BITS};

/// Button and state bits. Toggles (flight assist, ZERO) are sent as **states**, never as presses,
/// so a lost or duplicated packet can't flip them twice.
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

    pub const FIRE_MASK: u16 = FIRE_PRIMARY | FIRE_SECONDARY | MELEE;
    /// States that persist while a client is silent (see [`InputCmd::neutral`](super::InputCmd::neutral)).
    pub const STATES: u16 = FLIGHT_ASSIST | ZERO | GRAB;
    pub const BITS: u32 = 12;
}

/// Bits per axis for the aim direction (octahedral): ~0.005° precision.
pub const AIM_BITS: u32 = 16;
/// How far behind its own tick a command may place its lag-compensation view (in 1/16 ticks).
pub const VIEW_DELTA_BITS: u32 = 12;
pub const MAX_CMDS: usize = 4;

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
    /// Thrust demand in the suit's local frame: x right, y up, z forward. -127..=127.
    pub thrust: [i8; 3],
    /// Roll rate demand, -127..=127 (positive = clockwise when viewed from behind).
    pub roll: i8,
    /// [`buttons`] bitset.
    pub buttons: u16,
    /// Suit the pilot is locked on to ([`NO_SLOT`] = none). Used by ZERO and the saber lunge.
    pub lock_target: u16,
    /// Increments on every shot fired, so the shooter can match its predicted beams to the server's.
    pub shot_seq: u8,
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
        }
    }
}

impl InputCmd {
    /// A "hands off" command for `tick` that keeps the given aim and states (flight assist, ZERO,
    /// and a grip on whatever is in hand).
    pub fn neutral(tick: u32, aim: Vec3, keep_buttons: u16) -> Self {
        Self {
            tick,
            view_tick_q4: tick << 4,
            aim,
            buttons: keep_buttons & buttons::STATES,
            ..Self::default()
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
        c.buttons &= (1 << buttons::BITS) - 1;
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
    }

    fn read_body(r: &mut BitReader<'_>, tick: u32) -> Self {
        let delta = r.read_bits(VIEW_DELTA_BITS);
        let aim = quant::read_dir(r, AIM_BITS);
        let thrust = [r.read_u8() as i8, r.read_u8() as i8, r.read_u8() as i8];
        let roll = r.read_u8() as i8;
        let buttons = r.read_bits(buttons::BITS) as u16;
        let lock_target = r.read_bits(SLOT_BITS) as u16;
        let shot_seq = r.read_u8();
        Self {
            tick,
            view_tick_q4: (tick << 4).saturating_sub(delta),
            aim,
            thrust,
            roll,
            buttons,
            lock_target,
            shot_seq,
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
    /// Encodes into `buf`, returning the byte length (≤ 64 for four commands: 86 + 4 × 106 bits).
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
            }
            .quantized();
        }
        let mut buf = [0u8; 128];
        let n = p.encode(&mut buf).unwrap();
        assert!(n <= 64, "{n}");
        let back = InputPacket::decode(&buf[..n]).unwrap();
        assert_eq!(back.count, 3);
        for i in 0..3 {
            assert_eq!(back.cmds[i], p.cmds[i]);
        }
        assert_eq!(back.ack_snapshot, 991);
        assert_eq!(back.client_time_ms, 4242);
    }

    #[test]
    fn four_commands_fit_in_64_bytes() {
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
        assert_eq!(p.encode(&mut buf), Some(64));
    }

    #[test]
    fn a_silent_client_keeps_its_grip() {
        let n = InputCmd::neutral(9, Vec3::X, u16::MAX);
        assert_eq!(n.buttons, buttons::FLIGHT_ASSIST | buttons::ZERO | buttons::GRAB);
    }
}
