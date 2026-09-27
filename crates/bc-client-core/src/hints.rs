//! First-flight hints: one short line at a time, each gone once the pilot does what it says (or
//! after a while), and never shown again once seen (the seen set is kept with the settings).

/// A hint, in the order they're shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    Thrust,
    Boost,
    Fire,
    FlightAssist,
    Salvage,
    Menu,
}

impl Hint {
    pub const ALL: [Hint; 6] =
        [Hint::Thrust, Hint::Boost, Hint::Fire, Hint::FlightAssist, Hint::Salvage, Hint::Menu];

    pub fn bit(self) -> u32 {
        1 << self as u32
    }

    pub fn text(self) -> &'static str {
        match self {
            Hint::Thrust => "W A S D and Space / C thrust. The mouse aims; Q / E roll.",
            Hint::Boost => "Shift boosts, X brakes, R turns fast on thrusters.",
            Hint::Fire => "Left and right mouse fire, F strikes in melee, H is the suit's special.",
            Hint::FlightAssist => {
                "V turns flight assist off: then nothing slows you down, like a real spacecraft."
            }
            Hint::Salvage => "G grabs wreckage and ore, B stows it. Sell it at the colony's dock.",
            Hint::Menu => "Esc opens the menu. F1 lists every control.",
        }
    }

    /// Longest it stays up if the pilot doesn't do it, s.
    fn max_secs(self) -> f64 {
        match self {
            Hint::Thrust | Hint::Fire => 20.0,
            _ => 9.0,
        }
    }
}

/// What the pilot is doing this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct HintInput {
    /// Flying (alive, in the world, controls live).
    pub flying: bool,
    pub thrusting: bool,
    pub boosting: bool,
    pub firing: bool,
    pub toggled_assist: bool,
    pub grabbing: bool,
}

/// Seconds between one hint and the next.
const GAP: f64 = 2.0;
/// How long an action must go on to count (a tap on W isn't having learnt to fly).
const DOING: f64 = 1.0;

/// The hint on screen, and when the next may come.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hints {
    current: Option<(Hint, f64)>,
    /// How long the current hint's action has gone on.
    doing: f64,
    next_at: f64,
}

impl Hints {
    /// Steps once a frame; `seen` is the settings' seen set, and is updated. Returns the hint to
    /// show.
    pub fn step(&mut self, seen: &mut u32, now: f64, dt: f64, i: &HintInput) -> Option<Hint> {
        if !i.flying {
            // Nothing new while dead or in a menu; the one up stays for later.
            return None;
        }
        if let Some((h, since)) = self.current {
            let acting = match h {
                Hint::Thrust => i.thrusting,
                Hint::Boost => i.boosting,
                Hint::Fire => i.firing,
                Hint::FlightAssist => i.toggled_assist,
                Hint::Salvage => i.grabbing,
                Hint::Menu => false,
            };
            self.doing = if acting { self.doing + dt } else { self.doing };
            let done = self.doing >= DOING || (acting && matches!(h, Hint::FlightAssist | Hint::Salvage));
            if done || now - since > h.max_secs() {
                *seen |= h.bit();
                self.current = None;
                self.next_at = now + GAP;
            } else {
                return Some(h);
            }
        }
        if now < self.next_at {
            return None;
        }
        let h = Hint::ALL.into_iter().find(|h| *seen & h.bit() == 0)?;
        self.current = Some((h, now));
        self.doing = 0.0;
        Some(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(h: &mut Hints, seen: &mut u32, t: &mut f64, secs: f64, i: HintInput) -> Option<Hint> {
        let mut last = None;
        let end = *t + secs;
        while *t < end {
            *t += 0.05;
            last = h.step(seen, *t, 0.05, &i);
        }
        last
    }

    #[test]
    fn hints_come_in_order_and_go_when_done() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let flying = HintInput { flying: true, ..Default::default() };
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, flying), Some(Hint::Thrust));
        // Thrusting for a second clears it.
        run(&mut h, &mut seen, &mut t, 1.2, HintInput { thrusting: true, ..flying });
        assert!(seen & Hint::Thrust.bit() != 0);
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, flying), Some(Hint::Boost));
    }

    #[test]
    fn a_hint_ignored_goes_away_and_is_not_repeated() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let flying = HintInput { flying: true, ..Default::default() };
        run(&mut h, &mut seen, &mut t, 25.0, flying);
        assert!(seen & Hint::Thrust.bit() != 0);
        // A new session with the same seen set starts further on.
        let mut h2 = Hints::default();
        let mut t2 = 0.0;
        let first = run(&mut h2, &mut seen, &mut t2, 0.1, flying);
        assert_ne!(first, Some(Hint::Thrust));
    }

    #[test]
    fn nothing_while_not_flying_and_nothing_when_all_seen() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        assert_eq!(run(&mut h, &mut seen, &mut t, 5.0, HintInput::default()), None);
        let mut all = Hint::ALL.iter().fold(0, |a, h| a | h.bit());
        let flying = HintInput { flying: true, ..Default::default() };
        assert_eq!(run(&mut h, &mut all, &mut t, 5.0, flying), None);
    }
}
