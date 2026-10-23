//! Which of the suits in view the HUD names, and how much it says of each (`docs/UI.md`: quiet
//! until it matters). Armored Core VI and Mecha BREAK name the target that matters and leave the
//! rest as brackets; a screen of drones each tagged `LOCK!` says nothing the caution banner
//! doesn't, and the tags pile on top of each other.
//!
//! - One **focus**: the pilot's own lock-on, else their missile lock, else the Orrery's pick, else
//!   the nearest hostile locking on to them, else the nearest hostile. It gets the full tag and the
//!   bright bracket; the others' brackets are dimmed.
//! - Anyone within [`CLOSE`] is named in full too (someone near enough to talk to or to fight).
//! - A suit locking on to the pilot, and the nearest [`RANGED`] others, carry their range (and the
//!   lock: `LOCK 2.1 km`).
//! - No tag lands on another: in order of importance (the focus first, then the nearest), a tag
//!   that would overlap one already placed says only its range, or nothing. The focus is never cut.

/// A suit in view, as the HUD sees it: where its bracket is (screen px), how far, and what it is to
/// the pilot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    pub at: [f32; 2],
    pub dist: f32,
    pub hostile: bool,
    pub wreck: bool,
    /// The pilot's own lock-on is on it.
    pub locked_on: bool,
    /// The pilot's missile lock on it, building or acquired.
    pub locking: bool,
    /// The Orrery's recommended target.
    pub orrery: bool,
    /// It is locking on to the pilot.
    pub locked_on_you: bool,
}

/// How much a bracket's tag says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tag {
    /// Nothing: the bracket alone.
    #[default]
    Bare,
    /// Its range (and whether it's locking on to the pilot).
    Range,
    /// Its name, range and everything that matters about it.
    Full,
}

/// What the HUD draws of one suit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub tag: Tag,
    /// The focus: the bright bracket.
    pub focus: bool,
}

/// Within this (m), a suit is named in full.
pub const CLOSE: f32 = 600.0;
/// How many of the nearest others carry their range.
pub const RANGED: usize = 3;
/// The room a tag takes (px), to the right of its bracket: a full tag's two or three lines, a
/// range's one.
const FULL: [f32; 2] = [170.0, 44.0];
const RANGE: [f32; 2] = [84.0, 15.0];
/// Where a tag starts, from its bracket's centre (the bracket is 34 px, its tag 5 px right of it).
const OFFSET: [f32; 2] = [22.0, -19.0];

fn rank(s: &Seen) -> Option<u8> {
    if s.wreck {
        return None;
    }
    Some(if s.locked_on {
        0
    } else if s.locking {
        1
    } else if s.orrery {
        2
    } else if s.hostile && s.locked_on_you {
        3
    } else if s.hostile {
        4
    } else {
        return None;
    })
}

