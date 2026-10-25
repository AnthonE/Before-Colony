//! The instruments' readings (`docs/UI.md`, rule 9): what the cockpit's monitors and, chasing, the
//! panels in the screen's corners show, as numbers and gauges, apart from how they're drawn
//! (`bc-client`'s `hud::update_instruments`). The game reads them off the suit (`hud::update_hud`),
//! the showcase makes them up.
//!
//! A panel is a heading (with a word on its right: the frame's name, whether the weapons are safe),
//! then its dials (the flight's speed and G, after a car's), a table of rows (a key, a label, a bar,
//! a number, a state: MechWarrior's weapon table, the armour of each part) and notes under it. The
//! gauges are lit segments: a bar's along it, a dial's round its ring.

/// What a colour means (`docs/UI.md`, rule 2): cyan what is, amber what to do, red what's wrong,
/// green what was gained; the label blue and white for the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Plain,
    Label,
    /// What's there but doesn't matter now (a weapon cycling, a full tank).
    Dim,
    Cyan,
    Amber,
    Red,
    Green,
}

/// A gauge: a bar of segments, or a dial's ring of them, lit to `full`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gauge {
    /// How full, 0..1.
    pub full: f32,
    pub segments: u8,
    pub tone: Tone,
    /// A mark across it (0..1): the speed dial's cruise, the G dial's tolerance.
    pub mark: Option<f32>,
    /// From here on (0..1) its track is red: past what the pilot bears.
    pub red_from: Option<f32>,
}

impl Gauge {
    /// A gauge of `segments`, `full`, in `tone`.
    pub fn new(full: f32, segments: u8, tone: Tone) -> Self {
        Self { full: clamp01(full), segments, tone, mark: None, red_from: None }
    }

    /// A store, better full (armour, propellant, energy): amber under half, red under a quarter.
    pub fn store(full: f32, segments: u8) -> Self {
        let tone = if full < 0.25 {
            Tone::Red
        } else if full < 0.5 {
            Tone::Amber
        } else {
            Tone::Cyan
        };
        Self::new(full, segments, tone)
    }

    /// A load, worse full (heat, strain, the blows' impact): amber over 60%, red over 85%.
    pub fn load(full: f32, segments: u8) -> Self {
        let tone = if full > 0.85 {
            Tone::Red
        } else if full > 0.6 {
            Tone::Amber
        } else {
            Tone::Cyan
        };
        Self::new(full, segments, tone)
    }

    /// The speed dial: round to boost's cruise (`top`), marked at flight assist's (`cruise`).
    /// Faster (flight assist off, a fall) it stays full; the number says how fast.
    pub fn speed(speed: f32, cruise: f32, top: f32) -> Self {
        let top = top.max(1.0);
        Self { mark: Some(clamp01(cruise / top)), ..Self::new(speed / top, DIAL_SEGMENTS, Tone::Cyan) }
    }

    /// The G dial: round to half again what the pilot bears (`tolerance`), marked there and red past
    /// it; amber while flight assist holds them under it (`limited`), red over it.
    pub fn g(g: f32, tolerance: f32, limited: bool) -> Self {
        let top = (tolerance * G_HEADROOM).max(1.0);
        let tone = if g > tolerance {
            Tone::Red
        } else if limited {
            Tone::Amber
        } else {
            Tone::Cyan
        };
        let at = clamp01(tolerance / top);
        Self { mark: Some(at), red_from: Some(at), ..Self::new(g / top, DIAL_SEGMENTS, tone) }
    }

    /// How many of its segments are lit (rounded, so a full tank's last drop doesn't darken one).
    pub fn lit(&self) -> u8 {
        (self.full * f32::from(self.segments)).round().clamp(0.0, f32::from(self.segments)) as u8
    }
}

/// The dials' segments, round their ring.
pub const DIAL_SEGMENTS: u8 = 24;
/// The G dial reads to this many times the pilot's tolerance.
pub const G_HEADROOM: f32 = 1.5;

/// A dial: its ring, the number in it and the unit under that.
#[derive(Clone, Debug, PartialEq)]
pub struct Dial {
    pub gauge: Gauge,
    pub number: String,
    pub unit: String,
    pub unit_tone: Tone,
}

