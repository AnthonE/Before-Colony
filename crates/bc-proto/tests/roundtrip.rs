//! Property tests: every codec round-trips within half a quantization step, and hostile bytes never
//! panic a decoder.
#![cfg(not(target_arch = "wasm32"))]

use bc_proto::events::{BurstCause, Event};
use bc_proto::missiles::{MISSILE_RECORD_BITS, MISSILE_VEL_BITS, MISSILE_VEL_MAX};
use bc_proto::objects::{ROCK_RECORD_BITS, SPIN_MAX};
use bc_proto::quant::{self, VEL_MAX};
use bc_proto::snapshot::{
    ENTITY_MAX_BITS, ORRERY_HYPOTHESES, OWN_BITS_FREE, OWN_MAX_BITS, OrreryThreat, OwnArms, OwnBurst,
    entity_pos_step, footing,
};
use bc_proto::types::{RIDER_VEL_BITS, RIDER_VEL_MAX};
use bc_proto::{
    BodyRef, ChunkDesc, ChunkKind, Delta, EntityState, Faction, FrameId, InputCmd, InputPacket, LockOn,
    MAX_DATAGRAM, MissileState, NO_CHUNK, ObjectState, OrreryInfo, OwnState, OwnSurface, Part, PilotKind,
    QEntity, RiderOn, RockState, Segment, SnapshotHeader, SnapshotReader, SnapshotWriter, WeaponKind,
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

/// A body a rider can name: kind 0 a rock, 1 a landmark.
fn body_ref(kind: u32, id: u16) -> BodyRef {
    if kind == 0 { BodyRef::Rock(id % 1024) } else { BodyRef::Landmark((id % 16) as u8) }
}

/// A suit flying free (`on.0 == 0`) or riding a body: anywhere in the sector, or anywhere a rider
/// can be over its body.
fn entity() -> impl Strategy<Value = EntityState> {
    (
        0u16..1023,
        0u8..4,
        0u32..FrameId::COUNT as u32,
        (0u32..3, 0u16..1024, any::<bool>()),
        vec3(1.0),
        quat(),
        vec3(1.0),
        unit(),
        0u16..8192,
        prop::array::uniform6(0u8..8),
    )
        .prop_map(|(slot, generation, frame, (kind, id, aloft), pos, rot, vel, aim, flags, parts)| {
            let on = (kind > 0).then(|| RiderOn { body: body_ref(kind - 1, id), aloft });
            let (reach, speed) = on.map_or((32_000.0, 2_000.0), |on| (on.body.local_max(), RIDER_VEL_MAX));
            EntityState {
                slot,
                generation,
                frame: FrameId::from_bits(frame).unwrap(),
                faction: Faction::Colonies,
                pilot: PilotKind::Agent,
                on,
                pos: pos * reach,
                rot,
                vel: vel * speed,
                aim,
                flags,
                parts,
            }
        })
}

/// A decoded entity's pose matches what was sent within half a step: of the sector's grid, or of
/// its body's.
fn entity_pose_close(e: &EntityState, src: &EntityState) -> bool {
    let (pos_step, vel_step) = match src.on {
        None => (entity_pos_step(), quant::signed_step(VEL_MAX, quant::VEL_BITS)),
        Some(on) => (
            quant::signed_step(on.body.local_max(), on.body.local_bits()),
            quant::centered_step(RIDER_VEL_MAX, RIDER_VEL_BITS),
        ),
    };
    e.on == src.on
        && (e.pos - src.pos).abs().max_element() <= pos_step * 0.5 + 0.004
        && (e.vel - src.vel).abs().max_element() <= vel_step * 0.5 + 1e-3
        && e.rot.dot(src.rot).abs() > 0.999
}

fn burst() -> impl Strategy<Value = OwnBurst> {
    (0u8..16, 0u8..64, prop::array::uniform3(-1i8..=1), any::<bool>())
        .prop_map(|(left, cooldown, dir, held)| OwnBurst { left, cooldown, dir, held })
}

fn arms() -> impl Strategy<Value = OwnArms> {
    (
        0u8..4,
        0u8..32,
        0u8..4,
        0u8..8,
        prop::array::uniform4(0u8..64),
        prop::array::uniform2(0u8..8),
        prop::array::uniform2(0u8..4),
    )
        .prop_map(|(phase, timer, slot, fired_ago, wait, salvo, salvo_gap)| OwnArms {
            phase,
            timer,
            slot,
            fired_ago,
            wait,
            salvo,
            salvo_gap,
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
    (0u32..3, 0u8..4, 0u32..FrameId::COUNT as u32, 0u32..3, 0u32..6, 0u8..64, any::<u8>(), 0u32..4096)
        .prop_map(|(class, ore, frame, faction, part, parts, seed, tens)| {
            let frame = FrameId::from_bits(frame).unwrap();
            let faction = Faction::from_bits(faction);
            let kind = match class {
                0 => ChunkKind::Ore { ore },
                1 => ChunkKind::Limb { frame, faction, part: Part::from_bits(part).unwrap() },
                _ => ChunkKind::Hulk { frame, faction, parts },
            };
            ChunkDesc { kind, seed, mass_kg: tens * 10 }
        })
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
                               lock in 0u16..1024, shot in any::<u8>(), count in 1u8..=4,
                               locked in prop::array::uniform4(any::<bool>()), ref_vel in vec3(2_500.0), up in unit(),
                               converge in prop::array::uniform4(any::<u16>())) {
        let mut p = InputPacket { ack_stream: 65, ack_seq: tick as u16, ack_bits: tick, client_time_ms: 777, count, ..Default::default() };
        for (i, &on) in locked.iter().enumerate().take(count as usize) {
            let t = tick - i as u32;
            let lockon = on.then_some(LockOn { ref_vel, up });
            p.cmds[i] = InputCmd { tick: t, view_tick_q4: (t << 4).saturating_sub(view_back), aim, thrust, roll, buttons, lock_target: lock, shot_seq: shot, lockon, converge: converge[i], act: None }.quantized();
        }
        let mut buf = [0u8; 128];
        let n = p.encode(&mut buf).unwrap();
        prop_assert!(n <= 108, "{} bytes", n);
        if !locked[..count as usize].contains(&true) && converge[..count as usize].iter().all(|&c| c == 0) {
            prop_assert!(n <= 69, "{} bytes", n);
        }
        let back = InputPacket::decode(&buf[..n]).unwrap();
        for i in 0..count as usize {
            prop_assert_eq!(back.cmds[i], p.cmds[i]);
        }
    }

    #[test]
    fn snapshot_round_trip(ents in prop::collection::vec(entity(), 0..60), objs in prop::collection::vec(object(), 0..12),
                           missiles in prop::collection::vec(missile(), 0..=12),
                           pos in vec3(30_000.0), rot in quat(), extra in -131_071i32..131_071, credits in 0u32..16_777_215,
                           lock in 0u16..1024, progress in 0u8..16, special in any::<[u8; 2]>(), ready in 0u8..16,
                           arms in arms(), burst in burst(), g_strain in 0.0f32..3.0, on in 0u32..3, stance in 96u8..=146, cover in 0u8..4,
                           systems in 0u32..(1 << 24), modules in 0u32..(1 << 20),
                           timers in any::<[u8; 3]>(), repairing in 0u8..16, kits in any::<u8>(), stim in 0u16..4_096,
                           fed in 0u8..=4) {
        // Flying free, standing on a rock, or in a landmark's grip.
        let surface = match on {
            0 => None,
            1 => Some(OwnSurface { footing: footing::GROUNDED, body: body_ref(0, lock), stance_q: stance }),
            _ => Some(OwnSurface { footing: footing::ALOFT, body: body_ref(1, lock), stance_q: stance }),
        };
        let own = OwnState { slot: 5, alive: true, pos, vel: Vec3::new(10.0, -3.0, 250.0), rot, propellant: 812.5,
                             g_strain, parts: [1.0, 0.5, 0.0, 1.0, 0.25, 0.75], extra_mass_kg: extra, cargo_kg: [0, 8_191, 2_500, 1],
                             credits, held: 1_000, weapon_ready: ready, lock_target: lock, lock_progress: progress,
                             special_timer: special[0], special_charge: special[1], arms, burst, surface, cover, systems, modules,
                             scram: timers[0] & 127, concussed: timers[1] & 127, repairing, repair_left: timers[2] & 127, kits, stim, fed, grade: 2,
                             ..OwnState::default() };
        let mut orrery = OrreryInfo { threat_count: 2, has_solution: true, solution: Vec3::X, hit_p: 0.62, ..OrreryInfo::default() };
        orrery.threats[0] = OrreryThreat { slot: 9, probs: [0.1, 0.2, 0.3, 0.1, 0.1, 0.1, 0.1] };
        let events = [
            Event::BeamSpawn { id: 7, tick: 9_995, shooter: 3, weapon: WeaponKind::BeamRifle, shot_seq: 4,
                               origin: Vec3::new(100.0, 200.0, -300.0), velocity: Vec3::new(0.0, 0.0, 4000.0) },
            Event::Hit { id: 8, tick: 9_998, target: 9, part: Part::ArmR, shooter: 3, weapon: WeaponKind::BeamRifle, damage: 0.25 },
            Event::Kill { id: 9, tick: 9_999, victim: 9, killer: 3, weapon: Some(WeaponKind::BeamRifle), hulk: NO_CHUNK },
            Event::Detach { id: 10, tick: 9_999, source: 9, from_hulk: false, part: Part::ArmL, chunk: 55 },
            Event::Leave { tick: 10_000, slot: 44 },
            Event::MissileBurst { id: 11, tick: 10_000, missile: 12, pos: Vec3::ZERO, cause: BurstCause::Hit },
        ];
        let rocks = [RockState::new(0, false, 0.5, 0.9), RockState::new(1_022, true, 0.0, 0.0)];
        let header = SnapshotHeader { tick: 10_000, seq: 0xBEEF, stream: 70, ack_input_tick: 999, input_health: 2, time_echo_ms: 42, echo_hold_ms: 7, tidi_pct: 100, flags: 0 };
        let mut buf = [0u8; 1500];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&header);
        w.own(Some(&own));
        w.orrery(Some(&orrery));
        for e in &events { prop_assert!(w.event(e, 0)); }
        for r in &rocks { prop_assert!(w.rock(r, 0)); }
        // Missiles leave room for twenty entities and six of the largest objects.
        let reserve = 6 * (ObjectState::MAX_BITS + 1);
        for m in &missiles { prop_assert!(w.missile(m, reserve + 20 * (ENTITY_MAX_BITS + 1))); }
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
        // The strain comes back exact, so the client blacks out on the same tick as the server.
        prop_assert_eq!(o.g_strain, g_strain);
        prop_assert_eq!(o.arms, arms);
        prop_assert_eq!(o.burst, burst);
        prop_assert_eq!((o.extra_mass_kg, o.cargo_kg, o.credits, o.held), (extra, own.cargo_kg, credits, 1_000));
        prop_assert_eq!((o.weapon_ready, o.lock_target, o.lock_progress), (ready, lock, progress));
        prop_assert_eq!((o.special_timer, o.special_charge), (special[0], special[1]));
        prop_assert_eq!((o.kits, o.stim, o.fed), (kits, stim, fed));
        prop_assert_eq!(o.grade, 2);
        prop_assert_eq!((o.surface, o.cover), (surface, cover));
        // In a body's frame or the sector's, the velocity comes back exact too.
        prop_assert_eq!(o.vel, own.vel);
        prop_assert!(o.rot.dot(own.rot).abs() > 0.9999);
        let z = r.orrery().unwrap().unwrap();
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
            prop_assert!(entity_pose_close(&e, src), "{:?} vs {:?}", e, src);
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
    fn attached_entities_round_trip_within_half_a_step(kind in 0u32..2, id in 0u16..1024, aloft in any::<bool>(),
                                                       pos in vec3(1.0), vel in vec3(1.0), rot in quat(), aim in unit()) {
        let body = body_ref(kind, id);
        let e = EntityState {
            on: Some(RiderOn { body, aloft }),
            pos: pos * body.local_max(),
            vel: vel * RIDER_VEL_MAX,
            rot,
            aim,
            ..EntityState::default()
        };
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, 256);
        w.header(&SnapshotHeader::default());
        w.own(None);
        w.orrery(None);
        prop_assert!(w.entity(&e, 0));
        let n = w.finish().unwrap();
        let back = SnapshotReader::new(&buf[..n]).unwrap().next_entity().unwrap().unwrap();
        prop_assert_eq!(back.on, Some(RiderOn { body, aloft }));
        // 7.8 mm in the body's frame on any body, 3.2 cm/s over it.
        prop_assert!((back.pos - e.pos).abs().max_element() <= 0.0079, "{} vs {}", back.pos, e.pos);
        prop_assert!((back.vel - e.vel).abs().max_element() <= 0.032, "{} vs {}", back.vel, e.vel);
        prop_assert!(back.rot.dot(e.rot).abs() > 0.999);
        prop_assert!(back.aim.dot(e.aim) > 0.995);
        // At rest on its body, it comes out exactly at rest.
        let still = EntityState { vel: Vec3::ZERO, ..e };
        let mut buf = [0u8; 256];
        let mut w = SnapshotWriter::new(&mut buf, 256);
        w.header(&SnapshotHeader::default());
        w.own(None);
        w.orrery(None);
        prop_assert!(w.entity(&still, 0));
        let n = w.finish().unwrap();
        let back = SnapshotReader::new(&buf[..n]).unwrap().next_entity().unwrap().unwrap();
        prop_assert_eq!(back.vel.to_array().map(f32::to_bits), [0; 3]);
    }

    #[test]
    fn decoders_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..1200)) {
        let _ = InputPacket::decode(&bytes);
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            let _ = r.own();
            let _ = r.orrery();
            for _ in 0..64 { if !matches!(r.next_event(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_rock(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_missile(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_entity(), Ok(Some(_))) { break } }
            for _ in 0..64 { if !matches!(r.next_object(), Ok(Some(_))) { break } }
            let _ = r.foot_own();
            for _ in 0..64 { if !matches!(r.next_person(|_, _| None), Ok(Some(_))) { break } }
        }
        // Straight to a later list, skipping the earlier ones.
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            for _ in 0..64 { if !matches!(r.next_object(), Ok(Some(_))) { break } }
        }
        // A walker's sections, straight to them, with and without baselines held.
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            let _ = r.foot_own();
            for _ in 0..64 { if !matches!(r.next_person(|id, _| Some(bc_proto::QPerson { id, ..Default::default() })), Ok(Some(_))) { break } }
        }
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            for _ in 0..64 { if !matches!(r.next_person(|_, _| None), Ok(Some(_))) { break } }
        }
        // (A walker's acts are `InputPacket::decode`'s above; the Hello's and Welcome's flags, `ControlMsg::decode`'s below.)
        if let Ok(mut r) = SnapshotReader::new(&bytes) {
            for _ in 0..64 { if !matches!(r.next_missile(), Ok(Some(_))) { break } }
        }
        let _ = bc_proto::control::ControlMsg::decode(&bytes);
        let _ = bc_proto::control::Frame::decode(&bytes);
    }
}

