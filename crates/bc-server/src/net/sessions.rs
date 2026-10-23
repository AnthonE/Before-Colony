//! Who is connected: each session's id and its bay, handed out at its Hello and given back when
//! it goes (`docs/SCALE.md`, "Sessions that don't hold a flight slot").
//!
//! A session is not a sector slot. It leases one of those only while its pilot flies in space;
//! what names it the rest of the time (the plaza, the Welcome, `/status`) is its id here. Ids go
//! round a counter that skips the live ones, so one comes back only after 65,000 others: a name on
//! a client's screen never sticks to someone else for long. The bays (99 round the colony's ring)
//! are shared out to the least-used, lowest first; a launch from a bay whose cradle holds a suit
//! waits for it (`bc_sector::Outcome::BayBusy`).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use bc_sim::colony::hub::BAYS;

/// The id no session has: the roster's "flown by no session" and the client's own place in the
/// crowd (`bc_client::people::OWN`).
pub const NO_SESSION: u16 = u16::MAX;
/// The most sessions there can be (ids are 1 to 65,534).
pub const MAX_SESSIONS: usize = 65_000;

/// The sessions live now, and their bays.
pub struct Sessions {
    max: usize,
    table: Mutex<Table>,
}

struct Table {
    live: HashSet<u16>,
    /// The next id to try.
    next: u16,
    /// Sessions in each bay (bay `n` is `bays[n - 1]`).
    bays: [u16; BAYS as usize],
}

/// A session's place among them: its id and bay, given back when this is dropped.
pub struct Admitted {
    pub id: u16,
    pub bay: u8,
    sessions: Arc<Sessions>,
}

impl Sessions {
    /// Room for `max` at once (at most [`MAX_SESSIONS`]).
    pub fn new(max: usize) -> Arc<Self> {
        Arc::new(Self {
            max: max.clamp(1, MAX_SESSIONS),
            table: Mutex::new(Table { live: HashSet::new(), next: 1, bays: [0; BAYS as usize] }),
        })
    }

    /// An id and a bay for a new session, or `None` if there are as many as there's room for.
    pub fn admit(self: &Arc<Self>) -> Option<Admitted> {
        let mut t = self.table.lock().ok()?;
        if t.live.len() >= self.max {
            return None;
        }
        let mut id = t.next;
        while id == 0 || id == NO_SESSION || t.live.contains(&id) {
            id = id.wrapping_add(1);
        }
        t.next = id.wrapping_add(1);
        t.live.insert(id);
        // The least-used bay, the lowest of those.
        let (k, _) = t.bays.iter().enumerate().min_by_key(|&(k, &n)| (n, k))?;
        t.bays[k] += 1;
        Some(Admitted { id, bay: k as u8 + 1, sessions: self.clone() })
    }

    /// Sessions live now.
    pub fn live(&self) -> usize {
        self.table.lock().map_or(0, |t| t.live.len())
    }

    /// The most there can be at once.
    pub fn max(&self) -> usize {
        self.max
    }
}

impl Drop for Admitted {
    fn drop(&mut self) {
        if let Ok(mut t) = self.sessions.table.lock() {
            t.live.remove(&self.id);
            let n = &mut t.bays[usize::from(self.bay) - 1];
            *n = n.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_never_0_or_no_session_and_never_two_alike() {
        let s = Sessions::new(3);
        let (a, b, c) = (s.admit().unwrap(), s.admit().unwrap(), s.admit().unwrap());
        assert_eq!((a.id, b.id, c.id), (1, 2, 3));
        assert!(s.admit().is_none(), "no more than there's room for");
        assert_eq!(s.live(), 3);
        drop(b);
        // The next goes on round the counter, not back to the one just let go.
        let d = s.admit().unwrap();
        assert_eq!(d.id, 4);
        assert_eq!(s.live(), 3);
    }

    #[test]
    fn the_counter_wraps_past_the_live_ones() {
        let s = Sessions::new(10);
        let keep = s.admit().unwrap();
        assert_eq!(keep.id, 1);
        s.table.lock().unwrap().next = NO_SESSION - 1;
        let (a, b) = (s.admit().unwrap(), s.admit().unwrap());
        // 65,534, then not 65,535 nor 0 nor the live 1: 2.
        assert_eq!((a.id, b.id), (NO_SESSION - 1, 2));
    }

    #[test]
    fn bays_go_to_the_least_used_and_come_back() {
        let s = Sessions::new(500);
        let mut first: Vec<Admitted> = (0..BAYS).map(|_| s.admit().unwrap()).collect();
        let bays: Vec<u8> = first.iter().map(|a| a.bay).collect();
        assert_eq!(bays, (1..=BAYS as u8).collect::<Vec<_>>(), "one each, in order");
        // The hundredth shares the first bay.
        let extra = s.admit().unwrap();
        assert_eq!(extra.bay, 1);
        // Bay 7 empties: the next goes there.
        drop(first.remove(6));
        assert_eq!(s.admit().unwrap().bay, 7);
    }
}
