//! First-flight hints: one short line at a time, each gone once the pilot does what it says (or
//! after a while), and never shown again once seen (the seen set is kept with the settings).
//! Flying hints come while flying, and the hangar bay's (survival rules) while on foot in it, the
//! colony's city's while on foot there. The surface's come when they mean something: arming the
//! grip near a body, walking once on one, hiding once in a hide spot; and the way home when it's
//! near (the docking hub's face to land on, the inner gate inside the colony).

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
    /// Flying near something to land on: arming the grip.
    Grip,
    /// On a body: walking, hopping, crouching, letting go.
    Surface,
    /// In a hide spot: hiding.
    Hide,
    /// Flying: the map, and the objective's waypoint.
    Map,
    /// Flying with a hostile near: locking on.
    Lock,
    /// Locked on: the burst step.
    Step,
    /// On foot in the bay, the colony open: the airlock's cap lift down into the city.
    Airlock,
    /// On foot in the colony's city: its map, the trams, the cars, and the lift back up.
    City,
    /// Flying inside the colony: going home by the inner gate.
    InnerGate,
    /// Flying near the docking hub (survival): home by its dock, or landing on its face and
    /// walking in at the deck hatch.
    HubLanding,
}

impl Hint {
    /// In the order they're shown.
    pub const ALL: [Hint; 21] = [
        Hint::Walk,
        Hint::Use,
        Hint::Launch,
        Hint::Airlock,
        Hint::City,
        Hint::Thrust,
        Hint::Map,
        Hint::Boost,
        Hint::Fire,
        Hint::Lock,
        Hint::Step,
        Hint::Camera,
        Hint::FlightAssist,
        Hint::Salvage,
        Hint::Dock,
        Hint::InnerGate,
        Hint::Grip,
        Hint::HubLanding,
        Hint::Surface,
        Hint::Hide,
        Hint::Menu,
    ];

    pub fn bit(self) -> u32 {
        1 << self as u32
    }

    /// Shown on foot, in the hangar bay or the colony's city (else while flying).
    fn on_foot(self) -> bool {
        matches!(self, Hint::Walk | Hint::Use | Hint::Launch | Hint::Airlock | Hint::City)
    }

    /// Whether it's where it belongs: the bay's in the bay, the city's in the city, the inner gate's
    /// inside the colony and the dock's outside it.
    fn in_place(self, i: &HintInput) -> bool {
        match self {
            Hint::Use | Hint::Launch | Hint::Airlock => !i.in_city,
            Hint::City => i.in_city,
            Hint::InnerGate => i.inside,
            Hint::Dock | Hint::HubLanding => !i.inside,
            _ => true,
        }
    }

