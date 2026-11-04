//! The fx stage (`?showcase=fx&fx=<name>`, `scripts/fx.sh`, the `fx` skill): one effect staged
//! alone, fired at once in front of a dark card and a light card out in empty space, on the
//! showcase's fixed clock, which the film hook (`window.__bcFilm.step`) holds at any step, so
//! `scripts/fxstrip.mjs` shoots it frame by frame. What it fires is a real `FxEvent`, drained by
//! the effects as a match's would be, so a strip is what the game throws.
//!
//! The contract (framing, cards, the plate at step 0, the effect on step 1) is the shared
//! `fx` crate's (`fx::stage`, vendored from Gates). Our particles are still our own
//! (`particles.rs`); only the stage is shared so far.

use bc_proto::WeaponKind;
use bevy::prelude::*;
use fx::stage::Framing;

use crate::camera::MainCamera;
use crate::view::{FxEvent, FxEvents};

/// Where the stage stands: empty space, away from every other scene's cast.
pub const AT: Vec3 = Vec3::new(0.0, 1_200.0, -3_000.0);

/// The cards' sRGB greys, tuned under our tonemap until the plate reads the stage's ~25 and ~185
/// (`fxstrip.mjs` says `plate … ok`).
const CARDS: [f32; 2] = [fx::stage::DARK_SRGB, fx::stage::LIGHT_SRGB];

/// What an effect is fired by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fire {
    Hit(WeaponKind),
    Muzzle(WeaponKind),
    Kill,
    Blast,
    Clash,
    RockBreak,
    MissileBurst,
    Touchdown,
    Eject,
    Stagger,
}

/// One effect the stage can fire.
#[derive(Clone, Debug)]
pub struct Staged {
    pub name: String,
    pub what: String,
    /// Roughly how far it reaches, metres: the stage frames it by this.
    pub radius: f32,
    /// How long it lasts, seconds: a strip's frames span this.
    pub span_s: f32,
    pub fire: Fire,
}

