//! The enemy's gun, against the real simulation: a Leo grabs a Taurus's arm and fires its rifle in
//! bursts from the hand while turning hard (firing busies the arms, which slows the turn), then
//! lets go and fires its own machine cannon. Seeded from any snapshot and told which gun is in
//! hand, the owner's prediction keeps time with the server's arms tick after tick, and flies the
//! suit where the server does; told nothing of the gun, it doesn't.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_client_core::{InputHistory, Predictor};
use bc_proto::buttons::{FIRE_SECONDARY, FLIGHT_ASSIST, GRAB};
use bc_proto::snapshot::{OwnArms, own_flags};
use bc_proto::{
    ChunkDesc, ChunkKind, Faction, FrameId, InputCmd, OwnState, Part, PilotKind, Segment, SnapshotHeader,
    SnapshotReader, SnapshotWriter, WeaponKind,
};
use bc_sim::chunks::{self, Motion};
use bc_sim::content::ArmSlot;
use bc_sim::content::salvage::part_mass_kg;
use bc_sim::math::look_rotation;
use bc_sim::{Sim, SimConfig};
use glam::{Quat, Vec3};

/// Ticks flown, and the tick the pilot lets go of the arm.
const RUN: u32 = 500;
const LET_GO: u32 = 330;
/// How far ahead of each snapshot the prediction flies.
const LEAD: u32 = 10;

fn over_the_wire(own: &OwnState) -> OwnState {
    let mut buf = [0u8; 256];
    let mut w = SnapshotWriter::new(&mut buf, 256);
    w.header(&SnapshotHeader::default());
    w.own(Some(own));
    let n = w.finish().unwrap();
    SnapshotReader::new(&buf[..n]).unwrap().own().unwrap().unwrap()
}

fn command(t: u32) -> InputCmd {
    // Swing hard from side to side, firing in bursts, holding the arm until it lets go.
    let side = if (t / 30).is_multiple_of(2) { 1.2 } else { -1.2 };
    let aim = Vec3::new(side, 0.3 * side, 1.0).normalize();
    let mut buttons = FLIGHT_ASSIST | if t < LET_GO { GRAB } else { 0 };
    if t % 40 < 25 {
        buttons |= FIRE_SECONDARY;
    }
    InputCmd { tick: t, view_tick_q4: t << 4, aim, thrust: [0, 0, 90], buttons, ..InputCmd::default() }
        .quantized()
}

/// What the server did, tick by tick (index = tick).
struct Flown {
    cmds: Vec<InputCmd>,
    owns: Vec<OwnState>,
    in_hand: Vec<Option<WeaponKind>>,
    poses: Vec<(Vec3, Vec3, Quat)>,
    from_hand: u32,
    own_shots: u32,
}

fn fly() -> Flown {
    let mut sim = Sim::new(SimConfig { target_dolls: 0, field_rocks: 0, ..SimConfig::default() });
    let face = look_rotation(Vec3::Z, Vec3::Y);
    let me =
        sim.spawn_at(FrameId::Leo, Faction::Colonies, PilotKind::Human, Vec3::new(0.0, 4_000.0, 0.0), face);
    let me = me.unwrap();
    let i = me.idx();
    // A Taurus's right arm, at rest by the Leo's left hand.
    let desc = ChunkDesc {
        kind: ChunkKind::Limb { frame: FrameId::Taurus, faction: Faction::Oz, part: Part::ArmR },
        seed: 3,
        mass_kg: part_mass_kg(FrameId::Taurus, Part::ArmR),
    };
    let f = sim.suits.flight[i];
    let at = f.pos + f.rot * ArmSlot::Left.muzzle() - Vec3::X * (chunks::radius(&desc) + 0.5);
    let seg = Segment { pos: at, ..Segment::default() }.quantized();
    sim.chunks.spawn(desc, Motion::Free(seg), 90_000, 0).unwrap();
    let mut out = Flown {
        cmds: vec![InputCmd::default()],
        owns: vec![over_the_wire(&sim.own_state(i))],
        in_hand: vec![None],
        poses: vec![(f.pos, f.vel, f.rot)],
        from_hand: 0,
        own_shots: 0,
    };
    for t in 1..=RUN {
        let cmd = command(t);
        sim.set_input(me, cmd);
        let (shots, held) = (sim.stats(i).shots, sim.gun_in_hand(i).is_some());
        sim.step();
        if sim.stats(i).shots > shots {
            if held {
                out.from_hand += 1;
            } else {
                out.own_shots += 1;
            }
        }
        out.cmds.push(cmd);
        out.owns.push(over_the_wire(&sim.own_state(i)));
        out.in_hand.push(sim.gun_in_hand(i).map(|g| g.0));
        let s = &sim.suits.flight[i];
        out.poses.push((s.pos, s.vel, s.rot));
    }
    out
}

