//! Cockpit HUD: flight and armour readouts, weapons and the frame's special, missile lock, target
//! brackets and markers on missiles tracking you, kill feed, the ZERO System's recommendations and
//! alerts. Plain ASCII so the embedded font renders everything.

use bc_client_core::FeedLine;
use bc_client_core::world::ObjectMotion;
use bc_proto::buttons::MODE;
use bc_proto::snapshot::{ent_flags, own_flags, zero_mode};
use bc_proto::{ChunkKind, NO_CHUNK, NO_SLOT, OwnState, Part, PilotKind, WeaponKind};
use bc_sim::chunks;
use bc_sim::content::salvage::{CATCH_SPEED, DOCK_CENTER, PRICE, REACH, hold_kg, material};
use bc_sim::content::{
    ArmSlot, FrameSpec, PLAYABLE_ORDER, SpecialKind, frame, frame_name, weapon, weapon_name,
};
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
/// Target brackets on suits, then markers on missiles tracking the pilot.
const BRACKETS: usize = 24;
const MISSILE_MARKERS: usize = 8;

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
    Salvage,
}

/// The screen marker on the chunk nearest the free hand.
#[derive(Component)]
pub struct GrabMarker;
/// The screen marker on the colony's dock (while there is something to sell).
#[derive(Component)]
pub struct DockMarker;

/// Credits seen last frame, and the last sale: (amount, when).
#[derive(Default)]
pub struct Sales {
    credits: Option<u32>,
    last: Option<(u32, f64)>,
}

const ORES: [&str; 4] = ["NI-FE", "TITANIUM", "VOLATILES", "EXOTICS"];

/// A chunk's name on the HUD.
fn chunk_name(kind: ChunkKind) -> String {
    match kind {
        ChunkKind::Ore { ore } => format!("{} ORE", ORES[usize::from(ore) % 4]),
        ChunkKind::Limb { frame: f, part, .. } => {
            let part = ["HEAD", "TORSO", "L-ARM", "R-ARM", "LEGS", "BACKPACK"][part as usize];
            format!("{} {part}", frame_name(f).to_uppercase())
        }
        ChunkKind::Hulk { frame: f, .. } => format!("{} HULK", frame_name(f).to_uppercase()),
    }
}