fn rect(s: &Seen, size: [f32; 2]) -> [f32; 4] {
    let x = s.at[0] + OFFSET[0];
    let y = s.at[1] + OFFSET[1];
    [x, y, x + size[0], y + size[1]]
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// What to draw of each suit in `seen` (nearest first) into `out` (as long).
pub fn choose(seen: &[Seen], out: &mut [Shown]) {
    let n = seen.len().min(out.len());
    let focus = (0..n).filter_map(|i| rank(&seen[i]).map(|r| (r, i))).min().map(|(_, i)| i);
    let mut placed: Vec<[f32; 4]> = Vec::with_capacity(n);
    let mut ranged = 0;
    // The focus first, then the rest, nearest first.
    for i in focus.into_iter().chain((0..n).filter(|&i| Some(i) != focus)) {
        let s = &seen[i];
        let is_focus = Some(i) == focus;
        let want = if is_focus || (!s.wreck && (s.dist < CLOSE || s.locked_on || s.locking || s.orrery)) {
            Tag::Full
        } else if !s.wreck && (s.locked_on_you || ranged < RANGED) {
            ranged += 1;
            Tag::Range
        } else {
            Tag::Bare
        };
        let mut tag = want;
        if !is_focus {
            if tag == Tag::Full && placed.iter().any(|p| overlaps(*p, rect(s, FULL))) {
                tag = Tag::Range;
            }
            if tag == Tag::Range && placed.iter().any(|p| overlaps(*p, rect(s, RANGE))) {
                tag = Tag::Bare;
            }
        }
        match tag {
            Tag::Full => placed.push(rect(s, FULL)),
            Tag::Range => placed.push(rect(s, RANGE)),
            Tag::Bare => {}
        }
        out[i] = Shown { tag, focus: is_focus };
    }
    for o in out.iter_mut().skip(n) {
        *o = Shown::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drone(x: f32, y: f32, dist: f32) -> Seen {
        Seen { at: [x, y], dist, hostile: true, locked_on_you: true, ..Seen::default() }
    }

    fn shown(seen: &[Seen]) -> Vec<Shown> {
        let mut out = vec![Shown::default(); seen.len()];
        choose(seen, &mut out);
        out
    }

    #[test]
    fn a_swarm_locking_on_has_one_name_and_no_tag_on_another() {
        // The cockpit shot: a dozen drones 2–4 km out, bunched, every one locking on.
        let seen: Vec<Seen> = (0..12)
            .map(|i| {
                drone(
                    500.0 + (i % 4) as f32 * 40.0,
                    260.0 + (i / 4) as f32 * 30.0,
                    2_000.0 + i as f32 * 150.0,
                )
            })
            .collect();
        let out = shown(&seen);
        assert_eq!(out.iter().filter(|s| s.tag == Tag::Full).count(), 1);
        assert!(out[0].focus && out[0].tag == Tag::Full, "the nearest locker is the focus");
        assert_eq!(out.iter().filter(|s| s.focus).count(), 1);
        let rects: Vec<[f32; 4]> = seen
            .iter()
            .zip(&out)
            .filter_map(|(s, o)| match o.tag {
                Tag::Full => Some(rect(s, FULL)),
                Tag::Range => Some(rect(s, RANGE)),
                Tag::Bare => None,
            })
            .collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!overlaps(*a, *b), "two tags overlap: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn spread_out_lockers_each_say_they_are_locking() {
        let seen: Vec<Seen> =
            (0..5).map(|i| drone(100.0 + i as f32 * 220.0, 300.0, 1_500.0 + i as f32 * 300.0)).collect();
        let out = shown(&seen);
        assert_eq!(out[0].tag, Tag::Full);
        assert!(out[1..].iter().all(|s| s.tag == Tag::Range), "{out:?}");
    }

    #[test]
    fn the_pilots_own_lock_is_the_focus_wherever_it_is() {
        let mut seen: Vec<Seen> =
            (0..4).map(|i| drone(400.0, 300.0 + i as f32 * 4.0, 1_000.0 + i as f32 * 10.0)).collect();
        seen[3].locked_on = true;
        let out = shown(&seen);
        assert!(out[3].focus && out[3].tag == Tag::Full);
        // Bunched on top of it, the rest give way.
        assert!(out[..3].iter().all(|s| !s.focus && s.tag == Tag::Bare), "{out:?}");
    }

    #[test]
    fn friends_close_by_are_named_and_far_ones_are_not() {
        let near = Seen { at: [200.0, 200.0], dist: 300.0, ..Seen::default() };
        let far = Seen { at: [600.0, 200.0], dist: 3_000.0, ..Seen::default() };
        let farther = Seen { at: [900.0, 500.0], dist: 5_000.0, ..Seen::default() };
        let out = shown(&[near, far, farther]);
        assert_eq!(out[0], Shown { tag: Tag::Full, focus: false }, "no hostile: no focus");
        assert_eq!(out[1].tag, Tag::Range);
        assert_eq!(out[2].tag, Tag::Range);
    }

    #[test]
    fn wrecks_are_never_the_focus_and_say_nothing() {
        let wreck = Seen { at: [300.0, 300.0], dist: 100.0, hostile: true, wreck: true, ..Seen::default() };
        let out = shown(&[wreck, drone(700.0, 300.0, 2_000.0)]);
        assert_eq!(out[0], Shown { tag: Tag::Bare, focus: false });
        assert!(out[1].focus);
    }

    #[test]
    fn beyond_the_nearest_few_the_unlocked_are_bare() {
        let seen: Vec<Seen> = (0..8)
            .map(|i| Seen {
                at: [100.0 + i as f32 * 150.0, 100.0 + i as f32 * 60.0],
                dist: 2_000.0 + i as f32,
                hostile: true,
                ..Seen::default()
            })
            .collect();
        let out = shown(&seen);
        assert!(out[0].focus);
        assert_eq!(out.iter().filter(|s| s.tag == Tag::Range).count(), RANGED);
        assert!(out[1 + RANGED..].iter().all(|s| s.tag == Tag::Bare));
    }
}