/// Writes `cur` (as a delta against `base`, `back` ticks older, if one can be taken) in a snapshot of
/// tick 1 000, reads it back with `base` as the only baseline held, and checks the bits it took.
fn through_the_wire(cur: &QEntity, base: &QEntity, back: u32) -> (QEntity, bool) {
    let delta = Delta::plan(cur, base, back);
    let mut buf = [0u8; 1500];
    let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
    w.header(&SnapshotHeader { tick: 1_000, ..SnapshotHeader::default() });
    w.own(None);
    w.orrery(None);
    // Another suit first, whole, so the list is open when the bits are counted.
    let other = QEntity { slot: cur.slot ^ 1, ..*cur };
    w.entity_q(&other, None, 0).expect("it fits");
    let before = w.bits_remaining();
    let bits = w.entity_q(cur, delta.as_ref(), 0).expect("it fits");
    assert_eq!(before - w.bits_remaining(), bits + 1, "the record took the bits it said");
    assert_eq!(bits, delta.map_or(cur.absolute_bits(), |d| d.bits()));
    let n = w.finish().unwrap();
    let mut r = SnapshotReader::new(&buf[..n]).unwrap();
    let held = (base.slot, 1_000 - back);
    assert_eq!(r.next_entity_q(|_, _| None), Ok(Some(other)));
    let q = r
        .next_entity_q(|slot, tick| ((slot, tick) == held).then_some(*base))
        .expect("it reads")
        .expect("a record");
    assert_eq!(r.next_entity_q(|_, _| None), Ok(None));
    (q, delta.is_some())
}

