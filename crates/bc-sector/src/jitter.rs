//! Per-client input jitter buffer: commands arrive a few ticks early (and redundantly); the sector
//! takes exactly the one for the tick it is simulating.

use bc_proto::InputCmd;

const SLOTS: usize = 64;

#[derive(Clone, Copy)]
pub struct JitterBuffer {
    cmds: [InputCmd; SLOTS],
    /// The tick that was next when each command first arrived as it is (its later copies don't move
    /// it, a changed one does): how far back its view may honestly be
    /// (`bc_sim::config::rewind_floor_q4`).
    arrived: [u32; SLOTS],
    valid: u64,
    /// Newest command tick received.
    pub newest: u32,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self { cmds: [InputCmd::default(); SLOTS], arrived: [0; SLOTS], valid: 0, newest: 0 }
    }
}

impl JitterBuffer {
    pub fn clear(&mut self) {
        self.valid = 0;
        self.newest = 0;
    }

    /// Stores a command unless it is for a tick already simulated (`next` is the next tick to run)
    /// or absurdly far in the future.
    #[inline]
    pub fn insert(&mut self, cmd: InputCmd, next: u32) -> bool {
        if cmd.tick < next || cmd.tick >= next + (SLOTS as u32 - 2) {
            return false;
        }
        let k = cmd.tick as usize % SLOTS;
        // Only a copy keeps the first one's arrival: an honest client resends a command as it was,
        // so one that changed (a placeholder sent far ahead, then the real thing) arrived now.
        if self.valid & (1 << k) == 0 || self.cmds[k] != cmd {
            self.arrived[k] = next;
        }
        self.cmds[k] = cmd;
        self.valid |= 1 << k;
        self.newest = self.newest.max(cmd.tick);
        true
    }

    /// Removes and returns the command for `tick`, if it arrived, with the tick that was next when
    /// it first did as it is.
    #[inline]
    pub fn take(&mut self, tick: u32) -> Option<(InputCmd, u32)> {
        let k = tick as usize % SLOTS;
        if self.valid & (1 << k) != 0 && self.cmds[k].tick == tick {
            self.valid &= !(1 << k);
            Some((self.cmds[k], self.arrived[k]))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_keeps_the_tick_it_first_arrived_in() {
        let mut j = JitterBuffer::default();
        let cmd = |tick| InputCmd { tick, ..InputCmd::default() };
        assert!(j.insert(cmd(105), 100));
        // Its copies, in the packets after, don't make it any later.
        assert!(j.insert(cmd(105), 101));
        assert!(j.insert(cmd(105), 103));
        assert_eq!(j.take(105).map(|(c, at)| (c.tick, at)), Some((105, 100)));
        assert!(j.take(105).is_none());
        // A command that changed is not a copy: it arrived when it changed (else a placeholder sent
        // far ahead would lend the real command its early arrival, and a deep view with it).
        assert!(j.insert(cmd(108), 100));
        let older = InputCmd { view_tick_q4: (108 - 12) << 4, ..cmd(108) };
        assert!(j.insert(older, 104));
        assert!(j.insert(older, 105));
        assert_eq!(j.take(108).map(|(c, at)| (c.view_tick_q4, at)), Some(((108 - 12) << 4, 104)));
        // The next command in its slot is its own, taken or not before it.
        let lap = SLOTS as u32;
        assert!(j.insert(cmd(105 + lap), 110));
        assert!(j.insert(cmd(120), 110));
        assert!(j.insert(cmd(120 + lap), 125));
        assert_eq!(j.take(105 + lap).map(|(_, at)| at), Some(110));
        assert_eq!(j.take(120 + lap).map(|(_, at)| at), Some(125));
    }
}
