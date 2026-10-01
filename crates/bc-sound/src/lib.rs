//! Before Colony's sound. Every cue is synthesised at boot from arithmetic ([`synth`]); there are
//! no audio assets. The [`mixer`] picks which of a frame's requests get a voice and where they sit
//! in the stereo field, [`cockpit`] turns the pilot's state into engine loops, lock tones and
//! alarms, and [`music`] follows the fight. The title theme ([`title`]) plays on the Super
//! Famicom's sound chip, in software ([`spc`]).
//!
//! Pure Rust: no Bevy, no clock, no I/O. The browser client plays what this decides through Web
//! Audio (`bc-client/src/audio.rs`), and every rule here is a native test.
//!
//! **Sound in space.** There's no air to carry it, so what the pilot hears is the cockpit's: the
//! suit's own machinery through its frame, and everything else as the sensors render it, quieter
//! with distance and gone past a cue's radius.

pub mod cockpit;
pub mod mixer;
pub mod music;
pub mod spc;
pub mod synth;
pub mod title;

/// The bank's sample rate. The browser resamples to its own.
pub const SAMPLE_RATE: u32 = 48_000;

/// A mix bus: each has its own volume setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    /// Weapons, hits, explosions: the world.
    Effects,
    /// The suit: engines, lock tones, alarms, the ZERO System.
    Cockpit,
    /// Menus.
    Ui,
    Music,
}

impl Bus {
    pub const ALL: [Bus; 4] = [Bus::Effects, Bus::Cockpit, Bus::Ui, Bus::Music];
}

/// The volume settings, 0..1 each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub master: f32,
    pub effects: f32,
    pub cockpit: f32,
    pub music: f32,
}

impl Default for Mix {
    fn default() -> Self {
        Self { master: 0.8, effects: 0.9, cockpit: 0.8, music: 0.5 }
    }
}

impl Mix {
    /// A bus's volume (menus sit with the cockpit).
    pub fn bus(&self, bus: Bus) -> f32 {
        match bus {
            Bus::Effects => self.effects,
            Bus::Cockpit | Bus::Ui => self.cockpit,
            Bus::Music => self.music,
        }
    }
}

macro_rules! cues {
    ($($name:ident),* $(,)?) => {
        /// Every sound in the bank.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum Cue { $($name),* }

        impl Cue {
            pub const ALL: &'static [Cue] = &[$(Cue::$name),*];
        }
    };
}

cues!(
    // Weapons (positional).
    BeamRifle,
    BeamHeavy,
    BusterFire,
    Gun,
    BeamGun,
    Flame,
    MissileLaunch,
    Saber,
    // Impacts (positional).
    HitFar,
    MissileBurst,
    Explosion,
    RockBreak,
    Clash,
    // The pilot's own.
    HullHit,
    Transform,
    JammerOn,
    JammerOff,
    Grab,
    Stow,
    Throw,
    Jettison,
    Dock,
    Sale,
    LockBeep,
    LowFuel,
    RcsPuff,
    ZeroOn,
    Seizure,
    Destroyed,
    Launch,
    // The hangar bay: its klaxon, its doors, the airlock.
    Klaxon,
    DoorRumble,
    AirlockHiss,
    // Menus.
    UiClick,
    UiConfirm,
    // Loops.
    ThrusterLoop,
    BoostLoop,
    ChargeLoop,
    SaberHum,
    LockSolid,
    MissileAlarm,
    GStrain,
    ZeroDrone,
    CockpitHum,
    MusicCalm,
    MusicCombat,
    MusicTitle,
    // Something inside the suit: damaged, failed, a holed tank venting. (Last, so every cue
    // before keeps its seed and sounds as it did.)
    SystemCrit,
    SystemFail,
    Leak,
);

/// How many cues there are.
pub const CUE_COUNT: usize = Cue::ALL.len();

