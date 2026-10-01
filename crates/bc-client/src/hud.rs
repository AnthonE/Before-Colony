//! Cockpit HUD: flight and armour readouts, weapons and the frame's special, missile lock, target
//! brackets and markers on missiles tracking you, kill feed, the ZERO System's recommendations and
//! alerts. Plain ASCII so the embedded font renders everything.
//!
//! The crosshair is the aim. A weapon bears only within its mount's reach of the body's axis (a
//! hand's 50°, Neo-Bird's nose 2°), so while the suit is still turning onto the aim the crosshair
//! dims and a second marker shows where the primary weapon would fire. The velocity vector shows
//! which way the suit is drifting (`-o-`), or, moving backwards, the way it's drifting from
//! (`-x-`): relative to the body it's on, or to a landmark within 2 km.
//!
//! The bodies: a landmark within 3 km is named with its range and closure, relative to its
//! surface, and its hide spots marked (`<>`, from compiled content: they never say who is in one).
//! With the grip armed, a landing ring `( _ )` sits on the surface the suit is coming in on, green
//! when the next tick would catch it (the simulation's own test). On a body the flight panel says
//! how the suit stands, and its speed is over the body; the alerts say how well hidden it is.

use bc_client_core::surface::{SurfaceHint, surface_hint};
use bc_client_core::world::ObjectMotion;
use bc_client_core::{ClientCore, FeedLine};
use bc_proto::buttons::{FIRE_PRIMARY, FIRE_SECONDARY, FLIGHT_ASSIST, MELEE, MODE};
use bc_proto::snapshot::{cover, ent_flags, own_flags, zero_mode};
use bc_proto::{ChunkKind, NO_CHUNK, NO_SLOT, OwnState, Part, PilotKind, WeaponKind};
use bc_sim::bodies::Body;
use bc_sim::chunks;
use bc_sim::content::landmarks::LANDMARKS;
use bc_sim::content::salvage::{CATCH_SPEED, DOCK_CENTER, PRICE, REACH, hold_kg, material};
use bc_sim::content::{
    ArmSlot, FrameSpec, PLAYABLE_ORDER, SpecialKind, frame, frame_name, weapon, weapon_name,
};
use bc_sim::ground::{Footing, STANCE};
use bc_sim::sim::LURK_SETTLE_TICKS;
use bc_sim::world::{COLONY_CENTER, COLONY_HALF_LENGTH, COLONY_RADIUS};
use bc_sim::zero::hypotheses::Maneuver;
use bevy::prelude::*;

use crate::camera::{Chase, MainCamera};
use crate::input::{Aim, Controls};
use crate::net::{GameClient, now_s};
use crate::suits_vis::pilot_tag;
use crate::view::DrawnBodies;

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
    Salvage,
}

/// The HUD's root: shown only in the world.
#[derive(Component)]
pub struct HudRoot;

/// The screen marker on the chunk nearest the free hand.
#[derive(Component)]
pub struct GrabMarker;
/// The screen marker on the colony's dock (while there is something to sell).
#[derive(Component)]
pub struct DockMarker;
/// Where the primary weapon would fire, while the aim is beyond its reach.
#[derive(Component)]
pub struct BoreMarker;
/// The velocity vector.
#[derive(Component)]
pub struct VelocityMarker;
/// The landing ring: where the armed grip would land the suit.
#[derive(Component)]
pub struct LandingMarker;
/// A landmark's name, range and closure, while it's near.
#[derive(Component)]
pub struct LandmarkMarker(u8);
/// A hide spot: which landmark's, and which of its spots.
#[derive(Component)]
pub struct HideMarker(u8, u8);