fn tonnes(kg: u32) -> String {
    if kg < 1_000 { format!("{kg} kg") } else { format!("{:.1} t", kg as f32 / 1_000.0) }
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
                HudText::Salvage,
                label(
                    13.0,
                    AMBER,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(12.0),
                        left: Val::Percent(38.0),
                        ..default()
                    },
                ),
            ));
            p.spawn((
                GrabMarker,
                label(13.0, GREEN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                DockMarker,
                label(13.0, AMBER, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
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
            for i in 0..BRACKETS + MISSILE_MARKERS {
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

/// Seconds, from ticks.
fn secs(ticks: f32) -> f32 {
    ticks / bc_sim::TICK_HZ as f32
}

/// The frame's special on the H key, and its state: `asked` is whether MODE is held.
fn special_line(spec: &FrameSpec, o: &OwnState, asked: bool) -> Option<String> {
    let active = o.flags & own_flags::SPECIAL_ACTIVE != 0;
    let ready = o.weapon_ready & 8 != 0;
    let cooling = || format!("{:.0} s", secs(f32::from(o.special_cooldown) * 4.0).ceil());
    let timer = secs(f32::from(o.special_timer));
    let (name, state) = match spec.special {
        SpecialKind::None => return None,
        SpecialKind::Transform { to, .. } => {
            let name = if to == bc_proto::FrameId::WingZeroBird { "NEO-BIRD" } else { "MS MODE" };
            let state = if o.flags & own_flags::TRANSFORMING != 0 {
                format!("CHANGING {timer:.1} s")
            } else {
                "READY".into()
            };
            (name, state)
        }
        SpecialKind::HyperJammer { .. } => {
            let state = match (active, asked) {
                (true, _) => "JAMMING",
                // Held, but broken by firing, or short of energy.
                (false, true) => "SUPPRESSED",
                (false, false) => "OFF",
            };
            ("HYPER JAMMER", state.into())
        }
        SpecialKind::FullOpen { .. } => {
            let state = if active {
                format!("FIRING {timer:.1} s")
            } else if ready {
                "READY".into()
            } else {
                cooling()
            };
            ("FULL OPEN ATTACK", state)
        }
        SpecialKind::MeleeMove { .. } => {
            let state = if active {
                "STRIKING".into()
            } else if ready {
                "READY".into()
            } else {
                cooling()
            };
            ("CROSS CRUSHER", state)
        }
    };
    Some(format!("H   {name:<18} {state}\n"))
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
    mut markers: Query<
        (&mut Node, &mut Text, &mut TextColor, &mut Visibility, Has<GrabMarker>),
        (
            Or<(With<GrabMarker>, With<DockMarker>)>,
            Without<HudText>,
            Without<LeadMarker>,
            Without<Bracket>,
            Without<Reticle>,
        ),
    >,
    mut sales: Local<Sales>,
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
        // The form the prediction flies (a change of form shows as soon as it's made).
        let form = if o.alive { core.predict.frame() } else { o.frame };
        let spec = frame(form);
        let s = &core.predict.state;
        set(
            HudText::Flight,
            format!(
                "{} {}\nSPD {:>6.0} m/s\nPROP {} {:>3.0}%\nHEAT {} {:>3.0}%\nENGY {} {:>3.0}%\nG   {:>4.1} g  STRAIN {}",
                bc_sim::content::frame_designation(form),
                frame_name(form).to_uppercase(),
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
        if let Some(line) = special_line(spec, &o, core.last_cmd.buttons & MODE != 0) {
            w.push_str(&line);
        }
        if let Some(lock) = spec.lock_spec() {
            let line = if o.lock_target == NO_SLOT {
                "LOCK --  aim at a hostile".to_string()
            } else if o.flags & own_flags::LOCK_ACQUIRED != 0 {
                format!("LOCK {}  ACQUIRED", world.name_of(o.lock_target))
            } else {
                let progress = f32::from(o.lock_progress) / f32::from(lock.lock_ticks);
                format!("LOCK {}  {}", world.name_of(o.lock_target), bar(progress, 8))
            };
            w.push_str(&line);
            w.push('\n');
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

        // --- Salvage (bottom middle). ---
        let hold = hold_kg(o.frame);
        let cargo: u32 = o.cargo_kg.iter().map(|kg| u32::from(*kg)).sum();
        let mut sv = String::new();
        if hold > 0 {
            sv.push_str(&format!(
                "HOLD {} {}/{}",
                bar(cargo as f32 / hold as f32, 10),
                tonnes(cargo),
                tonnes(hold)
            ));
        } else {
            sv.push_str("NO HOLD");
        }
        sv.push_str(&format!("   CR {}\n", o.credits));
        if let Some(before) = sales.credits
            && o.credits > before
        {
            sales.last = Some((o.credits - before, now));
        }
        sales.credits = Some(o.credits);
        let base = spec.mass(core.predict.state.propellant);
        let accel = base / (base + o.extra_mass_kg as f32);
        match world.objects.get(usize::from(o.held)).and_then(Option::as_ref).filter(|_| o.held != NO_CHUNK) {
            Some(c) => {
                let fits = !matches!(c.desc.kind, ChunkKind::Hulk { .. })
                    && c.desc.mass_kg <= 2_500
                    && cargo + c.desc.mass_kg <= hold;
                sv.push_str(&format!(
                    "{} {} {}  {}T throw{}\n",
                    if fits { "IN HAND" } else { "TOWING" },
                    chunk_name(c.desc.kind),
                    tonnes(c.desc.mass_kg),
                    if fits { "B stow  " } else { "" },
                    if accel < 0.995 { format!("   accel {:.0}%", accel * 100.0) } else { String::new() }
                ));
            }
            None => sv.push_str(if controls.grab {
                "GRAB ON (G)  free hand reaching\n"
            } else {
                "G grab   J jettison\n"
            }),
        }
        if let Some((amount, at)) = sales.last
            && now - at < 5.0
        {
            sv.push_str(&format!("SOLD +{amount} cr\n"));
        }
        if o.flags & own_flags::DOCKED != 0 {
            sv.push_str("DOCKED  colony salvage yard\n");
        }
        set(HudText::Salvage, sv, None);
    } else {
        set(HudText::Salvage, String::new(), None);
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
                feed.push_str(&format!("{} x {} CLASH\n", world.name_of(a), world.name_of(b)))
            }
        }
    }
    set(HudText::Feed, feed, None);

    // --- Alerts. ---
    let own_pos = core.predict.render_pos();
    // Missiles tracking the pilot, nearest first.
    let mut incoming: Vec<(f32, Vec3)> = world
        .missiles()
        .filter(|m| m.latest.targets_you)
        .map(|m| {
            let p = m.pos_at(t);
            (p.distance(own_pos), p)
        })
        .collect();
    incoming.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (alert, alert_color) = match own {
        _ if game.disconnected => ("LINK LOST".to_string(), RED),
        Some(o) if !o.alive => {
            let menu: Vec<String> = PLAYABLE_ORDER
                .iter()
                .enumerate()
                .map(|(k, f)| format!("[{}] {}", k + 1, frame_name(*f)))
                .collect();
            let respawn = f32::from(o.respawn_in) * 4.0 / 30.0;
            (
                format!(
                    "DESTROYED\nrespawn in {respawn:.0} s\n{}\n{}",
                    menu[..3].join("  "),
                    menu[3..].join("  ")
                ),
                RED,
            )
        }
        Some(o) if o.zero_mode == zero_mode::SEIZED => ("ZERO HAS THE CONTROLS".into(), RED),
        Some(o) if o.flags & own_flags::BLACKOUT != 0 => ("G-LOC  BLACKOUT".into(), RED),
        Some(o) if o.flags & own_flags::MISSILE_INCOMING != 0 => {
            let near = incoming.first().map_or(String::new(), |(d, _)| format!("  {}", km(*d)));
            (format!("MISSILE{near}"), RED)
        }
        Some(o) if o.flags & own_flags::MISSILE_LOCK != 0 => ("MISSILE LOCK".into(), RED),
        Some(o) if o.flags & own_flags::LOCKED_ON != 0 => ("LOCK WARNING".into(), RED),
        Some(o) if o.flags & own_flags::TRANSFORMING != 0 => ("TRANSFORMING".into(), CYAN),
        _ => (String::new(), RED),
    };
    set(HudText::Alert, alert, Some(alert_color));
    let help = if game.autopilot || controls.locked || own.is_none() {
        String::new()
    } else {
        "CLICK TO TAKE CONTROL   WASD/Space/C thrust  Q/E roll  Shift boost  X brake\nLMB/RMB fire  F melee  H special  V flight assist  Z ZERO System  R RCS\nG grab  B stow  T throw  J jettison  (sell at the colony's -X end)".into()
    };
    set(HudText::Help, help, None);
    if let Ok(mut r) = reticle.single_mut() {
        r.0 = "+".into();
    }

    // --- Screen-space markers. ---
    let Ok((cam, cam_tf)) = camera.single() else { return };
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
    // The chunk nearest the free hand: green when it can be grabbed.
    let mut grab_at: Option<(Vec3, String, Color)> = None;
    let mut dock_at: Option<(Vec3, String)> = None;
    if let Some(o) = own.filter(|o| o.alive) {
        let rot = core.predict.state.rot;
        let right = o.parts[Part::ArmL as usize] <= 0.0;
        let hand = own_pos + rot * if right { ArmSlot::Right } else { ArmSlot::Left }.muzzle();
        let vel = core.predict.state.vel;
        let mut best: Option<(f32, u16)> = None;
        if o.held == NO_CHUNK {
            for (id, c) in world.objects.iter().enumerate() {
                let Some(c) = c else { continue };
                if !matches!(c.motion, ObjectMotion::Free(_)) {
                    continue;
                }
                let Some((p, _)) = world.object_pose(id as u16, t, &core.predict) else { continue };
                let gap = p.distance(hand) - chunks::radius(&c.desc);
                if gap < 150.0 && best.is_none_or(|(g, _)| gap < g) {
                    best = Some((gap, id as u16));
                }
            }
        }
        if let Some((gap, id)) = best
            && let (Some(c), Some((p, _))) =
                (world.objects[usize::from(id)].as_ref(), world.object_pose(id, t, &core.predict))
        {
            let ObjectMotion::Free(seg) = c.motion else { unreachable!() };
            let catchable = gap <= REACH && (seg.vel - vel).length() <= CATCH_SPEED;
            let color = if catchable { GREEN } else { AMBER };
            grab_at = Some((
                p,
                format!(
                    "[{}] {} {}  {:.0} m",
                    if catchable { "G" } else { " " },
                    chunk_name(c.desc.kind),
                    tonnes(c.desc.mass_kg),
                    gap.max(0.0)
                ),
                color,
            ));
        }
        let cargo_value: u32 = o.cargo_kg.iter().enumerate().map(|(k, kg)| u32::from(*kg) * PRICE[k]).sum();
        let held_value = world
            .objects
            .get(usize::from(o.held))
            .and_then(Option::as_ref)
            .filter(|_| o.held != NO_CHUNK)
            .map_or(0, |c| c.desc.mass_kg * PRICE[material(c.desc.kind)]);
        if cargo_value + held_value > 0 {
            dock_at = Some((
                DOCK_CENTER,
                format!("DOCK {}  ~{} cr", km(DOCK_CENTER.distance(own_pos)), cargo_value + held_value),
            ));
        }
    }
    for (mut node, mut text, mut color, mut vis, grab) in &mut markers {
        let what = if grab { grab_at.clone() } else { dock_at.clone().map(|(p, s)| (p, s, AMBER)) };
        match what.map(|(p, s, c)| (cam.world_to_viewport(cam_tf, p), s, c)) {
            Some((Ok(p), s, c)) => {
                node.left = Val::Px(p.x - 20.0);
                node.top = Val::Px(p.y - 8.0);
                text.0 = s;
                color.0 = c;
                *vis = Visibility::Visible;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    let mut shown: Vec<(f32, u16)> = world
        .entities
        .iter()
        .enumerate()
        .filter_map(|(slot, tr)| tr.as_ref().map(|tr| (tr.sample(t).pos.distance(own_pos), slot as u16)))
        .collect();
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    let lock = own.filter(|o| o.alive && o.lock_target != NO_SLOT);
    for (b, mut node, mut text, mut color, mut vis) in &mut brackets {
        if b.0 >= BRACKETS {
            // A missile tracking the pilot.
            match incoming.get(b.0 - BRACKETS).map(|&(d, p)| (d, cam.world_to_viewport(cam_tf, p))) {
                Some((d, Ok(p))) => {
                    node.left = Val::Px(p.x - 12.0);
                    node.top = Val::Px(p.y - 8.0);
                    text.0 = format!("<!> MSL {}", km(d));
                    color.0 = RED;
                    *vis = Visibility::Visible;
                }
                _ => *vis = Visibility::Hidden,
            }
            continue;
        }
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
                let warn = if e.flags & ent_flags::LOCKED_ON_YOU != 0 { " !LOCK" } else { "" };
                // The pilot's own missile lock on it: building, or acquired.
                let locking = lock.filter(|o| o.lock_target == slot).map(|o| {
                    let spec = frame(o.frame).lock_spec();
                    match spec {
                        _ if o.flags & own_flags::LOCK_ACQUIRED != 0 => "\n<< LOCKED >>".to_string(),
                        Some(l) => {
                            format!("\n<{}>", bar(f32::from(o.lock_progress) / f32::from(l.lock_ticks), 6))
                        }
                        None => String::new(),
                    }
                });
                text.0 = format!(
                    "{}{}{}\n{}{}{}",
                    world.name_of(slot),
                    pilot_tag(e.pilot),
                    warn,
                    km(dist),
                    if e.pilot == PilotKind::Agent { " agent" } else { "" },
                    locking.as_deref().unwrap_or("")
                );
                color.0 = if wreck {
                    Color::srgb(0.5, 0.5, 0.5)
                } else if locking.is_some() {
                    AMBER
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