/// `e` as the same suit, life and body as `base` (but whether it's aloft).
fn same_suit(e: EntityState, base: &EntityState) -> EntityState {
    let on = base.on.map(|b| RiderOn { body: b.body, aloft: e.on.is_some_and(|o| o.aloft) });
    let (reach, speed) = on.map_or((32_000.0, 2_000.0), |on| (on.body.local_max(), RIDER_VEL_MAX));
    let (pos, vel) = match (e.on.is_some(), on.is_some()) {
        (a, b) if a == b => (e.pos, e.vel),
        _ => (e.pos.normalize_or_zero() * reach * 0.5, e.vel.normalize_or_zero() * speed * 0.5),
    };
    EntityState {
        slot: base.slot,
        generation: base.generation,
        frame: base.frame,
        faction: base.faction,
        pilot: base.pilot,
        on,
        pos,
        vel,
        ..e
    }
}

proptest! {
    #[test]
    fn a_whole_record_is_its_codes(e in entity()) {
        let q = QEntity::quantize(&e);
        let mut buf = [0u8; 1500];
        let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
        w.header(&SnapshotHeader::default());
        w.own(None);
        w.orrery(None);
        prop_assert!(w.entity(&e, 0));
        let n = w.finish().unwrap();
        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        prop_assert_eq!(r.next_entity().unwrap().unwrap(), q.state());
        prop_assert_eq!(QEntity::quantize(&q.state()), q);
    }

    /// Anything against anything of the same suit: whatever the modes, the client rebuilds it exactly.
    #[test]
    fn a_delta_rebuilds_its_record_exactly(base in entity(), other in entity(), back in 1u32..=255) {
        let cur = same_suit(other, &base);
        let (b, c) = (QEntity::quantize(&base), QEntity::quantize(&cur));
        let (got, delta) = through_the_wire(&c, &b, back);
        prop_assert!(delta);
        prop_assert_eq!(got, c);
    }

    /// A suit a few ticks on from its baseline, as suits are: carried on by its velocity, turned a
    /// little, its aim moved. Exact, and smaller than whole.
    #[test]
    fn a_suit_moving_on_is_small(base in entity(), dv in vec3(30.0), turn in vec3(0.2), aim in vec3(0.1),
                                 back in 1u32..=8, flags in any::<bool>(), parts in any::<bool>()) {
        let dt = back as f32 / 30.0;
        let speed = if base.on.is_some() { 1.0 / 16.0 } else { 1.0 };
        let vel = base.vel + dv * speed;
        let cur = EntityState {
            pos: base.pos + (base.vel + vel) * 0.5 * dt,
            vel,
            rot: (Quat::from_scaled_axis(turn) * base.rot).normalize(),
            aim: (base.aim + aim).normalize(),
            flags: if flags { base.flags ^ 1 } else { base.flags },
            parts: if parts { [3; 6] } else { base.parts },
            ..base
        };
        let (b, c) = (QEntity::quantize(&base), QEntity::quantize(&cur));
        let (got, _) = through_the_wire(&c, &b, back);
        prop_assert_eq!(got, c);
        let d = Delta::plan(&c, &b, back).unwrap();
        prop_assert!(d.bits() < c.absolute_bits(), "{} bits of {}", d.bits(), c.absolute_bits());
    }
}

