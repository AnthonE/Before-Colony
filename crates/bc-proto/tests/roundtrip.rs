//! Property tests: every codec round-trips within half a quantization step, and hostile bytes never
//! panic a decoder.
#![cfg(not(target_arch = "wasm32"))]

use bc_proto::events::{BurstCause, Event};
use bc_proto::missiles::{MISSILE_RECORD_BITS, MISSILE_VEL_BITS, MISSILE_VEL_MAX};
use bc_proto::objects::{ROCK_RECORD_BITS, SPIN_MAX};
use bc_proto::quant::{self, VEL_MAX};
use bc_proto::snapshot::{ENTITY_BITS, OWN_BITS, ZERO_HYPOTHESES, ZeroThreat, entity_pos_step};
use bc_proto::{
    ChunkDesc, ChunkKind, EntityState, Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, MissileState,
    NO_CHUNK, ObjectState, OwnState, Part, PilotKind, RockState, Segment, SnapshotHeader, SnapshotReader,
    SnapshotWriter, WeaponKind, ZeroInfo,
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
        0u16..4096,
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

fn missile() -> impl Strategy<Value = MissileState> {
    (0u16..1024, 0u8..4, 0u32..WeaponKind::COUNT as u32, any::<[bool; 3]>(), vec3(32_000.0), vec3(4_000.0))
        .prop_map(|(id, generation, kind, [guided, targets_you, friendly], pos, vel)| MissileState {
            id,
            generation,
            kind: WeaponKind::from_bits(kind).unwrap(),
            guided,
            targets_you,
            friendly,
            pos,
            vel,
        })
}

fn desc() -> impl Strategy<Value = ChunkDesc> {
    (0u32..3, 0u8..4, 0u32..4, 0u32..3, 0u32..6, 0u8..64, any::<u8>(), 0u32..4096).prop_map(
        |(class, ore, frame, faction, part, parts, seed, tens)| {
            let frame = FrameId::from_bits(frame).unwrap();
            let faction = Faction::from_bits(faction);
            let kind = match class {
                0 => ChunkKind::Ore { ore },
                1 => ChunkKind::Limb { frame, faction, part: Part::from_bits(part).unwrap() },
                _ => ChunkKind::Hulk { frame, faction, parts },
            };
            ChunkDesc { kind, seed, mass_kg: tens * 10 }
        },
    )
}

fn object() -> impl Strategy<Value = ObjectState> {
    (0u32..3, 0u16..1023, 0u8..4, desc(), 0u32..9_000, vec3(30_000.0), vec3(2_000.0), quat(), vec3(SPIN_MAX))
        .prop_map(|(kind, id, generation, desc, age, pos, vel, rot, spin)| match kind {
            0 => ObjectState::Gone { id },
            1 => ObjectState::Free {
                id,
                generation,
                desc,
                seg: Segment { t0: 10_000 - age, pos, vel, rot, spin },
            },
            _ => ObjectState::Held {
                id,
                generation,
                desc,
                holder: id % 1000,
                right: age % 2 == 0,
                rot,
                since: 10_000 - age,
            },
        })
}

/// A decoded object matches what was sent within half a step of each quantity.
fn object_close(back: &ObjectState, sent: &ObjectState) -> bool {
    match (back, sent) {
        (ObjectState::Gone { id: a }, ObjectState::Gone { id: b }) => a == b,
        (
            ObjectState::Free { id, generation, desc, seg },
            ObjectState::Free { id: i2, generation: g2, desc: d2, seg: s2 },
        ) => {
            id == i2
                && generation == g2
                && desc == d2
                && seg.t0 == s2.t0
                && (seg.pos - s2.pos).abs().max_element() <= entity_pos_step() * 0.5 + 0.004
                && (seg.vel - s2.vel).abs().max_element()
                    <= quant::signed_step(VEL_MAX, quant::VEL_BITS) * 0.5 + 1e-3
                && seg.rot.dot(s2.rot).abs() > 0.999
                && (seg.spin - s2.spin).abs().max_element() <= quant::signed_step(SPIN_MAX, 10) * 0.5 + 1e-4
                && *seg == s2.quantized()
        }
        (
            ObjectState::Held { id, generation, desc, holder, right, rot, since },
            ObjectState::Held { id: i2, generation: g2, desc: d2, holder: h2, right: r2, rot: q2, since: s2 },
        ) => {
            id == i2
                && generation == g2
                && desc == d2
                && holder == h2
                && right == r2
                && since == s2
                && rot.dot(*q2).abs() > 0.995
        }
        _ => false,
    }
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
                               buttons in any::<u16>(), tick in 16u32..u32::MAX / 32, view_back in 0u32..4000,
                               lock in 0u16..1024, shot in any::<u8>(), count in 1u8..=4) {
        let mut p = InputPacket { ack_snapshot: tick - 3, client_time_ms: 777, count, ..Default::default() };
        for i in 0..count as usize {
            let t = tick - i as u32;
            p.cmds[i] = InputCmd { tick: t, view_tick_q4: (t << 4).saturating_sub(view_back), aim, thrust, roll, buttons, lock_target: lock, shot_seq: shot }.quantized();
        }
        let mut buf = [0u8; 128];
        let n = p.encode(&mut buf).unwrap();
        prop_assert!(n <= 64, "{} bytes", n);
        let back = InputPacket::decode(&buf[..n]).unwrap();
        for i in 0..count as usize {
            prop_assert_eq!(back.cmds[i], p.cmds[i]);
        }
    }

    #[test]
    fn snapshot_round_trip(ents in prop::collection::vec(entity(), 0..60), objs in prop::collection::vec(object(), 0..12),
                           missiles in prop::collection::vec(missile(), 0..=12),
                           pos in vec3(30_000.0), rot in quat(), extra in -131_071i32..131_071, credits in 0u32..16_777_215,
                           lock in 0u16..1024, progress in 0u8..16, special in any::<[u8; 2]>(), ready in 0u8..16) {
        let own = OwnState { slot: 5, alive: true, pos, vel: Vec3::new(10.0, -3.0, 250.0), rot, propellant: 812.5,
                             parts: [1.0, 0.5, 0.0, 1.0, 0.25, 0.75], extra_mass_kg: extra, cargo_kg: [0, 16_383, 2_500, 1],
                             credits, held: 1_000, weapon_ready: ready, lock_target: lock, lock_progress: progress,
                             special_timer: special[0], special_cooldown: special[1], ..OwnState::default() };
        let mut zero = ZeroInfo { threat_count: 2, has_solution: true, solution: Vec3::X, hit_p: 0.62, ..ZeroInfo::default() };
        zero.threats[0] = ZeroThreat { slot: 9, probs: [0.1, 0.2, 0.3, 0.1, 0.1, 0.1, 0.1] };
        let events = [
            Event::BeamSpawn { id: 7, tick: 9_995, shooter: 3, weapon: WeaponKind::BeamRifle, shot_seq: 4,
                               origin: Vec3::new(100.0, 200.0, -300.0), velocity: Vec3::new(0.0, 0.0, 4000.0) },
            Event::Hit { id: 8, tick: 9_998, target: 9, part: Part::ArmR, shooter: 3, weapon: WeaponKind::BeamRifle, damage: 0.25 },
            Event::Kill { id: 9, tick: 9_999, victim: 9, killer: 3, hulk: NO_CHUNK },
            Event::Detach { id: 10, tick: 9_999, source: 9, from_hulk: false, part: Part::ArmL, chunk: 55 },
            Event::Leave { tick: 10_000, slot: 44 },
            Event::MissileBurst { id: 11, tick: 10_000, missile: 12, pos: Vec3::ZERO, cause: BurstCause::Hit },
        ];
        let rocks = [RockState::new(0, false, 0.5, 0.9), RockState::new(1_022, true, 0.0, 0.0)];
        let header = SnapshotHeader { tick: 10_000, ack_input_tick: 999, input_health: 2, time_echo_ms: 42, echo_hold_ms: 7, tidi_pct: 100, flags: 0 };
        let mut buf = [0u8; 1500];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&header);
        w.own(Some(&own));
        w.zero(Some(&zero));
        for e in &events { prop_assert!(w.event(e, 0)); }
        for r in &rocks { prop_assert!(w.rock(r, 0)); }
        // Missiles leave room for twenty entities and six of the largest objects.
        let reserve = 6 * (ObjectState::MAX_BITS + 1);
        for m in &missiles { prop_assert!(w.missile(m, reserve + 20 * (ENTITY_BITS + 1))); }
        // Entities leave room for six of the largest objects.
        let mut written = 0;
        for e in &ents { if w.entity(e, reserve) { written += 1 } else { break } }
        let mut objects_written = 0;
        for o in &objs { if w.object(o) { objects_written += 1 } else { break } }
        let n = w.finish().unwrap();
        prop_assert!(n <= MAX_DATAGRAM);
        // The budget must still allow a healthy number of entities, and the objects reserved for.
        prop_assert!(written == ents.len() || written >= 20, "only {} entities fit", written);
        prop_assert!(objects_written >= objs.len().min(6), "only {} objects fit", objects_written);

        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        prop_assert_eq!(*r.header(), header);
        let o = r.own().unwrap().unwrap();
        prop_assert_eq!(o.pos, own.pos);
        prop_assert_eq!(o.propellant, own.propellant);
        prop_assert_eq!((o.extra_mass_kg, o.cargo_kg, o.credits, o.held), (extra, own.cargo_kg, credits, 1_000));
        prop_assert_eq!((o.weapon_ready, o.lock_target, o.lock_progress), (ready, lock, progress));
        prop_assert_eq!((o.special_timer, o.special_cooldown), (special[0], special[1]));
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
        let mut got_rocks = 0;
        while let Some(rock) = r.next_rock().unwrap() {
            prop_assert_eq!(rock, rocks[got_rocks]);
            got_rocks += 1;
        }
        prop_assert_eq!(got_rocks, rocks.len());
        let mut got_missiles = 0;
        while let Some(m) = r.next_missile().unwrap() {
            let src = &missiles[got_missiles];
            prop_assert_eq!((m.id, m.generation, m.kind), (src.id, src.generation, src.kind));
            prop_assert_eq!((m.guided, m.targets_you, m.friendly), (src.guided, src.targets_you, src.friendly));
            prop_assert!((m.pos - src.pos).abs().max_element() <= entity_pos_step() * 0.5 + 0.004);
            prop_assert!((m.vel - src.vel).abs().max_element() <= quant::signed_step(MISSILE_VEL_MAX, MISSILE_VEL_BITS) * 0.5 + 1e-3);
            got_missiles += 1;
        }
        prop_assert_eq!(got_missiles, missiles.len());
        let mut i = 0;
        while let Some(e) = r.next_entity().unwrap() {
            let src = &ents[i];
            prop_assert_eq!((e.slot, e.generation, e.frame), (src.slot, src.generation, src.frame));
            prop_assert!((e.pos - src.pos).abs().max_element() <= entity_pos_step() * 0.5 + 0.004);
            prop_assert!((e.vel - src.vel).abs().max_element() <= quant::signed_step(VEL_MAX, quant::VEL_BITS) * 0.5 + 1e-3);
            prop_assert!(e.rot.dot(src.rot).abs() > 0.999);
            prop_assert!(e.aim.dot(src.aim) > 0.995);
            prop_assert_eq!(e.flags, src.flags);
            prop_assert_eq!(e.parts, src.parts);
            i += 1;
        }
        prop_assert_eq!(i, written);
        let mut k = 0;
        while let Some(o) = r.next_object().unwrap() {
            prop_assert!(object_close(&o, &objs[k]), "{:?} vs {:?}", o, objs[k]);
            k += 1;
        }
        prop_assert_eq!(k, objects_written);
    }

    #[test]
    fn decoders_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..1200)) {
        let _ = InputPacket::decode(&bytes);
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            let _ = r.own();
            let _ = r.zero();
            for _ in 0..64 { if !matches!(r.next_event(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_rock(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_missile(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_entity(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_object(), Ok(Some(_))) { break } }
        }
        // Straight to a later list, skipping the earlier ones.
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            for _ in 0..64 { if !matches!(r.next_object(), Ok(Some(_))) { break } }
        }
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            for _ in 0..64 { if !matches!(r.next_missile(), Ok(Some(_))) { break } }
        }
        let _ = bc_proto::control::ControlMsg::decode(&bytes);
    }
}

#[test]
fn record_budgets_match_plan() {
    // ~26 bytes per entity; ~30 fit in a datagram next to header, own state, ZERO and events.
    const { assert!(ENTITY_BITS == 206) };
    const { assert!(ZERO_HYPOTHESES == 7) };
    const { assert!(OWN_BITS == 641) };
    const { assert!(ROCK_RECORD_BITS == 18) };
    const { assert!(MISSILE_RECORD_BITS == 119) };
    const { assert!(ObjectState::MAX_BITS <= 232) };
}

#[test]
fn every_enum_value_round_trips_and_nothing_else_decodes() {
    for (i, k) in WeaponKind::ALL.iter().enumerate() {
        assert_eq!(WeaponKind::from_bits(i as u32), Some(*k));
        assert_eq!(k.index(), i);
    }
    for v in WeaponKind::COUNT as u32..1 << WeaponKind::BITS {
        assert_eq!(WeaponKind::from_bits(v), None);
    }
    for (i, f) in FrameId::ALL.iter().enumerate() {
        assert_eq!(FrameId::from_bits(i as u32), Some(*f));
    }
    for v in FrameId::COUNT as u32..1 << FrameId::BITS {
        assert_eq!(FrameId::from_bits(v), None);
    }
}
