//! On foot in the hangar bay (survival rules): the pilot's body and eyes, what they can reach and
//! use, and the sequences between the bay and space.
//!
//! - The pilot walks the bay's layout (`bc_client_core::walker`): W A S D, Shift to run, Space to
//!   jump; the mouse looks. What's in view and in reach can be used with E: a terminal opens its
//!   panel on the page, the airlock the menu (the way out of the bay is through it), and the
//!   cockpit boards the suit and launches it.
//! - Launching: the pilot climbs in and flight control is asked; the bay vents, its doors open
//!   and the catapult throws the suit down the launch tunnel into space, where the flight camera
//!   (chasing, or the cockpit) picks it up.
//! - Coming home (Enter, at rest inside the dock's ring of lights): the suit glides in down the
//!   tunnel, the doors shut behind it and the pilot climbs out onto the catwalk. After a suit is
//!   lost the pilot comes back in through the airlock, to an empty gantry.
//!
//! The server knows none of this: only where the pilot is (`core.hangar.place`) and what they ask
//! for. Space skips a sequence.

use bc_client_core::bay::{CATWALK_Y, HATCH, Layout, SPAWN, SUIT_AT, Spot};
use bc_client_core::walker::{Guide, Stride, Walker};
use bc_econ::item::thousands;
use bc_econ::wire::{Outcome, Place, Request};
use bc_econ::{Bay, Suit};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::dev_hooks::DevStatus;
use crate::hangar::{BAY_ORIGIN, BayState, DOOR_SECS, Indoors};
use crate::net::{GameClient, NetState, now_s};
use crate::page::{Panel, Ui, UiCmd, UiCmds};
use crate::pointer::PointerRes;
use crate::settings::SettingsRes;
use crate::terminal::TerminalLog;
use crate::view::ViewPrefs;

const SENSITIVITY: f32 = 0.0022;
/// Climbing into the cockpit, s (then the screen waits, dark, on flight control).
const BOARD_SECS: f32 = 1.0;
/// How long flight control has to answer, s.
const ANSWER_SECS: f32 = 8.0;
/// The bay vents and its doors open, s.
const VENT_SECS: f32 = DOOR_SECS + 1.3;
/// The klaxon sounds this long before the doors move, s.
const KLAXON_SECS: f32 = 0.8;
/// The catapult's run down the tunnel, s, and how hard it throws, m/s².
const CATAPULT_SECS: f32 = 3.2;
const CATAPULT_ACCEL: f32 = 49.0;
/// A homecoming: the suit glides in from this far down the tunnel, m, taking this long, s; the
/// doors start to close once it's through; the pilot climbs out at the end.
const ARRIVE_FROM: f32 = 170.0;
const GLIDE_SECS: f32 = 4.2;
const ARRIVE_SECS: f32 = 7.0;
/// Coming in through the airlock, s.
const ENTER_SECS: f32 = 1.6;
/// The screen fades back in over this long, s.
const REVEAL_SECS: f32 = 0.5;

/// Where the pilot is in the bay's sequences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Seq {
    /// On their own feet (or flying: nothing going on).
    #[default]
    Walking,
    /// Climbing into the cockpit, waiting on flight control.
    Boarding,
    /// The bay vents and its doors open.
    Venting,
    /// The catapult throws the suit down the tunnel.
    Catapult,
    /// Home: the suit glides in and the doors close behind it.
    Arriving,
    /// Coming in through the airlock.
    Entering,
}

impl Seq {
    pub fn name(self) -> &'static str {
        match self {
            Seq::Walking => "walking",
            Seq::Boarding => "boarding",
            Seq::Venting => "venting",
            Seq::Catapult => "catapult",
            Seq::Arriving => "arriving",
            Seq::Entering => "entering",
        }
    }

    /// The view is in the bay while this goes on, wherever the pilot is.
    fn indoors(self) -> bool {
        self != Seq::Walking
    }

    fn skippable(self) -> bool {
        matches!(self, Seq::Venting | Seq::Catapult | Seq::Arriving)
    }
}