/// A row of a panel's table: what to press, what it is, its bar, its number, its state. Any can be
/// empty (a part's armour has no key, a beam saber no rounds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub key: &'static str,
    pub label: String,
    pub label_tone: Tone,
    pub gauge: Option<Gauge>,
    pub value: String,
    pub value_tone: Tone,
    pub state: String,
    pub state_tone: Tone,
}

impl Row {
    /// A gauge's row: `label`, the bar, the number.
    pub fn meter(label: impl Into<String>, gauge: Gauge, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            label_tone: Tone::Label,
            value_tone: gauge.tone.plain_unless_warning(),
            gauge: Some(gauge),
            value: value.into(),
            ..Self::default()
        }
    }

    /// With a state after the number.
    pub fn with_state(mut self, state: impl Into<String>, tone: Tone) -> Self {
        self.state = state.into();
        self.state_tone = tone;
        self
    }
}

impl Tone {
    /// A gauge's number in plain white unless the gauge warns.
    fn plain_unless_warning(self) -> Tone {
        match self {
            Tone::Amber | Tone::Red => self,
            _ => Tone::Plain,
        }
    }
}

/// The most rows a panel's table has (the arms': three mounts, the special, the lock, the Orrery).
pub const ROWS: usize = 8;
/// The most dials a panel has.
pub const DIALS: usize = 2;

/// One panel's reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    /// The word on the right of its heading.
    pub title: String,
    pub title_tone: Tone,
    pub dials: Vec<Dial>,
    pub rows: Vec<Row>,
    pub notes: String,
    pub notes_tone: Tone,
}

/// Every panel's reading (the Orrery's is text alone, and stays the HUD's).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readings {
    pub flight: Reading,
    pub suit: Reading,
    pub arms: Reading,
    pub hold: Reading,
    /// Each part's armour, for the suit's silhouette (`bc_proto::Part`'s order).
    pub parts: [f32; 6],
}

/// The scales a panel's content is drawn at on its monitor, and the steps between them.
pub const MIN_SCALE: f32 = 1.0;
pub const MAX_SCALE: f32 = 3.0;
const STEP: f32 = 0.1;
/// A panel grows only once it has this much more room, so a line coming and going doesn't make it
/// jitter.
const GROW: f32 = 0.15;

/// The scale a panel's content is drawn at on its monitor: as large as it fits its `room` (px), in
/// tenths. Drawn at `scale`, it measured `size`; it shrinks as soon as it overflows, and grows only
/// once there's room for it to grow by more than a step.
pub fn fit(scale: f32, size: [f32; 2], room: [f32; 2]) -> f32 {
    if !(size[0] > 0.0 && size[1] > 0.0) {
        return scale;
    }
    let most = (room[0] / size[0]).min(room[1] / size[1]) * scale;
    let step = ((most / STEP + 1e-3).floor() * STEP).clamp(MIN_SCALE, MAX_SCALE);
    if step < scale - 1e-4 || step >= scale + GROW { step } else { scale }
}

