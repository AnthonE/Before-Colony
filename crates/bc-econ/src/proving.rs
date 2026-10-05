//! The Proving Ground's board (`docs/TRAINING.md`): the day's best times round the course and
//! through the Blast Hall's drill, and the best ever, as the server's interior sector timed them.
//! X-Wing's high-score table, on the hall's back wall and at its desk.
//!
//! The server keeps one [`Board`] (behind a lock, off the tick, and in its data directory) and each
//! signed-in pilot's own [`Bests`] on their record. Pilots see a [`BoardView`]: names and times,
//! their own marked, and nobody's key. A pilot is on each of the day's lists once, with their best;
//! the lists turn over at midnight UTC.

use bc_sim::colony::course::{Class, PAR_S};
use bc_sim::colony::hall::DRILL_PAR_S;
use serde::{Deserialize, Serialize};

/// How many of the day's best each list keeps.
pub const KEPT: usize = 10;
/// A day on the board, s.
const DAY: u64 = 86_400;

/// What was flown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feat {
    /// The course of rings from the inner gate down to the pad on Hub Gate's square.
    Course,
    /// The Blast Hall's drill: its targets lit one at a time against the clock.
    Drill,
}

impl Feat {
    pub fn name(self) -> &'static str {
        match self {
            Feat::Course => "THE COURSE",
            Feat::Drill => "THE DRILL",
        }
    }

    /// Its par, s.
    pub fn par_s(self) -> f64 {
        match self {
            Feat::Course => PAR_S,
            Feat::Drill => DRILL_PAR_S,
        }
    }

    /// The Charter Board's certificate for a time, ms.
    pub fn class(self, ms: u32) -> Class {
        Class::against(f64::from(ms) / 1_000.0, self.par_s())
    }
}

/// A time as the board shows it: `1:42.3` (minutes, seconds and tenths).
pub fn clock(ms: u32) -> String {
    let tenths = ms / 100;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

/// A place on the board: `1ST`, `2ND`, `3RD`, `4TH`…
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "TH",
        (1, _) => "ST",
        (2, _) => "ND",
        (3, _) => "RD",
        _ => "TH",
    };
    format!("{n}{suffix}")
}

/// A time on the board: who flew it (their key: a wallet's, or a guest's for the visit; never
/// shown), what they go by, the time, and when (Unix seconds).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub who: String,
    pub name: String,
    pub ms: u32,
    pub unix: u64,
}

/// The day's best and the best ever, for the course and the drill.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// The day (since the Unix epoch, UTC) the day's lists are for.
    day: u64,
    /// The day's best, each pilot once, fastest first (a tie: who flew it first).
    course: Vec<Entry>,
    drill: Vec<Entry>,
    /// The best ever.
    #[serde(default)]
    course_record: Option<Entry>,
    #[serde(default)]
    drill_record: Option<Entry>,
}

/// Where a time went on the board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Placed {
    /// Its place on the day's list (1 and on), if it's on it.
    pub rank: Option<usize>,
    /// It's the pilot's best of the day (their first, or better than the one they had).
    pub improved: bool,
    /// It's the best ever.
    pub record: bool,
}

impl Board {
    fn list(&self, feat: Feat) -> &[Entry] {
        match feat {
            Feat::Course => &self.course,
            Feat::Drill => &self.drill,
        }
    }

    fn list_mut(&mut self, feat: Feat) -> &mut Vec<Entry> {
        match feat {
            Feat::Course => &mut self.course,
            Feat::Drill => &mut self.drill,
        }
    }

    fn best_ever(&mut self, feat: Feat) -> &mut Option<Entry> {
        match feat {
            Feat::Course => &mut self.course_record,
            Feat::Drill => &mut self.drill_record,
        }
    }

    /// Turns the day over at `now` (Unix seconds): a new day's lists start empty. Whether it did.
    pub fn turn(&mut self, now: u64) -> bool {
        let day = now / DAY;
        if day == self.day {
            return false;
        }
        self.day = day;
        self.course.clear();
        self.drill.clear();
        true
    }