/// The pilot on foot.
#[derive(Resource)]
pub struct OnFoot {
    layout: Layout,
    pub walker: Walker,
    /// Walking the pilot somewhere (a dev hook, or the autopilot).
    guide: Option<Guide>,
    /// What the pilot could use, where they stand and look.
    pub focus: Option<Spot>,
    pub seq: Seq,
    /// When the sequence began (the page's clock, s).
    since: f64,
    /// Where the pilot was, as last seen.
    place: Option<Place>,
    /// How the last sortie ended: it decides how the pilot comes home.
    outcome: Option<Outcome>,
    /// The suit being launched (the bay says it's out as soon as it is).
    launching: Option<Suit>,
    /// The eye and where it looked when boarding began (the camera eases away from them).
    from: (Vec3, Vec3),
    /// The screen fades in from black from this moment.
    reveal: Option<f64>,
    /// The airlock's door stands open until then.
    airlock_until: f64,
}

impl Default for OnFoot {
    fn default() -> Self {
        let layout = Layout::new();
        Self {
            layout,
            walker: Self::at_the_airlock(),
            guide: None,
            focus: None,
            seq: Seq::Walking,
            since: 0.0,
            place: None,
            outcome: None,
            launching: None,
            from: (Vec3::ZERO, Vec3::Z),
            reveal: None,
            airlock_until: 0.0,
        }
    }
}

impl OnFoot {
    /// Just in from the airlock, looking at the suit.
    fn at_the_airlock() -> Walker {
        Walker::at(SPAWN, (SUIT_AT - SPAWN).with_y(0.0).normalize())
    }

    fn start(&mut self, seq: Seq, now: f64) {
        self.seq = seq;
        self.since = now;
        self.guide = None;
    }

    /// Out of the sequence, on foot or flying, the screen fading back in.
    fn done(&mut self, now: f64) {
        self.seq = Seq::Walking;
        self.reveal = Some(now);
        self.launching = None;
    }

    fn enter(&mut self, now: f64) {
        self.walker = Self::at_the_airlock();
        self.start(Seq::Entering, now);
        self.airlock_until = now + f64::from(ENTER_SECS) * 0.5;
    }

    /// Out of the cockpit onto the catwalk, turned towards the stairs down.
    fn climb_out(&mut self) {
        self.walker = Walker::at(Vec3::new(0.8, CATWALK_Y, 1.9), -Vec3::X);
    }

    /// Until when the airlock's door stands open (it opens when the pilot comes in or leaves).
    pub fn airlock_until(&self) -> f64 {
        self.airlock_until
    }

    fn t(&self, now: f64) -> f32 {
        (now - self.since) as f32
    }

    /// Where the suit is in a sequence, from its place in the gantry, and how fast it's going.
    fn suit_motion(&self, now: f64) -> (Vec3, Vec3) {
        let t = self.t(now);
        match self.seq {
            Seq::Catapult => (Vec3::Z * (-0.5 * CATAPULT_ACCEL * t * t), Vec3::Z * (-CATAPULT_ACCEL * t)),
            Seq::Arriving => {
                // Easing out: the retros burn it down to rest in the gantry.
                let u = (t / GLIDE_SECS).clamp(0.0, 1.0);
                let left = (1.0 - u).powi(3);
                let speed = 3.0 * (1.0 - u).powi(2) * ARRIVE_FROM / GLIDE_SECS;
                (Vec3::Z * (-ARRIVE_FROM * left), Vec3::Z * speed)
            }
            _ => (Vec3::ZERO, Vec3::ZERO),
        }
    }