/// `HeavyBeamRifle` as `heavy-beam-rifle`.
fn kebab(w: WeaponKind) -> String {
    let mut out = String::new();
    for (i, c) in format!("{w:?}").chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Every effect the stage can fire, in a stable order.
pub fn list() -> Vec<Staged> {
    let s = |name: String, what: &str, radius: f32, span_s: f32, fire: Fire| Staged {
        name,
        what: what.into(),
        radius,
        span_s,
        fire,
    };
    let mut out = Vec::new();
    for w in WeaponKind::ALL {
        out.push(s(
            format!("hit-{}", kebab(w)),
            &format!("a {w:?} striking a suit"),
            16.0,
            1.2,
            Fire::Hit(w),
        ));
    }
    for w in WeaponKind::ALL {
        out.push(s(format!("muzzle-{}", kebab(w)), &format!("a {w:?} firing"), 12.0, 0.8, Fire::Muzzle(w)));
    }
    out.extend([
        s("kill".into(), "a suit destroyed: its blast and its armour flying", 60.0, 3.0, Fire::Kill),
        s("blast".into(), "a reactor blown by its pilot", 90.0, 3.0, Fire::Blast),
        s("clash".into(), "beam sabers meeting", 24.0, 1.0, Fire::Clash),
        s("rock-break".into(), "a rock shattering", 30.0, 3.0, Fire::RockBreak),
        s("missile-burst".into(), "a missile's warhead going off", 25.0, 1.5, Fire::MissileBurst),
        s("touchdown".into(), "a suit landing hard on rock", 20.0, 2.0, Fire::Touchdown),
        s("eject".into(), "a pilot's capsule blown clear", 15.0, 1.5, Fire::Eject),
        s("stagger".into(), "a suit staggered", 10.0, 1.0, Fire::Stagger),
    ]);
    out
}

pub fn find(name: &str) -> Option<Staged> {
    list().into_iter().find(|s| s.name == name)
}

/// The stage, when the showcase's scene is `fx`.
#[derive(Resource, Clone, Debug)]
pub struct Stage {
    pub staged: Staged,
    pub framing: Framing,
}

impl Stage {
    pub fn new(name: &str) -> Option<Stage> {
        let staged = find(name).or_else(|| {
            error!("fx stage: no effect `{name}`; firing hit-beam-rifle");
            find("hit-beam-rifle")
        })?;
        Some(Stage { framing: Framing::new(staged.radius), staged })
    }

    /// The camera: where it stands and which way it looks.
    pub fn eye(&self) -> (Vec3, Vec3) {
        let f = &self.framing;
        (AT + Vec3::from(f.eye), (Vec3::from(f.look) - Vec3::from(f.eye)).normalize())
    }

    /// The events that fire it, at both anchors.
    pub fn events(&self, out: &mut Vec<FxEvent>) {
        let f = &self.framing;
        let dir = Vec3::from(f.dir);
        for a in f.anchors {
            let pos = AT + Vec3::from(a);
            out.push(match self.staged.fire {
                Fire::Hit(weapon) => FxEvent::Hit { pos, weapon, normal: Some(dir), target: None },
                Fire::Muzzle(weapon) => FxEvent::Muzzle { pos, dir, vel: Vec3::ZERO, weapon, shooter: None },
                Fire::Kill => FxEvent::Kill { pos, vel: Vec3::ZERO, victim: None },
                Fire::Blast => FxEvent::Blast { pos },
                Fire::Clash => FxEvent::Clash { pos, vel: Vec3::ZERO },
                Fire::RockBreak => FxEvent::RockBreak { pos, radius: 12.0, ore: Vec3::new(0.85, 0.62, 0.35) },
                Fire::MissileBurst => {
                    FxEvent::MissileBurst { pos, kind: WeaponKind::HomingMissile, struck: false }
                }
                Fire::Touchdown => {
                    FxEvent::Touchdown { pos, vel: Vec3::ZERO, normal: Vec3::Y, speed: 12.0, rock: true }
                }
                Fire::Eject => FxEvent::Eject { pos, vel: dir * 20.0, own: false },
                Fire::Stagger => FxEvent::Stagger { pos, vel: Vec3::ZERO, own: false },
            });
        }
    }

    /// Whether the effect fires between scene times `prev` and `now`: once, half a step after
    /// `t0` (the clock's step 1), then again every span and a half second for a person watching.
    pub fn fires(&self, t0: f64, step: f64, prev: f64, now: f64) -> bool {
        let first = t0 + step * 0.5;
        let cycle = f64::from(self.staged.span_s) + 0.5;
        let k = ((now - first) / cycle).floor().max(0.0);
        let at = first + k * cycle;
        at > prev && at <= now
    }
}

/// The two cards, behind where the effects fire, and `window.__fx` for the shooter.
pub fn setup(
    stage: Res<Stage>,
    gfx: Res<crate::gfx::Gfx>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    for (c, g) in stage.framing.cards.iter().zip(CARDS) {
        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(g, g, g),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(AT + Vec3::from(c.centre) - Vec3::Z * 0.01).with_scale(Vec3::new(
                c.half[0] * 2.0,
                c.half[1] * 2.0,
                1.0,
            )),
        ));
    }
    if let Some(w) = web_sys::window() {
        use wasm_bindgen::JsValue;
        let o = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &v);
        };
        set("game", "Before Colony".into());
        set("name", stage.staged.name.as_str().into());
        set("what", stage.staged.what.as_str().into());
        set("span", stage.staged.span_s.into());
        // The tier's HDR is its bloom (`gfx.rs`): high has it, low doesn't.
        set("bloom", gfx.settings.hdr.into());
        let _ = js_sys::Reflect::set(&w, &JsValue::from_str("__fx"), &o);
    }
}

/// The stage's lens, every frame (the showcase places the camera; the stage sets its field).
pub fn lens(stage: Res<Stage>, mut cam: Query<&mut Projection, With<MainCamera>>) {
    if let Ok(mut p) = cam.single_mut()
        && let Projection::Perspective(pp) = &mut *p
    {
        pp.fov = stage.framing.fov_y;
    }
}

/// Push the effect's events when the clock crosses its moment.
pub fn fire(stage: &Stage, t0: f64, step: f64, prev: f64, now: f64, events: &mut FxEvents) {
    if stage.fires(t0, step, prev, now) {
        stage.events(&mut events.0);
    }
}
