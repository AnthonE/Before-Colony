//! Sound, through the browser's Web Audio. The bank (`bc_sound::synth`) is rendered into
//! AudioBuffers a few cues a frame from boot; each frame the mixer (`bc_sound::mixer`) picks the
//! one-shots to start, the cockpit model (`bc_sound::cockpit`) sets the engine loops and alarms,
//! and the music director (`bc_sound::music`) crossfades the score.
//!
//! The AudioContext is made here and left on the page as `window.bcAudio`, where `web/ui.js`
//! resumes it on the first click or key (browsers won't start audio without a gesture) and
//! suspends it while the tab is hidden. Web Audio renders on its own thread: a slow frame delays a
//! sound's start by a frame, and never glitches one that's playing.

use std::collections::HashMap;

use bc_proto::buttons::RCS_SHARP;
use bc_proto::snapshot::{own_flags, zero_mode};
use bc_proto::{NO_CHUNK, NO_SLOT, WeaponKind};
use bc_sim::content::{SpecialKind, WeaponClass, frame, weapon};
use bc_sound::cockpit::{Cockpit, CockpitIn};
use bc_sound::mixer::{Listener, Mixer, Request, STARTS_PER_FRAME, Start};
use bc_sound::music::{Director, MusicIn};
use bc_sound::{CUE_COUNT, Cue, Mix};
use bevy::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::{
    AudioBuffer, AudioBufferSourceNode, AudioContext, AudioContextState, BiquadFilterNode, BiquadFilterType,
    DynamicsCompressorNode, GainNode, StereoPannerNode,
};

use crate::camera::MainCamera;
use crate::dev_hooks::DevStatus;
use crate::net::{GameClient, now_s};
use crate::page::{Ui, UiCmd, UiCmds};
use crate::settings::SettingsRes;
use crate::view::{FxEvent, FxEvents};

/// One-shot voices at once.
const VOICES: usize = 24;
/// How long bank building may take per frame, ms (the first frames, on the title screen).
const BUILD_BUDGET_MS: f64 = 6.0;

/// The order the bank is built in: what the title and the first seconds need, first.
fn build_order() -> Vec<Cue> {
    let first =
        [Cue::UiClick, Cue::UiConfirm, Cue::MusicTitle, Cue::CockpitHum, Cue::ThrusterLoop, Cue::Launch];
    let mut order: Vec<Cue> = first.to_vec();
    order.extend(Cue::ALL.iter().copied().filter(|c| !first.contains(c)));
    order
}

struct Strip {
    gain: GainNode,
    pan: StereoPannerNode,
    busy_until: f64,
}

struct LoopVoice {
    cue: Cue,
    src: AudioBufferSourceNode,
    gain: GainNode,
    /// The last targets set (so automation events aren't piled on every frame).
    at: (f32, f32),
}

/// Non-send: the page's audio graph.
pub struct WebAudio {
    ctx: AudioContext,
    input: GainNode,
    lowpass: BiquadFilterNode,
    lowpass_at: f32,
    buffers: Vec<Option<AudioBuffer>>,
    order: Vec<Cue>,
    built: usize,
    strips: Vec<Strip>,
    loops: Vec<LoopVoice>,
}