    /// The camera, in the bay's frame: where it is, and what it looks at.
    fn view(&self, now: f64) -> (Vec3, Vec3) {
        let t = self.t(now);
        let eye = self.walker.eye();
        let first_person = (eye, eye + self.walker.look());
        // The whole bay from its back corner, the suit below and the doors beyond, pushing in.
        let corner = |t: f32| {
            (Vec3::new(12.5, 21.0, 22.0) - Vec3::new(0.35, 0.2, 0.6) * t, Vec3::new(-1.5, 7.0, -14.0))
        };
        let (offset, _) = self.suit_motion(now);
        let suit = SUIT_AT + offset;
        match self.seq {
            Seq::Walking | Seq::Entering => first_person,
            Seq::Boarding => {
                let u = smoothstep(0.0, BOARD_SECS, t);
                let (from, look) = self.from;
                let to = HATCH + Vec3::new(0.0, 0.2, 0.6);
                (from.lerp(to, u), (from + look).lerp(to + Vec3::Z, u))
            }
            Seq::Venting => corner(t),
            Seq::Catapult => {
                // From the corner to chasing it down the tunnel.
                let u = smoothstep(0.0, 1.0, t);
                let chase = (suit + Vec3::new(2.5, 5.5, 17.0), suit + Vec3::new(0.0, 3.0, -30.0));
                let (eye, at) = corner(VENT_SECS);
                (eye.lerp(chase.0, u), at.lerp(chase.1, u))
            }
            // By the exchange terminal, watching it come in through the doors.
            Seq::Arriving => (Vec3::new(-13.5, 2.5, -21.0), suit + Vec3::Y * 4.0),
        }
    }