fn clamp01(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// The suit panel's note for how fed the pilot is (the own state's `fed`): what it does to the G
/// they bear, under the real flight rules as the stim's line says it (`WELL FED +0.5G`). Fed and
/// peckish cost nothing, and say nothing; nor does a condition the wire doesn't know.
pub fn fed_note(fed: u8) -> Option<String> {
    use bc_sim::content::body::{Fed, fed_g};
    let f = Fed::from_index(fed)?;
    let g = fed_g(f);
    (g != 0.0).then(|| format!("{} {g:+}G", f.tag()))
}

/// Whether the pilot's condition (the own state's `fed`) costs them G: the suit panel's notes warn.
pub fn fed_warns(fed: u8) -> bool {
    use bc_sim::content::body::{Fed, fed_g};
    Fed::from_index(fed).is_some_and(|f| fed_g(f) < 0.0)
}

/// A holdup's words split at their middle dot: the cause (`OVERHEATED`), for the weapon's row, and
/// what to do about it (`let it cool`), for the notes.
pub fn cause_and_cure(words: &str) -> (&str, Option<&str>) {
    match words.split_once(" · ") {
        Some((cause, cure)) => (cause, Some(cure)),
        None => (words, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_condition_says_what_it_does_to_the_g() {
        use bc_sim::content::body::Fed;
        assert_eq!(fed_note(Fed::WellFed as u8).as_deref(), Some("WELL FED +0.5G"));
        assert_eq!(fed_note(Fed::Fed as u8), None);
        assert_eq!(fed_note(Fed::Peckish as u8), None);
        assert_eq!(fed_note(Fed::Hungry as u8).as_deref(), Some("HUNGRY -0.5G"));
        assert_eq!(fed_note(Fed::Starving as u8).as_deref(), Some("STARVING -1G"));
        assert_eq!(fed_note(5), None, "not a condition");
        assert!(fed_warns(Fed::Hungry as u8) && fed_warns(Fed::Starving as u8));
        assert!(!fed_warns(Fed::WellFed as u8) && !fed_warns(Fed::Peckish as u8) && !fed_warns(9));
    }

    #[test]
    fn stores_warn_empty_and_loads_warn_full() {
        assert_eq!(Gauge::store(1.0, 10).tone, Tone::Cyan);
        assert_eq!(Gauge::store(0.4, 10).tone, Tone::Amber);
        assert_eq!(Gauge::store(0.2, 10).tone, Tone::Red);
        assert_eq!(Gauge::load(0.1, 10).tone, Tone::Cyan);
        assert_eq!(Gauge::load(0.7, 10).tone, Tone::Amber);
        assert_eq!(Gauge::load(0.9, 10).tone, Tone::Red);
        // Its number is white unless it warns.
        assert_eq!(Row::meter("HEAT", Gauge::load(0.1, 10), "10%").value_tone, Tone::Plain);
        assert_eq!(Row::meter("HEAT", Gauge::load(0.9, 10), "90%").value_tone, Tone::Red);
    }

    #[test]
    fn segments_light_rounded_and_never_past_the_end() {
        assert_eq!(Gauge::store(1.0, 10).lit(), 10);
        assert_eq!(Gauge::store(0.96, 10).lit(), 10);
        assert_eq!(Gauge::store(0.04, 10).lit(), 0);
        assert_eq!(Gauge::store(0.55, 10).lit(), 6);
        assert_eq!(Gauge::new(7.0, 8, Tone::Cyan).lit(), 8);
        assert_eq!(Gauge::new(f32::NAN, 8, Tone::Cyan).lit(), 0);
    }

    #[test]
    fn the_dials_read_against_the_suit_and_the_pilot() {
        // Cruise is marked; boost's cruise fills it; faster stays full.
        let s = Gauge::speed(60.0, 60.0, 108.0);
        assert!((s.mark.unwrap() - 60.0 / 108.0).abs() < 1e-6);
        assert!((s.full - 60.0 / 108.0).abs() < 1e-6);
        assert_eq!(Gauge::speed(400.0, 60.0, 108.0).full, 1.0);
        // The tolerance is marked two thirds round, red past it; flight assist's hold is amber.
        let g = Gauge::g(3.0, 6.0, false);
        assert!((g.mark.unwrap() - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(g.red_from, g.mark);
        assert_eq!(g.tone, Tone::Cyan);
        assert_eq!(Gauge::g(5.97, 6.0, true).tone, Tone::Amber);
        assert_eq!(Gauge::g(7.0, 6.0, false).tone, Tone::Red);
    }

    #[test]
    fn a_panel_fits_its_monitor_without_jitter() {
        // Half the room it has, at 1: twice as large.
        assert!((fit(1.0, [100.0, 50.0], [200.0, 400.0]) - 2.0).abs() < 1e-4);
        // A hair too large: a step smaller, and there it stays.
        let s = fit(2.0, [201.0, 100.0], [200.0, 400.0]);
        assert!((s - 1.9).abs() < 1e-4, "{s}");
        assert_eq!(fit(s, [201.0 * s / 2.0, 100.0 * s / 2.0], [200.0, 400.0]), s);
        // A line gone gives it a little room: not enough to grow by.
        assert_eq!(fit(2.0, [190.0, 100.0], [200.0, 400.0]), 2.0);
        // Much more room: it grows, as far as the largest scale.
        assert!((fit(2.0, [100.0, 50.0], [200.0, 400.0]) - 3.0).abs() < 1e-4);
        // Nothing measured yet: as it is.
        assert_eq!(fit(1.7, [0.0, 0.0], [200.0, 400.0]), 1.7);
    }

    #[test]
    fn holdups_split_into_cause_and_cure() {
        assert_eq!(cause_and_cure("OVERHEATED · let it cool"), ("OVERHEATED", Some("let it cool")));
        assert_eq!(cause_and_cure("ARM LOST"), ("ARM LOST", None));
    }
}
