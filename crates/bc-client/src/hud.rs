//! Cockpit HUD: flight and armour readouts, weapons and the frame's special, missile lock, target
//! brackets and markers on missiles tracking you, kill feed, the ZERO System's recommendations and
//! alerts. Drawn in the page's font (`UiFont`) and palette (`bc_client_core::palette`), each line
//! with a soft shadow so it reads over the sunlit hull, Earth or a blast.
//!
//! The crosshair is the aim. A weapon bears only within its mount's reach of the body's axis (a
//! hand's 50°, Neo-Bird's nose 2°), so while the suit is still turning onto the aim the crosshair
//! dims and a second marker shows where the primary weapon would fire. The velocity vector shows
//! which way the suit is drifting (`-o-`), or, moving backwards, the way it's drifting from
//! (`-x-`).

use bc_client_core::FeedLine;
use bc_client_core::palette;
use bc_client_core::world::ObjectMotion;
use bc_proto::buttons::{FLIGHT_ASSIST, MODE};
use bc_proto::snapshot::{ent_flags, own_flags, zero_mode};
use bc_proto::{ChunkKind, NO_CHUNK, NO_SLOT, OwnState, Part, PilotKind, WeaponKind};
use bc_sim::chunks;
use bc_sim::content::salvage::{CATCH_SPEED, DOCK_CENTER, PRICE, REACH, hold_kg, material};
use bc_sim::content::{
    ArmSlot, FrameSpec, PLAYABLE_ORDER, SpecialKind, frame, frame_name, weapon, weapon_name,
};
use bc_sim::zero::hypotheses::Maneuver;
use bevy::prelude::*;

use crate::camera::{Chase, MainCamera};
use crate::input::{Aim, Controls};
use crate::net::{GameClient, now_s};
use crate::suits_vis::pilot_tag;

const fn colour(hex: palette::Hex) -> Color {
    let [r, g, b] = hex.srgb();
    Color::srgb(r, g, b)
}

const CYAN: Color = colour(palette::CYAN);
const AMBER: Color = colour(palette::AMBER);
const RED: Color = colour(palette::RED);
const GREEN: Color = colour(palette::GREEN);
const ZERO_PINK: Color = colour(palette::PINK);
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

/// The font the HUD (and the page) is set in: Share Tech Mono (`web/fonts`, SIL OFL).
#[derive(Resource, Clone)]
pub struct UiFont(pub Handle<Font>);

impl UiFont {
    /// Adds the font the page loads to the app's fonts.
    pub fn load(fonts: &mut Assets<Font>) -> Self {
        let bytes = include_bytes!("../../../web/fonts/ShareTechMono-Regular.ttf");
        Self(fonts.add(Font::from_bytes(bytes.to_vec())))
    }

    pub fn text(&self, size: f32) -> TextFont {
        TextFont { font: self.0.clone().into(), font_size: FontSize::Px(size), ..default() }
    }
}

/// A soft shadow under HUD text, so it reads over anything bright.
pub const SHADOW: TextShadow =
    TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.75) };

fn label(
    font: &UiFont,
    size: f32,
    color: Color,
    node: Node,
) -> (Text, TextFont, TextColor, TextShadow, Node) {
    (Text::new(""), font.text(size), TextColor(color), SHADOW, node)
}

/// A full-width row at `top`, centring what's in it.
fn banner(top: Val) -> Node {
    Node {
        position_type: PositionType::Absolute,
        top,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        ..default()
    }
}

/// Centres a node on its `left`/`top` point (a symbol on the spot it marks).
fn centred() -> UiTransform {
    UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() }
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

