//! The burst step's double tap (`bc_sim::flight::Burst`): a direction key pressed twice within
//! [`WINDOW`] steps that way. The second press holds BURST while the key is down, and for at least
//! [`LATCH`] (so a quick tap still reaches a tick), with the stick held that way meanwhile.

/// Two presses of one key this close together, s, are a double tap.
pub const WINDOW: f64 = 0.25;
/// A step's press is held at least this long, s: a tick or two.
pub const LATCH: f64 = 0.08;

/// The six directions, as keys: right, left, up, down, forward, back (D, A, Space, C, W, S).
pub const DIRECTIONS: usize = 6;

/// Watches the direction keys for a double tap.
#[derive(Clone, Copy, Debug, Default)]
pub struct DoubleTap {
    /// The last press: its direction, and when.
    last: Option<(usize, f64)>,
    /// The step asked for: its direction, and how long its press holds at least.
    step: Option<(usize, f64)>,
}

impl DoubleTap {
    /// A press of direction `dir` (an index into [`DIRECTIONS`]) at `now`, s.
    pub fn press(&mut self, dir: usize, now: f64) {
        match self.last {
            Some((d, t)) if d == dir && now - t <= WINDOW => {
                self.step = Some((dir, now + LATCH));
                self.last = None;
            }
            _ => self.last = Some((dir, now)),
        }
    }

    /// The step being asked for at `now`, with `held` the direction keys down: its axis (0 right,
    /// 1 up, 2 forward) and which way along it, while its key is down or the latch lasts.
    pub fn stepping(&mut self, held: [bool; DIRECTIONS], now: f64) -> Option<(usize, f32)> {
        let (dir, until) = self.step?;
        if held[dir] || now < until {
            Some((dir / 2, if dir % 2 == 0 { 1.0 } else { -1.0 }))
        } else {
            self.step = None;
            None
        }
    }

    /// Forgets what it saw (on foot, in a menu, the setting off).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: [bool; DIRECTIONS] = [false; DIRECTIONS];

    fn down(dir: usize) -> [bool; DIRECTIONS] {
        let mut held = UP;
        held[dir] = true;
        held
    }

    #[test]
    fn two_quick_presses_step_that_way_while_held() {
        let mut tap = DoubleTap::default();
        tap.press(1, 10.0);
        assert_eq!(tap.stepping(UP, 10.05), None, "one press is just thrust");
        tap.press(1, 10.2);
        assert_eq!(tap.stepping(down(1), 10.2), Some((0, -1.0)), "A twice: a step left");
        assert_eq!(tap.stepping(down(1), 10.6), Some((0, -1.0)), "held, still pressed");
        assert_eq!(tap.stepping(UP, 10.61), None, "let go");
        // A third press soon after is the first of a new pair.
        tap.press(1, 10.7);
        assert_eq!(tap.stepping(down(1), 10.7), None);
    }

    #[test]
    fn a_quick_second_tap_still_reaches_a_tick() {
        let mut tap = DoubleTap::default();
        tap.press(4, 3.0);
        tap.press(4, 3.1);
        assert_eq!(tap.stepping(UP, 3.15), Some((2, 1.0)), "W twice, already let go: latched");
        assert_eq!(tap.stepping(UP, 3.2), None);
    }

    #[test]
    fn slow_or_mixed_presses_dont_step() {
        let mut tap = DoubleTap::default();
        tap.press(2, 0.0);
        tap.press(2, 0.3);
        assert_eq!(tap.stepping(down(2), 0.3), None, "too slow");
        tap.press(0, 0.4);
        tap.press(5, 0.5);
        assert_eq!(tap.stepping(down(5), 0.5), None, "two different keys");
        tap.press(3, 1.0);
        tap.clear();
        tap.press(3, 1.1);
        assert_eq!(tap.stepping(down(3), 1.1), None, "forgotten");
    }
}