/// A landmark is named, and its hide spots marked, within this range of its surface (m).
const LANDMARK_NEAR: f32 = 3_000.0;
/// The velocity vector is taken relative to a landmark within this range of its surface (m).
const RELATIVE_NEAR: f32 = 2_000.0;
/// With the grip armed this close to the colony's hull (m), the HUD says it can't grip there.
const HULL_NEAR: f32 = 500.0;
/// How long the HUD says the grip lost its hold (s), and that a hider was seen (s).
const LOST_SECS: f64 = 3.0;
const SEEN_SECS: f64 = 5.0;
/// Pale blue, for COLD; grey, for what's merely possible.
const PALE_BLUE: Color = Color::srgb(0.55, 0.72, 1.0);
const GREY: Color = Color::srgb(0.62, 0.66, 0.7);

/// The own suit and the bodies, as predicted: how it stands, what it could land on, which hide
/// spot it's in. What the HUD, the hints and the pause menu say of it.
#[derive(Clone, Copy, Debug)]
pub struct Footed {
    pub footing: Footing,
    pub body: Body,
    /// How high its origin rides over the ground, m.
    pub stance: f32,
    /// The hide spot it stands in.
    pub spot: Option<&'static str>,
    /// Flying: the surface its grip, armed, would land it on (the landing ring).
    pub hint: Option<SurfaceHint>,
    /// Flying within a kilometre of a surface it could land on.
    pub near: bool,
}

/// How the own suit stands with respect to the bodies, as predicted after the newest command;
/// what it could land on is the simulation's test for the next tick.
pub fn footed(core: &ClientCore) -> Footed {
    let p = &core.predict;
    let m = p.mover();
    let bodies = p.bodies(p.tick + 1);
    let spot = match (m.footing, m.anchor.body) {
        (Footing::Grounded, b @ Body::Landmark(k)) => bodies
            .hide_spot_of(b, m.anchor.local)
            .and_then(|s| p.landmarks().get(usize::from(k))?.hides.get(usize::from(s)))
            .map(|h| h.name),
        _ => None,
    };
    let flying = m.footing == Footing::Free && core.world.own.is_some_and(|o| o.alive);
    Footed {
        footing: m.footing,
        body: m.anchor.body,
        stance: m.anchor.stance,
        spot,
        hint: if flying { surface_hint(&bodies, &m.flight) } else { None },
        near: flying && bodies.nearest_grippable(&m.flight, 1_000.0, f32::INFINITY, f32::INFINITY).is_some(),
    }
}

/// The hide spot the own suit is in, as the server last had it: its name.
pub fn hide_spot(core: &ClientCore) -> Option<&'static str> {
    let own = core.world.own_sent()?;
    let s = own.surface.filter(|s| s.footing == bc_proto::snapshot::footing::GROUNDED)?;
    let bc_proto::BodyRef::Landmark(k) = s.body else { return None };
    let def = core.world.bodies.landmarks().get(usize::from(k))?;
    // The body's frame (the own state on a body is in it), as `Bodies::hide_spot_of` has it.
    def.hides.iter().find(|h| (own.pos - h.center).length() <= h.radius).map(|h| h.name)
}

/// How far a point is outside the colony's hull (m): to its side, or past an end.
fn off_the_hull(p: Vec3) -> f32 {
    let rel = p - COLONY_CENTER;
    let radial = Vec2::new(rel.y, rel.z).length() - COLONY_RADIUS;
    let axial = rel.x.abs() - COLONY_HALF_LENGTH;
    if axial <= 0.0 { radial.abs() } else { Vec2::new(axial, radial.max(0.0)).length() }
}

/// What the HUD remembers of hiding between frames.
#[derive(Default)]
pub struct Lurk {
    cover: u8,
    /// When the own suit began to settle into hiding (s).
    settling: Option<f64>,
    /// Seen (it fired, or was hit, while settled) until then (s).
    seen_until: f64,
    hits_taken: u32,
    /// How it stood last frame, and when the grip last lost its hold (s).
    footing: Option<Footing>,
    lost_at: f64,
}

