//! History of the commands this client sent, by tick (for prediction replay and redundancy).

use bc_proto::InputCmd;

const HISTORY: usize = 128;

#[derive(Clone, Debug)]
pub struct InputHistory {
    cmds: [InputCmd; HISTORY],
    valid: [bool; HISTORY],
    pub newest: u32,
}

impl Default for InputHistory {
    fn default() -> Self {
        Self { cmds: [InputCmd::default(); HISTORY], valid: [false; HISTORY], newest: 0 }
    }
}

impl InputHistory {
    pub fn push(&mut self, cmd: InputCmd) {
        let k = cmd.tick as usize % HISTORY;
        self.cmds[k] = cmd;
        self.valid[k] = true;
        self.newest = self.newest.max(cmd.tick);
    }

    pub fn get(&self, tick: u32) -> Option<InputCmd> {
        let k = tick as usize % HISTORY;
        (self.valid[k] && self.cmds[k].tick == tick).then_some(self.cmds[k])
    }
}
