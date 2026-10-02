//! The same scenario must produce bit-identical state on every machine and on wasm32 (the browser
//! predicts with this code). Native runs need `--features deterministic` (glam scalar math, which is
//! what wasm32 uses too); `cargo test --workspace` gets it through feature unification.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
#![cfg(any(feature = "deterministic", target_arch = "wasm32"))]

mod common;

/// Hash after 600 ticks of the reference scenario (update deliberately when the sim changes).
const GOLDEN: u64 = 0x2e6d_0082_f337_169d;

fn scenario_hash() -> u64 {
    let (mut sim, players) = common::arena(8, 24, 42);
    common::run(&mut sim, &players, 600);
    // The hash covers wreckage too, so the scenario must leave some.
    assert!(sim.chunks.count() > 0, "no limbs or hulks after the battle");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn golden_hash_native() {
    let h = scenario_hash();
    assert_eq!(h, scenario_hash(), "must be reproducible within a process");
    assert_eq!(h, GOLDEN, "state hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn golden_hash_wasm() {
    assert_eq!(scenario_hash(), GOLDEN);
}

/// Hash after 450 ticks of the Gundams duelling in pairs among Mobile Dolls: every blade, the
/// Cross Crusher, the Dragon Fang, the flamethrower, the Hyper Jammer, guided missiles, Full Open,
/// Neo-Bird and the Gundams' guns (changes deliberately as their mechanics arrive).
const GUNDAMS_GOLDEN: u64 = 0xc8b4_b787_b868_e3d3;

fn gundams_hash() -> u64 {
    use bc_proto::events::Event;
    use bc_proto::{FrameId, WeaponKind};

    let (mut sim, pilots) = common::gundam_arena(12, 7);
    let mut landed = [false; WeaponKind::COUNT];
    let mut clashes = 0;
    for _ in 0..450 {
        let t = sim.next_tick();
        let from = sim.events.next_seq();
        for pair in pilots.chunks(2) {
            let (a, b) = (pair[0], pair[1]);
            let (ca, cb) = (common::duel_scripted(&sim, a, b, t), common::duel_scripted(&sim, b, a, t));
            sim.set_input(a, ca);
            sim.set_input(b, cb);
        }
        sim.step();
        for s in from..sim.events.next_seq() {
            match sim.events.get(s) {
                Some(Event::Hit { weapon, .. }) => landed[*weapon as usize] = true,
                Some(Event::Clash { .. }) => clashes += 1,
                _ => {}
            }
        }
    }
    assert!(clashes > 0, "no blades met in the Gundams' scenario");
    for k in [
        WeaponKind::BeamSaber,
        WeaponKind::ArmyKnife,
        WeaponKind::BeamScythe,
        WeaponKind::HeatShotel,
        WeaponKind::CrossCrusher,
        WeaponKind::DragonFang,
        WeaponKind::BeamGlaive,
        WeaponKind::Flamethrower,
        WeaponKind::BeamGatling,
        WeaponKind::BusterShield,
        WeaponKind::BeamMachineGun,
        WeaponKind::HomingMissile,
        WeaponKind::MicroMissile,
        WeaponKind::ChestGatling,
    ] {
        assert!(landed[k as usize], "no {k:?} hit in the Gundams' scenario");
    }
    for id in &pilots {
        let f = sim.suits.frame[id.idx()];
        if matches!(
            f,
            FrameId::WingZero
                | FrameId::WingZeroBird
                | FrameId::Heavyarms
                | FrameId::Sandrock
                | FrameId::Deathscythe
        ) {
            assert!(sim.stats(id.idx()).specials > 0, "{f:?} never used its special");
        }
    }
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn gundams_golden_native() {
    let h = gundams_hash();
    assert_eq!(h, gundams_hash(), "must be reproducible within a process");
    assert_eq!(h, GUNDAMS_GOLDEN, "Gundams' scenario hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn gundams_golden_wasm() {
    assert_eq!(gundams_hash(), GUNDAMS_GOLDEN);
}

/// Hash after 450 ticks of pilots locked on to their foes (`bc_proto::LockOn`): flight assist
/// holding each foe's velocity in the fight's axes, levelled to the colony's up, closing in and
/// circling, burst-stepping now and then, among Mobile Dolls.
const LOCKON_GOLDEN: u64 = 0xc8af_9bcc_9e59_1ede;

fn lockon_hash() -> u64 {
    let (mut sim, duels) = common::gundam_crowd(8, 12, 21);
    let mut steps = 0;
    for _ in 0..450 {
        let t = sim.next_tick();
        for &(a, b) in &duels {
            let (ca, cb) = (common::locked_scripted(&sim, a, b, t), common::locked_scripted(&sim, b, a, t));
            sim.set_input(a, ca);
            sim.set_input(b, cb);
        }
        sim.step();
        let started =
            |id: bc_sim::SuitId| sim.suits.flight[id.idx()].burst.left == bc_sim::flight::BURST_TICKS - 1;
        steps += duels.iter().map(|&(a, b)| usize::from(started(a)) + usize::from(started(b))).sum::<usize>();
    }
    assert!(steps >= 2 * duels.len() * 4, "only {steps} burst steps");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn lockon_golden_native() {
    let h = lockon_hash();
    assert_eq!(h, lockon_hash(), "must be reproducible within a process");
    assert_eq!(h, LOCKON_GOLDEN, "lock-on scenario hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn lockon_golden_wasm() {
    assert_eq!(lockon_hash(), LOCKON_GOLDEN);
}

/// Hash of the generated debris field (clients build it from the Welcome's seed and count).
const FIELD_GOLDEN: u64 = 0x8631_3a1b_8b14_b993;

fn fnv(h: &mut u64, v: u32) {
    for b in v.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
}

fn field_hash() -> u64 {
    let f =
        bc_sim::field::Field::generate(bc_sim::field::Field::DEFAULT_SEED, bc_sim::field::Field::MAX_ROCKS);
    let mut h = 0xcbf2_9ce4_8422_2325;
    for r in f.rocks() {
        for v in [
            r.pos.x, r.pos.y, r.pos.z, r.radius, r.axes.x, r.axes.y, r.axes.z, r.rot.x, r.rot.y, r.rot.z,
            r.rot.w,
        ] {
            fnv(&mut h, v.to_bits());
        }
        fnv(&mut h, u32::from(r.shape) | u32::from(r.ore) << 8);
    }
    h
}

/// Hash after suits have flown into rocks and fired into them, wearing them down, while a Leo cuts
/// a small one apart with its saber.
const ROCKS_GOLDEN: u64 = 0x91c4_8251_0774_173a;

fn rocks_hash() -> u64 {
    use bc_proto::buttons::{FIRE_PRIMARY, MELEE};
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::math::{cos, look_rotation, sin};
    use glam::Vec3;

    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig { target_dolls: 0, ..bc_sim::SimConfig::default() });
    let rocks: Vec<_> = sim.field.rocks().iter().filter(|r| r.radius > 25.0).take(6).copied().collect();
    let mut ids = Vec::new();
    for (k, r) in rocks.iter().enumerate() {
        // From all round, at speeds up to 2 km/s, firing at whatever is behind the rock.
        let a = k as f32 * 1.1;
        // libm's trig, as in the sim: std's differs between native and wasm32.
        let dir = Vec3::new(cos(a), 0.3 * sin(a), sin(a)).normalize();
        let pos = r.pos - dir * (r.radius + 120.0 + 40.0 * k as f32);
        let frame = if k % 2 == 0 { FrameId::WingZero } else { FrameId::Leo };
        let id = sim
            .spawn_at(frame, Faction::Colonies, PilotKind::Human, pos, look_rotation(dir, Vec3::Y))
            .unwrap();
        sim.suits.flight[id.idx()].vel = dir * (300.0 + 340.0 * k as f32);
        ids.push((id, dir));
    }
    let k = sim.field.rocks().iter().position(|r| r.radius > 8.0 && r.radius < 12.0).unwrap();
    let small = sim.field.rocks()[k];
    let dir = Vec3::new(1.0, 0.1, 0.3).normalize();
    let face = small.surface(small.pos - dir * (small.radius + 50.0), 0.0);
    let miner = sim
        .spawn_at(
            FrameId::Leo,
            Faction::Colonies,
            PilotKind::Human,
            face - dir * bc_sim::field::SUIT_CLEARANCE,
            look_rotation(dir, Vec3::Y),
        )
        .unwrap();
    for _ in 0..150 {
        let t = sim.next_tick();
        let cut = if t % 45 < 3 { MELEE } else { 0 };
        let aim = (small.pos - sim.suits.flight[miner.idx()].pos).normalize();
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim,
            thrust: [0, 0, 20],
            buttons: cut,
            ..InputCmd::default()
        };
        sim.set_input(miner, cmd);
        for &(id, dir) in &ids {
            sim.set_input(
                id,
                InputCmd {
                    tick: t,
                    view_tick_q4: t << 4,
                    aim: dir,
                    buttons: common::pull(&sim, id, FIRE_PRIMARY),
                    ..InputCmd::default()
                },
            );
        }
        sim.step();
    }
    assert!(sim.rocks.destroyed.get(k), "the miner never broke its rock");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn field_and_rocks_golden_native() {
    assert_eq!(field_hash(), FIELD_GOLDEN, "field hash changed: {:#018x}", field_hash());
    let h = rocks_hash();
    assert_eq!(h, rocks_hash(), "must be reproducible within a process");
    assert_eq!(h, ROCKS_GOLDEN, "rocks scenario hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn field_and_rocks_golden_wasm() {
    assert_eq!(field_hash(), FIELD_GOLDEN);
    assert_eq!(rocks_hash(), ROCKS_GOLDEN);
}

/// Hash after a salvage run: pilots gather ore, stow it, tow a hulk, throw, jettison, and sell (and
/// refuel) at the dock.
const SALVAGE_GOLDEN: u64 = 0xea7b_0f6f_3c58_79f6;

fn salvage_hash() -> u64 {
    use bc_proto::buttons::{FLIGHT_ASSIST, GRAB, JETTISON, STOW, THROW};
    use bc_proto::{ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, Part, PilotKind, Segment};
    use bc_sim::chunks::{self, Motion};
    use bc_sim::content::ArmSlot;
    use bc_sim::content::salvage::DOCK_CENTER;
    use bc_sim::math::look_rotation;
    use glam::{Quat, Vec3};

    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig { target_dolls: 0, ..bc_sim::SimConfig::default() });
    let mut ids = Vec::new();
    for k in 0..4 {
        let pos =
            if k == 3 { DOCK_CENTER + Vec3::X * 40.0 } else { Vec3::new(k as f32 * 150.0, 1_000.0, 0.0) };
        let id = sim
            .spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, pos, look_rotation(Vec3::Z, Vec3::Y))
            .unwrap();
        ids.push(id);
    }
    let t = sim.tick();
    let chunk = |sim: &mut bc_sim::Sim, kind: ChunkKind, kg: u32, pos: Vec3, seed: u8| {
        let desc = ChunkDesc { kind, seed, mass_kg: kg };
        let seg =
            Segment { t0: t, pos, vel: Vec3::ZERO, rot: Quat::IDENTITY, spin: Vec3::new(0.0, 0.2, 0.1) }
                .quantized();
        sim.chunks.spawn(desc, Motion::Free(seg), t + 9_000, t).unwrap();
    };
    // Ore strung out ahead of each pilot's left hand, and a hulk for the second pilot to tow.
    for (k, id) in ids.iter().enumerate() {
        let f = sim.suits.flight[id.idx()];
        let hand = f.pos + f.rot * ArmSlot::Left.muzzle();
        for n in 0..4u8 {
            let desc =
                ChunkDesc { kind: ChunkKind::Ore { ore: n % 4 }, seed: n, mass_kg: 300 + 100 * u32::from(n) };
            let at = hand + Vec3::new(-chunks::radius(&desc) - 1.0, 0.0, 12.0 * f32::from(n));
            chunk(&mut sim, desc.kind, desc.mass_kg, at, n + 10 * k as u8);
        }
    }
    let hulk = ChunkKind::Hulk { frame: FrameId::Taurus, faction: Faction::Oz, parts: 0b10_1111 };
    let tow = sim.suits.flight[ids[1].idx()].pos + Vec3::new(-12.0, 0.0, 70.0);
    chunk(&mut sim, hulk, 5_600, tow, 99);
    let arm = ChunkKind::Limb { frame: FrameId::WingZero, faction: Faction::Colonies, part: Part::ArmL };
    let f3 = sim.suits.flight[ids[3].idx()];
    chunk(&mut sim, arm, 640, f3.pos + f3.rot * ArmSlot::Left.muzzle() + Vec3::new(-4.0, 0.0, 0.0), 77);

    for _ in 0..360 {
        let t = sim.next_tick();
        for (k, &id) in ids.iter().enumerate() {
            let k = k as u32;
            let mut buttons = FLIGHT_ASSIST | GRAB;
            if (t + k * 7).is_multiple_of(30) && t < 250 {
                buttons |= STOW;
            }
            if k == 0 && t == 300 {
                buttons |= THROW;
            }
            if k == 2 && t == 320 {
                buttons |= JETTISON;
            }
            // Creeping forward (5/127 of a Leo's 220 m/s cruise), slow enough to catch what's ahead.
            let cmd = InputCmd {
                tick: t,
                view_tick_q4: t << 4,
                aim: Vec3::Z,
                thrust: [0, 0, 5],
                buttons,
                ..InputCmd::default()
            };
            sim.set_input(id, cmd.quantized());
        }
        sim.step();
    }
    let stowed: u32 = ids.iter().map(|id| sim.suits.cargo_total_kg(id.idx())).sum();
    assert!(stowed > 0, "nothing was stowed");
    assert!(ids.iter().any(|id| sim.held_chunk(id.idx()).is_some()), "nobody is holding anything");
    assert!(sim.suits.credits[ids[3].idx()] > 0, "nothing sold at the dock");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn salvage_golden_native() {
    let h = salvage_hash();
    assert_eq!(h, salvage_hash(), "must be reproducible within a process");
    assert_eq!(h, SALVAGE_GOLDEN, "salvage hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn salvage_golden_wasm() {
    assert_eq!(salvage_hash(), SALVAGE_GOLDEN);
}

/// Hash after sleepers' lives: a pilot at rest against a rock parks there until the rock is
/// shattered under it; others tumble off on what they had; one is shot down asleep; the longest
/// asleep are cleared past the cap; one wakes and flies; Mobile Dolls look on. (The hash covers
/// what a sleeper is parked on more fully since suits stand on bodies: how it's turned there, when
/// it last fought, and its hide spot; and, with wear and tear, every suit's systems, equipment and
/// statuses. The scenario itself runs bit for bit as it did.)
const SLEEPERS_GOLDEN: u64 = 0x82d4_286e_fe11_0a4c;

fn sleepers_hash() -> u64 {
    use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST};
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::field::SUIT_CLEARANCE;
    use bc_sim::math::look_rotation;
    use bc_sim::sim::Gone;
    use glam::Vec3;

    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig {
        target_dolls: 6,
        max_sleepers: 4,
        ..bc_sim::SimConfig::default()
    });
    let (r, rock) =
        sim.field.rocks().iter().copied().enumerate().find(|(_, r)| r.radius > 20.0).expect("a big rock");
    let spawn = |sim: &mut bc_sim::Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3| {
        sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
    };
    // Resting on the rock.
    let out = Vec3::new(-1.0, 0.1, -0.3).normalize();
    let surface = rock.surface(rock.pos + out * (rock.radius + 50.0), 0.0);
    let parked =
        spawn(&mut sim, FrameId::Leo, Faction::Colonies, surface + out * (SUIT_CLEARANCE + 0.5), out);
    // Drifting and tumbling, out in the open.
    let open = Vec3::new(0.0, 4_000.0, 8_000.0);
    let drifters: Vec<_> = (0..3)
        .map(|k| {
            let id = spawn(&mut sim, FrameId::Leo, Faction::Oz, open + Vec3::X * 60.0 * k as f32, Vec3::Z);
            let f = &mut sim.suits.flight[id.idx()];
            f.vel = Vec3::new(3.0 * k as f32, -2.0, 7.5);
            f.ang_vel = Vec3::new(0.1, 0.3 * k as f32, -0.2);
            id
        })
        .collect();
    // A Wing Zero with the last of them in its sights.
    let target = sim.suits.flight[drifters[2].idx()].pos;
    let zero = spawn(&mut sim, FrameId::WingZero, Faction::Colonies, target - Vec3::Z * 1_200.0, Vec3::Z);

    for _ in 0..450 {
        let t = sim.next_tick();
        match t {
            5 => {
                for &d in &drifters {
                    assert!(sim.sleep(d));
                }
            }
            10 => assert!(sim.sleep(parked) && sim.is_parked(parked.idx())),
            // A fifth sleeper: the longest asleep goes.
            40 => {
                assert!(sim.sleep(zero) && sim.wake(zero));
                assert!(!sim.suits.valid(drifters[0]), "the longest asleep is cleared");
            }
            120 => sim.field.set_dead(r, true),
            130 => assert!(sim.is_sleeping(parked.idx()) && !sim.is_parked(parked.idx()), "adrift"),
            200 => assert!(sim.wake(drifters[1])),
            _ => {}
        }
        let aim = (sim.suits.flight[drifters[2].idx()].pos - sim.suits.flight[zero.idx()].pos)
            .normalize_or(Vec3::Z);
        let fire = if (60..160).contains(&t) { FIRE_PRIMARY } else { 0 };
        let cmd = InputCmd {
            tick: t,
            view_tick_q4: t << 4,
            aim,
            buttons: FLIGHT_ASSIST | fire,
            ..InputCmd::default()
        };
        if !sim.is_sleeping(zero.idx()) {
            sim.set_input(zero, cmd.quantized());
        }
        if t > 200 && sim.suits.valid(drifters[1]) && !sim.is_sleeping(drifters[1].idx()) {
            let fly = InputCmd { thrust: [0, 40, 90], aim: Vec3::Z, buttons: FLIGHT_ASSIST, ..cmd };
            sim.set_input(drifters[1], fly.quantized());
        }
        sim.step();
    }
    let mut fates = Vec::new();
    sim.drain_fates(|f| fates.push((f.suit as usize, f.gone)));
    assert!(fates.contains(&(drifters[0].idx(), Gone::Evicted)), "{fates:?}");
    assert!(fates.contains(&(drifters[2].idx(), Gone::Destroyed { killer: zero.idx() as u16 })), "{fates:?}");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn sleepers_golden_native() {
    let h = sleepers_hash();
    assert_eq!(h, sleepers_hash(), "must be reproducible within a process");
    assert_eq!(h, SLEEPERS_GOLDEN, "sleepers hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn sleepers_golden_wasm() {
    assert_eq!(sleepers_hash(), SLEEPERS_GOLDEN);
}

/// Hash after 900 ticks on the surfaces, among Mobile Dolls. An OZ Leo is caught over a rock, lands,
/// walks a square, hops, crouches, stands, and digs the rock out from under itself, crouched. On MO-II, a Heavyarms
/// runs over a pylon's edge and down its side with rewound shots coming at it, and another walks
/// to the Aft Well's rim, hops over it, crouches on the floor and hides there, sleeps and wakes. A
/// Wing Zero is caught over Hermit's Deep, lands in it, and changes into the Neo-Bird and flies
/// off. A guided missile goes at the Leo on its rock. (The hash covers the suits' cover since they
/// hide, and the dolls hunting the riders come at them from above; with wear and tear, every suit's
/// systems, equipment and statuses.)
const SURFACE_GOLDEN: u64 = 0x2e92_5182_35ee_f345;

fn surface_hash() -> u64 {
    use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, GRIP, MELEE, MODE};
    use bc_proto::events::Event;
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::bodies::{Bodies, Body};
    use bc_sim::content::landmarks::LANDMARKS;
    use bc_sim::ground::{CROUCH_STANCE, Footing, STANCE, place};
    use bc_sim::math::look_rotation;
    use glam::Vec3;

    let mut sim = bc_sim::Sim::new(bc_sim::SimConfig { target_dolls: 4, ..bc_sim::SimConfig::default() });
    let (mo_ii, hermit) = (Body::Landmark(0), Body::Landmark(1));
    let spawn = |sim: &mut bc_sim::Sim, frame: FrameId, faction: Faction, pos: Vec3, facing: Vec3| {
        sim.spawn_at(frame, faction, PilotKind::Human, pos, look_rotation(facing, Vec3::Y)).unwrap()
    };
    // The Leo, 20 m over the smallest rock it can grip.
    let (r, rock) = common::grippable_rock(&sim, 300.0);
    let (top, up, over, h) = {
        let bodies = Bodies::at(&sim.field, &LANDMARKS, 0);
        let (top, up) = bodies.surface_along(Body::Rock(r as u16), Vec3::Y).unwrap();
        let over = bodies.pose(mo_ii).unwrap().to_world(Vec3::new(0.0, 600.0, 0.0));
        (rock.pos + rock.rot * top, rock.rot * up, over, bodies.pose(hermit).unwrap())
    };
    let ahead = (Vec3::Z - up * up.z).normalize();
    let leo = spawn(&mut sim, FrameId::Leo, Faction::Oz, top + up * (STANCE + 20.0), ahead);
    // A Heavyarms 900 m off, with its missiles.
    let gunner = spawn(&mut sim, FrameId::Heavyarms, Faction::Colonies, top + up * 900.0, -up);
    // On MO-II: one on a pylon's top, one on the aft module's face beside the Aft Well.
    let runner = common::standing_on(&mut sim, FrameId::Heavyarms, Faction::Alliance, mo_ii, Vec3::Y);
    let hider = common::standing_on(
        &mut sim,
        FrameId::Heavyarms,
        Faction::Alliance,
        mo_ii,
        Vec3::new(-260.0, 65.0, 0.0),
    );
    // Two Leos 500 m over the pylon, shooting at the runner as they saw it 6 ticks back.
    let shooters = [Vec3::new(-60.0, 0.0, 0.0), Vec3::new(60.0, 0.0, 40.0)]
        .map(|d| spawn(&mut sim, FrameId::Leo, Faction::Colonies, over + d, -Vec3::Y));
    // The Wing Zero, 15 m over the floor of Hermit's Deep.
    let deep = LANDMARKS[1].hides[0].center;
    let zero = sim
        .spawn_at(
            FrameId::WingZero,
            Faction::Oz,
            PilotKind::Human,
            h.to_world(deep + Vec3::Y * (STANCE + 15.0)),
            look_rotation(h.rot * Vec3::Z, h.rot * Vec3::Y),
        )
        .unwrap();

    let local = |sim: &bc_sim::Sim, id: bc_sim::SuitId| sim.suits.anchor[id.idx()].local;
    let pose = |sim: &bc_sim::Sim, b: Body| Bodies::at(&sim.field, &LANDMARKS, sim.tick()).pose(b).unwrap();
    // The ground's normal under an attached suit, in the sector's frame.
    let normal = |sim: &bc_sim::Sim, id: bc_sim::SuitId| {
        let a = sim.suits.anchor[id.idx()];
        let b = Bodies::at(&sim.field, &LANDMARKS, sim.tick());
        b.pose(a.body).unwrap().rot * place(&b.shape(a.body).unwrap(), a.local, a.stance).1
    };
    let (mut leo_seen, mut hopped, mut crouched, mut dug_free) = ([false; 3], false, false, false);
    let (mut round_the_edge, mut in_the_well, mut zero_landed) = (false, false, false);
    let mut hidden_at = None;
    let (mut hits_on_runner, mut bursts) = (0, 0);
    for _ in 0..900 {
        let t = sim.next_tick();
        let cmd = |aim: Vec3, thrust: [i8; 3], buttons: u16| {
            InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() }
                .quantized()
        };
        // The Leo.
        let footing = sim.footing(leo.idx());
        let c = if t >= 600 && footing == Footing::Free {
            cmd(ahead, [0; 3], FLIGHT_ASSIST)
        } else if t >= 600 {
            let dig = if t % 45 < 3 { MELEE } else { 0 };
            cmd(-normal(&sim, leo), [0, -127, 0], common::pull(&sim, leo, GRIP | FIRE_PRIMARY | dig))
        } else {
            let square = [[0, 0, 127], [127, 0, 0], [0, 0, -127], [-127, 0, 0]];
            let thrust = match t {
                100..200 => square[(t as usize - 100) / 25],
                200 => [0, 127, 0],
                330..350 => [0, -127, 0],
                450..460 => [0, 64, 0],
                _ => [0; 3],
            };
            cmd(ahead, thrust, GRIP | FLIGHT_ASSIST)
        };
        sim.set_input(leo, c);
        // Its hunter locks on, and lets one salvo go.
        let to_leo = (sim.suits.flight[leo.idx()].pos - sim.suits.flight[gunner.idx()].pos).normalize();
        let fire = if t == 400 { FIRE_SECONDARY } else { 0 };
        sim.set_input(
            gunner,
            InputCmd { lock_target: leo.idx() as u16, ..cmd(to_leo, [0; 3], FLIGHT_ASSIST | fire) },
        );
        // The runner: off the pylon's top toward -x, then straight on, over the edge and down.
        let deck = pose(&sim, mo_ii);
        let aim = if local(&sim, runner).x > -30.0 {
            deck.rot * -Vec3::X
        } else {
            sim.suits.flight[runner.idx()].rot * Vec3::Z
        };
        sim.set_input(runner, cmd(aim, [0, 0, 127], GRIP | BOOST));
        // The hider: to the rim, over it, to the floor; crouched; asleep from 650 to 850.
        if t == 650 {
            assert!(sim.sleep(hider) && sim.is_parked(hider.idx()), "the hider didn't park");
        }
        if t == 850 {
            assert!(sim.wake(hider));
        }
        if !sim.is_sleeping(hider.idx()) {
            let floor = LANDMARKS[0].hides[0].center + Vec3::X * -STANCE;
            let to_floor = floor - local(&sim, hider);
            let (aim, thrust, buttons) = match t {
                ..150 => (deck.rot * -Vec3::Y, [0, 0, 127], GRIP),
                150 => (deck.rot * -Vec3::Y, [0, 127, 127], GRIP | FLIGHT_ASSIST),
                _ if sim.footing(hider.idx()) == Footing::Aloft => {
                    (deck.rot * -Vec3::Y, [0, 0, 127], GRIP | FLIGHT_ASSIST)
                }
                400..420 => (deck.rot * to_floor.normalize_or(Vec3::Y), [0, -127, 0], GRIP),
                _ if to_floor.length() > 4.0 => {
                    (deck.rot * to_floor.normalize_or(Vec3::Y), [0, 0, 127], GRIP)
                }
                _ => (sim.suits.flight[hider.idx()].rot * Vec3::Z, [0; 3], GRIP),
            };
            sim.set_input(hider, cmd(aim, thrust, buttons));
        }
        // The shooters, from t = 60.
        let target = sim.suits.flight[runner.idx()].pos;
        for &s in &shooters {
            let aim = (target - sim.suits.flight[s.idx()].pos).normalize();
            let fire = if t >= 60 { FIRE_PRIMARY } else { 0 };
            let fire = common::pull(&sim, s, FLIGHT_ASSIST | fire);
            let c = InputCmd { view_tick_q4: (t - 6) << 4, ..cmd(aim, [0; 3], fire) };
            sim.set_input(s, c);
        }
        // The Wing Zero: down into the Deep, then off as a bird at 500.
        let hz = pose(&sim, hermit).rot;
        let c = if t < 500 {
            cmd(hz * Vec3::Z, [0; 3], GRIP)
        } else {
            cmd(sim.suits.flight[zero.idx()].rot * Vec3::Z, [0, 60, 127], MODE | FLIGHT_ASSIST)
        };
        sim.set_input(zero, c);

        let from = sim.events.next_seq();
        sim.step();
        common::check_invariants(&sim);
        for e in (from..sim.events.next_seq()).filter_map(|s| sim.events.get(s)) {
            match e {
                Event::Hit { target, .. } if *target as usize == runner.idx() => hits_on_runner += 1,
                Event::MissileBurst { .. } => bursts += 1,
                _ => {}
            }
        }
        let t = sim.tick();
        match sim.footing(leo.idx()) {
            Footing::Aloft => leo_seen[1] = true,
            Footing::Grounded => leo_seen[2] = true,
            Footing::Free if leo_seen[2] => leo_seen[0] = true,
            Footing::Free => {}
        }
        hopped |= (200..260).contains(&t) && sim.footing(leo.idx()) == Footing::Aloft;
        crouched |= t == 400 && sim.suits.anchor[leo.idx()].stance == CROUCH_STANCE;
        dug_free |= sim.rocks.destroyed.get(r) && sim.footing(leo.idx()) == Footing::Free;
        if sim.footing(runner.idx()) == Footing::Grounded {
            round_the_edge |= normal(&sim, runner).dot(pose(&sim, mo_ii).rot * -Vec3::X) > 0.95;
        }
        let hider_at = Bodies::at(&sim.field, &LANDMARKS, t).hide_spot_of(mo_ii, local(&sim, hider));
        in_the_well |= t == 640
            && sim.footing(hider.idx()) == Footing::Grounded
            && hider_at == Some(0)
            && sim.suits.anchor[hider.idx()].stance == CROUCH_STANCE;
        if t == 499 {
            let b = Bodies::at(&sim.field, &LANDMARKS, t);
            zero_landed = sim.footing(zero.idx()) == Footing::Grounded
                && b.hide_spot_of(hermit, local(&sim, zero)) == Some(0);
        }
        if hidden_at.is_none() && sim.cover_code(hider.idx()) == bc_sim::sim::cover::HIDDEN {
            hidden_at = Some(t);
        }
        if (651..850).contains(&t) {
            assert!(sim.is_parked(hider.idx()));
        }
    }
    assert!(
        leo_seen == [true; 3] && hopped && crouched && dug_free,
        "the Leo: {leo_seen:?} {hopped} {crouched} {dug_free}"
    );
    assert!(sim.stats(gunner.idx()).missiles > 0 && bursts > 0, "no missile went at the Leo");
    assert!(round_the_edge, "the runner never went over the pylon's edge");
    assert!(hits_on_runner > 0, "no rewound shot hit the runner");
    assert!(in_the_well, "the hider isn't crouched in the Aft Well");
    assert!(hidden_at.is_some_and(|t| t < 650), "the hider never hid before sleeping: {hidden_at:?}");
    let (hider, zero_f) = (hider.idx(), zero.idx());
    assert_eq!(sim.footing(hider), Footing::Grounded);
    assert_eq!(sim.suits.anchor[hider].stance, CROUCH_STANCE, "woke crouched");
    assert!(zero_landed, "the Wing Zero never landed in the Deep");
    assert_eq!((sim.footing(zero_f), sim.suits.frame[zero_f]), (Footing::Free, FrameId::WingZeroBird));
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn surface_golden_native() {
    let h = surface_hash();
    assert_eq!(h, surface_hash(), "must be reproducible within a process");
    assert_eq!(h, SURFACE_GOLDEN, "surface scenario hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn surface_golden_wasm() {
    assert_eq!(surface_hash(), SURFACE_GOLDEN);
}

/// Hash of the colony's closed forms: the city's blocks and buildings on every strip (every fifth
/// block along, every row), what's solid at scattered points, the colony's day and its frames,
/// its trams (their timetable, the stations' platforms), and the key places' rooms and the halls
/// round them. Every client draws and walks this, and the server checks poses against it.
const CITY_GOLDEN: u64 = 0x3a9e_d960_4c23_7688;

fn city_hash() -> u64 {
    use bc_sim::colony::{city, frame, time};
    let stage = city::Stage(0);
    let mut h = 0xcbf2_9ce4_8422_2325;
    for k in 0..3u8 {
        for bx in (city::HUB_GATE.0..=city::FAR_FOOT.1).step_by(5) {
            for row in -city::BANK_ROW..=city::BANK_ROW {
                for b in city::texel(k, bx, row, stage) {
                    fnv(&mut h, u32::from(b));
                }
                if let Some(b) = city::block(k, bx, row, stage) {
                    for bd in city::lots(&b).as_slice() {
                        for v in [bd.foot.s0, bd.foot.s1, bd.foot.x0, bd.foot.x1, bd.height, bd.top()] {
                            fnv(&mut h, v.to_bits());
                        }
                    }
                }
            }
        }
    }
    let mut rng = bc_sim::math::Rng::new(99);
    for _ in 0..4_000 {
        let k = (rng.next_u32() % 3) as u8;
        let p = glam::Vec3::new(
            rng.signed() * 16_000.0,
            rng.next_f32() * 20.0 - 1.0,
            -rng.next_f32() * frame::STRIP_WIDTH,
        );
        let e = glam::Vec3::new(0.3, 0.9, 0.3);
        fnv(&mut h, u32::from(city::solid(k, p - e, p + e, stage)));
        fnv(&mut h, city::ground(k, -p.z, p.x, stage).to_bits());
    }
    for t in (0..time::DAY_TICKS).step_by(997) {
        let d = time::day(t, 0.5);
        for v in [d.daylight, d.mirror_beta, d.lamps, d.sun_elev] {
            fnv(&mut h, v.to_bits());
        }
        let l = time::key_light((t % 3) as usize, &d);
        for v in [l.x, l.y, l.z] {
            fnv(&mut h, v.to_bits());
        }
    }
    for i in 0..300 {
        let p = frame::CityPos::new(
            (i % 3) as u8,
            -15_000.0 + 100.0 * i as f32,
            11.0 * i as f32,
            (i % 17) as f32,
        );
        let c = p.to_colony();
        for v in [c.x, c.y, c.z] {
            fnv(&mut h, v.to_bits());
        }
    }
    // The trams: their timetable, and the stations' platforms.
    use bc_sim::colony::transit;
    for strip in 0..3u8 {
        for k in 0..transit::TRAINS as u8 {
            for t in (0..transit::PERIOD_TICKS).step_by(7_919) {
                let tr = transit::train(strip, k, t, 0.37);
                for v in [tr.x, tr.s, tr.dir, tr.speed, tr.accel] {
                    fnv(&mut h, v.to_bits());
                }
                fnv(&mut h, u32::from(tr.doors) | tr.at.map_or(0, |a| a as u32 + 2) << 1);
            }
        }
    }
    for i in 0..400 {
        let x = transit::station_x(i % transit::STATIONS) - 45.0 + 0.23 * i as f32;
        let p = glam::Vec3::new(x, 0.3, -frame::STRIP_WIDTH * 0.5 - 1.0);
        let e = glam::Vec3::new(0.3, 0.9, 0.3);
        fnv(&mut h, u32::from(city::solid((i % 3) as u8, p - e, p + e, stage)));
    }
    // The key places' rooms, and the halls round them.
    use bc_sim::content::city::PLACES;
    for (i, p) in PLACES.iter().enumerate() {
        let Some(room) = city::room(i) else { continue };
        let ((cs, cx), _) = room.counter_spot();
        for r in [room.rect, room.door, room.counter] {
            for v in [r.s0, r.s1, r.x0, r.x1] {
                fnv(&mut h, v.to_bits());
            }
        }
        for v in [room.ceiling, cs, cx] {
            fnv(&mut h, v.to_bits());
        }
        let b = city::block(p.strip, p.bx, p.row, stage).expect("its block");
        let mut boxes = [city::CityBox::default(); city::MAX_SOLIDS];
        let n = city::lots(&b).as_slice()[0].solids(&mut boxes);
        for bx in &boxes[..n] {
            for v in [bx.rect.s0, bx.rect.s1, bx.rect.x0, bx.rect.x1, bx.h0, bx.h1] {
                fnv(&mut h, v.to_bits());
            }
        }
    }
    h
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn city_golden_native() {
    let h = city_hash();
    assert_eq!(h, city_hash(), "must be reproducible within a process");
    assert_eq!(h, CITY_GOLDEN, "the city's hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn city_golden_wasm() {
    assert_eq!(city_hash(), CITY_GOLDEN);
}

/// Hash after 600 ticks of suits flying the colony's inside: launched from the inner gate, some on
/// flight assist weaving among the towers, some falling to the floor and the roofs, firing all the
/// while (which the colony's law ignores). The spin's pull, Coriolis, the air and the city's boxes,
/// to the bit native and wasm.
const INTERIOR_GOLDEN: u64 = 0x8d4d_0a78_c0d1_b884;

fn interior_hash() -> u64 {
    use bc_proto::buttons::{BOOST, FIRE_PRIMARY, FLIGHT_ASSIST};
    use bc_proto::{Faction, FrameId, InputCmd, PilotKind};
    use bc_sim::colony::frame::CityPos;
    use bc_sim::colony::interior::WorldKind;
    use bc_sim::sim::Loadout;
    use bc_sim::{Sim, SimConfig};
    use glam::Vec3;

    let mut sim = Sim::new(SimConfig {
        target_dolls: 0,
        field_rocks: 0,
        landmarks: 0,
        survival: true,
        world: WorldKind::Interior,
        ..SimConfig::default()
    });
    let frames = [
        FrameId::Leo,
        FrameId::WingZero,
        FrameId::Heavyarms,
        FrameId::Deathscythe,
        FrameId::Leo,
        FrameId::Sandrock,
    ];
    let mut ids = Vec::new();
    for (k, f) in frames.into_iter().enumerate() {
        let id = sim.launch(f, Faction::Colonies, PilotKind::Human, &Loadout::full(f)).unwrap();
        if k >= 3 {
            // Over the city, low among the buildings.
            let at =
                CityPos::new((k % 3) as u8, -8_000.0 + k as f32 * 300.0, 1_200.0 + k as f32 * 150.0, 60.0);
            sim.suits.flight[id.idx()].pos = at.to_colony();
        }
        ids.push(id);
    }
    for n in 0..600u32 {
        let t = sim.next_tick();
        for (k, id) in ids.iter().enumerate() {
            let f = &sim.suits.flight[id.idx()];
            let phase = (n / 60 + k as u32) % 4;
            let yaw = bc_sim::math::sin(n as f32 * 0.01 + k as f32);
            let aim = (f.rot * Vec3::Z + Vec3::new(0.0, yaw * 0.3, yaw * 0.2)).normalize();
            let (buttons, thrust) = match (k % 2, phase) {
                (0, 0) => (FLIGHT_ASSIST | FIRE_PRIMARY, [0, 0, 100]),
                (0, 1) => (FLIGHT_ASSIST | BOOST, [40, 0, 127]),
                (0, _) => (FLIGHT_ASSIST, [-30, 20, 0]),
                (_, 0) => (0, [0, 0, 0]),
                (_, _) => (FIRE_PRIMARY, [0, -60, 50]),
            };
            sim.set_input(
                *id,
                InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust, buttons, ..InputCmd::default() },
            );
        }
        sim.step();
    }
    assert_eq!(sim.projectiles.count(), 0, "weapons safe");
    sim.state_hash()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn interior_golden_native() {
    let h = interior_hash();
    assert_eq!(h, interior_hash(), "must be reproducible within a process");
    assert_eq!(h, INTERIOR_GOLDEN, "the interior's hash changed: {h:#018x}");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn interior_golden_wasm() {
    assert_eq!(interior_hash(), INTERIOR_GOLDEN);
}