/// The velocity vector's colour, and the speed below which it isn't shown (m/s).
const PALE_GREEN: Color = Color::srgb(0.75, 1.0, 0.85);
const DRIFT: f32 = 2.0;

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
        .spawn((
            HudRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
        ))
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
            // Above the armour readout, clear of the weapons panel however narrow the window.
            p.spawn((HudText::Salvage, label(13.0, AMBER, abs(Some(250.0), None, None, Some(136.0)))));
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
                BoreMarker,
                label(18.0, CYAN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                VelocityMarker,
                label(16.0, PALE_GREEN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                LandingMarker,
                label(18.0, GREEN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            for (k, def) in LANDMARKS.iter().enumerate() {
                p.spawn((
                    LandmarkMarker(k as u8),
                    label(13.0, CYAN, abs(Some(0.0), None, Some(0.0), None)),
                    Visibility::Hidden,
                ));
                for i in 0..def.hides.len() {
                    p.spawn((
                        HideMarker(k as u8, i as u8),
                        label(12.0, AMBER, abs(Some(0.0), None, Some(0.0), None)),
                        Visibility::Hidden,
                    ));
                }
            }
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
    chase: Res<Chase>,
    mut texts: Query<(&HudText, &mut Text, &mut TextColor)>,
    mut reticle: Query<
        (&mut Text, &mut TextColor),
        (With<Reticle>, Without<HudText>, Without<LeadMarker>, Without<Bracket>),
    >,
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
        (
            &mut Node,
            &mut Text,
            &mut TextColor,
            &mut Visibility,
            Has<GrabMarker>,
            Has<BoreMarker>,
            Has<VelocityMarker>,
            Has<LandingMarker>,
            Option<&LandmarkMarker>,
            Option<&HideMarker>,
        ),
        (
            Or<(
                With<GrabMarker>,
                With<DockMarker>,
                With<BoreMarker>,
                With<VelocityMarker>,
                With<LandingMarker>,
                With<LandmarkMarker>,
                With<HideMarker>,
            )>,
            Without<HudText>,
            Without<LeadMarker>,
            Without<Bracket>,
            Without<Reticle>,
        ),
    >,
    mut sales: Local<Sales>,
    mut lurk: Local<Lurk>,
    bodies: Res<DrawnBodies>,
) {
    let game = game.borrow();
    let core = &game.core;
    let world = &core.world;
    let now = now_s();
    let t = core.render_tick(now);
    let own = world.own;
    let zero = world.zero;
    // Survival rules: the pilot flies what they built, and docks to go home.
    let survival = core.welcome.is_some_and(|w| w.survival);
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
    // As flown: the command the prediction flies (the autopilot's too, which flies unassisted when
    // G-strain builds); the server's word while ZERO has the controls.
    let assisted = match own {
        Some(o) if o.zero_mode == zero_mode::SEIZED => o.flags & own_flags::FLIGHT_ASSIST != 0,
        Some(_) if core.inputs.newest != 0 => core.last_cmd.buttons & FLIGHT_ASSIST != 0,
        _ => controls.flight_assist,
    };
    let fa = if assisted { "FA ON" } else { "FA OFF" };
    let mode = if game.autopilot { format!("AUTOPILOT ({})", game.brain.name()) } else { "MANUAL".into() };
    // The ping is a default until the first snapshot measures it.
    let ping =
        if core.clock.synced() { format!("{:.0} ms", core.clock.rtt * 1_000.0) } else { "--".to_string() };
    set(
        HudText::Status,
        format!(
            "BEFORE COLONY  L1 COLONY CLUSTER\nLINK OK  ping {}  tick {}\n{}  {}\ncontacts {}  hits {}  kills {}  deaths {}",
            ping,
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
        // The suit as drawn: its form, speed and the pilot's body, all at the same moment.
        let view = core.own_view().copied();
        let form = view.map_or(o.frame, |v| v.frame);
        let spec = frame(form);
        let s = &core.predict.state;
        // On a body, the speed is over it.
        let (speed, g, strain, limited) = view.map_or((o.vel.length(), 0.0, o.g_strain, false), |v| {
            let speed = v.ground.map_or(v.flight_vel.length(), |g| g.rel_vel.length());
            (speed, v.g, v.g_strain, v.g_limited)
        });
        let feet = footed(core);
        let stands = match feet.footing {
            Footing::Grounded if feet.stance < STANCE => "  CROUCHED".to_string(),
            Footing::Grounded => "  GROUNDED".to_string(),
            Footing::Aloft => {
                let alt = view.and_then(|v| v.ground).map_or(0.0, |g| g.height - STANCE);
                format!("  ALOFT  ALT {:.0} m", alt.max(0.0))
            }
            Footing::Free if controls.grip => "  GRIP ARMED".to_string(),
            Footing::Free => String::new(),
        };
        set(
            HudText::Flight,
            format!(
                "{} {}\nSPD {:>6.0} m/s{}\nPROP {} {:>3.0}%\nHEAT {} {:>3.0}%\nENGY {} {:>3.0}%\nG   {:>4.1} g {:<3} STRAIN {}",
                bc_sim::content::frame_designation(form),
                frame_name(form).to_uppercase(),
                speed,
                stands,
                bar(s.propellant / spec.propellant_cap, 10),
                100.0 * s.propellant / spec.propellant_cap,
                bar(o.heat, 10),
                o.heat * 100.0,
                bar(o.energy, 10),
                o.energy * 100.0,
                g,
                // Flight assist holding the pilot under their G tolerance.
                if limited { "LIM" } else { "" },
                bar(strain, 8),
            ),
            None,
        );
        let names = ["HEAD", "TORSO", "L-ARM", "R-ARM", "LEGS", "BPACK"];
        let mut armor = String::from("ARMOR\n");
        for (i, n) in names.iter().enumerate() {
            let f = o.parts[i];
            // Without the head's main camera the cockpit sees through the sub-camera.
            let lost = match f <= 0.0 {
                true if i == Part::Head as usize && chase.sub_camera() => "LOST  SUB-CAM",
                true => "LOST",
                false => "",
            };
            armor.push_str(&format!("{n:<6}{} {lost}\n", bar(f, 8)));
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
        if survival {
            // The hangar's credits; what this sortie has earned in bounties is paid on docking.
            sv.push_str(&format!("   CR {}", core.hangar.credits()));
            if o.credits > 0 {
                sv.push_str(&format!("  +{} bounty", o.credits));
            }
            sv.push('\n');
        } else {
            sv.push_str(&format!("   CR {}\n", o.credits));
            if let Some(before) = sales.credits
                && o.credits > before
            {
                sales.last = Some((o.credits - before, now));
            }
            sales.credits = Some(o.credits);
        }
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
            sv.push_str(if survival {
                "IN THE DOCK  ENTER: into your bay\n"
            } else {
                "DOCKED  colony salvage yard\n"
            });
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
    let drawn = core.own_view().copied();
    let own_pos = drawn.map_or(Vec3::ZERO, |v| v.pos);
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
    // Hiding, as the server has it: settling into it, and seen again for firing or being hit.
    let feet = footed(core);
    let cover_now = own.filter(|o| o.alive).map_or(cover::EXPOSED, |o| o.cover);
    if cover_now == cover::SETTLING && lurk.cover != cover::SETTLING {
        lurk.settling = Some(now);
    }
    let fought = core.last_cmd.buttons & (FIRE_PRIMARY | FIRE_SECONDARY | MELEE) != 0
        || world.hits_taken != lurk.hits_taken;
    if fought && matches!(lurk.cover, cover::COLD | cover::HIDDEN) {
        lurk.seen_until = now + SEEN_SECS;
    }
    if cover_now != cover::SETTLING {
        lurk.settling = None;
    }
    lurk.cover = cover_now;
    lurk.hits_taken = world.hits_taken;
    // The grip let go of its own accord (too high, too fast, or the rock gone), still armed.
    if feet.footing == Footing::Free && lurk.footing.is_some_and(|f| f != Footing::Free) && controls.grip {
        lurk.lost_at = now;
    }
    lurk.footing = Some(feet.footing);
    let lurking = own.filter(|o| o.alive).and_then(|_| {
        let left = |since: f64| {
            let settle = f64::from(LURK_SETTLE_TICKS) / f64::from(bc_sim::TICK_HZ);
            (settle - (now - since)).ceil().max(1.0)
        };
        Some(match cover_now {
            _ if now < lurk.seen_until => (format!("SEEN {:.0}", (lurk.seen_until - now).ceil()), AMBER),
            cover::SETTLING => (format!("HIDING {:.0}", lurk.settling.map_or(3.0, left)), AMBER),
            cover::HIDDEN => match feet.spot.or(hide_spot(core)) {
                Some(spot) => (format!("HIDDEN - {spot}"), GREEN),
                None => ("HIDDEN".into(), GREEN),
            },
            cover::COLD => ("COLD".into(), PALE_BLUE),
            _ => return None,
        })
    });
    let own_now = drawn.filter(|v| v.alive).map(|v| v.pos);
    let landing = feet.hint.filter(|_| feet.footing == Footing::Free).map(|h| {
        if !controls.grip {
            ("L - GRIP".to_string(), GREY)
        } else if h.catch {
            (format!("LAND {:.0} m {:.1} m/s", h.height.max(0.0), h.speed), GREEN)
        } else if h.speed > bc_sim::ground::CATCH_SPEED {
            (format!("TOO FAST {:.0} m/s", h.speed), AMBER)
        } else {
            (format!("LAND {:.0} m {:.1} m/s", h.height.max(0.0), h.speed), AMBER)
        }
    });
    let hull = controls.grip
        && feet.footing == Footing::Free
        && own_now.is_some_and(|p| off_the_hull(p) < HULL_NEAR);
    let signed_in = core.welcome.is_some_and(|w| w.signed_in);
    let (alert, alert_color) = match own {
        Some(o) if !o.alive && survival => ("SUIT LOST\nthe colony's rescue boat is on its way".into(), RED),
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
        Some(o) if o.alive && drawn.is_some_and(|v| v.blackout) => ("G-LOC  BLACKOUT".into(), RED),
        Some(o) if o.flags & own_flags::MISSILE_INCOMING != 0 => {
            let near = incoming.first().map_or(String::new(), |(d, _)| format!("  {}", km(*d)));
            (format!("MISSILE{near}"), RED)
        }
        Some(o) if o.flags & own_flags::MISSILE_LOCK != 0 => ("MISSILE LOCK".into(), RED),
        Some(o) if o.flags & own_flags::LOCKED_ON != 0 => ("LOCK WARNING".into(), RED),
        Some(o) if o.flags & own_flags::TRANSFORMING != 0 => ("TRANSFORMING".into(), CYAN),
        Some(o) if o.alive && now - lurk.lost_at < LOST_SECS => ("GRIP LOST".into(), AMBER),
        Some(_) if lurking.is_some() => lurking.unwrap_or_default(),
        Some(_) if landing.is_some() => landing.unwrap_or_default(),
        Some(_) if hull => ("HULL SPINS - NO GRIP".into(), GREY),
        // At rest on a body, signed in: leave now and the suit stays parked here, hidden; in a
        // hide spot, better hidden.
        Some(o) if o.flags & own_flags::PARKABLE != 0 && signed_in && feet.spot.is_some() => {
            ("HIDE SPOT - log off to leave your suit hidden".into(), GREEN)
        }
        Some(o) if o.flags & own_flags::PARKABLE != 0 && signed_in => {
            ("PARKED - safe to log off here".into(), GREEN)
        }
        _ => (String::new(), RED),
    };
    set(HudText::Alert, alert, Some(alert_color));
    // Where the primary weapon would fire: the aim, held within its mount's reach of the body's
    // axis (as the server fires it). Off the aim while the suit is still turning onto it; none
    // once the part carrying it is shot off.
    let bore = match (own, drawn) {
        (Some(o), Some(v)) if o.alive && v.alive => frame(v.frame).loadout[0]
            .filter(|m| o.parts[m.arm.part() as usize] > 0.0)
            .map(|m| bc_sim::math::clamp_to_cone(aim.dir, v.rot * Vec3::Z, m.arm.cone())),
        _ => None,
    }
    .filter(|b| b.angle_between(aim.dir) > 1f32.to_radians());
    if let Ok((mut r, mut color)) = reticle.single_mut() {
        r.0 = "+".into();
        color.0 = if bore.is_some() { Color::srgba(0.55, 0.92, 1.0, 0.35) } else { CYAN };
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
    // The chunk nearest the free hand: green when it can be grabbed.
    let mut grab_at: Option<(Vec3, String, Color)> = None;
    let mut dock_at: Option<(Vec3, String)> = None;
    if let (Some(o), Some(view)) = (own.filter(|o| o.alive), drawn) {
        let rot = view.rot;
        let right = o.parts[Part::ArmL as usize] <= 0.0;
        let hand = own_pos + rot * if right { ArmSlot::Right } else { ArmSlot::Left }.muzzle();
        let vel = view.flight_vel;
        let held_by = Some((view.pos, view.rot));
        let mut best: Option<(f32, u16)> = None;
        if o.held == NO_CHUNK {
            for (id, c) in world.objects.iter().enumerate() {
                let Some(c) = c else { continue };
                if !matches!(c.motion, ObjectMotion::Free(_)) {
                    continue;
                }
                let Some((p, _)) = world.object_pose(id as u16, t, held_by) else { continue };
                let gap = p.distance(hand) - chunks::radius(&c.desc);
                if gap < 150.0 && best.is_none_or(|(g, _)| gap < g) {
                    best = Some((gap, id as u16));
                }
            }
        }
        if let Some((gap, id)) = best
            && let (Some(c), Some((p, _))) =
                (world.objects[usize::from(id)].as_ref(), world.object_pose(id, t, held_by))
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
        if survival {
            // Home is always marked: at rest inside the dock's ring of lights, Enter.
            let d = km(DOCK_CENTER.distance(own_pos));
            let text = if o.flags & own_flags::DOCKED != 0 {
                format!("DOCK {d}  ENTER: home")
            } else {
                format!("DOCK {d}")
            };
            dock_at = Some((DOCK_CENTER, text));
        } else if cargo_value + held_value > 0 {
            dock_at = Some((
                DOCK_CENTER,
                format!("DOCK {}  ~{} cr", km(DOCK_CENTER.distance(own_pos)), cargo_value + held_value),
            ));
        }
    }
    // Which way the suit drifts: ahead of the camera, or behind it (then the way it comes from).
    // On a body, over it; flying, relative to a landmark near enough to matter.
    let landmarks: Vec<(u8, bc_sim::bodies::BodyPose, f32, Vec3)> = bodies
        .set
        .landmarks()
        .iter()
        .enumerate()
        .filter_map(|(k, d)| {
            let pose = bodies.pose(Body::Landmark(k as u8))?;
            let at = own_now?;
            let pr = d.shape.probe(pose.to_local(at));
            Some((k as u8, pose, pr.dist, pose.rot * pr.normal))
        })
        .collect();
    let drift = drawn.map(|v| match v.ground {
        Some(g) => g.rel_vel,
        None => landmarks
            .iter()
            .filter(|l| l.2 < RELATIVE_NEAR)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map_or(v.flight_vel, |l| v.flight_vel - l.1.point_vel(v.pos)),
    });
    let velocity = match (own, drawn, drift) {
        (Some(o), Some(v), Some(drift)) if o.alive && v.alive && drift.length() > DRIFT => {
            let dir = drift.normalize();
            let ahead = dir.dot(cam_tf.forward().as_vec3()) >= 0.0;
            Some(if ahead { (dir, "-o-") } else { (-dir, "-x-") })
        }
        _ => None,
    };
    // The landing ring, on the surface the armed grip would land the suit on.
    let ring = feet
        .hint
        .filter(|_| controls.grip && feet.footing == Footing::Free && own_now.is_some())
        .map(|h| (h.point, "( _ )".to_string(), if h.catch { GREEN } else { AMBER }));
    for (mut node, mut text, mut color, mut vis, grab, is_bore, is_velocity, is_ring, landmark, hide) in
        &mut markers
    {
        let centred = is_bore || is_velocity || is_ring;
        let what = if is_bore {
            bore.map(|b| (own_pos + b * 1_500.0, "( )".to_string(), CYAN))
        } else if is_velocity {
            velocity.map(|(d, s)| (cam_tf.translation() + d * 1_500.0, s.to_string(), PALE_GREEN))
        } else if is_ring {
            ring.clone()
        } else if let Some(&LandmarkMarker(k)) = landmark {
            // Named while it's near, but not while standing on it.
            landmarks
                .iter()
                .find(|l| l.0 == k && l.2 < LANDMARK_NEAR && feet.body != Body::Landmark(k))
                .zip(drawn)
                .map(|(l, v)| {
                    let closure = (v.flight_vel - l.1.point_vel(v.pos)).dot(l.3);
                    let name = LANDMARKS[usize::from(k)].name;
                    (l.1.pos, format!("{name}  {}  {closure:+.0} m/s", km(l.2.max(0.0))), CYAN)
                })
        } else if let Some(&HideMarker(k, i)) = hide {
            landmarks.iter().find(|l| l.0 == k && l.2 < LANDMARK_NEAR).and_then(|l| {
                let spot = LANDMARKS[usize::from(k)].hides.get(usize::from(i))?;
                let at = l.1.to_world(spot.center);
                Some((at, format!("<> {} {}", spot.name, km(at.distance(own_pos))), AMBER))
            })
        } else if grab {
            grab_at.clone()
        } else {
            dock_at.clone().map(|(p, s)| (p, s, AMBER))
        };
        match what.map(|(p, s, c)| (cam.world_to_viewport(cam_tf, p), s, c)) {
            Some((Ok(p), s, c)) => {
                // Symbols are centred on their point; labels start just left of theirs.
                let (dx, dy) = if centred { (-13.0, -10.0) } else { (-20.0, -8.0) };
                node.left = Val::Px(p.x + dx);
                node.top = Val::Px(p.y + dy);
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
        .filter_map(|(slot, tr)| {
            tr.as_ref().map(|tr| (tr.sample(t, &world.bodies).pos.distance(own_pos), slot as u16))
        })
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
        let pos = track.sample(t, &world.bodies).pos;
        match cam.world_to_viewport(cam_tf, pos) {
            Ok(p) => {
                let hostile = e.faction != core.cfg.faction;
                let wreck = e.flags & ent_flags::WRECK != 0;
                // Its pilot is offline, asleep in the cockpit.
                let asleep = e.flags & ent_flags::ASLEEP != 0 && !wreck;
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
                    "{}{}{}\n{}{}{}{}",
                    world.name_of(slot),
                    pilot_tag(e.pilot),
                    warn,
                    km(dist),
                    if e.pilot == PilotKind::Agent { " agent" } else { "" },
                    if asleep { " ASLEEP" } else { "" },
                    locking.as_deref().unwrap_or("")
                );
                color.0 = if wreck {
                    Color::srgb(0.5, 0.5, 0.5)
                } else if locking.is_some() {
                    AMBER
                } else if zero_target {
                    ZERO_PINK
                } else if asleep {
                    Color::srgb(0.55, 0.66, 0.8)
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

/// The HUD is the cockpit's: hidden on the title and while the link is down.
pub fn show_hud(
    ui: Res<crate::page::Ui>,
    indoors: Res<crate::hangar::Indoors>,
    mut root: Query<&mut Visibility, With<HudRoot>>,
) {
    // On foot in the bay the page draws what the pilot needs.
    let want = if ui.playing() && !indoors.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(want);
    }
}