#[test]
fn a_suit_at_rest_on_its_body_is_30_bits() {
    let rider = EntityState {
        on: Some(RiderOn { body: BodyRef::Landmark(2), aloft: false }),
        pos: Vec3::new(100.0, -40.0, 3.0),
        ..EntityState::default()
    };
    let q = QEntity::quantize(&rider);
    assert_eq!(Delta::plan(&q, &q, 5).unwrap().bits(), 30);
    assert_eq!(through_the_wire(&q, &q, 5), (q, true));
    // Drifting in space at 120 m/s: carried on, it's where it was said it'd be.
    let free = EntityState {
        pos: Vec3::new(9_000.0, 50.0, -1_200.0),
        vel: Vec3::new(120.0, 0.0, -35.5),
        ..EntityState::default()
    };
    let later = EntityState { pos: free.pos + free.vel * (4.0 / 30.0), ..free };
    let (b, c) = (QEntity::quantize(&free), QEntity::quantize(&later));
    let d = Delta::plan(&c, &b, 4).unwrap();
    // Within a code or two of where it was carried to (each end of it is rounded to the grid):
    // the head and three small differences.
    assert_eq!(d.bits(), 29 + 3 * 6, "{d:?}");
    assert_eq!(through_the_wire(&c, &b, 4), (c, true));
}

