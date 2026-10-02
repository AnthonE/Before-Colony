//! The colony's radio: one channel, everyone connected (`Request::Say`, `Update::Said`). A ring of
//! the latest lines in the server's shared state, which each session reads on its 100 ms tick, off
//! the sector's hot path. Lines are never logged; `/status` counts them.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Lines kept for sessions to catch up on.
pub const KEPT: usize = 64;
/// A pilot may say at most this many lines...
pub const BURST_LINES: usize = 5;
/// ...in this long.
pub const BURST_WINDOW: Duration = Duration::from_secs(10);

/// One line said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Counts every line said since the server started, from 1.
    pub seq: u64,
    pub from: String,
    pub text: String,
}

#[derive(Default)]
pub struct Radio {
    lines: Mutex<VecDeque<Line>>,
    said: AtomicU64,
}

impl Radio {
    /// Puts a line on the air.
    pub fn say(&self, from: &str, text: String) {
        let Ok(mut lines) = self.lines.lock() else { return };
        let seq = self.said.fetch_add(1, Ordering::AcqRel) + 1;
        if lines.len() == KEPT {
            lines.pop_front();
        }
        lines.push_back(Line { seq, from: from.to_string(), text });
    }

    /// The lines said after line `seq`, oldest first (those that fell out of the ring are missed).
    pub fn since(&self, seq: u64) -> Vec<Line> {
        if self.said() <= seq {
            return Vec::new();
        }
        self.lines.lock().map(|l| l.iter().filter(|l| l.seq > seq).cloned().collect()).unwrap_or_default()
    }

    /// How many lines have been said since the server started.
    pub fn said(&self) -> u64 {
        self.said.load(Ordering::Acquire)
    }
}

/// One pilot's say: at most [`BURST_LINES`] in any [`BURST_WINDOW`].
#[derive(Debug, Default)]
pub struct Mouth {
    recent: VecDeque<Instant>,
}

impl Mouth {
    /// Whether a line may go out at `now` (and counts it if so).
    pub fn allow(&mut self, now: Instant) -> bool {
        while self.recent.front().is_some_and(|t| now.duration_since(*t) >= BURST_WINDOW) {
            self.recent.pop_front();
        }
        if self.recent.len() >= BURST_LINES {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_hear_what_was_said_after_them_and_the_ring_forgets() {
        let radio = Radio::default();
        assert!(radio.since(0).is_empty());
        radio.say("Heero", "mission accepted".into());
        let joined = radio.said();
        radio.say("Duo", "o7".into());
        let heard = radio.since(joined);
        assert_eq!(heard.len(), 1);
        assert_eq!((heard[0].from.as_str(), heard[0].text.as_str()), ("Duo", "o7"));
        assert!(radio.since(radio.said()).is_empty());
        for k in 0..KEPT + 10 {
            radio.say("Trowa", k.to_string());
        }
        let all = radio.since(0);
        assert_eq!(all.len(), KEPT);
        assert_eq!(all.last().unwrap().text, (KEPT + 9).to_string());
    }

    #[test]
    fn a_mouth_says_five_lines_in_ten_seconds() {
        let mut m = Mouth::default();
        let t = Instant::now();
        assert!((0..BURST_LINES).all(|k| m.allow(t + Duration::from_millis(100 * k as u64))));
        assert!(!m.allow(t + Duration::from_secs(2)), "a sixth too soon");
        assert!(!m.allow(t + Duration::from_millis(9_999)));
        assert!(m.allow(t + BURST_WINDOW), "the first has aged out");
    }
}
