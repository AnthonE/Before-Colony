//! The sector tick (input drain, simulation, snapshot encoding for 64 clients) never allocates.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_alloc::CountingAlloc;
use bc_proto::buttons::{FIRE_PRIMARY, FLIGHT_ASSIST, ZERO};
use bc_proto::{Faction, FrameId, InputCmd, InputPacket, MAX_DATAGRAM, PilotKind};
use bc_sector::{Comeback, Control, InputMsg, Outcome, SectorConfig, read_packet};
use bc_sim::SimConfig;
use glam::Vec3;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn sector_tick_never_allocates() {
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 256, seed: 3, ..SimConfig::default() },
        max_clients: 64,
        oracle: true,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, mut oracle) = bc_sector::build(cfg);
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        let frame = if l.slot % 2 == 0 { FrameId::WingZero } else { FrameId::Leo };
        shared
            .control
            .push(Control::Join {
                slot: l.slot,
                pilot: PilotKind::Human,
                frame,
                faction: Faction::Colonies,
                max_datagram: MAX_DATAGRAM as u16,
                comeback: Comeback::default(),
                launch: None,
            })
            .unwrap();
        leases.push(l);
    }
    let mut buf = [0u8; 2048];
    let mut total = 0u64;
    let mut snapshots = 0u64;
    // Pilots come and go signed in: every 20 ticks one leaves (its suit sleeps where it is) and
    // the one who left before comes back and wakes in theirs.
    let mut asleep: Option<(u16, (u16, u16))> = None;
    let (mut slept, mut woke) = (0, 0);
    for step in 0..1_300u32 {
        let leaving = (step >= 300 && step.is_multiple_of(20)).then_some(((step / 20) % 64) as u16);
        let mut returning = None;
        if let Some(slot) = leaving {
            if let Some((back, id)) = asleep.take() {
                returning = Some(back);
                let join = Control::Join {
                    slot: back,
                    pilot: PilotKind::Human,
                    frame: FrameId::Leo,
                    faction: Faction::Colonies,
                    max_datagram: MAX_DATAGRAM as u16,
                    comeback: Comeback { sleeper: Some(id), credits: 500 },
                    launch: None,
                };
                shared.control.push(join).unwrap();
            }
            shared.control.push(Control::Sleep { slot }).unwrap();
        }
        // Network side (outside the counted region): inputs in, packets out, oracle ring drained.
        let next = sector.sim.next_tick();
        for l in &mut leases {
            let mut p = InputPacket {
                ack_snapshot: next.saturating_sub(3),
                client_time_ms: step as u16,
                count: 1,
                ..InputPacket::default()
            };
            let a = (step as f32 * 0.02 + f32::from(l.slot)).sin();
            p.cmds[0] = InputCmd {
                tick: next + 2,
                view_tick_q4: (next << 4) - 30,
                aim: Vec3::new(a, 0.1, 1.0).normalize(),
                thrust: [40, 0, 90],
                buttons: FLIGHT_ASSIST | FIRE_PRIMARY | ZERO,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let ((), n) = bc_alloc::count(|| sector.tick());
        if step >= 300 {
            total += n; // the first 300 ticks let the doll spawner fill the sector
        }
        if let Some(slot) = leaving {
            let st = &shared.slots[slot as usize];
            if st.outcome() == Outcome::Asleep {
                slept += 1;
                asleep = st.suit_id().map(|id| (slot, id));
            }
        }
        if let Some(back) = returning
            && shared.slots[back as usize].outcome() == Outcome::Woke
        {
            woke += 1;
        }
        for ring in &mut egress.rings {
            while let Some(len) = read_packet(ring, &mut buf) {
                assert!(len <= MAX_DATAGRAM);
                snapshots += 1;
            }
        }
        while oracle.pictures.pop().is_ok() {}
        while shared.notes.pop().is_some() {}
    }
    assert!(slept > 40 && woke > 40, "slept {slept}, woke {woke}");
    let m = &shared.metrics;
    println!(
        "snapshots {snapshots}, max {} B, pictures {}, alive {}",
        bc_sector::Metrics::load(&m.snapshot_max_bytes),
        bc_sector::Metrics::load(&m.pictures),
        bc_sector::Metrics::load(&m.suits_alive)
    );
    assert!(snapshots > 60_000, "snapshots {snapshots}");
    assert!(bc_sector::Metrics::load(&m.pictures) > 0);
    assert_eq!(total, 0, "heap operations inside sector ticks: {total}");
}

#[test]
fn a_missile_barrage_never_allocates() {
    use bc_proto::SnapshotReader;
    use bc_proto::buttons::{FIRE_SECONDARY, SPECIAL};

    // 32 missile boats (Heavyarms and Sandrocks, each locking the nearest doll) against 128 Mobile
    // Dolls, with Full Open Attacks.
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 128, seed: 9, ..SimConfig::default() },
        max_clients: 32,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        let frame = if l.slot % 2 == 0 { FrameId::Heavyarms } else { FrameId::Sandrock };
        shared
            .control
            .push(Control::Join {
                slot: l.slot,
                pilot: PilotKind::Human,
                frame,
                faction: Faction::Colonies,
                max_datagram: MAX_DATAGRAM as u16,
                comeback: Comeback::default(),
                launch: None,
            })
            .unwrap();
        leases.push(l);
    }
    let mut buf = [0u8; 2048];
    let (mut total, mut missiles_seen) = (0u64, 0u64);
    for step in 0..1_200u32 {
        let next = sector.sim.next_tick();
        for (n, l) in leases.iter_mut().enumerate() {
            // Aim at and designate the nearest Oz suit (outside the counted region).
            let me = sector
                .sim
                .suits
                .alive
                .iter()
                .find(|&i| sector.sim.suits.pilot[i] == PilotKind::Human && i % 32 == n);
            let (aim, lock) = me
                .and_then(|i| {
                    let at = sector.sim.suits.flight[i].pos;
                    sector
                        .sim
                        .suits
                        .alive
                        .iter()
                        .filter(|&j| sector.sim.suits.faction[j] == Faction::Oz)
                        .min_by(|&a, &b| {
                            let d = |j: usize| (sector.sim.suits.flight[j].pos - at).length_squared();
                            d(a).total_cmp(&d(b))
                        })
                        .map(|j| ((sector.sim.suits.flight[j].pos - at).normalize_or(Vec3::Z), j as u16))
                })
                .unwrap_or((Vec3::Z, bc_proto::NO_SLOT));
            let mut buttons = FLIGHT_ASSIST | FIRE_SECONDARY | FIRE_PRIMARY;
            if (step + n as u32).is_multiple_of(450) {
                buttons |= SPECIAL;
            }
            let mut p = InputPacket {
                ack_snapshot: next.saturating_sub(3),
                client_time_ms: step as u16,
                count: 1,
                ..InputPacket::default()
            };
            p.cmds[0] = InputCmd {
                tick: next + 2,
                view_tick_q4: (next << 4) - 30,
                aim,
                thrust: [0, 0, 60],
                buttons,
                lock_target: lock,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let ((), n) = bc_alloc::count(|| sector.tick());
        if step >= 300 {
            total += n;
        }
        for ring in &mut egress.rings {
            while let Some(len) = read_packet(ring, &mut buf) {
                assert!(len <= MAX_DATAGRAM);
                let mut r = SnapshotReader::new(&buf[..len]).expect("a snapshot");
                let (_, _) = (r.own().expect("own"), r.zero().expect("zero"));
                while let Ok(Some(_)) = r.next_event() {}
                while let Ok(Some(_)) = r.next_rock() {}
                while let Ok(Some(_)) = r.next_missile() {
                    missiles_seen += 1;
                }
            }
        }
    }
    println!(
        "peak missiles {}, missile records sent {missiles_seen}, max snapshot {} B",
        sector.sim.peak_missiles,
        bc_sector::Metrics::load(&shared.metrics.snapshot_max_bytes)
    );
    assert!(sector.sim.peak_missiles > 150, "peak missiles {}", sector.sim.peak_missiles);
    assert!(missiles_seen > 10_000, "missiles reached the clients: {missiles_seen}");
    assert_eq!(total, 0, "heap operations inside sector ticks: {total}");
}

#[test]
fn survival_launches_docks_and_losses_never_allocate() {
    use bc_proto::buttons::GRIP;
    use bc_sector::{Report, SlotState};
    use bc_sim::SuitId;
    use bc_sim::bodies::Body;
    use bc_sim::content::salvage::DOCK_CENTER;
    use bc_sim::handle::Handle;
    use bc_sim::sim::Loadout;

    // 64 pilots flying the suits they built among 128 Mobile Dolls: every few ticks one docks and
    // one launches again; the dolls shoot some down. Now and then one is left in MO-II's Aft Well
    // as its pilot logs off (the sector records it), and the suit is put back from that record,
    // as a restarted server would. Zodiac's aces come out among the Dolls, and pilots down them and
    // send the tugs for their wrecks.
    let cfg = SectorConfig {
        sim: SimConfig {
            target_dolls: 128,
            seed: 11,
            survival: true,
            ace_every: 150,
            ..SimConfig::default()
        },
        max_clients: 64,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let loadout = |slot: u16| {
        let mut l = Loadout::full(FrameId::Leo);
        // A full rack: the hotbar's uses below.
        for kit in bc_sim::content::Kit::ALL {
            l.kits.set(kit, 3);
        }
        if slot.is_multiple_of(3) {
            l.parts[bc_proto::Part::ArmR as usize] = 0.0;
            l.mounts = 0b110;
        }
        l.parts[bc_proto::Part::Head as usize] = 0.4;
        // Some fly with failing systems: a leak, coughing thrusters, a hurt pilot.
        if slot.is_multiple_of(2) {
            use bc_sim::content::systems::{DAMAGED, FAILED};
            use bc_sim::content::{System, Systems};
            l.systems = Systems::OK
                .with(System::Tank, FAILED)
                .with(System::MainThrusters, DAMAGED)
                .with(System::Cockpit, DAMAGED);
            use bc_sim::content::ModuleKind;
            l.modules.set(1, Some(ModuleKind::DamageControl));
            l.modules.set(4, Some(ModuleKind::AuxiliaryTank));
        }
        l
    };
    let launch = |slot: u16| Control::Join {
        slot,
        pilot: PilotKind::Human,
        frame: FrameId::Leo,
        faction: Faction::Colonies,
        max_datagram: MAX_DATAGRAM as u16,
        comeback: Comeback::default(),
        launch: Some(loadout(slot)),
    };
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        shared.control.push(launch(l.slot)).unwrap();
        leases.push(l);
    }
    let mut buf = [0u8; 2048];
    let (mut total, mut docked, mut relaunched, mut lost) = (0u64, 0u32, 0u32, 0u32);
    let (mut parked, mut restored, mut reparked, mut record) = (0u32, 0u32, 0u32, None);
    let mut aces = 0u32;
    for step in 0..1_300u32 {
        // Network side: one pilot is put down in the Aft Well, and leaves a few ticks later; the
        // last suit recorded there is put back.
        if step >= 300 && step % 11 >= 5 {
            let slot = ((step / 11) % 64) as u16;
            let status = &shared.slots[slot as usize];
            match (step % 11, status.state(), status.suit_id()) {
                (5, SlotState::Active, Some((idx, generation))) => {
                    let id = SuitId(Handle { idx, generation });
                    let _ = sector.sim.place_on(id, Body::Landmark(0), Vec3::new(-1.0, 0.02, 0.02));
                }
                (9, SlotState::Active, _) => shared.control.push(Control::Sleep { slot }).unwrap(),
                (10, ..) => {
                    if let Some(rec) = record.take() {
                        shared.control.push(Control::Restore { key: step, rec }).unwrap();
                    }
                }
                _ => {}
            }
        }
        // Network side, outside the counted region: one pilot at rest in the dock asks to dock,
        // one who's in the hangar launches again.
        if step >= 300 && step.is_multiple_of(5) {
            let slot = ((step / 5) % 64) as u16;
            match shared.slots[slot as usize].state() {
                SlotState::Active => {
                    if let Some((idx, _)) = shared.slots[slot as usize].suit_id() {
                        let f = &mut sector.sim.suits.flight[usize::from(idx)];
                        f.pos = DOCK_CENTER + Vec3::new(0.0, 60.0, 0.0);
                        f.vel = Vec3::ZERO;
                    }
                    shared.control.push(Control::Dock { slot }).unwrap();
                }
                _ => {
                    shared.control.push(launch(slot)).unwrap();
                    relaunched += 1;
                }
            }
        }
        // Pilots use what's in their racks (patch kits, coolant, chaff, stims).
        if step >= 300 && step % 3 == 1 {
            let slot = ((step / 3) % 64) as u16;
            let kit = bc_sim::content::Kit::ALL[(step / 3 % 4) as usize];
            shared.control.push(Control::UseKit { slot, kit }).unwrap();
        }
        // And now and then one is shot down (as the damage step leaves a suit it destroys).
        if step >= 300 && step % 7 == 3 {
            let slot = ((step / 7) % 64) as u16;
            if let (SlotState::Active, Some((idx, _))) =
                (shared.slots[slot as usize].state(), shared.slots[slot as usize].suit_id())
            {
                let i = usize::from(idx);
                if sector.sim.suits.alive.get(i) {
                    sector.sim.suits.alive.set(i, false);
                    sector.sim.suits.respawn_at[i] = sector.sim.tick() + 20;
                }
            }
        }
        // The ace out is downed by a pilot (as the damage step leaves a suit it destroys).
        if step >= 300
            && let Some((i, _)) = sector.sim.ace_out()
            && sector.sim.suits.alive.get(i)
            && let Some(shooter) = (0..64)
                .filter(|&s| shared.slots[s].state() == SlotState::Active)
                .find_map(|s| shared.slots[s].suit_id())
                .map(|(idx, _)| usize::from(idx))
        {
            sector.sim.strike(i, bc_proto::Part::Torso, 1.0e9, shooter, bc_proto::WeaponKind::BeamRifle);
        }
        let next = sector.sim.next_tick();
        // And those asleep there are shot at (as the damage step leaves a suit it hits): what's
        // left of them goes to the server.
        if step >= 300 && step % 13 == 0 {
            let s = &mut sector.sim.suits;
            for i in s.sleeping.iter() {
                s.last_hit[i] = next;
            }
        }
        for l in &mut leases {
            let mut p =
                InputPacket { ack_snapshot: next.saturating_sub(3), count: 1, ..InputPacket::default() };
            p.cmds[0] = InputCmd {
                tick: next + 2,
                view_tick_q4: (next << 4) - 30,
                aim: Vec3::new(0.3, 0.2, -1.0).normalize(),
                thrust: [0, 20, 60],
                // Gripping: put down on a body, a suit stays there (and walks).
                buttons: FLIGHT_ASSIST | FIRE_PRIMARY | GRIP,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let ((), n) = bc_alloc::count(|| sector.tick());
        if step >= 300 {
            total += n;
        }
        for l in &mut leases {
            while let Ok(r) = l.reports.pop() {
                match r {
                    Report::Home(_) => docked += 1,
                    Report::Lost { .. } => lost += 1,
                    // (In space there's no Proving Ground to time.)
                    Report::DockRefused | Report::Course { .. } | Report::Drill { .. } => {}
                    Report::Towed { .. } => {}
                    // Its wreck, for the tugs (the session's to ask, on its terms).
                    Report::AceDown { ace, hulk, generation } => {
                        aces += 1;
                        shared.control.push(Control::Claim { slot: l.slot, hulk, generation, ace }).unwrap();
                    }
                    Report::Parked { rec, .. } => {
                        parked += 1;
                        record = Some(rec);
                    }
                }
            }
        }
        while let Some(r) = shared.restored.pop() {
            restored += 1;
            // Every other one as though it came back after the server stopped waiting: it goes.
            if restored % 2 == 0 {
                let (suit, generation) = (r.suit, r.generation);
                shared.control.push(Control::Discard { suit, generation }).unwrap();
            }
        }
        while shared.reparked.pop().is_some() {
            reparked += 1;
        }
        for ring in &mut egress.rings {
            while read_packet(ring, &mut buf).is_some() {}
        }
    }
    println!(
        "docked {docked}, relaunched {relaunched}, lost {lost}, parked {parked}, restored {restored}, \
         reparked {reparked}, aces {aces}"
    );
    assert!(aces >= 3, "aces downed {aces}");
    assert!(
        docked > 50 && relaunched > 50 && lost > 50,
        "docked {docked}, relaunched {relaunched}, lost {lost}"
    );
    assert!(
        parked > 20 && restored > 20 && reparked > 10,
        "parked {parked}, restored {restored}, reparked {reparked}"
    );
    assert_eq!(total, 0, "heap operations inside sector ticks: {total}");
}

#[test]
fn riders_and_hides_never_allocate() {
    use bc_proto::buttons::{BOOST, GRIP};
    use bc_sim::SuitId;
    use bc_sim::bodies::{Body, GRIP_MIN_AXIS};
    use bc_sim::handle::Handle;

    // 64 pilots among 256 Mobile Dolls, 16 of them on bodies: four lying crouched in MO-II's Aft
    // Well, four walking its core, four on Hermit and four on big rocks. Those walk, run, hop and
    // crouch; two of the hiders log off where they lie and stay asleep, and the walkers take
    // turns to sleep and wake on their feet.
    let cfg = SectorConfig {
        sim: SimConfig { target_dolls: 256, seed: 3, ..SimConfig::default() },
        max_clients: 64,
        ..SectorConfig::default()
    };
    let (mut sector, shared, mut egress, _oracle) = bc_sector::build(cfg);
    let mut leases = Vec::new();
    while let Some(l) = shared.leases.pop() {
        let frame = if l.slot % 2 == 0 { FrameId::Leo } else { FrameId::Heavyarms };
        shared
            .control
            .push(Control::Join {
                slot: l.slot,
                pilot: PilotKind::Human,
                frame,
                faction: Faction::Colonies,
                max_datagram: MAX_DATAGRAM as u16,
                comeback: Comeback::default(),
                launch: None,
            })
            .unwrap();
        leases.push(l);
    }
    sector.tick();
    const RIDERS: u16 = 16;
    let rocks: Vec<u16> = (0..sector.sim.field.len())
        .filter(|&r| sector.sim.field.rocks()[r].axes.min_element() >= GRIP_MIN_AXIS)
        .map(|r| r as u16)
        .collect();
    for slot in 0..RIDERS {
        let (idx, generation) = shared.slots[usize::from(slot)].suit_id().expect("a suit");
        let k = f32::from(slot / 4);
        let (body, dir) = match slot % 4 {
            0 => (Body::Landmark(0), Vec3::new(-1.0, 0.02 * k, 0.02)),
            1 => (Body::Landmark(0), Vec3::new(1.0, 0.45 + 0.1 * k, 0.45 - 0.1 * k)),
            2 => (Body::Landmark(1), Vec3::new(0.5 + 0.1 * k, 1.0, 0.3)),
            _ => (Body::Rock(rocks[usize::from(slot / 4)]), Vec3::new(0.2, 1.0, 0.1 * k)),
        };
        assert!(
            sector.sim.place_on(SuitId(Handle { idx, generation }), body, dir),
            "slot {slot} on {body:?}"
        );
    }
    let hider = |slot: u16| slot < RIDERS && slot.is_multiple_of(4);
    let mut buf = [0u8; 2048];
    let mut total = 0u64;
    let (mut grounded, mut aloft, mut parked, mut hidden) = (0u64, 0u64, 0u64, 0u64);
    let mut asleep: Option<(u16, (u16, u16))> = None;
    let (mut slept, mut woke) = (0, 0);
    for step in 1..1_300u32 {
        // Network side (outside the counted region): two hiders log off for good; every 30 ticks
        // a walker logs off, and the one before comes back.
        if step == 350 {
            for slot in [0, 4] {
                shared.control.push(Control::Sleep { slot }).unwrap();
            }
        }
        let leaving = (step >= 300 && step.is_multiple_of(30))
            .then_some(((step / 30) % 16) as u16)
            .filter(|&s| !hider(s));
        let mut returning = None;
        if let Some(slot) = leaving {
            if let Some((back, id)) = asleep.take() {
                returning = Some(back);
                let join = Control::Join {
                    slot: back,
                    pilot: PilotKind::Human,
                    frame: FrameId::Leo,
                    faction: Faction::Colonies,
                    max_datagram: MAX_DATAGRAM as u16,
                    comeback: Comeback { sleeper: Some(id), credits: 0 },
                    launch: None,
                };
                shared.control.push(join).unwrap();
            }
            shared.control.push(Control::Sleep { slot }).unwrap();
        }
        let next = sector.sim.next_tick();
        for l in &mut leases {
            let mut p = InputPacket {
                ack_snapshot: next.saturating_sub(3),
                client_time_ms: step as u16,
                count: 1,
                ..InputPacket::default()
            };
            let a = (step as f32 * 0.02 + f32::from(l.slot)).sin();
            let aim = Vec3::new(a, 0.1, 1.0).normalize();
            // (Flight assist is a state a client keeps on: in the air in a body's grip, it holds
            // the walking pace.)
            let grip = FLIGHT_ASSIST | GRIP;
            let (thrust, buttons) = match l.slot {
                // Crouched and still: hidden once it settles.
                s if hider(s) => ([0, -127, 0], grip),
                // Walking, running, hopping, crouching, standing.
                s if s < RIDERS => match (step + u32::from(s) * 7) % 150 {
                    0..60 => ([40, 0, 127], grip),
                    60..90 => ([0, 0, 127], grip | BOOST),
                    90 => ([0, 127, 0], grip),
                    91..120 => ([0, 0, 0], grip),
                    120..135 => ([-60, -127, 80], grip),
                    _ => ([0, 64, 0], grip),
                },
                _ => ([40, 0, 90], FLIGHT_ASSIST | FIRE_PRIMARY | ZERO),
            };
            p.cmds[0] = InputCmd {
                tick: next + 2,
                view_tick_q4: (next << 4) - 30,
                aim,
                thrust,
                buttons,
                ..InputCmd::default()
            }
            .quantized();
            let _ = l.input.push(InputMsg { packet: p, recv_us: u64::from(step) * 33_333 });
        }
        let ((), n) = bc_alloc::count(|| sector.tick());
        if step >= 300 {
            total += n; // the first 300 ticks let the doll spawner fill the sector
            let m = &shared.metrics;
            let load = |c| bc_sector::Metrics::load(c);
            grounded = grounded.max(load(&m.grounded));
            aloft = aloft.max(load(&m.aloft));
            parked = parked.max(load(&m.parked));
            hidden = hidden.max(load(&m.hidden));
        }
        if let Some(slot) = leaving {
            let st = &shared.slots[slot as usize];
            if st.outcome() == Outcome::Asleep {
                slept += 1;
                asleep = st.suit_id().map(|id| (slot, id));
            }
        }
        if let Some(back) = returning
            && shared.slots[back as usize].outcome() == Outcome::Woke
        {
            woke += 1;
        }
        for ring in &mut egress.rings {
            while let Some(len) = read_packet(ring, &mut buf) {
                assert!(len <= MAX_DATAGRAM);
            }
        }
        while shared.notes.pop().is_some() {}
    }
    println!(
        "at most {grounded} suits on their feet, {aloft} aloft, {parked} parked, {hidden} hidden; slept {slept}, woke {woke}"
    );
    assert!(slept > 20 && woke > 20, "slept {slept}, woke {woke}");
    assert!(
        grounded > 0 && aloft > 0 && parked > 0 && hidden > 0,
        "grounded {grounded}, aloft {aloft}, parked {parked}, hidden {hidden}"
    );
    assert_eq!(total, 0, "heap operations inside sector ticks: {total}");
}
