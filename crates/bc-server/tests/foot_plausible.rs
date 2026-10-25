//! The tick's walker (`bc_sim::foot`, stepped a tick at a time as the sector will) is a pose the
//! plaza believes: walked from Hub Gate to every key place's counter and up a block of flats'
//! stairs, every pose it sends, through the wire's `PosePacket`, is taken.

use std::time::{Duration, Instant};

use bc_client_core::city::pose_of;
use bc_client_core::city_nav::route_to_place;
use bc_client_core::walker::Guide;
use bc_proto::presence::PosePacket;
use bc_server::plaza::{Plaza, Verdict};
use bc_sim::DT;
use bc_sim::colony::city::{CITY, KERB, ROWS, Stage, block, lots, place, place_door};
use bc_sim::colony::frame::CityPos;
use bc_sim::colony::homes;
use bc_sim::content::city::PLACES;
use bc_sim::foot::{CityGround, Stride, Walker};
use glam::Vec3;

/// Ticks `w` along `g` on `ground`, sending the plaza its pose every second tick (15 a second, as
/// a client does) as pilot `id`, each heard `DT` apart: how many ticks it took.
fn walk_to_the_plaza(
    plaza: &Plaza,
    id: u16,
    ground: &CityGround,
    w: &mut Walker,
    g: &mut Guide,
    most: u32,
) -> u32 {
    let t0 = Instant::now();
    let mut seq = 0u16;
    let mut n = 0u32;
    while !g.arrived() {
        let stride = g.steer(w, DT);
        w.tick(ground, &stride);
        n += 1;
        if n.is_multiple_of(2) {
            seq = seq.wrapping_add(1);
            let mut buf = [0u8; 64];
            let len = PosePacket { seq, pose: pose_of(ground.strip, w) }.encode(&mut buf).unwrap();
            let sent = PosePacket::decode(&buf[..len]).unwrap();
            let now = t0 + Duration::from_secs_f32(n as f32 * DT);
            assert_eq!(plaza.accept(id, sent.seq, sent.pose, now, n), Verdict::Taken, "{:?}", sent.pose);
        }
        assert!(n < most, "lost at {:?}", ground.place(w.feet));
    }
    n
}

#[test]
fn the_ticks_walker_walks_from_hub_gate_to_every_place_and_the_plaza_takes_it() {
    for (i, p) in PLACES.iter().enumerate() {
        let plaza = Plaza::default();
        let id = i as u16 + 1;
        let ground = CityGround { strip: p.strip, stage: Stage(0) };
        let (gate, _) = place_door(place(&format!("hub_gate_{}", p.strip + 1)).unwrap().1);
        plaza.enter_at(id, "Kai", p.strip, gate);
        let mut w = Walker::at(CityPos::new(p.strip, gate.1, gate.0, 0.0).walker(), Vec3::X);
        for _ in 0..15 {
            w.tick(&ground, &Stride::default());
        }
        let mut g = Guide::new(route_to_place(p.strip, ground.place(w.feet), i), None);
        let n = walk_to_the_plaza(&plaza, id, &ground, &mut w, &mut g, 30 * 400);
        assert!(n > 0, "{} is somewhere", p.name);
    }
}

#[test]
fn the_ticks_walker_climbs_a_block_of_flats_and_the_plaza_takes_it() {
    let plan = (CITY.0..CITY.1)
        .flat_map(|bx| (1..=ROWS).filter_map(move |row| block(0, bx, row, Stage(0))))
        .find_map(|b| {
            lots(&b).as_slice().iter().find_map(|bd| {
                homes::plan(&b, bd).filter(|p| p.kind == homes::Kind::Flats && p.storeys >= 4 && p.sides != 0)
            })
        })
        .expect("a block of flats");
    let plaza = Plaza::default();
    let door = plan.point(0.0, -homes::SHELL - 1.2);
    plaza.enter_at(1, "Kai", 0, door);
    let mut way = vec![(plan.point(0.0, 0.8), KERB), (plan.point(0.0, homes::NEAR * 0.5), KERB)];
    for k in 0..plan.storeys - 1 {
        way.extend(plan.stairs_up(k, 0));
    }
    let ground = CityGround { strip: 0, stage: Stage(0) };
    let mut w = Walker::at(CityPos::new(0, door.1, door.0, KERB + 0.05).walker(), Vec3::X);
    let route = way.iter().map(|&((s, x), h)| CityPos::new(0, x, s, h).walker()).collect();
    let mut g = Guide::new(route, None);
    walk_to_the_plaza(&plaza, 1, &ground, &mut w, &mut g, 30 * 240);
    let up = ground.place(w.feet);
    assert!((up.h - homes::level(plan.storeys - 1)).abs() < 0.05, "on the top storey: {up:?}");
}