pub fn setup_hud(mut commands: Commands, font: Res<UiFont>) {
    let f = &*font;
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
            p.spawn((HudText::Status, label(f, 13.0, CYAN, abs(Some(14.0), None, Some(10.0), None))));
            // ZERO's panel, centred across the top.
            p.spawn(banner(Val::Px(10.0))).with_children(|row| {
                row.spawn((
                    HudText::Zero,
                    label(f, 14.0, ZERO_PINK, Node::default()),
                    TextLayout::justify(Justify::Center),
                ));
            });
            p.spawn((HudText::Feed, label(f, 13.0, AMBER, abs(None, Some(14.0), Some(10.0), None))));
            p.spawn((HudText::Flight, label(f, 13.0, CYAN, abs(Some(14.0), None, None, Some(12.0)))));
            p.spawn((HudText::Armor, label(f, 13.0, CYAN, abs(Some(250.0), None, None, Some(12.0)))));
            p.spawn((HudText::Weapons, label(f, 13.0, CYAN, abs(None, Some(14.0), None, Some(12.0)))));
            // Above the armour readout, clear of the weapons panel however narrow the window.
            p.spawn((HudText::Salvage, label(f, 13.0, AMBER, abs(Some(250.0), None, None, Some(136.0)))));
            p.spawn((
                GrabMarker,
                label(f, 13.0, GREEN, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                DockMarker,
                label(f, 13.0, AMBER, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            p.spawn((
                BoreMarker,
                label(f, 18.0, CYAN, abs(Some(0.0), None, Some(0.0), None)),
                centred(),
                Visibility::Hidden,
            ));
            p.spawn((
                VelocityMarker,
                label(f, 16.0, PALE_GREEN, abs(Some(0.0), None, Some(0.0), None)),
                centred(),
                Visibility::Hidden,
            ));
            // Alerts, centred a third of the way down.
            p.spawn(banner(Val::Percent(30.0))).with_children(|row| {
                row.spawn((
                    HudText::Alert,
                    label(f, 22.0, RED, Node::default()),
                    TextLayout::justify(Justify::Center),
                ));
            });
            p.spawn((
                Reticle,
                label(
                    f,
                    26.0,
                    CYAN,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Percent(50.0),
                        ..default()
                    },
                ),
                centred(),
            ));
            p.spawn((
                LeadMarker,
                label(f, 16.0, ZERO_PINK, abs(Some(0.0), None, Some(0.0), None)),
                Visibility::Hidden,
            ));
            for i in 0..BRACKETS + MISSILE_MARKERS {
                p.spawn((
                    Bracket(i),
                    label(f, 11.0, RED, abs(Some(0.0), None, Some(0.0), None)),
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
        ),
        (
            Or<(With<GrabMarker>, With<DockMarker>, With<BoreMarker>, With<VelocityMarker>)>,
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
    let mode = if game.autopilot { "AUTOPILOT (Mobile Doll brain)" } else { "MANUAL" };
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
        let (speed, g, strain, limited) = view.map_or((o.vel.length(), 0.0, o.g_strain, false), |v| {
            (v.flight_vel.length(), v.g, v.g_strain, v.g_limited)
        });
        set(
            HudText::Flight,
            format!(
                "{} {}\nSPD {:>6.0} m/s\nPROP {} {:>3.0}%\nHEAT {} {:>3.0}%\nENGY {} {:>3.0}%\nG   {:>4.1} g {:<3} STRAIN {}",
                bc_sim::content::frame_designation(form),
                frame_name(form).to_uppercase(),
                speed,
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
        // Resting on a rock, signed in: leave now and the suit stays parked here, hidden.
        Some(o) if o.flags & own_flags::PARKABLE != 0 && core.welcome.is_some_and(|w| w.signed_in) => {
            ("PARKED · safe to log off here".into(), GREEN)
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
    let velocity = match (own, drawn) {
        (Some(o), Some(v)) if o.alive && v.alive && v.flight_vel.length() > DRIFT => {
            let dir = v.flight_vel.normalize();
            let ahead = dir.dot(cam_tf.forward().as_vec3()) >= 0.0;
            Some(if ahead { (dir, "-o-") } else { (-dir, "-x-") })
        }
        _ => None,
    };
    for (mut node, mut text, mut color, mut vis, grab, is_bore, is_velocity) in &mut markers {
        let centred = is_bore || is_velocity;
        let what = if is_bore {
            bore.map(|b| (own_pos + b * 1_500.0, "( )".to_string(), CYAN))
        } else if is_velocity {
            velocity.map(|(d, s)| (cam_tf.translation() + d * 1_500.0, s.to_string(), PALE_GREEN))
        } else if grab {
            grab_at.clone()
        } else {
            dock_at.clone().map(|(p, s)| (p, s, AMBER))
        };
        match what.map(|(p, s, c)| (cam.world_to_viewport(cam_tf, p), s, c)) {
            Some((Ok(p), s, c)) => {
                // Symbols are centred on their point (their `UiTransform`); labels start just left
                // of theirs.
                let (dx, dy) = if centred { (0.0, 0.0) } else { (-20.0, -8.0) };
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
