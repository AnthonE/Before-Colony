//! First-flight hints: one short line at a time, each gone once the pilot does what it says (or
//! after a while), and never shown again once seen (the seen set is kept with the settings).
//! Flying hints come while flying, and the hangar bay's (survival rules) while on foot in it.

/// A hint. Each one's bit in the seen set is its discriminant: new ones go at the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    Thrust,
    Boost,
    Fire,
    FlightAssist,
    Salvage,
    Menu,
    /// On foot: walking and looking; using what's in view; boarding at the hatch.
    Walk,
    Use,
    Launch,
    /// Survival rules, flying: docking to go home.
    Dock,
    /// Flying: the cockpit view (players from other games press V, which is flight assist here).
    Camera,
}

impl Hint {
    /// In the order they're shown.
    pub const ALL: [Hint; 11] = [
        Hint::Walk,
        Hint::Use,
        Hint::Launch,
        Hint::Thrust,
        Hint::Boost,
        Hint::Fire,
        Hint::Camera,
        Hint::FlightAssist,
        Hint::Salvage,
        Hint::Dock,
        Hint::Menu,
    ];

    pub fn bit(self) -> u32 {
        1 << self as u32
    }

    /// Shown on foot in the hangar bay (else while flying).
    fn on_foot(self) -> bool {
        matches!(self, Hint::Walk | Hint::Use | Hint::Launch)
    }

    pub fn text(self) -> &'static str {
        match self {
            Hint::Thrust => "W A S D and Space / C thrust. The mouse aims; Q / E roll.",
            Hint::Boost => "Shift boosts, X brakes, R turns fast on thrusters.",
            Hint::Fire => "Left and right mouse fire, F strikes in melee, H is the suit's special.",
            Hint::FlightAssist => {
                "V turns flight assist off: then nothing slows you down, like a real spacecraft."
            }
            Hint::Salvage => "G grabs wreckage and ore, B stows it. Bring it to the colony's dock.",
            Hint::Menu => "Esc opens the menu. F1 lists every control.",
            Hint::Walk => "W A S D walk, Shift runs, Space jumps. The mouse looks.",
            Hint::Use => {
                "Look at a terminal and press E: the fabricator and the stores on the right, the \
                 exchange by the doors, the suit's console at its feet."
            }
            Hint::Launch => {
                "Up the stairs to the catwalk: E at the cockpit hatch boards the suit and launches."
            }
            Hint::Dock => "To go home, come to rest inside the dock's ring of lights and press Enter.",
            Hint::Camera => "Tab (or the mouse wheel) switches between the cockpit and the chase camera.",
        }
    }

    /// Longest it stays up if the pilot doesn't do it, s.
    fn max_secs(self) -> f64 {
        match self {
            Hint::Thrust | Hint::Fire | Hint::Walk => 20.0,
            Hint::Use | Hint::Launch => 30.0,
            Hint::Dock | Hint::Camera => 15.0,
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
    /// Switched between the chase camera and the cockpit.
    pub switched_camera: bool,
    pub grabbing: bool,
    /// Survival rules (the hangar's hints, and docking's).
    pub survival: bool,
    /// On foot in the hangar bay (controls live), walking about, using something, boarding.
    pub walking: bool,
    pub strolling: bool,
    pub using: bool,
    pub boarding: bool,
    /// Asked to dock.
    pub docking: bool,
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
        if !i.flying && !i.walking {
            // Nothing new while dead or in a menu; the one up stays for later.
            return None;
        }
        if let Some((h, since)) = self.current {
            if h.on_foot() != i.walking {
                // Its place is elsewhere (flying, or on foot): it waits there.
                return None;
            }
            let acting = match h {
                Hint::Thrust => i.thrusting,
                Hint::Boost => i.boosting,
                Hint::Fire => i.firing,
                Hint::FlightAssist => i.toggled_assist,
                Hint::Salvage => i.grabbing,
                Hint::Menu => false,
                Hint::Walk => i.strolling,
                Hint::Use => i.using,
                Hint::Launch => i.boarding,
                Hint::Dock => i.docking,
                Hint::Camera => i.switched_camera,
            };
            self.doing = if acting { self.doing + dt } else { self.doing };
            let at_once = matches!(
                h,
                Hint::FlightAssist | Hint::Camera | Hint::Salvage | Hint::Use | Hint::Launch | Hint::Dock
            );
            let done = self.doing >= DOING || (acting && at_once);
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
        let h = Hint::ALL.into_iter().find(|h| {
            *seen & h.bit() == 0 && h.on_foot() == i.walking && (i.survival || !matches!(h, Hint::Dock))
        })?;
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
    fn switching_the_camera_clears_its_hint() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let flying = HintInput { flying: true, ..Default::default() };
        // Everything before it already seen.
        for x in [Hint::Thrust, Hint::Boost, Hint::Fire] {
            seen |= x.bit();
        }
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, flying), Some(Hint::Camera));
        run(&mut h, &mut seen, &mut t, 0.05, HintInput { switched_camera: true, ..flying });
        assert!(seen & Hint::Camera.bit() != 0);
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, flying), Some(Hint::FlightAssist));
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
    fn the_bays_hints_come_on_foot_and_the_flying_ones_wait() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let walking = HintInput { walking: true, survival: true, ..Default::default() };
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, walking), Some(Hint::Walk));
        run(&mut h, &mut seen, &mut t, 1.2, HintInput { strolling: true, ..walking });
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, walking), Some(Hint::Use));
        // Out flying (the bay's hint waits), then back on foot.
        let flying = HintInput { flying: true, survival: true, ..Default::default() };
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.5, flying), None);
        run(&mut h, &mut seen, &mut t, 0.2, HintInput { using: true, ..walking });
        assert!(seen & Hint::Use.bit() != 0);
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, walking), Some(Hint::Launch));
        run(&mut h, &mut seen, &mut t, 0.2, HintInput { boarding: true, ..walking });
        // Flying: the flight hints, and (survival) docking before the menu.
        let mut order = Vec::new();
        for _ in 0..4_000 {
            if let Some(x) = run(&mut h, &mut seen, &mut t, 0.05, flying)
                && order.last() != Some(&x)
            {
                order.push(x);
            }
        }
        let dock = order.iter().position(|x| *x == Hint::Dock).expect("docking's hint");
        let menu = order.iter().position(|x| *x == Hint::Menu).expect("the menu's hint");
        assert!(dock < menu, "{order:?}");
        // Under arcade rules there's no docking hint.
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let arcade = HintInput { flying: true, ..Default::default() };
        for _ in 0..4_000 {
            assert_ne!(run(&mut h, &mut seen, &mut t, 0.05, arcade), Some(Hint::Dock));
        }
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