impl WebAudio {
    fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        let input = ctx.create_gain()?;
        let lowpass = ctx.create_biquad_filter()?;
        lowpass.set_type(BiquadFilterType::Lowpass);
        lowpass.frequency().set_value(20_000.0);
        let limiter: DynamicsCompressorNode = ctx.create_dynamics_compressor()?;
        limiter.threshold().set_value(-8.0);
        limiter.knee().set_value(6.0);
        limiter.ratio().set_value(10.0);
        limiter.attack().set_value(0.003);
        limiter.release().set_value(0.2);
        input.connect_with_audio_node(&lowpass)?;
        lowpass.connect_with_audio_node(&limiter)?;
        limiter.connect_with_audio_node(&ctx.destination())?;
        let mut strips = Vec::with_capacity(VOICES);
        for _ in 0..VOICES {
            let gain = ctx.create_gain()?;
            let pan = ctx.create_stereo_panner()?;
            gain.connect_with_audio_node(&pan)?;
            pan.connect_with_audio_node(&input)?;
            strips.push(Strip { gain, pan, busy_until: 0.0 });
        }
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("bcAudio"), &ctx);
        }
        Ok(Self {
            ctx,
            input,
            lowpass,
            lowpass_at: 20_000.0,
            buffers: vec![None; CUE_COUNT],
            order: build_order(),
            built: 0,
            strips,
            loops: Vec::new(),
        })
    }

    fn running(&self) -> bool {
        self.ctx.state() == AudioContextState::Running
    }

    /// Renders bank cues until the frame's budget is spent.
    fn build_some(&mut self) {
        let start = now_s();
        while self.built < self.order.len() && (now_s() - start) * 1_000.0 < BUILD_BUDGET_MS {
            let cue = self.order[self.built];
            self.built += 1;
            let samples = bc_sound::synth::render(cue);
            let rate = bc_sound::sample_rate(cue) as f32;
            // A stereo cue comes planar: the left channel's samples, then the right's.
            let channels = bc_sound::channels(cue);
            let frames = samples.len() / channels;
            let Ok(buf) = self.ctx.create_buffer(channels as u32, frames as u32, rate) else { continue };
            if samples
                .chunks_exact(frames)
                .enumerate()
                .any(|(c, s)| buf.copy_to_channel(s, c as i32).is_err())
            {
                continue;
            }
            if cue.def().looped {
                self.start_loop(cue, &buf);
            }
            self.buffers[cue as usize] = Some(buf);
        }
    }

    fn start_loop(&mut self, cue: Cue, buf: &AudioBuffer) {
        let Ok(src) = self.ctx.create_buffer_source() else { return };
        let Ok(gain) = self.ctx.create_gain() else { return };
        src.set_buffer(Some(buf));
        src.set_loop(true);
        gain.gain().set_value(0.0);
        if src.connect_with_audio_node(&gain).is_err() || gain.connect_with_audio_node(&self.input).is_err() {
            return;
        }
        let _ = src.start();
        self.loops.push(LoopVoice { cue, src, gain, at: (0.0, 1.0) });
    }

    fn set_loop(&mut self, cue: Cue, gain: f32, rate: f32) {
        let now = self.ctx.current_time();
        if let Some(v) = self.loops.iter_mut().find(|v| v.cue == cue) {
            if (v.at.0 - gain).abs() > 0.005 {
                let _ = v.gain.gain().set_target_at_time(gain, now, 0.06);
                v.at.0 = gain;
            }
            if (v.at.1 - rate).abs() > 0.01 {
                let _ = v.src.playback_rate().set_target_at_time(rate, now, 0.08);
                v.at.1 = rate;
            }
        }
    }

    fn set_lowpass(&mut self, hz: f32) {
        if (self.lowpass_at - hz).abs() > 50.0 {
            let _ = self.lowpass.frequency().set_target_at_time(hz, self.ctx.current_time(), 0.15);
            self.lowpass_at = hz;
        }
    }

    fn play(&mut self, s: &Start) {
        let Some(Some(buf)) = self.buffers.get(s.cue as usize) else { return };
        let now = self.ctx.current_time();
        let Some(strip) = self.strips.iter_mut().find(|st| st.busy_until <= now) else { return };
        let Ok(src) = self.ctx.create_buffer_source() else { return };
        src.set_buffer(Some(buf));
        src.playback_rate().set_value(s.rate);
        strip.gain.gain().set_value(s.gain);
        strip.pan.pan().set_value(s.pan);
        if src.connect_with_audio_node(&strip.gain).is_ok() && src.start().is_ok() {
            strip.busy_until = now + buf.duration() / f64::from(s.rate.max(0.25)) + 0.05;
        }
    }
}

/// The sound model: the mixer, the cockpit and the music director.
#[derive(Resource, Default)]
pub struct Sound {
    mixer: Mixer,
    cockpit: Cockpit,
    music: Director,
    /// Each suit's saber flag last frame (a blade lighting up is a swing).
    sabers: HashMap<u16, bool>,
    /// The pilot's own, last frame.
    own_saber: bool,
    /// Counters for the dev hooks.
    pub started: u32,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sound>();
        match WebAudio::new() {
            Ok(a) => {
                app.insert_non_send(a);
            }
            Err(e) => web_sys::console::warn_1(&format!("no Web Audio: {e:?}").into()),
        }
    }
}

/// What each weapon sounds like leaving the muzzle.
fn muzzle_cue(w: WeaponKind) -> Cue {
    match w {
        WeaponKind::TwinBusterRifle => Cue::BusterFire,
        WeaponKind::BeamCannon => Cue::BeamHeavy,
        WeaponKind::BeamGatling | WeaponKind::BeamMachineGun => Cue::BeamGun,
        WeaponKind::Flamethrower => Cue::Flame,
        _ => match weapon(w).class {
            WeaponClass::Beam => Cue::BeamRifle,
            WeaponClass::Ballistic => Cue::Gun,
            WeaponClass::Missile => Cue::MissileLaunch,
            WeaponClass::Melee => Cue::Saber,
            WeaponClass::Cone => Cue::Flame,
        },
    }
}