/// How a cue is played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueDef {
    pub bus: Bus,
    /// Who wins a voice when there are too few (higher wins).
    pub priority: u8,
    /// Heard within this distance, m. 0: the pilot's own (not positional).
    pub radius: f32,
    /// The shortest time between two starts, s (a retrigger storm defence).
    pub cooldown: f32,
    /// Linear gain before the buses.
    pub gain: f32,
    /// Playback rate varies by up to ± this, so repeats don't sound identical.
    pub pitch_var: f32,
    /// Loops forever at a gain the caller drives.
    pub looped: bool,
}

const fn def(bus: Bus, priority: u8, radius: f32, cooldown: f32, gain: f32, pitch_var: f32) -> CueDef {
    CueDef { bus, priority, radius, cooldown, gain, pitch_var, looped: false }
}

const fn looped(bus: Bus, gain: f32) -> CueDef {
    CueDef { bus, priority: 255, radius: 0.0, cooldown: 0.0, gain, pitch_var: 0.0, looped: true }
}

use Bus::{Cockpit, Effects, Music, Ui};

impl Cue {
    pub fn def(self) -> CueDef {
        match self {
            Cue::BeamRifle => def(Effects, 6, 2_500.0, 0.03, 0.55, 0.04),
            Cue::BeamHeavy => def(Effects, 7, 3_500.0, 0.05, 0.7, 0.04),
            Cue::BusterFire => def(Effects, 9, 8_000.0, 0.3, 1.0, 0.0),
            Cue::Gun => def(Effects, 4, 1_800.0, 0.025, 0.35, 0.1),
            Cue::BeamGun => def(Effects, 4, 2_000.0, 0.03, 0.35, 0.08),
            Cue::Flame => def(Effects, 4, 800.0, 0.08, 0.4, 0.1),
            Cue::MissileLaunch => def(Effects, 5, 2_500.0, 0.06, 0.45, 0.08),
            Cue::Saber => def(Effects, 6, 600.0, 0.15, 0.6, 0.06),
            Cue::HitFar => def(Effects, 3, 2_000.0, 0.03, 0.4, 0.12),
            Cue::MissileBurst => def(Effects, 6, 4_000.0, 0.05, 0.6, 0.08),
            Cue::Explosion => def(Effects, 8, 8_000.0, 0.1, 1.0, 0.06),
            Cue::RockBreak => def(Effects, 7, 5_000.0, 0.2, 0.9, 0.05),
            Cue::Clash => def(Effects, 8, 1_500.0, 0.1, 0.8, 0.05),
            Cue::HullHit => def(Cockpit, 9, 0.0, 0.06, 0.8, 0.08),
            Cue::Transform => def(Cockpit, 7, 0.0, 0.3, 0.6, 0.0),
            Cue::JammerOn => def(Cockpit, 7, 0.0, 0.3, 0.5, 0.0),
            Cue::JammerOff => def(Cockpit, 7, 0.0, 0.3, 0.45, 0.0),
            Cue::Grab => def(Cockpit, 6, 0.0, 0.15, 0.6, 0.05),
            Cue::Stow => def(Cockpit, 6, 0.0, 0.2, 0.6, 0.0),
            Cue::Throw => def(Cockpit, 6, 0.0, 0.2, 0.5, 0.05),
            Cue::Jettison => def(Cockpit, 6, 0.0, 0.5, 0.55, 0.0),
            Cue::Dock => def(Cockpit, 7, 0.0, 1.0, 0.5, 0.0),
            Cue::Sale => def(Cockpit, 7, 0.0, 0.5, 0.5, 0.0),
            Cue::LockBeep => def(Cockpit, 6, 0.0, 0.05, 0.3, 0.0),
            Cue::LowFuel => def(Cockpit, 7, 0.0, 1.0, 0.35, 0.0),
            Cue::RcsPuff => def(Cockpit, 3, 0.0, 0.1, 0.25, 0.15),
            Cue::ZeroOn => def(Cockpit, 9, 0.0, 1.0, 0.7, 0.0),
            Cue::Seizure => def(Cockpit, 9, 0.0, 1.0, 0.7, 0.0),
            Cue::Destroyed => def(Cockpit, 10, 0.0, 1.0, 0.9, 0.0),
            Cue::Launch => def(Cockpit, 8, 0.0, 1.0, 0.6, 0.0),
            Cue::SystemCrit => def(Cockpit, 8, 0.0, 0.25, 0.5, 0.0),
            Cue::SystemFail => def(Cockpit, 9, 0.0, 0.6, 0.55, 0.0),
            Cue::Leak => def(Cockpit, 4, 0.0, 0.5, 0.25, 0.1),
            Cue::Klaxon => def(Cockpit, 7, 0.0, 1.0, 0.4, 0.0),
            Cue::DoorRumble => def(Cockpit, 6, 0.0, 1.0, 0.55, 0.0),
            Cue::AirlockHiss => def(Cockpit, 5, 0.0, 0.5, 0.35, 0.05),
            Cue::UiClick => def(Ui, 5, 0.0, 0.04, 0.35, 0.03),
            Cue::UiConfirm => def(Ui, 6, 0.0, 0.2, 0.45, 0.0),
            Cue::ThrusterLoop => looped(Cockpit, 0.35),
            Cue::BoostLoop => looped(Cockpit, 0.45),
            Cue::ChargeLoop => looped(Cockpit, 0.3),
            Cue::SaberHum => looped(Cockpit, 0.25),
            Cue::LockSolid => looped(Cockpit, 0.2),
            Cue::MissileAlarm => looped(Cockpit, 0.3),
            Cue::GStrain => looped(Cockpit, 0.5),
            Cue::ZeroDrone => looped(Cockpit, 0.35),
            Cue::CockpitHum => looped(Cockpit, 0.12),
            Cue::MusicCalm => looped(Music, 0.5),
            Cue::MusicCombat => looped(Music, 0.5),
            Cue::MusicTitle => looped(Music, 0.5),
        }
    }

