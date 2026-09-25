//! Property tests: every codec round-trips within half a quantization step, and hostile bytes never
//! panic a decoder.
#![cfg(not(target_arch = "wasm32"))]

use bc_proto::events::Event;
use bc_proto::quant::{self, VEL_MAX};
use bc_proto::snapshot::{ENTITY_BITS, ZERO_HYPOTHESES, ZeroThreat, entity_pos_step};
use bc_proto::{
    EntityState, Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, OwnState, Part, PilotKind,
    SnapshotHeader, SnapshotReader, SnapshotWriter, WeaponKind, ZeroInfo,
};
use glam::{Quat, Vec3};
use proptest::prelude::*;

fn vec3(max: f32) -> impl Strategy<Value = Vec3> {
    (-max..max, -max..max, -max..max).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn unit() -> impl Strategy<Value = Vec3> {
    vec3(1.0).prop_filter("non-degenerate", |v| v.length() > 0.05).prop_map(|v| v.normalize())
}

fn quat() -> impl Strategy<Value = Quat> {
    (unit(), -core::f32::consts::PI..core::f32::consts::PI)
        .prop_map(|(axis, angle)| Quat::from_axis_angle(axis, angle))
}

fn entity() -> impl Strategy<Value = EntityState> {
    (
        0u16..1023,
        0u8..4,
        0u32..4,
        vec3(32_000.0),
        quat(),
        vec3(2000.0),
        unit(),
        0u16..1024,
        prop::array::uniform6(0u8..8),
    )
        .prop_map(|(slot, generation, frame, pos, rot, vel, aim, flags, parts)| EntityState {
            slot,
            generation,
            frame: FrameId::from_bits(frame).unwrap(),
            faction: Faction::Colonies,
            pilot: PilotKind::Agent,
            pos,
            rot,
            vel,
            aim,
            flags,
            parts,
        })
}

proptest! {
    #[test]
    fn positions_within_half_lsb(v in -32_760.0f32..32_760.0) {
        let step = entity_pos_step();
        let back = quant::dequantize_signed(quant::quantize_signed(v, 32_768.0, quant::POS_BITS), 32_768.0, quant::POS_BITS);
        // Half a step, plus the f32 representation error of the result (ulp at 32 km ≈ 4 mm).
        prop_assert!((back - v).abs() <= step * 0.5 + 0.004, "v {} back {} step {}", v, back, step);
    }

    #[test]
    fn input_packet_round_trip(aim in unit(), thrust in prop::array::uniform3(any::<i8>()), roll in any::<i8>(),
                               buttons in 0u16..256, tick in 16u32..u32::MAX / 32, view_back in 0u32..4000,
                               lock in 0u16..1024, shot in any::<u8>(), count in 1u8..=4) {
        let mut p = InputPacket { ack_snapshot: tick - 3, client_time_ms: 777, count, ..Default::default() };
        for i in 0..count as usize {
            let t = tick - i as u32;
            p.cmds[i] = InputCmd { tick: t, view_tick_q4: (t << 4).saturating_sub(view_back), aim, thrust, roll, buttons, lock_target: lock, shot_seq: shot }.quantized();
        }
        let mut buf = [0u8; 128];
        let n = p.encode(&mut buf).unwrap();
        let back = InputPacket::decode(&buf[..n]).unwrap();
        for i in 0..count as usize {
            prop_assert_eq!(back.cmds[i], p.cmds[i]);
        }
    }

    #[test]
    fn snapshot_round_trip(ents in prop::collection::vec(entity(), 0..60), pos in vec3(30_000.0), rot in quat()) {
        let own = OwnState { slot: 5, alive: true, pos, vel: Vec3::new(10.0, -3.0, 250.0), rot, propellant: 812.5,
                             parts: [1.0, 0.5, 0.0, 1.0, 0.25, 0.75], ..OwnState::default() };
        let mut zero = ZeroInfo { threat_count: 2, has_solution: true, solution: Vec3::X, hit_p: 0.62, ..ZeroInfo::default() };
        zero.threats[0] = ZeroThreat { slot: 9, probs: [0.1, 0.2, 0.3, 0.1, 0.1, 0.1, 0.1] };
        let events = [
            Event::BeamSpawn { id: 7, tick: 995, shooter: 3, weapon: WeaponKind::BeamRifle, shot_seq: 4,
                               origin: Vec3::new(100.0, 200.0, -300.0), velocity: Vec3::new(0.0, 0.0, 4000.0) },
            Event::Hit { id: 8, tick: 998, target: 9, part: Part::ArmR, shooter: 3, weapon: WeaponKind::BeamRifle, damage: 0.25 },
            Event::Kill { id: 9, tick: 999, victim: 9, killer: 3 },
            Event::Leave { tick: 1000, slot: 44 },
        ];
        let header = SnapshotHeader { tick: 1000, ack_input_tick: 999, input_health: 2, time_echo_ms: 42, echo_hold_ms: 7, tidi_pct: 100, flags: 0 };
        let mut buf = [0u8; 1500];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&header);
        w.own(Some(&own));
        w.zero(Some(&zero));
        for e in &events { prop_assert!(w.event(e, 0)); }
        w.end_events();
        let mut written = 0;
        for e in &ents { if w.entity(e) { written += 1 } else { break } }
        let n = w.finish().unwrap();
        prop_assert!(n <= MAX_DATAGRAM);
        // The budget must still allow a healthy number of entities.
        prop_assert!(written == ents.len() || written >= 25, "only {} entities fit", written);

        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        prop_assert_eq!(*r.header(), header);
        let o = r.own().unwrap().unwrap();
        prop_assert_eq!(o.pos, own.pos);
        prop_assert_eq!(o.propellant, own.propellant);
        prop_assert!(o.rot.dot(own.rot).abs() > 0.9999);
        let z = r.zero().unwrap().unwrap();
        prop_assert_eq!(z.threat_count, 2);
        prop_assert!((z.hit_p - 0.62).abs() < 0.01);
        let mut got_events = 0;
        while let Some(e) = r.next_event().unwrap() {
            prop_assert_eq!(e.tick(), events[got_events].tick());
            got_events += 1;
        }
        prop_assert_eq!(got_events, events.len());
        let mut i = 0;
        while let Some(e) = r.next_entity().unwrap() {
            let src = &ents[i];
            prop_assert_eq!(e.slot, src.slot);
            prop_assert!((e.pos - src.pos).abs().max_element() <= entity_pos_step() * 0.5 + 0.004);
            prop_assert!((e.vel - src.vel).abs().max_element() <= quant::signed_step(VEL_MAX, quant::VEL_BITS) * 0.5 + 1e-3);
            prop_assert!(e.rot.dot(src.rot).abs() > 0.999);
            prop_assert!(e.aim.dot(src.aim) > 0.995);
            prop_assert_eq!(e.flags, src.flags);
            prop_assert_eq!(e.parts, src.parts);
            i += 1;
        }
        prop_assert_eq!(i, written);
    }

    #[test]
    fn decoders_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..1200)) {
        let _ = InputPacket::decode(&bytes);
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            let _ = r.own();
            let _ = r.zero();
            for _ in 0..64 { if !matches!(r.next_event(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_entity(), Ok(Some(_))) { break } }
        }
        let _ = bc_proto::control::ControlMsg::decode(&bytes);
    }
}

#[test]
fn entity_budget_matches_plan() {
    // ~25.5 bytes per entity; ~30 fit in a datagram next to header, own state, ZERO and events.
    const { assert!(ENTITY_BITS <= 206) };
    const { assert!(ZERO_HYPOTHESES == 7) };
}