#[test]
fn no_delta_across_suits_lives_or_bodies_or_too_far_back() {
    let a = QEntity::quantize(&EntityState::default());
    assert!(Delta::plan(&a, &a, 0).is_none());
    assert!(Delta::plan(&a, &a, 256).is_none());
    assert!(Delta::plan(&QEntity { slot: 3, ..a }, &a, 1).is_none());
    assert!(Delta::plan(&QEntity { generation: 1, ..a }, &a, 1).is_none());
    let on = |body| QEntity { on: Some(RiderOn { body, aloft: false }), ..a };
    assert!(Delta::plan(&on(BodyRef::Rock(1)), &on(BodyRef::Rock(2)), 1).is_none());
    assert!(Delta::plan(&on(BodyRef::Rock(1)), &a, 1).is_none());
    assert!(Delta::plan(&on(BodyRef::City), &on(BodyRef::City), 1).is_some());
}

#[test]
fn a_delta_without_its_baseline_is_refused() {
    let a = QEntity::quantize(&EntityState::default());
    let d = Delta::plan(&a, &a, 3).unwrap();
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
    w.header(&SnapshotHeader { tick: 50, ..SnapshotHeader::default() });
    w.own(None);
    w.orrery(None);
    w.entity_q(&a, Some(&d), 0).unwrap();
    let n = w.finish().unwrap();
    let read = |held: u32| {
        let mut r = SnapshotReader::new(&buf[..n]).unwrap();
        r.next_entity_q(|slot, tick| (slot == a.slot && tick == held).then_some(a))
    };
    assert_eq!(read(46), Err(bc_proto::DecodeError::NoBaseline));
    assert_eq!(read(47), Ok(Some(a)));
    let mut r = SnapshotReader::new(&buf[..n]).unwrap();
    assert_eq!(r.next_entity(), Err(bc_proto::DecodeError::NoBaseline));
}

