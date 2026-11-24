//! The per-frame sound work (the mixer, the cockpit, the street, the music director) allocates
//! nothing.

use bc_sound::cockpit::{Cockpit, CockpitIn};
use bc_sound::mixer::{Listener, Mixer, Request, STARTS_PER_FRAME, Start};
use bc_sound::music::{Director, MusicIn};
use bc_sound::street::{Street, StreetIn};
use bc_sound::{Cue, Mix};

#[global_allocator]
static A: bc_alloc::CountingAlloc = bc_alloc::CountingAlloc;

#[test]
fn a_frame_of_sound_does_not_allocate() {
    let ears = Listener { pos: [0.0; 3], right: [1.0, 0.0, 0.0] };
    let mut mixer = Mixer::default();
    let mut cockpit = Cockpit::default();
    let mut music = Director::default();
    let mut street = Street::default();
    let mut out = [Start { cue: Cue::UiClick, gain: 0.0, pan: 0.0, rate: 1.0 }; STARTS_PER_FRAME];
    let mix = Mix::default();
    let mut frame =
        |k: usize, mixer: &mut Mixer, cockpit: &mut Cockpit, street: &mut Street, music: &mut Director| {
            let now = k as f64 / 60.0;
            let i = CockpitIn {
                in_world: true,
                alive: !k.is_multiple_of(200),
                thrust: (k % 7) as f32 / 7.0,
                lock_progress: (k % 50) as f32 / 50.0,
                propellant: 0.1,
                rcs: k.is_multiple_of(3),
                held: k % 90 < 45,
                // Gripping, landing, walking and hiding on a body.
                grip: k % 120 < 100,
                footing: [0, 2, 1, 1][k / 30 % 4],
                touchdown: (k % 11) as f32,
                footfalls: (k / 12) as u32,
                cover: (k / 40 % 4) as u8,
                ..Default::default()
            };
            let cues = &mut |c: Cue, gain: f32| mixer.request(Request { gain, ..Request::own(c) });
            let _ = cockpit.frame(now, &i, cues);
            for j in 0..10 {
                mixer.request(Request::at(Cue::HitFar, [j as f32 * 50.0, 0.0, 0.0]));
            }
            mixer.request(Request::at(Cue::FootstepFar, [120.0, 0.0, 0.0]));
            // On foot in the town: walking, then at the wheel, the traffic going by, up a lift.
            let driving = k % 400 > 250;
            let s = StreetIn {
                in_town: k % 900 < 800,
                in_bay: k % 900 >= 800,
                feet: (!driving).then_some([k as f32 * 0.07, 0.0, 0.0]),
                grounded: k % 50 < 45,
                driving,
                speed: (k % 30) as f32,
                traffic: (k % 13) as f32 / 13.0,
                crowd: (k % 17) as f32 / 17.0,
                rides: (k / 300) as u32,
                ..Default::default()
            };
            for id in 0..40u32 {
                if let Some(gain) = street.car(now, id, ((k as u32 + id * 7) % 30) as f32, 12.0) {
                    mixer.request(Request { gain, ..Request::at(Cue::CarPass, [5.0, 0.0, 0.0]) });
                }
            }
            let cues = &mut |c: Cue, gain: f32| mixer.request(Request { gain, ..Request::own(c) });
            let _ = street.frame(now, &s, cues);
            let _ = mixer.frame(now, &ears, &mix, &mut out);
            let _ = music
                .frame(1.0 / 60.0, &MusicIn { in_world: true, heat: 0.01, threatened: k.is_multiple_of(2) });
        };
    // Warm up, then count.
    frame(0, &mut mixer, &mut cockpit, &mut street, &mut music);
    let ((), allocs) = bc_alloc::count(|| {
        for k in 1..2_000 {
            frame(k, &mut mixer, &mut cockpit, &mut street, &mut music);
        }
    });
    assert_eq!(allocs, 0);
}
