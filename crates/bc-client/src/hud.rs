//! Cockpit HUD: flight and armour readouts, weapons, target brackets, kill feed, the ZERO System's
//! recommendations and alerts. Plain ASCII so the embedded font renders everything.

use bc_client_core::FeedLine;
use bc_proto::snapshot::{ent_flags, own_flags, zero_mode};
use bc_proto::{Part, PilotKind, WeaponKind};
use bc_sim::content::{frame, frame_name, weapon, weapon_name};
use bc_sim::zero::hypotheses::Maneuver;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::input::{Aim, Controls};
use crate::net::{GameClient, now_s};
use crate::suits_vis::pilot_tag;

const CYAN: Color = Color::srgb(0.55, 0.92, 1.0);
const AMBER: Color = Color::srgb(1.0, 0.75, 0.25);
const RED: Color = Color::srgb(1.0, 0.3, 0.3);
const GREEN: Color = Color::srgb(0.45, 1.0, 0.55);
const ZERO_PINK: Color = Color::srgb(1.0, 0.45, 0.8);
const BRACKETS: usize = 24;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum HudText {
    Status,
    Flight,
    Armor,
    Weapons,
    Feed,
    Zero,
    Alert,
    Help,
}

#[derive(Component)]
pub struct Reticle;
#[derive(Component)]
pub struct LeadMarker;
#[derive(Component)]
pub struct Bracket(usize);

fn label(size: f32, color: Color, node: Node) -> (Text, TextFont, TextColor, Node) {
    (Text::new(""), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color), node)
}

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, Val::Px),
        right: right.map_or(Val::Auto, Val::Px),
        top: top.map_or(Val::Auto, Val::Px),
        bottom: bottom.map_or(Val::Auto, Val::Px),
        ..default()
    }
}

pub fn setup_hud(mut commands: Commands) {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        })
        .with_children(|p| {
            p.spawn((HudText::Status, label(13.0, CYAN, abs(Some(14.0), None, Some(10.0), None))));
            p.spawn((
                HudText::Zero,
                label(
                    14.0,
                    ZERO_PINK,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(10.0),
                        left: Val::Percent(36.0),
                        ..default()
                    },
                ),
            ));
            p.spawn((HudText::Feed, label(13.0, AMBER, abs(None, Some(14.0), Some(10.0), None))));
            p.spawn((HudText::Flight, label(13.0, CYAN, abs(Some(14.0), None, None, Some(12.0)))));
            p.spawn((HudText::Armor, label(13.0, CYAN, abs(Some(250.0), None, None, Some(12.0)))));
            p.spawn((HudText::Weapons, label(13.0, CYAN, abs(None, Some(14.0), None, Some(12.0)))));
            p.spawn((
                HudText::Alert,
                label(
                    22.0,
                    RED,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Percent(30.0),
                        left: Val::Percent(40.0),
                        ..default()
                    },
                ),
            ));
            p.spawn((
                HudText::Help,
                label(
                    12.0,
                    Color::srgba(0.7, 0.9, 1.0, 0.7),
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(90.0),
                        left: Val::Percent(34.0),
                        ..default()
                    },
                ),
            ));
            p.spawn((
                Reticle,
                label(
                    26.0,
                    CYAN,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Percent(50.0),
                        margin: UiRect { left: Val::Px(-7.0), top: Val::Px(-16.0), ..default() },
                        ..default()
                    },
                ),
            ));
            p.spawn((
                LeadMarker,
                label(16.0, ZERO_PINK, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            for i in 0..BRACKETS {
                p.spawn((
                    Bracket(i),
                    label(11.0, RED, abs(Some(0.0), None, Some(0.0), None)),
                    Visibility::Hidden,
                ));
            }
        });
}

fn bar(f: f32, n: usize) -> String {
    let k = ((f.clamp(0.0, 1.0) * n as f32).round() as usize).min(n);
    format!("[{}{}]", "#".repeat(k), "-".repeat(n - k))
}