    /// Whether it means something now: the surface's hints wait for their moment, the bay's for
    /// the bay and the city's for the city.
    fn due(self, i: &HintInput) -> bool {
        match self {
            Hint::Use | Hint::Launch => !i.in_city,
            Hint::Airlock => !i.in_city && i.colony,
            Hint::City => i.in_city,
            Hint::Dock => i.survival && !i.inside,
            Hint::InnerGate => i.inside,
            Hint::HubLanding => i.survival && i.near_hub,
            Hint::Grip => i.near_surface && !i.gripping,
            Hint::Surface => i.grounded,
            Hint::Hide => i.grounded && i.in_hide_spot,
            Hint::Lock => i.hostile_near && !i.locked,
            Hint::Step => i.locked,
            _ => true,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Hint::Thrust => "W A S D and Space / C thrust. The mouse aims; Q / E roll.",
            Hint::Boost => "Shift boosts (it spends propellant: watch the gauge), X brakes, R turns fast.",
            Hint::Fire => {
                "Left and right mouse fire (hold a beam rifle's to charge it), F strikes in melee, H is the suit's special."
            }
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
                "Up the stairs to the catwalk: at the cockpit hatch E launches the suit out of your bay's door into space (Q, into the colony)."
            }
            Hint::Airlock => {
                "The airlock you came in by: E rides the cap lift down into the colony's city, on foot."
            }
            Hint::City => {
                "M is the city's map. Walk onto a platform to ride a tram, E at a motor pool takes a car; E at Hub Gate's lift goes back up to your bay."
            }
            Hint::InnerGate => {
                "To go home from in here, come to rest inside the inner gate's ring of lights and press Enter."
            }
            Hint::HubLanding => {
                "Home is the docking hub: rest in its dock's ring of lights and press Enter, or arm the grip (L), land on its face near the middle and walk onto the deck hatch."
            }
            Hint::Dock => "To go home, come to rest inside the dock's ring of lights and press Enter.",
            Hint::Camera => "Tab (or the mouse wheel) switches between the cockpit and the chase camera.",
            Hint::Grip => "L arms your grip: come in slow and close, and it lands you",
            Hint::Surface => {
                "W A S D walk - Shift runs - Space hops, hold it to lift off - C crouches - L lets go"
            }
            Hint::Hide => {
                "Crouch still in a hide spot and sensors lose you. Log off here and your suit stays hidden"
            }
            Hint::Map => "M opens the chart. Pick where to go, then N: the auto-nav flies you there.",
            Hint::Lock => {
                "Y (or the middle button) locks on: W closes in, A / D circle it, ◆ shows where to lead. Hold Y to let go."
            }
            Hint::Step => "Double-tap a direction (W A S D, Space, C) to burst-step that way: a quick dodge.",
        }
    }

    /// Longest it stays up if the pilot doesn't do it, s.
    fn max_secs(self) -> f64 {
        match self {
            Hint::Thrust | Hint::Fire | Hint::Walk => 20.0,
            Hint::Use | Hint::Launch | Hint::Airlock | Hint::City | Hint::HubLanding => 30.0,
            Hint::InnerGate => 15.0,
            Hint::Dock | Hint::Camera | Hint::Grip | Hint::Hide | Hint::Map | Hint::Lock | Hint::Step => 15.0,
            Hint::Surface => 20.0,
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
    /// Flying within a kilometre of something it could land on.
    pub near_surface: bool,
    /// On a body, in its grip (caught, standing, or aloft).
    pub gripping: bool,
    /// Standing on a body.
    pub grounded: bool,
    /// Walking on it (the stick held), and hopping this frame.
    pub walked: bool,
    pub hopped: bool,
    /// In one of a landmark's hide spots, and hidden (sensors have lost it).
    pub in_hide_spot: bool,
    pub hidden: bool,
    /// Opened the map.
    pub opened_map: bool,
    /// A hostile within a couple of kilometres, and the pilot locked on to one.
    pub hostile_near: bool,
    pub locked: bool,
    /// Asked for a burst step.
    pub stepped: bool,
    /// The colony is open (its cap lifts run), and on foot it's the city's streets rather than the
    /// bay underfoot; or flying inside the colony.
    pub colony: bool,
    pub in_city: bool,
    pub inside: bool,
    /// Rode the cap lift down (or used the airlock).
    pub went_down: bool,
    /// Flying within a couple of kilometres of the docking hub's mouth, outside the colony.
    pub near_hub: bool,
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
    /// The pilot hopped while the surface's hint was up.
    hopped: bool,
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
            if h.on_foot() != i.walking || !h.in_place(i) {
                // Its place is elsewhere (flying, or on foot; the bay, the city, inside the
                // colony): it waits there.
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
                Hint::Grip => i.gripping,
                Hint::Surface => i.walked,
                Hint::Hide => i.hidden,
                Hint::Map => i.opened_map,
                Hint::Lock => i.locked,
                Hint::Step => i.stepped,
                Hint::Airlock => i.went_down,
                Hint::City => i.opened_map,
                Hint::InnerGate => i.docking,
                Hint::HubLanding => i.docking || i.gripping,
            };
            self.doing = if acting { self.doing + dt } else { self.doing };
            self.hopped |= i.hopped;
            let at_once = matches!(
                h,
                Hint::FlightAssist
                    | Hint::Camera
                    | Hint::Salvage
                    | Hint::Use
                    | Hint::Launch
                    | Hint::Dock
                    | Hint::Grip
                    | Hint::Hide
                    | Hint::Map
                    | Hint::Lock
                    | Hint::Step
                    | Hint::Airlock
                    | Hint::City
                    | Hint::InnerGate
                    | Hint::HubLanding
            );
            // The surface's: a second of walking, and a hop.
            let done = if h == Hint::Surface {
                self.doing >= DOING && self.hopped
            } else {
                self.doing >= DOING || (acting && at_once)
            };
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
        let h =
            Hint::ALL.into_iter().find(|h| *seen & h.bit() == 0 && h.on_foot() == i.walking && h.due(i))?;
        self.current = Some((h, now));
        self.doing = 0.0;
        self.hopped = false;
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
        // Then the map: opening it clears it at once.
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, flying), Some(Hint::Map));
        run(&mut h, &mut seen, &mut t, 0.05, HintInput { opened_map: true, ..flying });
        assert!(seen & Hint::Map.bit() != 0);
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, flying), Some(Hint::Boost));
    }

    #[test]
    fn switching_the_camera_clears_its_hint() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let flying = HintInput { flying: true, ..Default::default() };
        // Everything before it already seen.
        for x in [Hint::Thrust, Hint::Map, Hint::Boost, Hint::Fire] {
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
    fn the_way_into_the_colony_and_home_is_hinted_where_it_means_something() {
        let (mut h, mut seen, mut t) = (Hints::default(), 0u32, 0.0);
        let bay = HintInput { walking: true, survival: true, colony: true, ..Default::default() };
        for x in [Hint::Walk, Hint::Use, Hint::Launch] {
            seen |= x.bit();
        }
        // In the bay with the colony open: the airlock, gone once the pilot rides down.
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, bay), Some(Hint::Airlock));
        run(&mut h, &mut seen, &mut t, 0.05, HintInput { went_down: true, ..bay });
        assert!(seen & Hint::Airlock.bit() != 0);
        // Not the city's in the bay; in the city, its map.
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, bay), None);
        let city = HintInput { in_city: true, ..bay };
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, city), Some(Hint::City));
        run(&mut h, &mut seen, &mut t, 0.05, HintInput { opened_map: true, ..city });
        assert!(seen & Hint::City.bit() != 0);
        // With the colony closed there's no airlock hint.
        let (mut h2, mut seen2, mut t2) =
            (Hints::default(), Hint::Walk.bit() | Hint::Use.bit() | Hint::Launch.bit(), 0.0);
        assert_eq!(run(&mut h2, &mut seen2, &mut t2, 0.1, HintInput { colony: false, ..bay }), None);
        // Flying near the hub, the way home by landing; inside the colony, the inner gate.
        let mut flying_seen =
            (0..21).fold(0u32, |a, k| a | 1 << k) & !(Hint::HubLanding.bit() | Hint::InnerGate.bit());
        let (mut h3, mut t3) = (Hints::default(), 0.0);
        let near = HintInput { flying: true, survival: true, near_hub: true, ..Default::default() };
        assert_eq!(run(&mut h3, &mut flying_seen, &mut t3, 0.1, near), Some(Hint::HubLanding));
        run(&mut h3, &mut flying_seen, &mut t3, 0.05, HintInput { gripping: true, ..near });
        let inside = HintInput { flying: true, survival: true, inside: true, ..Default::default() };
        assert_eq!(run(&mut h3, &mut flying_seen, &mut t3, GAP + 0.2, inside), Some(Hint::InnerGate));
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

    #[test]
    fn the_surfaces_hints_take_the_next_bits() {
        assert_eq!((Hint::Grip.bit(), Hint::Surface.bit(), Hint::Hide.bit()), (1 << 11, 1 << 12, 1 << 13));
        let mut bits: Vec<u32> = Hint::ALL.iter().map(|h| h.bit()).collect();
        bits.sort_unstable();
        bits.dedup();
        assert_eq!(bits.len(), Hint::ALL.len(), "every hint listed once");
    }

    #[test]
    fn the_surfaces_hints_come_when_they_mean_something() {
        let (mut h, mut t) = (Hints::default(), 0.0);
        // Everything else seen.
        let mut seen = Hint::ALL
            .iter()
            .filter(|h| !matches!(h, Hint::Grip | Hint::Surface | Hint::Hide))
            .fold(0, |a, h| a | h.bit());
        let flying = HintInput { flying: true, ..Default::default() };
        assert_eq!(run(&mut h, &mut seen, &mut t, 5.0, flying), None, "nothing to land on");
        // Near a body: arm the grip; caught, it's done.
        let near = HintInput { near_surface: true, ..flying };
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, near), Some(Hint::Grip));
        run(&mut h, &mut seen, &mut t, 0.1, HintInput { gripping: true, ..near });
        assert!(seen & Hint::Grip.bit() != 0);
        // Standing on it: walking alone doesn't clear the surface's hint; a hop as well does.
        let grounded = HintInput { gripping: true, grounded: true, ..flying };
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, grounded), Some(Hint::Surface));
        assert_eq!(
            run(&mut h, &mut seen, &mut t, 2.0, HintInput { walked: true, ..grounded }),
            Some(Hint::Surface)
        );
        run(&mut h, &mut seen, &mut t, 0.05, HintInput { hopped: true, ..grounded });
        assert!(seen & Hint::Surface.bit() != 0);
        // Not in a hide spot: nothing; in one, hiding is explained until the suit is hidden.
        assert_eq!(run(&mut h, &mut seen, &mut t, GAP + 0.2, grounded), None);
        let spot = HintInput { in_hide_spot: true, ..grounded };
        assert_eq!(run(&mut h, &mut seen, &mut t, 0.1, spot), Some(Hint::Hide));
        run(&mut h, &mut seen, &mut t, 0.05, HintInput { hidden: true, ..spot });
        assert!(seen & Hint::Hide.bit() != 0);
    }
}