    /// How dark the screen is, 0..1.
    fn curtain(&self, now: f64) -> f32 {
        let t = self.t(now);
        let dark = match self.seq {
            Seq::Walking => 0.0,
            Seq::Boarding => smoothstep(BOARD_SECS * 0.55, BOARD_SECS, t),
            Seq::Venting => 1.0 - smoothstep(0.0, REVEAL_SECS, t),
            Seq::Catapult => smoothstep(CATAPULT_SECS - 0.35, CATAPULT_SECS, t),
            Seq::Arriving => smoothstep(ARRIVE_SECS - 0.7, ARRIVE_SECS, t),
            Seq::Entering => 1.0 - smoothstep(0.0, ENTER_SECS * 0.6, t),
        };
        let reveal = self.reveal.map_or(0.0, |at| 1.0 - smoothstep(0.0, REVEAL_SECS, (now - at) as f32));
        dark.max(reveal)
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A full-screen black veil over the game (and under the page) for the sequences' cuts.
#[derive(Component)]
pub struct Curtain;

pub fn setup_onfoot(mut commands: Commands) {
    commands.spawn((
        Curtain,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(100),
    ));
}

/// Sends a request to the hangar.
fn ask(net: &NetState, game: &crate::net::Game, req: &Request) {
    if let Some(t) = net.get() {
        t.send_control(game.core.request(req));
    }
}

/// What using a place is called on the prompt.
fn verb(spot: Spot, bay: Option<&Bay>) -> String {
    match (spot, bay) {
        (Spot::Cockpit, Some(Bay::Docked { .. })) => "BOARD & LAUNCH".into(),
        (Spot::Cockpit, Some(Bay::Out { .. })) => "COCKPIT: YOUR SUIT IS OUT".into(),
        (Spot::Cockpit, _) => "COCKPIT: THE GANTRY IS EMPTY".into(),
        (Spot::Airlock, _) => "AIRLOCK: LEAVE THE BAY".into(),
        (spot, _) => spot.name().into(),
    }
}

/// Once a frame, in game mode: the hangar's news, where the pilot is, their legs and eyes, what
/// they use, and the sequences; then what the bay and the page show.
#[allow(clippy::too_many_arguments)]
pub fn drive_onfoot(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    cmds: Res<UiCmds>,
    pointer: Res<PointerRes>,
    settings: Res<SettingsRes>,
    time: Res<Time<Real>>,
    net: NonSend<NetState>,
    game: NonSend<GameClient>,
    mut ui: ResMut<Ui>,
    mut me: ResMut<OnFoot>,
    mut indoors: ResMut<Indoors>,
    mut bay: ResMut<BayState>,
    mut log: ResMut<TerminalLog>,
    mut curtain: Query<&mut BackgroundColor, With<Curtain>>,
) {
    let now = now_s();
    let dt = time.delta_secs().min(0.1);
    // Walking keeps up with slow frames (up to a second's worth).
    let walk_dt = time.delta_secs().min(1.0);
    let mut g = game.borrow_mut();
    let autopilot = g.autopilot;

    // What the server had to say.
    let mut refused = false;
    for (text, ok) in g.core.hangar.notes.drain(..) {
        refused |= !ok;
        ui.toast(if ok { text.clone() } else { format!("CAN'T: {}", text.to_uppercase()) });
        log.push(text, ok);
    }
    for (outcome, text) in g.core.hangar.sorties.drain(..) {
        me.outcome = Some(outcome);
        ui.news(text.clone(), outcome == Outcome::Lost);
        log.push(text, outcome != Outcome::Lost);
    }
    for text in g.core.hangar.news.drain(..) {
        ui.news(text.clone(), false);
        log.push(text, true);
    }

    // Where the pilot is.
    let place = g.core.hangar.place;
    if place != me.place {
        let before = me.place;
        match (before, place) {
            // A new session (or the link was reset): start afresh.
            (_, None) => *me = OnFoot::default(),
            (Some(Place::Hangar), Some(Place::Space)) if me.seq == Seq::Boarding => {
                me.start(Seq::Venting, now);
            }
            (Some(Place::Space), Some(Place::Hangar)) if me.outcome == Some(Outcome::Docked) => {
                me.climb_out();
                me.start(Seq::Arriving, now);
            }
            (_, Some(Place::Hangar)) => me.enter(now),
            // Woke in the suit they'd left out there, or launched by other means: flying.
            (_, Some(Place::Space)) => {
                if me.seq.indoors() {
                    me.done(now);
                }
            }
        }
        me.place = place;
    }

    // The sequences.
    let t = me.t(now);
    let skip =
        (keys.just_pressed(KeyCode::Space) && ui.playing() && !ui.panel_open()) || cmds.has(&UiCmd::Skip);
    match me.seq {
        Seq::Boarding if refused => me.seq = Seq::Walking,
        Seq::Boarding if t > ANSWER_SECS => {
            me.seq = Seq::Walking;
            ui.toast("FLIGHT CONTROL DIDN'T ANSWER");
        }
        s if s.skippable() && skip => {
            if s == Seq::Arriving {
                me.climb_out();
            }
            me.done(now);
        }
        Seq::Venting if t > VENT_SECS => me.start(Seq::Catapult, now),
        Seq::Catapult if t > CATAPULT_SECS => me.done(now),
        Seq::Arriving if t > ARRIVE_SECS => me.done(now),
        Seq::Entering if t > ENTER_SECS => me.seq = Seq::Walking,
        _ => {}
    }
    let inside = me.seq.indoors() || place == Some(Place::Hangar);
    if indoors.0 != inside {
        indoors.0 = inside;
    }

    // On foot: legs, eyes and hands.
    let on_foot = me.seq == Seq::Walking && place == Some(Place::Hangar);
    let live = on_foot && ui.playing() && !ui.panel_open();
    let hangar_bay = g.core.hangar.view.as_ref().map(|v| v.bay.clone());
    if on_foot {
        for cmd in &cmds.0 {
            if let UiCmd::WalkTo(slug) = cmd
                && let Some(spot) = Spot::from_slug(slug)
            {
                let (_, facing) = spot.stand();
                let route = me.layout.route(me.walker.feet, spot);
                me.guide = Some(Guide::new(route, Some(facing)));
            }
        }
        // The autopilot walks to the cockpit and launches whatever stands in the gantry.
        if autopilot
            && me.guide.is_none()
            && me.focus != Some(Spot::Cockpit)
            && matches!(hangar_bay, Some(Bay::Docked { .. }))
        {
            let (_, facing) = Spot::Cockpit.stand();
            let route = me.layout.route(me.walker.feet, Spot::Cockpit);
            me.guide = Some(Guide::new(route, Some(facing)));
        }
        let me = &mut *me;
        let mut stride = Stride::default();
        if live && pointer.0.flying() && motion.delta != Vec2::ZERO {
            let mut d = motion.delta * SENSITIVITY * settings.0.sensitivity;
            if settings.0.invert_y {
                d.y = -d.y;
            }
            me.walker.turn(-d.x, -d.y);
        }
        if live {
            let axis =
                |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
            stride = Stride {
                forward: axis(KeyCode::KeyW, KeyCode::KeyS),
                right: axis(KeyCode::KeyD, KeyCode::KeyA),
                run: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
                jump: keys.pressed(KeyCode::Space),
            };
        }
        if stride != Stride::default() {
            // The pilot's own feet take over.
            me.guide = None;
        }
        // In slices, so a slow frame walks as far as a fast one would.
        let mut left = walk_dt;
        while left > 1e-4 {
            let h = left.min(0.1);
            left -= h;
            if let Some(guide) = me.guide.as_mut() {
                stride = guide.steer(&mut me.walker, h);
                if guide.arrived() && guide.facing.is_none_or(|f| me.walker.heading().dot(f) > 0.97) {
                    me.guide = None;
                }
            }
            me.walker.step(&me.layout, &stride, h);
        }
        me.focus = Layout::spot_in_view(me.walker.eye(), me.walker.look());

        // Using what's in view.
        let used = (live && keys.just_pressed(KeyCode::KeyE))
            || cmds.has(&UiCmd::Use)
            || (autopilot && me.focus == Some(Spot::Cockpit) && me.guide.is_none());
        if used
            && !ui.panel_open()
            && let Some(spot) = me.focus
        {
            match (spot, &hangar_bay) {
                (Spot::Cockpit, Some(Bay::Docked { suit })) => {
                    me.launching = Some(suit.clone());
                    me.from = (me.walker.eye(), me.walker.look());
                    me.start(Seq::Boarding, now);
                    ask(&net, &g, &Request::Launch);
                }
                (Spot::Cockpit, Some(Bay::Out { .. })) => ui.toast("YOUR SUIT IS OUT IN THE SECTOR"),
                (Spot::Cockpit, _) => ui.toast("THE GANTRY IS EMPTY: BUILD A SUIT AT THE FABRICATOR"),
                (Spot::Airlock, _) => {
                    me.airlock_until = now + 1.5;
                    ui.open_pause();
                }
                (terminal, _) => ui.panel = Panel::Terminal(terminal),
            }
        }
    } else {
        me.focus = None;
        me.guide = None;
    }

    // In the sector: Enter docks.
    let flying = place == Some(Place::Space) && me.seq == Seq::Walking;
    if flying
        && ui.playing()
        && !ui.panel_open()
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter))
    {
        ask(&net, &g, &Request::Dock);
    }
    // Requests from the terminals (launching and docking are the bay's own business).
    for cmd in &cmds.0 {
        if let UiCmd::Hangar(req) = cmd
            && !matches!(req, Request::Launch | Request::Dock)
        {
            ask(&net, &g, req);
        }
    }

