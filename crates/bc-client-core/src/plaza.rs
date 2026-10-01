//! The people in the colony's city, as this pilot sees them (`bc_proto::presence`): the poses the
//! server relays, each kept with when it was heard, and drawn a little in the past, between two of
//! them. Off the sector's tick, like the hangar, but on its clock: each plaza datagram carries the
//! sector's tick, which keeps the client's clock (and the colony's day) right on foot.

use std::collections::{HashMap, VecDeque};

use bc_proto::presence::{PersonPose, PlazaReader};
use bc_sim::TICK_HZ;

/// People are drawn this far behind the server's clock, ticks (200 ms): poses come at 15 Hz and
/// are relayed at 10, so two of them nearly always straddle it.
pub const DELAY_TICKS: f64 = 6.0;
/// The own pose goes out this often, s (15 Hz).
pub const POSE_EVERY: f64 = 1.0 / 15.0;
/// Someone missing from the plaza for this long is gone, s.
const GONE: f64 = 1.0;
/// Poses kept per person.
const KEEP: usize = 16;

#[derive(Clone, Debug, Default)]
struct Track {
    /// (server tick heard, pose), oldest first.
    samples: VecDeque<(f64, PersonPose)>,
    /// When they were last in a plaza datagram (local s).
    seen: f64,
}

/// Everyone in the city near the pilot.
#[derive(Clone, Debug, Default)]
pub struct PlazaView {
    /// The strip the plaza's people are on (`None`: not in the city).
    pub strip: Option<u8>,
    tracks: HashMap<u16, Track>,
    /// The newest plaza datagram's tick.
    pub tick: u32,
    pub packets: u64,
}

