//! Who owns the mouse pointer: flying needs it locked (the mouse aims), menus need it free.
//!
//! The browser has the last word. Pressing Esc while the pointer is locked makes Chrome drop the
//! lock itself, and the page never sees that Esc; alt-tab does the same. So the lock can't be a
//! flag the game sets: [`Pointer`] compares what it asked for with what the browser reports, and
//! when the browser took the lock back it opens the pause menu, which is what the player meant.
//!
//! The pointer is locked while the pilot is *engaged*: they clicked into the world (or pressed
//! Resume) and haven't left it since. A panel (pause, settings) frees it for as long as it's open.
//! Locking needs a user gesture, and Esc isn't one, so a lock asked for after Esc is refused;
//! after [`LOCK_GRACE`] the pointer gives up and the page asks for a click.
//!
//! Pure, so every sequence is a test (the Gates client's pointer bug was a sequence, found months
//! late by playing).

/// How long to wait for the browser to grant a lock, s.
pub const LOCK_GRACE: f64 = 1.5;

/// What to do with the cursor this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// Lock and hide it: flying.
    Lock,
    /// Release and show it: a menu wants clicks.
    Release,
    /// Leave it as it is.
    Leave,
}

/// One frame's facts.
#[derive(Clone, Copy, Debug, Default)]
pub struct PointerIn {
    pub now: f64,
    /// In the world (not the title, not reconnecting).
    pub playing: bool,
    /// A modal panel (pause, settings) is open.
    pub panel_open: bool,
    /// A left click landed on the game this frame.
    pub clicked: bool,
    /// The player pressed Resume.
    pub resume: bool,
    /// The game has asked for a lock (the window's grab mode).
    pub asked: bool,
    /// The browser reports the pointer locked to the game.
    pub browser_locked: bool,
}

/// What the frame decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerOut {
    pub grab: Grab,
    /// The browser took the lock back mid-flight: open the pause menu.
    pub open_pause: bool,
    /// The browser refused a lock: the player must click.
    pub refused: bool,
}

/// The pointer's memory across frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pointer {
    /// The pilot wants to fly: they clicked in (or resumed) and haven't left since.
    pub engaged: bool,
    /// The browser granted the current lock.
    granted: bool,
    /// We released the pointer ourselves, so the browser dropping it is expected.
    releasing: bool,
    /// When the pending lock was asked for.
    asked_at: Option<f64>,
}

impl Pointer {
    /// Whether the pointer is locked and the pilot is flying.
    pub fn flying(&self) -> bool {
        self.engaged && self.granted
    }

