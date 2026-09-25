//! Per-client input jitter buffer: commands arrive a few ticks early (and redundantly); the sector
//! takes exactly the one for the tick it is simulating.

use bc_proto::InputCmd;

const SLOTS: usize = 64;

#[derive(Clone, Copy)]
pub struct JitterBuffer {
    cmds: [InputCmd; SLOTS],
    valid: u64,
    /// Newest command tick received.
    pub newest: u32,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self { cmds: [InputCmd::default(); SLOTS], valid: 0, newest: 0 }
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
        self.cmds[k] = cmd;
        self.valid |= 1 << k;
        self.newest = self.newest.max(cmd.tick);
        true
    }

    /// Removes and returns the command for `tick`, if it arrived.
    #[inline]
    pub fn take(&mut self, tick: u32) -> Option<InputCmd> {
        let k = tick as usize % SLOTS;
        if self.valid & (1 << k) != 0 && self.cmds[k].tick == tick {
            self.valid &= !(1 << k);
            Some(self.cmds[k])
        } else {
            None
        }
    }
}