    /// A time flown at `now` by `who`, who goes by `name`: on the day's list if it's their best of
    /// the day and among the [`KEPT`] best, and the best ever if it beats it.
    pub fn record(&mut self, feat: Feat, who: &str, name: &str, ms: u32, now: u64) -> Placed {
        self.turn(now);
        let entry = Entry { who: who.into(), name: name.into(), ms, unix: now };
        let list = self.list_mut(feat);
        let theirs = list.iter().position(|e| e.who == who);
        let improved = theirs.is_none_or(|k| ms < list[k].ms);
        if improved {
            if let Some(k) = theirs {
                list.remove(k);
            }
            let at = list.partition_point(|e| e.ms <= ms);
            list.insert(at, entry.clone());
            list.truncate(KEPT);
        }
        let rank = list.iter().position(|e| e.who == who).map(|k| k + 1);
        let ever = self.best_ever(feat);
        let record = ever.as_ref().is_none_or(|e| ms < e.ms);
        if record {
            *ever = Some(entry);
        }
        Placed { rank, improved, record }
    }

    /// The board as `who` sees it at `now`: the day's lists (empty, if the day has turned since
    /// they were kept) and the records, their own rows marked.
    pub fn view(&self, who: &str, now: u64) -> BoardView {
        let today = now / DAY == self.day;
        let row = |e: &Entry| Row { name: e.name.clone(), ms: e.ms, you: e.who == who };
        let rows = |feat: Feat| if today { self.list(feat).iter().map(row).collect() } else { Vec::new() };
        BoardView {
            course: rows(Feat::Course),
            drill: rows(Feat::Drill),
            course_record: self.course_record.as_ref().map(row),
            drill_record: self.drill_record.as_ref().map(row),
            course_par_ms: (PAR_S * 1_000.0) as u32,
            drill_par_ms: (DRILL_PAR_S * 1_000.0) as u32,
            mine: Bests::default(),
        }
    }
}

/// The board, as a pilot sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardView {
    /// The day's best round the course and through the drill, fastest first.
    pub course: Vec<Row>,
    pub drill: Vec<Row>,
    /// The best ever.
    pub course_record: Option<Row>,
    pub drill_record: Option<Row>,
    /// The pars, ms.
    pub course_par_ms: u32,
    pub drill_par_ms: u32,
    /// The pilot's own best (a signed-in pilot's, ever; a guest's, this visit).
    #[serde(default)]
    pub mine: Bests,
}

/// A time on the board as a pilot sees it: whose (by name), and whether it's theirs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub name: String,
    pub ms: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub you: bool,
}

/// A pilot's best times, ms (none: never flown). A signed-in pilot's are kept on their record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bests {
    #[serde(default)]
    pub course_ms: Option<u32>,
    #[serde(default)]
    pub drill_ms: Option<u32>,
}

impl Bests {
    /// Their best at `feat`.
    pub fn get(&self, feat: Feat) -> Option<u32> {
        match feat {
            Feat::Course => self.course_ms,
            Feat::Drill => self.drill_ms,
        }
    }

