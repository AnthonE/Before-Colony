//! The per-frame sound work (the mixer, the cockpit, the music director) allocates nothing.

use bc_sound::cockpit::{Cockpit, CockpitIn};
use bc_sound::mixer::{Listener, Mixer, Request, STARTS_PER_FRAME, Start};
use bc_sound::music::{Director, MusicIn};
use bc_sound::{Cue, Mix};

#[global_allocator]
static A: bc_alloc::CountingAlloc = bc_alloc::CountingAlloc;

#[test]
fn a_frame_of_sound_does_not_allocate() {
    let ears = Listener { pos: [0.0; 3], right: [1.0, 0.0, 0.0] };
    let mut mixer = Mixer::default();
    let mut cockpit = Cockpit::default();
    let mut music = Director::default();
    let mut out = [Start { cue: Cue::UiClick, gain: 0.0, pan: 0.0, rate: 1.0 }; STARTS_PER_FRAME];
    let mix = Mix::default();
    let mut frame = |k: usize, mixer: &mut Mixer, cockpit: &mut Cockpit, music: &mut Director| {
        let now = k as f64 / 60.0;
        let i = CockpitIn {
            in_world: true,
            alive: !k.is_multiple_of(200),
            thrust: (k % 7) as f32 / 7.0,
            lock_progress: (k % 50) as f32 / 50.0,
            propellant: 0.1,
            rcs: k.is_multiple_of(3),
            held: k % 90 < 45,
            ..Default::default()
        };
        let cues = &mut |c: Cue| mixer.request(Request::own(c));
        let _ = cockpit.frame(now, &i, cues);
        for j in 0..10 {
            mixer.request(Request::at(Cue::HitFar, [j as f32 * 50.0, 0.0, 0.0]));
        }
        let _ = mixer.frame(now, &ears, &mix, &mut out);
        let _ =
            music.frame(1.0 / 60.0, &MusicIn { in_world: true, heat: 0.01, threatened: k.is_multiple_of(2) });
    };
    // Warm up, then count.
    frame(0, &mut mixer, &mut cockpit, &mut music);
    let ((), allocs) = bc_alloc::count(|| {
        for k in 1..2_000 {
            frame(k, &mut mixer, &mut cockpit, &mut music);
        }
    });
    assert_eq!(allocs, 0);
}