/// Seeds the prediction at every snapshot (told the gun in hand, or not) and flies it `LEAD`
/// ticks on: the windows checked, and the first tick where the arms or the pose were off.
fn check(run: &Flown, told: bool) -> (u32, Option<String>) {
    let mut checked = 0;
    for seed in 1..=RUN - LEAD {
        // A grab is the server's to decide, and so is anything the own state can't foresee.
        let foreseeable = (seed + 1..=seed + LEAD).all(|t| {
            let (a, b) = (&run.owns[t as usize - 1], &run.owns[t as usize]);
            let never = |o: &OwnState| o.arms.wait.map(|w| w == OwnArms::NEVER);
            let hot = |o: &OwnState| o.flags & own_flags::OVERHEAT != 0;
            let grabbed = run.in_hand[t as usize - 1].is_none() && run.in_hand[t as usize].is_some();
            never(a) == never(b) && hot(a) == hot(b) && !grabbed
        });
        if !foreseeable {
            continue;
        }
        checked += 1;
        let mut history = InputHistory::default();
        for t in seed.saturating_sub(4)..=seed + LEAD {
            history.push(run.cmds[t as usize]);
        }
        let mut p = Predictor::default();
        p.set_in_hand(if told { run.in_hand[seed as usize] } else { None });
        p.reconcile(seed, &run.owns[seed as usize], &history);
        for t in seed + 1..=seed + LEAD {
            p.advance(&run.cmds[t as usize], &history);
            let own = &run.owns[t as usize];
            let busy = own.arms.phase != 0 || own.arms.fired_ago < 5;
            let (pos, vel, rot) = run.poses[t as usize];
            let (dp, dv) = ((p.state.pos - pos).length(), (p.state.vel - vel).length());
            let turn = 2.0 * (p.state.rot.inverse() * rot).xyz().length();
            if p.arms.busy(t + 1) != busy || dp > 0.01 || dv > 0.05 || turn > 1e-3 {
                let off =
                    format!("seeded at {seed}, tick {t}: busy {busy}, off by {dp} m, {dv} m/s, {turn} rad");
                return (checked, Some(off));
            }
        }
    }
    (checked, None)
}

#[test]
fn a_gun_in_hand_is_predicted_exactly() {
    let run = fly();
    let (from_hand, own) = (run.from_hand, run.own_shots);
    println!("{from_hand} shots from the hand, {own} of its own");
    assert!(from_hand >= 12 && own >= 20, "{from_hand} from the hand, {own} of its own");
    let (checked, off) = check(&run, true);
    println!("{checked} windows checked");
    assert!(checked > 350, "{checked} windows checked");
    assert_eq!(off, None);
    // Told nothing of the gun, the prediction fires the Leo's own machine cannon on the
    // secondary's trigger, and keeps its arms busy between the rifle's shots.
    let (_, off) = check(&run, false);
    println!("uninformed: {off:?}");
    assert!(off.is_some(), "the gun in hand made no difference to the prediction");
}