    /// A time flown: whether it's their best yet (kept, then).
    pub fn better(&mut self, feat: Feat, ms: u32) -> bool {
        let best = match feat {
            Feat::Course => &mut self.course_ms,
            Feat::Drill => &mut self.drill_ms,
        };
        let better = best.is_none_or(|b| ms < b);
        if better {
            *best = Some(ms);
        }
        better
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: u64 = 20_000 * DAY + DAY / 2;

    #[test]
    fn a_pilot_is_on_the_days_list_once_with_their_best_fastest_first() {
        let mut b = Board::default();
        let p = b.record(Feat::Course, "a", "Heero", 130_000, NOON);
        assert_eq!(p, Placed { rank: Some(1), improved: true, record: true });
        assert_eq!(b.record(Feat::Course, "b", "Duo", 118_000, NOON + 1).rank, Some(1));
        // Slower than their best: they keep it, and it's no record.
        let p = b.record(Feat::Course, "a", "Heero", 140_000, NOON + 2);
        assert_eq!(p, Placed { rank: Some(2), improved: false, record: false });
        // Better: moved up, and the old one's gone.
        let p = b.record(Feat::Course, "a", "Heero", 117_500, NOON + 3);
        assert_eq!(p, Placed { rank: Some(1), improved: true, record: true });
        let v = b.view("a", NOON + 4);
        let rows: Vec<_> = v.course.iter().map(|r| (r.name.as_str(), r.ms, r.you)).collect();
        assert_eq!(rows, [("Heero", 117_500, true), ("Duo", 118_000, false)]);
        assert_eq!(v.course_record.as_ref().map(|r| r.ms), Some(117_500));
        assert!(v.drill.is_empty() && v.drill_record.is_none());
        assert_eq!((v.course_par_ms, v.drill_par_ms), (120_000, 25_000));
        // A tie goes to whoever flew it first.
        b.record(Feat::Course, "c", "Trowa", 117_500, NOON + 5);
        assert_eq!(b.view("c", NOON + 6).course[1].name, "Trowa");
        // Nobody's key is in the view.
        let json = serde_json::to_string(&b.view("a", NOON)).unwrap();
        assert!(!json.contains("\"who\"") && !json.contains("\"a\""), "{json}");
    }

    #[test]
    fn the_list_keeps_the_best_ten_and_the_day_turns_over_at_midnight() {
        let mut b = Board::default();
        for k in 0..15u32 {
            b.record(Feat::Drill, &format!("p{k}"), &format!("P{k}"), 20_000 + 1_000 * (15 - k), NOON);
        }
        let v = b.view("p0", NOON);
        assert_eq!(v.drill.len(), KEPT);
        assert_eq!(v.drill[0].ms, 21_000);
        assert!(v.drill.windows(2).all(|w| w[0].ms <= w[1].ms));
        assert!(!v.drill.iter().any(|r| r.you), "p0's 35 s didn't make it");
        // Too slow for the list: not on it.
        assert_eq!(b.record(Feat::Drill, "slow", "Slow", 60_000, NOON).rank, None);
        // The next day: the lists are empty (even before anyone flies), the record stays.
        let tomorrow = NOON + DAY;
        assert!(b.view("p0", tomorrow).drill.is_empty());
        assert_eq!(b.view("p0", tomorrow).drill_record.map(|r| r.ms), Some(21_000));
        assert!(b.turn(tomorrow) && !b.turn(tomorrow + 60));
        let p = b.record(Feat::Drill, "p0", "P0", 30_000, tomorrow);
        assert_eq!(p, Placed { rank: Some(1), improved: true, record: false });
    }

    #[test]
    fn times_and_places_read_as_the_board_shows_them() {
        assert_eq!(clock(102_370), "1:42.3");
        assert_eq!(clock(59_990), "0:59.9");
        assert_eq!(clock(0), "0:00.0");
        let places: Vec<_> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101].into_iter().map(ordinal).collect();
        assert_eq!(places, ["1ST", "2ND", "3RD", "4TH", "11TH", "12TH", "13TH", "21ST", "22ND", "101ST"]);
    }

    #[test]
    fn a_pilots_bests_keep_the_fastest_and_certificates_go_by_each_par() {
        let mut m = Bests::default();
        assert!(m.better(Feat::Drill, 30_000));
        assert!(!m.better(Feat::Drill, 31_000));
        assert!(m.better(Feat::Drill, 22_000));
        assert_eq!((m.get(Feat::Drill), m.get(Feat::Course)), (Some(22_000), None));
        assert_eq!(Feat::Course.class(119_000), Class::First);
        assert_eq!(Feat::Course.class(150_000), Class::Second);
        assert_eq!(Feat::Drill.class(25_000), Class::First);
        assert_eq!(Feat::Drill.class(40_000), Class::Third);
        // A board saved before records were kept loads.
        let old: Board = serde_json::from_str(r#"{"day":3,"course":[],"drill":[]}"#).unwrap();
        assert_eq!(old.day, 3);
    }
}