fn v3(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// The pilot's suit, as the cockpit model reads it.
fn cockpit_in(game: &GameClient, in_world: bool) -> CockpitIn {
    let g = game.borrow();
    let core = &g.core;
    let Some(o) = core.world.own.filter(|_| in_world) else { return CockpitIn::default() };
    let spec = frame(o.frame);
    // The suit as drawn: what its thrusters are doing and what the pilot's body feels, now.
    let view = core.own_view().copied();
    let thrust = view.map_or(0.0, |v| v.throttle.abs().max_element().min(1.0));
    let lock_progress = match spec.lock_spec() {
        Some(lock) if o.lock_target != NO_SLOT => {
            f32::from(o.lock_progress) / f32::from(lock.lock_ticks).max(1.0)
        }
        _ => 0.0,
    };
    let jammer = matches!(spec.special, SpecialKind::HyperJammer { .. });
    CockpitIn {
        in_world: true,
        alive: o.alive,
        thrust,
        boost: view.is_some_and(|v| v.boosting && v.throttle.z > 0.05),
        rcs: core.last_cmd.buttons & RCS_SHARP != 0,
        propellant: core.predict.state.propellant / spec.propellant_cap.max(1.0),
        g_strain: view.map_or(o.g_strain, |v| v.g_strain).clamp(0.0, 1.0),
        blackout: view.is_some_and(|v| v.blackout),
        charge: o.charge,
        // As predicted: the blade lights as the swing starts.
        saber: view.is_some_and(|v| v.strike.is_some()),
        lock_progress,
        locked: o.flags & own_flags::LOCK_ACQUIRED != 0,
        warned: o.flags & own_flags::MISSILE_LOCK != 0,
        incoming: o.flags & own_flags::MISSILE_INCOMING != 0,
        docked: o.flags & own_flags::DOCKED != 0,
        zero: o.zero_mode == zero_mode::ACTIVE,
        seized: o.zero_mode == zero_mode::SEIZED,
        jamming: jammer && o.flags & own_flags::SPECIAL_ACTIVE != 0,
        transforming: o.flags & own_flags::TRANSFORMING != 0,
        held: o.held != NO_CHUNK,
        cargo_kg: o.cargo_kg.iter().map(|kg| u32::from(*kg)).sum(),
        credits: o.credits,
        footing: view.and_then(|v| v.ground).map_or(0, |g| if g.aloft { 2 } else { 1 }),
        touchdown: view.and_then(|v| v.touchdown).unwrap_or(0.0),
        footfalls: 0,
        grip: core.last_cmd.buttons & bc_proto::buttons::GRIP != 0,
        cover: o.cover,
    }
}

/// Builds the bank a little at a time.
pub fn build_bank(audio: Option<NonSendMut<WebAudio>>) {
    if let Some(mut a) = audio {
        a.build_some();
    }
}

/// Once a frame, after the camera moves and before the effects drain the events.
#[allow(clippy::too_many_arguments)]
pub fn play_sound(
    audio: Option<NonSendMut<WebAudio>>,
    mut sound: ResMut<Sound>,
    events: Res<FxEvents>,
    cmds: Res<UiCmds>,
    ui: Res<Ui>,
    settings: Res<SettingsRes>,
    game: NonSend<GameClient>,
    drives: Query<&crate::view::SuitDrive>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    time: Res<Time<Real>>,
    indoors: Res<crate::hangar::Indoors>,
    onfoot: Res<crate::onfoot::OnFoot>,
    mut last_seq: Local<crate::onfoot::Seq>,
    mut airlock_at: Local<f64>,
    mut dev: ResMut<DevStatus>,
) {
    let Some(mut audio) = audio else { return };
    dev.set("audio_state", if audio.running() { "running" } else { "suspended" });
    dev.set("audio_built", audio.buffers.iter().filter(|b| b.is_some()).count() as u32);
    dev.set("audio_cues", CUE_COUNT as u32);
    dev.set("audio_started", sound.started);
    if !audio.running() {
        return;
    }
    let s = &settings.0;
    let mix =
        Mix { master: s.vol_master, effects: s.vol_effects, cockpit: s.vol_cockpit, music: s.vol_music };
    let now = now_s();
    let sound = &mut *sound;
    let own_slot = game.borrow().core.world.own_slot();

    // The page's clicks.
    for cmd in &cmds.0 {
        match cmd {
            UiCmd::Sfx(name) if name == "confirm" => sound.mixer.request(Request::own(Cue::UiConfirm)),
            UiCmd::Sfx(_) => sound.mixer.request(Request::own(Cue::UiClick)),
            _ => {}
        }
    }

    // The world's events.
    let listener = camera.single().map_or(Listener { pos: [0.0; 3], right: [1.0, 0.0, 0.0] }, |c| Listener {
        pos: v3(c.translation()),
        right: v3(c.right().as_vec3()),
    });
    let ears = Vec3::from(listener.pos);
    let mut heat = 0.0f32;
    for ev in &events.0 {
        match *ev {
            FxEvent::Muzzle { pos, weapon, shooter, .. } => {
                let own = shooter.is_some() && shooter == own_slot;
                let cue = muzzle_cue(weapon);
                sound.mixer.request(if own { Request::own(cue) } else { Request::at(cue, v3(pos)) });
                heat += if own { 0.004 } else { 0.0 };
            }
            FxEvent::Hit { pos, target, .. } => {
                if target.is_none_or(|(slot, _)| Some(slot) != own_slot) {
                    sound.mixer.request(Request::at(Cue::HitFar, v3(pos)));
                }
                if pos.distance(ears) < 1_500.0 {
                    heat += 0.03;
                }
            }
            FxEvent::Struck { .. } => {
                sound.mixer.request(Request::own(Cue::HullHit));
                heat += 0.12;
            }
            FxEvent::Kill { pos } => {
                sound.mixer.request(Request::at(Cue::Explosion, v3(pos)));
                if pos.distance(ears) < 3_000.0 {
                    heat += 0.25;
                }
            }
            FxEvent::RockBreak { pos, .. } => sound.mixer.request(Request::at(Cue::RockBreak, v3(pos))),
            FxEvent::Clash { pos } => {
                sound.mixer.request(Request::at(Cue::Clash, v3(pos)));
                heat += 0.08;
            }
            FxEvent::MissileBurst { pos, .. } => {
                sound.mixer.request(Request::at(Cue::MissileBurst, v3(pos)));
                if pos.distance(ears) < 2_000.0 {
                    heat += 0.05;
                }
            }
            FxEvent::MissileLaunch { pos, own } => {
                sound.mixer.request(if own {
                    Request::own(Cue::MissileLaunch)
                } else {
                    Request::at(Cue::MissileLaunch, v3(pos))
                });
            }
            FxEvent::Transform { .. } => {}
        }
    }
    // Other suits' blades lighting up.
    for d in &drives {
        let on = d.flags & bc_proto::snapshot::ent_flags::SABER != 0;
        let was = sound.sabers.insert(d.slot, on).unwrap_or(false);
        if on && !was && !d.own {
            sound.mixer.request(Request::at(Cue::Saber, v3(d.pos)));
        }
    }
    sound.sabers.retain(|slot, _| drives.iter().any(|d| d.slot == *slot));

    // The bay: the klaxon and the doors as it cycles, the airlock's hiss. (The launch itself sounds
    // as the suit leaves the tunnel, when the cockpit below finds itself in the world.)
    if onfoot.seq != *last_seq {
        use crate::onfoot::Seq;
        let cues: &[Cue] = match onfoot.seq {
            Seq::Venting => &[Cue::Klaxon, Cue::DoorRumble],
            Seq::Arriving => &[Cue::Klaxon, Cue::DoorRumble, Cue::Dock],
            Seq::Entering => &[Cue::AirlockHiss],
            _ => &[],
        };
        for &c in cues {
            sound.mixer.request(Request::own(c));
        }
        *last_seq = onfoot.seq;
    }
    if onfoot.airlock_until() > *airlock_at {
        *airlock_at = onfoot.airlock_until();
        if onfoot.seq == crate::onfoot::Seq::Walking {
            sound.mixer.request(Request::own(Cue::AirlockHiss));
        }
    }

    // The cockpit (not while the view is in the bay).
    let cin = cockpit_in(&game, ui.playing() && !indoors.0);
    let mixer = &mut sound.mixer;
    let out = sound.cockpit.frame(now, &cin, &mut |c, gain| {
        mixer.request(Request { gain, ..Request::own(c) });
    });
    let saber = cin.alive && cin.saber;
    if saber && !sound.own_saber {
        sound.mixer.request(Request::own(Cue::Saber));
    }
    sound.own_saber = saber;
    for l in out.loops {
        let bus = mix.bus(l.cue.def().bus) * mix.master;
        audio.set_loop(l.cue, l.gain * l.cue.def().gain * bus, l.rate);
    }
    audio.set_lowpass(out.lowpass_hz);

    // Music.
    let m = sound.music.frame(
        time.delta_secs(),
        &MusicIn { in_world: ui.playing(), heat, threatened: cin.warned || cin.incoming },
    );
    let music = mix.music * mix.master;
    audio.set_loop(Cue::MusicTitle, m.title * Cue::MusicTitle.def().gain * music, 1.0);
    audio.set_loop(Cue::MusicCalm, m.calm * Cue::MusicCalm.def().gain * music, 1.0);
    audio.set_loop(Cue::MusicCombat, m.combat * Cue::MusicCombat.def().gain * music, 1.0);

    // Start the frame's voices.
    let mut starts = [Start { cue: Cue::UiClick, gain: 0.0, pan: 0.0, rate: 1.0 }; STARTS_PER_FRAME];
    let n = sound.mixer.frame(now, &listener, &mix, &mut starts);
    for s in &starts[..n] {
        audio.play(s);
    }
    sound.started += n as u32;
}