    /// The loops, in [`cockpit::Loops`] order.
    pub fn loops() -> impl Iterator<Item = Cue> {
        Cue::ALL.iter().copied().filter(|c| c.def().looped)
    }
}

/// The sample rate a cue is rendered at: [`SAMPLE_RATE`], the score's lower one, or the sound
/// chip's for the title theme.
pub fn sample_rate(cue: Cue) -> u32 {
    match cue {
        Cue::MusicCalm | Cue::MusicCombat => music::MUSIC_RATE,
        Cue::MusicTitle => spc::RATE,
        _ => SAMPLE_RATE,
    }
}

/// A cue's channels: the title theme is stereo, the rest mono.
pub fn channels(cue: Cue) -> usize {
    if cue == Cue::MusicTitle { 2 } else { 1 }
}

/// How much of a positional cue reaches the pilot from `dist` m away: 1 up close, falling off with
/// distance, and silent at the cue's `radius`.
pub fn falloff(dist: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 1.0;
    }
    if dist >= radius {
        return 0.0;
    }
    // Inverse distance from a reference a tenth of the way out, tapered to reach zero at the edge.
    let reference = radius * 0.1;
    let near = reference / (reference + dist.max(0.0));
    let x = dist / radius;
    let taper = 1.0 - x * x * x * x;
    (near * taper).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_is_one_close_and_falls_to_silence() {
        assert!((falloff(0.0, 1_000.0) - 1.0).abs() < 1e-6);
        let mut last = 1.0;
        for d in (0..1_000).step_by(25) {
            let g = falloff(d as f32, 1_000.0);
            assert!(g <= last + 1e-6, "rises at {d}");
            last = g;
        }
        assert_eq!(falloff(1_000.0, 1_000.0), 0.0);
        assert_eq!(falloff(5.0, 0.0), 1.0, "the pilot's own isn't positional");
    }

    #[test]
    fn every_cue_has_a_sane_definition() {
        for c in Cue::ALL {
            let d = c.def();
            assert!(d.gain > 0.0 && d.gain <= 1.0, "{c:?}");
            assert!(d.pitch_var >= 0.0 && d.pitch_var < 0.3, "{c:?}");
            if d.looped {
                assert_eq!(d.radius, 0.0, "{c:?}: loops are the cockpit's or the music's");
            }
        }
    }
}