fn lerp_angle(a: f32, b: f32, u: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let d = (b - a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    (a + d * u).rem_euclid(tau)
}

/// Between two poses, `u` of the way.
pub fn blend(a: &PersonPose, b: &PersonPose, u: f32) -> PersonPose {
    let l = |x: f32, y: f32| x + (y - x) * u;
    PersonPose {
        strip: b.strip,
        x: l(a.x, b.x),
        s: l(a.s, b.s),
        h: l(a.h, b.h),
        yaw: lerp_angle(a.yaw, b.yaw, u),
        pitch: l(a.pitch, b.pitch),
        speed: l(a.speed, b.speed),
        grounded: if u < 0.5 { a.grounded } else { b.grounded },
        running: if u < 0.5 { a.running } else { b.running },
        train: b.train,
    }
}

impl PlazaView {
    /// Takes in a plaza datagram received at local time `now` (s). Older than one already taken:
    /// ignored.
    pub fn on_datagram(&mut self, mut r: PlazaReader<'_>, now: f64) {
        if self.packets > 0 && r.tick < self.tick {
            return;
        }
        self.packets += 1;
        self.tick = r.tick;
        if r.strip != self.strip {
            self.tracks.clear();
            self.strip = r.strip;
        }
        let hz = f64::from(TICK_HZ);
        while let Some((id, age, pose)) = r.next_person() {
            let t = f64::from(r.tick) - f64::from(age) * hz;
            let track = self.tracks.entry(id).or_default();
            track.seen = now;
            // The same pose comes again until there's a newer one.
            match track.samples.back() {
                Some((last, p)) if (t - last).abs() < 0.5 || (*p == pose && t > *last) => continue,
                Some((last, _)) if t < *last => continue,
                _ => {}
            }
            track.samples.push_back((t, pose));
            while track.samples.len() > KEEP {
                track.samples.pop_front();
            }
        }
        self.tracks.retain(|_, t| now - t.seen < GONE);
    }

    /// How many people are in view.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Everyone at server time `t` (ticks): between the two poses either side of it, or at the
    /// nearest one.
    pub fn people_at(&self, t: f64) -> Vec<(u16, PersonPose)> {
        let mut out: Vec<(u16, PersonPose)> = self
            .tracks
            .iter()
            .filter_map(|(id, track)| {
                let s = &track.samples;
                let (first, last) = (s.front()?, s.back()?);
                let pose = if t <= first.0 {
                    first.1
                } else if t >= last.0 {
                    last.1
                } else {
                    let k = s.iter().position(|(ts, _)| *ts > t).unwrap_or(s.len() - 1);
                    let (a, b) = (&s[k - 1], &s[k]);
                    let u = ((t - a.0) / (b.0 - a.0).max(1e-6)) as f32;
                    // Getting on or off a train: no blending a place in it with one outside.
                    if a.1.train != b.1.train {
                        if u < 0.5 { a.1 } else { b.1 }
                    } else {
                        blend(&a.1, &b.1, u)
                    }
                };
                Some((*id, pose))
            })
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }
}

/// A pilot's flight suit's colour, from their name: the same on every screen. Bright enough to
/// pick out across a square, never grey.
pub fn suit_colour(name: &str) -> [f32; 3] {
    let mut h: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    let hue = (h % 360) as f32 / 60.0;
    let sat = 0.55 + 0.35 * ((h >> 9) % 100) as f32 / 100.0;
    let val = 0.55 + 0.35 * ((h >> 17) % 100) as f32 / 100.0;
    let c = val * sat;
    let x = c * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = val - c;
    [r + m, g + m, b + m]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::MAX_DATAGRAM;
    use bc_proto::presence::PlazaWriter;

    fn packet(tick: u32, people: &[(u16, f32, PersonPose)]) -> Vec<u8> {
        let mut buf = vec![0u8; MAX_DATAGRAM];
        let mut w = PlazaWriter::new(&mut buf, tick, Some(0));
        for (id, age, p) in people {
            w.push(*id, *age, p);
        }
        let n = w.finish();
        buf.truncate(n);
        buf
    }

    fn at(x: f32, yaw: f32) -> PersonPose {
        PersonPose { x, s: 1_675.0, yaw, grounded: true, ..PersonPose::default() }
    }

    #[test]
    fn people_are_drawn_between_the_poses_heard() {
        let mut v = PlazaView::default();
        // Walking +x at 3 m/s, a pose every 2 ticks, relayed every 3.
        for k in 0..10u32 {
            let tick = 100 + 3 * k;
            let heard = tick as f32 - 1.0;
            let p = at(heard * 0.1, 0.1);
            v.on_datagram(
                PlazaReader::new(&packet(tick, &[(5, 1.0 / 30.0, p)])).unwrap(),
                f64::from(k) * 0.1,
            );
        }
        let people = v.people_at(115.5);
        assert_eq!(people.len(), 1);
        assert!((people[0].1.x - 11.55).abs() < 0.02, "{}", people[0].1.x);
        // Past the newest pose: held there, not run on.
        let last = v.people_at(1_000.0)[0].1.x;
        assert!((last - 12.6).abs() < 0.02, "{last}");
    }

    #[test]
    fn headings_turn_the_short_way_and_the_old_and_the_gone_drop_out() {
        let m = lerp_angle(6.2, 0.1, 0.5);
        assert!(!(0.05..=6.25).contains(&m), "across 0, not the long way round: {m}");
        let mut v = PlazaView::default();
        v.on_datagram(
            PlazaReader::new(&packet(50, &[(1, 0.0, at(0.0, 0.0)), (2, 0.0, at(5.0, 0.0))])).unwrap(),
            0.0,
        );
        // An older datagram (reordered) changes nothing.
        v.on_datagram(PlazaReader::new(&packet(40, &[(3, 0.0, at(9.0, 0.0))])).unwrap(), 0.1);
        assert_eq!(v.len(), 2);
        // Only one of them stays in the plaza.
        for k in 1..=12u32 {
            v.on_datagram(
                PlazaReader::new(&packet(50 + 3 * k, &[(1, 0.0, at(0.0, 0.0))])).unwrap(),
                0.1 * f64::from(k),
            );
        }
        assert_eq!(v.people_at(80.0).iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn everyone_has_a_colour_of_their_own() {
        let a = suit_colour("Heero");
        let b = suit_colour("Duo");
        assert_ne!(a, b);
        assert_eq!(a, suit_colour("Heero"));
        for name in ["Heero", "Duo", "Trowa", "Quatre", "Wufei", "Relena", ""] {
            let c = suit_colour(name);
            let (hi, lo) = (c.iter().copied().fold(0.0, f32::max), c.iter().copied().fold(1.0, f32::min));
            assert!(hi - lo > 0.2 && hi <= 1.0 && lo >= 0.0, "{name}: {c:?}");
        }
    }
}
