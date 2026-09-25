//! The content tables: every frame and weapon is where its id says, the Gundams fly as the design
//! describes, and Neo-Bird is Wing Zero in another shape (same armour, energy, heat and tank).

use bc_proto::{FrameId, Part, WeaponKind};
use bc_sim::config::G0;
use bc_sim::content::{
    PLAYABLE_ORDER, Replication, SpecialKind, WeaponClass, frame, frame_name, playable, weapon, weapon_name,
};

#[test]
fn every_row_is_where_its_id_says() {
    for f in FrameId::ALL {
        assert_eq!(frame(f).id, f);
        assert!(!frame_name(f).is_empty());
    }
    for k in WeaponKind::ALL {
        assert_eq!(weapon(k).kind, k);
        assert!(!weapon_name(k).is_empty());
    }
}

/// Acceleration on a full tank (g) and Δv (km/s), as `docs/DESIGN.md` lists them.
#[test]
fn the_gundams_fly_as_designed() {
    let design = [
        (FrameId::Heavyarms, 5.2, 2.79),
        (FrameId::Deathscythe, 7.0, 3.32),
        (FrameId::Sandrock, 4.6, 2.43),
        (FrameId::Shenlong, 7.4, 3.32),
        (FrameId::WingZeroBird, 8.5, 3.75),
        (FrameId::WingZero, 8.0, 3.75),
    ];
    for (f, g, dv) in design {
        let s = frame(f);
        let wet = s.mass(s.propellant_cap);
        let accel = s.main_thrust / wet / G0;
        let delta_v = s.exhaust_velocity() * libm::logf(wet / s.dry_mass) / 1_000.0;
        assert!((accel - g).abs() / g < 0.02, "{f:?}: {accel:.2} g, designed {g}");
        assert!((delta_v - dv).abs() / dv < 0.02, "{f:?}: Δv {delta_v:.2} km/s, designed {dv}");
    }
}

#[test]
fn neo_bird_is_wing_zero_in_another_shape() {
    let (wz, bird) = (frame(FrameId::WingZero), frame(FrameId::WingZeroBird));
    assert_eq!(wz.part_hp, bird.part_hp);
    assert_eq!((wz.dry_mass, wz.propellant_cap, wz.isp), (bird.dry_mass, bird.propellant_cap, bird.isp));
    assert_eq!((wz.energy_cap, wz.energy_regen), (bird.energy_cap, bird.energy_regen));
    assert_eq!((wz.heat_cap, wz.heat_dissipation), (bird.heat_cap, bird.heat_dissipation));
    assert_eq!((wz.armor, wz.zero), (bird.armor, bird.zero));
    // Each transforms into the other, taking the same time.
    let (SpecialKind::Transform { to: a, ticks: ta, .. }, SpecialKind::Transform { to: b, ticks: tb, .. }) =
        (wz.special, bird.special)
    else {
        panic!("both forms transform");
    };
    assert_eq!((a, b), (FrameId::WingZeroBird, FrameId::WingZero));
    assert_eq!(ta, tb);
    // Faster in a straight line, worse in turns, and no melee.
    assert!(bird.fa_speed > wz.fa_speed && bird.main_thrust > wz.main_thrust);
    assert!(bird.ambac_rate < wz.ambac_rate && bird.side_thrust < wz.side_thrust);
    assert!(bird.loadout[2].is_none());
    assert_eq!(bird.loadout[0].map(|m| m.weapon), Some(WeaponKind::TwinBusterRifle));
    // Its wings are wider than a humanoid's arms.
    assert!(bird.capsules[Part::ArmR as usize].b.x > 7.0);
}

#[test]
fn only_pilots_frames_are_playable() {
    for f in FrameId::ALL {
        if playable(f) {
            assert!(PLAYABLE_ORDER.contains(&f), "{f:?} is playable but has no respawn key");
        }
    }
    assert!(playable(FrameId::Leo) && playable(FrameId::WingZero));
    for f in [FrameId::Taurus, FrameId::Virgo, FrameId::WingZeroBird] {
        assert!(!playable(f), "{f:?} must not be chosen");
    }
}

#[test]
fn every_mount_fits_its_weapon() {
    for f in FrameId::ALL {
        let s = frame(f);
        let mounts = s.loadout.iter().chain(s.special_mounts.iter()).flatten();
        for m in mounts {
            let w = weapon(m.weapon);
            match w.class {
                WeaponClass::Missile => {
                    let spec = w.missile.expect("missile spec");
                    assert!(spec.lock_range <= spec.seeker_range, "{:?}", w.kind);
                    assert_eq!(w.replication, Replication::List);
                }
                WeaponClass::Melee => assert!(w.melee.is_some()),
                WeaponClass::Cone => assert!(w.cone.is_some() && w.ammo > 0),
                WeaponClass::Beam | WeaponClass::Ballistic => {
                    assert!(w.ttl_ticks() > 0, "{:?} never flies", w.kind)
                }
            }
        }
        // A frame with a special that uses a mount has that mount.
        match s.special {
            SpecialKind::MeleeMove { .. } => {
                let w = s.special_mounts[0].map(|m| weapon(m.weapon));
                assert!(w.is_some_and(|w| w.class == WeaponClass::Melee), "{f:?}");
            }
            SpecialKind::FullOpen { .. } => assert!(s.special_mounts.iter().all(Option::is_some), "{f:?}"),
            _ => {}
        }
    }
    // The existing arsenal keeps its replication: beams by event, cannon rounds by flag.
    assert_eq!(weapon(WeaponKind::BeamRifle).replication, Replication::PerShot);
    assert_eq!(weapon(WeaponKind::MachineCannon).replication, Replication::Stream);
    assert!(weapon(WeaponKind::TwinBusterRifle).engulfs);
    assert!(WeaponKind::ALL.iter().filter(|k| weapon(**k).engulfs).count() == 1);
}
