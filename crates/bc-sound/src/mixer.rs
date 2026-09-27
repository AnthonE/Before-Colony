//! Which of a frame's cues get a voice, how loud, and where in the stereo field.
//!
//! Order matters (it's the Gates mixer's, `crates/sound/src/mixer.rs` there):
//! 1. cull by distance: a cue past its radius is no voice, and costs no budget;
//! 2. the cue's cooldown (a retrigger storm's cheapest defence);
//! 3. priority, then loudness, against the frame's start budget.
//!
//! No allocation per frame: the queue and the output are fixed arrays.

use crate::{CUE_COUNT, Cue, Mix, falloff};

/// The most requests one frame can hold.
pub const QUEUE: usize = 96;
/// The most voices one frame starts.
pub const STARTS_PER_FRAME: usize = 8;

/// A cue somebody wants heard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub cue: Cue,
    /// Where it happened (world m); `None`: the pilot's own, not positional.
    pub at: Option<[f32; 3]>,
    /// The caller's scaling, 0..1.
    pub gain: f32,
}

impl Request {
    pub fn own(cue: Cue) -> Self {
        Self { cue, at: None, gain: 1.0 }
    }

    pub fn at(cue: Cue, at: [f32; 3]) -> Self {
        Self { cue, at: Some(at), gain: 1.0 }
    }
}

/// The ears: where they are and which way is right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub pos: [f32; 3],
    pub right: [f32; 3],
}

/// A voice to start now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Start {
    pub cue: Cue,
    /// Final linear gain: the cue's, the caller's, distance, bus and master.
    pub gain: f32,
    /// -1 (left) to 1 (right).
    pub pan: f32,
    /// Playback rate.
    pub rate: f32,
}

const NONE: Start = Start { cue: Cue::UiClick, gain: 0.0, pan: 0.0, rate: 1.0 };

pub struct Mixer {
    queue: [Request; QUEUE],
    queued: usize,
    /// When each cue last started.
    last: [f64; CUE_COUNT],
    rng: u32,
    /// Requests refused because the queue was full (a caller bug if not zero).
    pub dropped: u32,
    /// Audible, off cooldown, and still refused for want of budget.
    pub starved: u32,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            queue: [Request::own(Cue::UiClick); QUEUE],
            queued: 0,
            last: [f64::NEG_INFINITY; CUE_COUNT],
            rng: 0x2545_F491,
            dropped: 0,
            starved: 0,
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Mixer {
    pub fn request(&mut self, r: Request) {
        if self.queued < QUEUE {
            self.queue[self.queued] = r;
            self.queued += 1;
        } else {
            self.dropped += 1;
        }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Decides this frame's voices into `out`; returns how many. Empties the queue.
    pub fn frame(&mut self, now: f64, l: &Listener, mix: &Mix, out: &mut [Start; STARTS_PER_FRAME]) -> usize {
        // Candidates: (priority, gain, pan, cue), audible only.
        let mut cand: [(u8, f32, f32, Cue); QUEUE] = [(0, 0.0, 0.0, Cue::UiClick); QUEUE];
        let mut n = 0;
        for k in 0..self.queued {
            let r = self.queue[k];
            let d = r.cue.def();
            if d.looped {
                continue;
            }
            // The pilot's own cues (radius 0) aren't positional, wherever they're asked from.
            let (dist_gain, pan) = match r.at.filter(|_| d.radius > 0.0) {
                Some(at) => {
                    let rel = sub(at, l.pos);
                    let dist = dot(rel, rel).sqrt();
                    let pan =
                        if dist > 1.0 { (dot(rel, l.right) / dist).clamp(-1.0, 1.0) * 0.8 } else { 0.0 };
                    (falloff(dist, d.radius), pan)
                }
                None => (1.0, 0.0),
            };
            let gain = d.gain * r.gain.clamp(0.0, 1.0) * dist_gain;
            if gain < 0.002 {
                continue;
            }
            cand[n] = (d.priority, gain, pan, r.cue);
            n += 1;
        }
        self.queued = 0;
        // Priority first, then loudness.
        cand[..n].sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
        let mut started = 0;
        for &(_, gain, pan, cue) in &cand[..n] {
            let d = cue.def();
            if now - self.last[cue as usize] < f64::from(d.cooldown) {
                continue;
            }
            if started == STARTS_PER_FRAME {
                self.starved += 1;
                continue;
            }
            self.last[cue as usize] = now;
            let rate = 1.0 + d.pitch_var * self.rand();
            let gain = gain * mix.bus(d.bus) * mix.master;
            out[started] = Start { cue, gain, pan, rate };
            started += 1;
        }
        for s in &mut out[started..] {
            *s = NONE;
        }
        started
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EARS: Listener = Listener { pos: [0.0; 3], right: [1.0, 0.0, 0.0] };

    fn run(m: &mut Mixer, now: f64, reqs: &[Request]) -> Vec<Start> {
        for r in reqs {
            m.request(*r);
        }
        let mut out = [NONE; STARTS_PER_FRAME];
        let n = m.frame(now, &EARS, &Mix::default(), &mut out);
        out[..n].to_vec()
    }

    #[test]
    fn far_cues_are_no_voice_at_all() {
        let mut m = Mixer::default();
        let far = [Cue::BeamRifle.def().radius + 10.0, 0.0, 0.0];
        assert!(run(&mut m, 0.0, &[Request::at(Cue::BeamRifle, far)]).is_empty());
        assert_eq!(m.starved, 0, "culled isn't starved");
    }

    #[test]
    fn right_is_right() {
        let mut m = Mixer::default();
        let s = run(&mut m, 0.0, &[Request::at(Cue::BeamRifle, [100.0, 0.0, 0.0])]);
        assert!(s[0].pan > 0.5);
        let s = run(&mut m, 1.0, &[Request::at(Cue::BeamRifle, [-100.0, 0.0, 0.0])]);
        assert!(s[0].pan < -0.5);
    }

    #[test]
    fn cooldown_stops_a_retrigger_storm() {
        let mut m = Mixer::default();
        let storm = [Request::own(Cue::Gun); 20];
        assert_eq!(run(&mut m, 0.0, &storm).len(), 1);
        assert!(run(&mut m, 0.01, &storm).is_empty());
        assert_eq!(run(&mut m, 0.1, &storm).len(), 1);
    }

    #[test]
    fn the_budget_goes_to_what_matters() {
        let mut m = Mixer::default();
        let mut reqs: Vec<Request> = [
            Cue::HitFar,
            Cue::Gun,
            Cue::BeamGun,
            Cue::Flame,
            Cue::RcsPuff,
            Cue::UiClick,
            Cue::MissileLaunch,
            Cue::BeamRifle,
            Cue::Saber,
        ]
        .iter()
        .map(|c| Request::at(*c, [10.0, 0.0, 0.0]))
        .collect();
        reqs.push(Request::own(Cue::HullHit));
        let s = run(&mut m, 0.0, &reqs);
        assert_eq!(s.len(), STARTS_PER_FRAME);
        assert_eq!(s[0].cue, Cue::HullHit, "the pilot being hit comes first");
        assert!(m.starved >= 2);
    }

    #[test]
    fn volume_settings_apply() {
        let mut m = Mixer::default();
        m.request(Request::own(Cue::HullHit));
        let mut out = [NONE; STARTS_PER_FRAME];
        let quiet = Mix { master: 0.0, ..Mix::default() };
        m.frame(0.0, &EARS, &quiet, &mut out);
        assert_eq!(out[0].gain, 0.0);
    }
}