    // What the bay shows.
    let t = me.t(now);
    let (offset, vel) = me.suit_motion(now);
    bay.suit = match me.seq {
        Seq::Boarding | Seq::Venting | Seq::Catapult => me.launching.clone(),
        _ => match &hangar_bay {
            Some(Bay::Docked { suit }) => Some(suit.clone()),
            _ => None,
        },
    };
    bay.boarded = match me.seq {
        Seq::Boarding | Seq::Venting | Seq::Catapult => true,
        Seq::Arriving => t < ARRIVE_SECS - 1.0,
        _ => false,
    };
    bay.doors = match me.seq {
        Seq::Venting => ((t - KLAXON_SECS) / DOOR_SECS).clamp(0.0, 1.0),
        Seq::Catapult => 1.0,
        Seq::Arriving => 1.0 - ((t - GLIDE_SECS * 0.72) / DOOR_SECS).clamp(0.0, 1.0),
        _ => 0.0,
    };
    // The tunnel's outer doors open after the bay's, and shut behind a suit coming in.
    bay.outer = match me.seq {
        Seq::Venting => ((t - KLAXON_SECS - 0.5) / DOOR_SECS).clamp(0.0, 1.0),
        Seq::Catapult => 1.0,
        Seq::Arriving => 1.0 - ((t - 0.3) / DOOR_SECS).clamp(0.0, 1.0),
        _ => 0.0,
    };
    bay.alarm =
        matches!(me.seq, Seq::Venting | Seq::Catapult) || (me.seq == Seq::Arriving && bay.doors > 0.0);
    bay.offset = offset;
    bay.vel = vel;
    (bay.thrust, bay.boost) = match me.seq {
        Seq::Catapult => (Vec3::Z, true),
        Seq::Arriving if t < GLIDE_SECS => (-Vec3::Z * (1.0 - t / GLIDE_SECS), false),
        _ => (Vec3::ZERO, false),
    };
    // The airlock's door opens as the pilot comes in (or goes to leave), and closes after them.
    let open = (me.seq == Seq::Entering && t < ENTER_SECS * 0.8) || now < me.airlock_until;
    bay.airlock = if open { (bay.airlock + dt * 2.5).min(1.0) } else { (bay.airlock - dt * 1.5).max(0.0) };
    if let Ok(mut c) = curtain.single_mut() {
        let a = me.curtain(now);
        let want = Color::srgba(0.0, 0.0, 0.0, a);
        if c.0 != want {
            c.0 = want;
        }
    }