fn km(d: f32) -> String {
    if d < 1_000.0 { format!("{d:.0} m") } else { format!("{:.1} km", d / 1_000.0) }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_hud(
    game: NonSend<GameClient>,
    controls: Res<Controls>,
    aim: Res<Aim>,
    mut texts: Query<(&HudText, &mut Text, &mut TextColor)>,
    mut reticle: Query<&mut Text, (With<Reticle>, Without<HudText>, Without<LeadMarker>, Without<Bracket>)>,
    mut lead: Query<
        (&mut Node, &mut Text, &mut Visibility),
        (With<LeadMarker>, Without<HudText>, Without<Bracket>, Without<Reticle>),
    >,
    mut brackets: Query<
        (&Bracket, &mut Node, &mut Text, &mut TextColor, &mut Visibility),
        (Without<HudText>, Without<LeadMarker>, Without<Reticle>),
    >,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let now = now_s();
    let t = core.render_tick(now);
    let own = world.own;
    let zero = world.zero;
    let mut set = |which: HudText, s: String, color: Option<Color>| {
        for (h, mut text, mut c) in &mut texts {
            if *h == which {
                text.0.clone_from(&s);
                if let Some(color) = color {
                    c.0 = color;
                }
            }
        }
    };

    // --- Status (top left). ---
    let fa = if controls.flight_assist { "FA ON" } else { "FA OFF" };
    let mode = if game.autopilot { "AUTOPILOT (Mobile Doll brain)" } else { "MANUAL" };
    set(
        HudText::Status,
        format!(
            "BEFORE COLONY  L1 COLONY CLUSTER\n{}  ping {:.0} ms  tick {}\n{}  {}\ncontacts {}  hits {}  kills {}  deaths {}",
            if game.disconnected {
                "LINK LOST"
            } else if core.welcome.is_some() {
                "LINK OK"
            } else {
                "CONNECTING"
            },
            core.clock.rtt * 1_000.0,
            world.tick,
            mode,
            fa,
            world.entities.iter().flatten().count(),
            world.my_hits,
            world.my_kills,
            world.my_deaths
        ),
        None,
    );

    // --- Flight, armour, weapons (bottom). ---
    if let Some(o) = own {
        let spec = frame(o.frame);
        let s = &core.predict.state;
        set(
            HudText::Flight,
            format!(
                "{} {}\nSPD {:>6.0} m/s\nPROP {} {:>3.0}%\nHEAT {} {:>3.0}%\nENGY {} {:>3.0}%\nG   {:>4.1} g  STRAIN {}",
                bc_sim::content::frame_designation(o.frame),
                frame_name(o.frame).to_uppercase(),
                s.vel.length(),
                bar(s.propellant / spec.propellant_cap, 10),
                100.0 * s.propellant / spec.propellant_cap,
                bar(o.heat, 10),
                o.heat * 100.0,
                bar(o.energy, 10),
                o.energy * 100.0,
                s.g_load,
                bar(o.g_strain, 8),
            ),
            None,
        );
        let names = ["HEAD", "TORSO", "L-ARM", "R-ARM", "LEGS", "BPACK"];
        let mut armor = String::from("ARMOR\n");
        for (i, n) in names.iter().enumerate() {
            let f = o.parts[i];
            armor.push_str(&format!("{n:<6}{} {}\n", bar(f, 8), if f <= 0.0 { "LOST" } else { "" }));
        }
        let hull_color = if o.parts[Part::Torso as usize] < 0.3 { RED } else { CYAN };
        set(HudText::Armor, armor, Some(hull_color));
        let mut w = String::new();
        for (slot, key) in [(0usize, "LMB"), (1, "RMB"), (2, "F")] {
            if let Some(m) = spec.loadout[slot] {
                let ready = o.weapon_ready & (1 << slot) != 0;
                let ammo = if weapon(m.weapon).ammo > 0 {
                    format!(" {:>3}", o.ammo[slot.min(1)])
                } else {
                    String::new()
                };
                let extra = if m.weapon == WeaponKind::TwinBusterRifle && o.charge > 0.0 {
                    format!(" CHARGE {}", bar(o.charge, 6))
                } else {
                    String::new()
                };
                w.push_str(&format!(
                    "{key:<3} {:<18}{}{}{}\n",
                    weapon_name(m.weapon).to_uppercase(),
                    if ready { " RDY" } else { " ---" },
                    ammo,
                    extra
                ));
            }
        }
        if o.flags & own_flags::ZERO_CAPABLE != 0 {
            let z = match o.zero_mode {
                zero_mode::ACTIVE => "ACTIVE",
                zero_mode::SEIZED => "SEIZED",
                zero_mode::LOCKOUT => "LOCKOUT",
                _ => "STANDBY (Z)",
            };
            w.push_str(&format!("ZERO {z}  STRAIN {}", bar(o.zero_strain, 8)));
        }
        set(HudText::Weapons, w, None);
    }

    // --- ZERO panel. ---
    match zero {
        Some(z) => {
            let target =
                if z.rec_target != bc_proto::NO_SLOT { world.name_of(z.rec_target) } else { "-".into() };
            let level = ["LOW", "MODERATE", "HIGH", "LETHAL"][z.threat_level.min(3) as usize];
            let mut s = format!(
                "Z E R O   S Y S T E M{}\nTARGET   {target}  {:.0}%\nMANEUVER {}  {:.0}%\nTHREAT   {level}  (conf {:.2})\nFLANKED  {:.0}%\n",
                if z.source_jev { "   [Jev]" } else { "" },
                z.rec_target_p * 100.0,
                Maneuver::from_index(z.rec_maneuver as usize).own_label(),
                z.rec_maneuver_p * 100.0,
                z.threat_confidence,
                z.flanked * 100.0
            );
            for th in &z.threats[..z.threat_count as usize] {
                let mut best = 0;
                for k in 1..7 {
                    if th.probs[k] > th.probs[best] {
                        best = k;
                    }
                }
                s.push_str(&format!(
                    "{:<14} next {} {:.0}%\n",
                    world.name_of(th.slot),
                    Maneuver::from_index(best).label(),
                    th.probs[best] * 100.0
                ));
            }
            set(HudText::Zero, s, None);
        }
        None => set(HudText::Zero, String::new(), None),
    }

    // --- Kill feed. ---
    let mut feed = String::new();
    for line in world.feed.iter().rev().take(6) {
        match *line {
            FeedLine::Kill { victim, killer, .. } => {
                feed.push_str(&format!("{} > {}\n", world.name_of(killer), world.name_of(victim)))
            }
            FeedLine::Seizure { pilot, active, .. } => feed.push_str(&format!(
                "{} {}\n",
                world.name_of(pilot),
                if active { "ZERO SEIZURE" } else { "released" }
            )),
            FeedLine::Clash { a, b, .. } => {
                feed.push_str(&format!("{} x {} SABER CLASH\n", world.name_of(a), world.name_of(b)))
            }
        }
    }
    set(HudText::Feed, feed, None);

    // --- Alerts. ---
    let alert = match own {
        _ if game.disconnected => "LINK LOST".to_string(),
        Some(o) if !o.alive => format!(
            "DESTROYED\nrespawn in {:.0} s   [1] Leo  [2] Wing Zero",
            f32::from(o.respawn_in) * 4.0 / 30.0
        ),
        Some(o) if o.zero_mode == zero_mode::SEIZED => "ZERO HAS THE CONTROLS".into(),
        Some(o) if o.flags & own_flags::BLACKOUT != 0 => "G-LOC  BLACKOUT".into(),
        Some(o) if o.flags & own_flags::LOCKED_ON != 0 => "LOCK WARNING".into(),
        _ => String::new(),
    };
    set(HudText::Alert, alert, None);
    let help = if game.autopilot || controls.locked || own.is_none() {
        String::new()
    } else {
        "CLICK TO TAKE CONTROL   WASD/Space/C thrust  Q/E roll  Shift boost  X brake\nLMB/RMB fire  F saber  V flight assist  Z ZERO System  R RCS".into()
    };
    set(HudText::Help, help, None);
    if let Ok(mut r) = reticle.single_mut() {
        r.0 = "+".into();
    }

    // --- Screen-space markers. ---
    let Ok((cam, cam_tf)) = camera.single() else { return };
    let own_pos = core.predict.render_pos();
    if let Ok((mut node, mut text, mut vis)) = lead.single_mut() {
        match (zero, own) {
            (Some(z), Some(o)) if z.has_solution && o.alive => {
                let point = own_pos + z.solution * 1_500.0;
                match cam.world_to_viewport(cam_tf, point) {
                    Ok(p) => {
                        node.left = Val::Px(p.x - 16.0);
                        node.top = Val::Px(p.y - 10.0);
                        text.0 = format!("[ ]{:.0}%", z.hit_p * 100.0);
                        *vis = Visibility::Visible;
                    }
                    Err(_) => *vis = Visibility::Hidden,
                }
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    let _ = aim;
    let mut shown: Vec<(f32, u16)> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(slot, tr)| tr.as_ref().map(|tr| (tr.sample(t).pos.distance(own_pos), slot as u16)))
        .collect();
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (b, mut node, mut text, mut color, mut vis) in &mut brackets {
        let Some(&(dist, slot)) = shown.get(b.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let Some(track) = world.entity(slot) else { continue };
        let e = &track.latest;
        let pos = track.sample(t).pos;
        match cam.world_to_viewport(cam_tf, pos) {
            Ok(p) => {
                let hostile = e.faction != core.cfg.faction;
                let wreck = e.flags & ent_flags::WRECK != 0;
                let zero_target = zero.is_some_and(|z| z.rec_target == slot);
                node.left = Val::Px(p.x - 30.0);
                node.top = Val::Px(p.y - 26.0);
                let lock = if e.flags & ent_flags::LOCKED_ON_YOU != 0 { " !LOCK" } else { "" };
                text.0 = format!(
                    "{}{}{}\n{}{}",
                    world.name_of(slot),
                    pilot_tag(e.pilot),
                    lock,
                    km(dist),
                    if e.pilot == PilotKind::Agent { " agent" } else { "" }
                );
                color.0 = if wreck {
                    Color::srgb(0.5, 0.5, 0.5)
                } else if zero_target {
                    ZERO_PINK
                } else if hostile {
                    RED
                } else {
                    GREEN
                };
                *vis = Visibility::Visible;
            }
            Err(_) => *vis = Visibility::Hidden,
        }
    }
}