    pub fn step(&mut self, i: PointerIn) -> PointerOut {
        let mut out = PointerOut { grab: Grab::Leave, open_pause: false, refused: false };
        if i.browser_locked {
            self.granted = true;
            self.asked_at = None;
        } else if self.granted {
            self.granted = false;
            if !self.releasing && self.engaged {
                // The browser took it back (Esc, alt-tab): the player left.
                self.engaged = false;
                out.open_pause = i.playing && !i.panel_open;
            }
            self.releasing = false;
        }
        if !i.playing {
            self.engaged = false;
        } else if i.resume || (i.clicked && !i.panel_open) {
            self.engaged = true;
        }
        let want = i.playing && !i.panel_open && !out.open_pause && self.engaged;
        if want && !i.asked {
            self.asked_at = Some(i.now);
            out.grab = Grab::Lock;
        } else if want && !self.granted && self.asked_at.is_some_and(|at| i.now - at > LOCK_GRACE) {
            // Refused (Chrome won't relock straight after Esc, nor without a gesture).
            self.engaged = false;
            self.asked_at = None;
            out.refused = true;
            out.grab = Grab::Release;
        } else if !want && i.asked {
            self.releasing = self.granted;
            self.asked_at = None;
            out.grab = Grab::Release;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small harness: a browser that grants locks on the next frame.
    struct Sim {
        p: Pointer,
        asked: bool,
        browser: bool,
        now: f64,
        grant: bool,
    }

    impl Sim {
        fn new() -> Self {
            Self { p: Pointer::default(), asked: false, browser: false, now: 0.0, grant: true }
        }

        fn frame(&mut self, playing: bool, panel_open: bool, clicked: bool, resume: bool) -> PointerOut {
            self.now += 1.0 / 60.0;
            let out = self.p.step(PointerIn {
                now: self.now,
                playing,
                panel_open,
                clicked,
                resume,
                asked: self.asked,
                browser_locked: self.browser,
            });
            match out.grab {
                Grab::Lock => {
                    self.asked = true;
                    self.browser = self.grant;
                }
                Grab::Release => {
                    self.asked = false;
                    self.browser = false;
                }
                Grab::Leave => {}
            }
            out
        }
    }

    #[test]
    fn a_click_in_the_world_locks() {
        let mut s = Sim::new();
        assert_eq!(s.frame(true, false, false, false).grab, Grab::Leave, "no lock before a click");
        assert_eq!(s.frame(true, false, true, false).grab, Grab::Lock);
        s.frame(true, false, false, false);
        assert!(s.p.flying());
    }

    #[test]
    fn the_title_never_locks() {
        let mut s = Sim::new();
        assert_eq!(s.frame(false, false, true, false).grab, Grab::Leave);
        assert!(!s.p.engaged);
    }

    #[test]
    fn the_browser_taking_the_lock_opens_the_pause_menu() {
        let mut s = Sim::new();
        s.frame(true, false, true, false);
        s.frame(true, false, false, false);
        // Esc: Chrome unlocks and the page never sees the key.
        s.browser = false;
        let out = s.frame(true, false, false, false);
        assert!(out.open_pause);
        assert_eq!(out.grab, Grab::Release, "the game's grab state follows the browser");
        assert!(!s.p.engaged);
        // The menu is open now; Resume takes the pilot back.
        s.frame(true, true, false, false);
        assert_eq!(s.frame(true, false, false, true).grab, Grab::Lock);
    }

    #[test]
    fn opening_a_menu_ourselves_is_not_a_loss() {
        let mut s = Sim::new();
        s.frame(true, false, true, false);
        s.frame(true, false, false, false);
        let out = s.frame(true, true, false, false);
        assert_eq!(out.grab, Grab::Release);
        let out = s.frame(true, true, false, false);
        assert!(!out.open_pause, "our own release isn't the browser taking the pointer");
        // Closing the menu gives the pointer back (the pilot never left).
        assert_eq!(s.frame(true, false, false, false).grab, Grab::Lock);
    }

    #[test]
    fn a_refused_lock_asks_for_a_click() {
        let mut s = Sim::new();
        s.grant = false;
        s.frame(true, false, true, false);
        let mut refused = false;
        for _ in 0..120 {
            let out = s.frame(true, false, false, false);
            refused |= out.refused;
        }
        assert!(refused);
        assert!(!s.p.engaged, "waits for the next click");
        s.grant = true;
        assert_eq!(s.frame(true, false, true, false).grab, Grab::Lock);
    }

    #[test]
    fn leaving_the_world_releases() {
        let mut s = Sim::new();
        s.frame(true, false, true, false);
        s.frame(true, false, false, false);
        assert_eq!(s.frame(false, false, false, false).grab, Grab::Release);
        assert!(!s.p.engaged);
        // Back in the world, the pointer waits for a click.
        assert_eq!(s.frame(true, false, false, false).grab, Grab::Leave);
    }

    #[test]
    fn a_click_on_a_menu_does_not_capture() {
        let mut s = Sim::new();
        assert_eq!(s.frame(true, true, true, false).grab, Grab::Leave);
        assert_eq!(s.frame(true, false, false, false).grab, Grab::Leave);
    }
}