#[test]
fn record_budgets_match_plan() {
    // ~26 bytes per entity (24 for a suit on a body); ~30 fit in a datagram next to header, own
    // state, the Orrery and events.
    const { assert!(ENTITY_MAX_BITS == 212) };
    const { assert!(ORRERY_HYPOTHESES == 7) };
    const { assert!(OWN_BITS_FREE == 814) };
    const { assert!(OWN_MAX_BITS == 834) };
    const { assert!(ROCK_RECORD_BITS == 18) };
    const { assert!(MISSILE_RECORD_BITS == 119) };
    const { assert!(ObjectState::MAX_BITS <= 232) };
    let rider = |body| EntityState { on: Some(RiderOn { body, aloft: false }), ..EntityState::default() };
    assert_eq!(rider(BodyRef::Rock(0)).encoded_bits(), 195);
    assert_eq!(rider(BodyRef::Landmark(0)).encoded_bits(), 195);
    assert_eq!(EntityState::default().encoded_bits(), ENTITY_MAX_BITS);
}

/// How many entities fill a whole datagram after the header and `own`, with the Orrery off and no
/// objects.
fn entities_that_fit(own: &OwnState, e: &EntityState) -> usize {
    let mut buf = [0u8; 1500];
    let mut w = SnapshotWriter::new(&mut buf, MAX_DATAGRAM);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    w.orrery(None);
    let mut n = 0;
    while w.entity(e, 0) {
        n += 1;
    }
    let len = w.finish().unwrap();
    assert!(len <= MAX_DATAGRAM);
    let mut r = SnapshotReader::new(&buf[..len]).unwrap();
    let mut back = 0;
    while let Some(got) = r.next_entity().unwrap() {
        assert_eq!(got.on, e.on);
        back += 1;
    }
    assert_eq!(back, n);
    n
}

#[test]
fn a_full_datagram_holds_36_whole_records_free_or_39_riders() {
    let free = EntityState { pos: Vec3::new(9_000.0, 5_000.0, -12_000.0), ..EntityState::default() };
    let rider = EntityState {
        on: Some(RiderOn { body: BodyRef::Landmark(1), aloft: false }),
        pos: Vec3::new(0.0, 599.0, 0.0),
        ..EntityState::default()
    };
    // The worst own state: standing on a rock. Free traffic loses nothing to wear and tear's 36.
    let on_a_rock = OwnState {
        alive: true,
        surface: Some(OwnSurface { footing: footing::GROUNDED, body: BodyRef::Rock(1_000), stance_q: 146 }),
        ..OwnState::default()
    };
    assert_eq!(on_a_rock.encoded_bits(), OWN_MAX_BITS);
    assert_eq!(entities_that_fit(&on_a_rock, &free), 36);
    assert_eq!(entities_that_fit(&on_a_rock, &rider), 39);
    // Flying free, a fortieth rider fitted with a bit to spare until the fed condition's three
    // (v38).
    let flying = OwnState { alive: true, ..OwnState::default() };
    assert_eq!(entities_that_fit(&flying, &free), 36);
    assert_eq!(entities_that_fit(&flying, &rider), 39);
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
