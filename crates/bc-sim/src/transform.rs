//! Changing form (Wing Zero ↔ Neo-Bird). The server and the owner's client step it alike, so the
//! client's prediction flies through a transformation exactly as the server does.
//!
//! A transformable frame's playable form is its MODE-off form: holding MODE asks for the other.
//! Once a change starts it runs its course (`ticks`): weapons are down and main thrust is cut
//! meanwhile. On its last tick the suit becomes the other frame.

use bc_proto::FrameId;

use crate::content::{SpecialKind, frame};

/// A suit's form: its frame, and the ticks left of a change under way (0: none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Form {
    pub frame: FrameId,
    pub timer: u16,
}

impl Form {
    pub fn changing(&self) -> bool {
        self.timer > 0
    }
}

/// Steps `form` a tick with MODE held or not. Returns whether a change started.
pub fn transform_step(form: &mut Form, mode: bool) -> bool {
    let SpecialKind::Transform { to, ticks, .. } = frame(form.frame).special else {
        form.timer = 0;
        return false;
    };
    if form.timer > 0 {
        form.timer -= 1;
        if form.timer == 0 {
            form.frame = to;
        }
        return false;
    }
    if mode == frame(form.frame).playable {
        form.timer = u16::from(ticks);
        return true;
    }
    false
}

/// What main thrust is multiplied by: cut while changing form.
pub fn transform_thrust(form: &Form) -> f32 {
    match frame(form.frame).special {
        SpecialKind::Transform { thrust, .. } if form.changing() => thrust,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_turns_wing_zero_into_neo_bird_and_back() {
        let SpecialKind::Transform { ticks, .. } = frame(FrameId::WingZero).special else { panic!() };
        let mut f = Form { frame: FrameId::WingZero, timer: 0 };
        assert!(!transform_step(&mut f, false));
        assert!(transform_step(&mut f, true));
        for k in 1..u16::from(ticks) {
            // Letting go doesn't stop a change under way.
            assert!(!transform_step(&mut f, k % 2 == 0));
            assert_eq!((f.frame, f.changing()), (FrameId::WingZero, true));
            assert!(transform_thrust(&f) < 1.0);
        }
        transform_step(&mut f, true);
        assert_eq!(f, Form { frame: FrameId::WingZeroBird, timer: 0 });
        assert_eq!(transform_thrust(&f), 1.0);
        // Held, it stays a bird; let go, it changes back.
        assert!(!transform_step(&mut f, true));
        assert!(transform_step(&mut f, false));
        for _ in 0..ticks {
            transform_step(&mut f, false);
        }
        assert_eq!(f, Form { frame: FrameId::WingZero, timer: 0 });
        // Frames that don't transform never change.
        let mut leo = Form { frame: FrameId::Leo, timer: 0 };
        assert!(!transform_step(&mut leo, true));
        assert_eq!(leo, Form { frame: FrameId::Leo, timer: 0 });
    }
}