    // What the page shows.
    ui.place = match place {
        Some(Place::Hangar) => "hangar",
        Some(Place::Space) => "space",
        None => "",
    };
    ui.on_foot = on_foot;
    ui.sequence = me.seq != Seq::Walking;
    ui.prompt = match me.seq {
        Seq::Walking if on_foot => {
            me.focus.map_or_else(String::new, |s| format!("E  {}", verb(s, hangar_bay.as_ref())))
        }
        s if s.skippable() => "SPACE  SKIP".into(),
        _ => String::new(),
    };
    ui.bay_line = if inside {
        format!("BAY {:02} · {} CR", g.core.hangar.bay, thousands(g.core.hangar.credits()))
    } else {
        String::new()
    };
}

/// The camera at the pilot's eyes (or where a sequence puts it), after the chase camera has had
/// its turn.
pub fn onfoot_camera(
    me: Res<OnFoot>,
    indoors: Res<Indoors>,
    prefs: Res<ViewPrefs>,
    mut cam: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut tf, mut projection)) = cam.single_mut() else { return };
    // Walls come close on foot.
    let near = if indoors.0 { 0.05 } else { 0.5 };
    if let Projection::Perspective(p) = &mut *projection {
        if p.near != near {
            p.near = near;
        }
        if indoors.0 {
            p.fov = prefs.fov.clamp(40.0, 120.0).to_radians();
        }
    }
    if !indoors.0 {
        return;
    }
    let (eye, at) = me.view(now_s());
    *tf = Transform::from_translation(BAY_ORIGIN + eye).looking_at(BAY_ORIGIN + at, Vec3::Y);
}

/// On foot, for the E2E tests: where the pilot is and what they could use.
pub fn publish_onfoot(me: Res<OnFoot>, ui: Res<Ui>, game: NonSend<GameClient>, mut dev: ResMut<DevStatus>) {
    let g = game.borrow();
    let h = &g.core.hangar;
    dev.set("place", ui.place);
    dev.set("seq", me.seq.name());
    dev.set("focus", me.focus.map_or("", Spot::slug));
    dev.set("terminal", ui.terminal().map_or("", Spot::slug));
    dev.set("walking_to", me.guide.is_some());
    let f = me.walker.feet;
    dev.set("feet", format!("{:.1},{:.1},{:.1}", f.x, f.y, f.z));
    dev.set("hangar_credits", h.credits() as f64);
    dev.set("hangar_version", h.version as f64);
    let (bay, line) = match h.view.as_ref().map(|v| &v.bay) {
        Some(Bay::Docked { suit }) => ("docked", suit.line.slug()),
        Some(Bay::Out { suit }) => ("out", suit.line.slug()),
        Some(Bay::Empty) => ("empty", ""),
        None => ("", ""),
    };
    dev.set("bay", bay);
    dev.set("bay_line", line);
    // What's broken inside the suit in the bay, and what it carries.
    let suit = h.view.as_ref().and_then(|v| match &v.bay {
        Bay::Docked { suit } | Bay::Out { suit } => Some(suit),
        Bay::Empty => None,
    });
    dev.set("bay_faults", suit.map_or(0, |s| s.faults.count() as u32));
    dev.set("bay_modules", suit.map_or(0, |s| s.modules.iter().flatten().count() as u32));
    dev.set("jobs", h.view.as_ref().map_or(0, |v| v.jobs.len() as u32));
    dev.set("orders", h.market.as_ref().map_or(0, |m| m.orders.len() as u32));
}
