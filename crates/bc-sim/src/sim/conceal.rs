//! Concealment: how hard a suit is to find, for its enemies' sensors and their missiles' seekers.
//!
//! Cover and concealment are different things. *Cover* is geometry: a suit crouched behind a rim or
//! a pylon is shielded by the body itself, anywhere (shots meet the first thing in their way). This
//! is *concealment*: what a suit shows. One function, [`Sim::concealment`], answers it for sensors
//! ([`Sim::detects`]) and seekers alike, and only for enemies: allies always see their own.
//!
//! - **Parked.** A sleeper parked on a body stays in sight for [`POWER_DOWN_TICKS`] after its pilot
//!   leaves, or [`FOUGHT_DARK_TICKS`] after it last fired or was hit, whichever is later. Then its
//!   reactor idles: off sensors, and eyes find it only within [`PARKED_VISUAL`], or its hide spot's
//!   own range.
//! - **Lurking.** An awake suit crouched on a body, stick idle and barely moving, settles in
//!   [`LURK_SETTLE_TICKS`]. In a hide spot it is then as dark as a parked suit (eyes a little
//!   further: [`HIDE_AWAKE_VISUAL_MUL`]); anywhere else it runs cold, at [`COLD_SIG`] of its
//!   signature. Firing, or being hit, shows it for [`EXPOSE_TICKS`].
//!
//! [`Sim::cover_step`] keeps what that needs, once a tick after the suits have moved: since when
//! each has lain still, the hide spot it's in, which suits stand still (replication's priorities),
//! what each pilot could park on, and the counts for the metrics.

use bc_proto::PilotKind;
use bc_proto::buttons::BOOST;

use super::{PARKED_VISUAL, Sim};
use crate::bodies::{Bodies, Body};
use crate::config::VISUAL_RANGE;
use crate::ground::{CROUCH_STANCE, Footing};
use crate::math::length;
use crate::suits::{MeleePhase, NO_SPOT, NOT_STILL};

/// A parked suit stays in sight this long after its pilot leaves, ticks (8 s)...
pub const POWER_DOWN_TICKS: u32 = 240;
/// ...and this long after it last fired or was hit, ticks (60 s): nobody logs off out of a fight.
pub const FOUGHT_DARK_TICKS: u32 = 1_800;
/// An awake suit crouched still on a body settles, cold or hidden, after this long, ticks (3 s).
pub const LURK_SETTLE_TICKS: u32 = 90;
/// Firing or being hit shows a settled suit for this long, ticks (5 s).
pub const EXPOSE_TICKS: u32 = 150;
/// Slower than this over its body, m/s, a crouched suit lies still.
pub const LURK_STILL: f32 = 0.5;
/// What a suit settled out of a hide spot shows of its signature.
pub const COLD_SIG: f32 = 0.5;
/// Awake in a hide spot, eyes find a suit this much further off than they would a parked one.
pub const HIDE_AWAKE_VISUAL_MUL: f32 = 1.5;
/// Standing still for replication: slower than this over its body, m/s, turning slower than this,
/// rad/s, and quiet (no shot, no strike) for this long, ticks.
const STILL_SPEED: f32 = 0.05;
const STILL_SPIN: f32 = 0.01;
const STILL_QUIET_TICKS: u32 = 30;

/// What a suit's cover amounts to, as its pilot is told ([`Sim::cover_code`]): the own state's
/// wire codes.
pub use bc_proto::snapshot::cover;

/// How much of a suit's signature its enemies' sensors and seekers get, and how far off their eyes
/// still find it, m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conceal {
    pub sig: f32,
    pub visual: f32,
}

impl Conceal {
    /// Nothing hidden.
    pub const NONE: Conceal = Conceal { sig: 1.0, visual: VISUAL_RANGE };
}

impl Sim {
    /// How well suit `j` is concealed from its enemies now (see the module docs). O(1).
    pub fn concealment(&self, j: usize) -> Conceal {
        let (s, t) = (&self.suits, self.tick());
        // When it last fought (0: never; tick 0 is never simulated).
        let q = s.last_fired[j].max(s.last_hit[j]);
        if self.is_parked(j) {
            let fought = if q == 0 { 0 } else { q + FOUGHT_DARK_TICKS };
            let dark_at = (s.slept_at[j] + POWER_DOWN_TICKS).max(fought);
            return if t >= dark_at {
                Conceal { sig: 0.0, visual: self.spot_visual(j).unwrap_or(PARKED_VISUAL) }
            } else {
                Conceal::NONE
            };
        }
        let settled = s.still_since[j] != NOT_STILL && t >= s.still_since[j] + LURK_SETTLE_TICKS;
        let quiet = q == 0 || t >= q + EXPOSE_TICKS;
        if !s.sleeping.get(j) && s.footing[j] == Footing::Grounded && settled && quiet {
            return match self.spot_visual(j) {
                Some(v) => Conceal { sig: 0.0, visual: v * HIDE_AWAKE_VISUAL_MUL },
                None => Conceal { sig: COLD_SIG, visual: VISUAL_RANGE },
            };
        }
        Conceal::NONE
    }

    /// What suit `i`'s cover amounts to ([`cover`]): hidden, cold, settling (crouched still, or
    /// just shown by a shot or a hit), or exposed.
    pub fn cover_code(&self, i: usize) -> u8 {
        if i >= self.suits.cap || !self.suits.alive.get(i) {
            return cover::EXPOSED;
        }
        let c = self.concealment(i);
        if c.sig == 0.0 {
            cover::HIDDEN
        } else if c.sig < 1.0 {
            cover::COLD
        } else if self.suits.still_since[i] != NOT_STILL {
            cover::SETTLING
        } else {
            cover::EXPOSED
        }
    }

    /// Whether suit `j` stands still (awake on its feet, not moving, turning or fighting), or is
    /// parked: resending it tells its viewers nothing new.
    pub fn is_still(&self, j: usize) -> bool {
        j < self.suits.cap && self.suits.still.get(j)
    }

    /// How far off eyes find suit `j` in the hide spot it's in, m (`None`: it's in none).
    fn spot_visual(&self, j: usize) -> Option<f32> {
        let Body::Landmark(k) = self.suits.anchor[j].body else { return None };
        let spot = self.suits.hide_spot[j];
        if spot == NO_SPOT {
            return None;
        }
        self.landmarks().get(usize::from(k))?.hides.get(usize::from(spot)).map(|h| h.visual)
    }

    /// Whether awake suit `i` is lurking: crouched on a body, stick idle, barely moving over it, and
    /// neither striking, running nor changing form.
    fn lurking(&self, i: usize) -> bool {
        let s = &self.suits;
        let cmd = &s.input[i];
        !s.sleeping.get(i)
            && s.footing[i] == Footing::Grounded
            && s.anchor[i].stance == CROUCH_STANCE
            && length(s.anchor[i].vel) < LURK_STILL
            && cmd.thrust[0] == 0
            && cmd.thrust[2] == 0
            && s.melee[i].phase == MeleePhase::Idle
            && !cmd.pressed(BOOST)
            && !s.form(i).changing()
    }

    /// Once a tick, after the suits have moved: since when each has lain still, the hide spot each
    /// suit on a landmark is in, which stand still, what each pilot could park on, and the counts.
    /// A sleeper keeps the stillness it fell asleep with: one that slept hidden wakes hidden.
    pub(super) fn cover_step(&mut self, t: u32) {
        let mut used = core::mem::take(&mut self.cover_bits);
        used.copy_from(&self.suits.used);
        let bodies = Bodies::at(&self.field, self.landmarks(), t);
        let (mut grounded, mut aloft, mut hidden, mut hidden_asleep) = (0, 0, 0, 0);
        for i in used.iter() {
            if !self.suits.alive.get(i) {
                let s = &mut self.suits;
                s.still_since[i] = NOT_STILL;
                s.hide_spot[i] = NO_SPOT;
                s.parkable[i] = Body::None;
                s.still.set(i, false);
                continue;
            }
            let asleep = self.suits.sleeping.get(i);
            let parked = self.is_parked(i);
            let lurking = self.lurking(i);
            let pilot = !asleep && self.suits.pilot[i] != PilotKind::MobileDoll;
            let parkable = if pilot { self.parkable(i).unwrap_or(Body::None) } else { Body::None };
            let s = &mut self.suits;
            let (footing, a) = (s.footing[i], s.anchor[i]);
            if !asleep {
                s.still_since[i] = match (lurking, s.still_since[i]) {
                    (false, _) => NOT_STILL,
                    (true, NOT_STILL) => t,
                    (true, since) => since,
                };
            }
            let on_it = (footing == Footing::Grounded && !asleep) || parked;
            s.hide_spot[i] =
                if on_it { bodies.hide_spot_of(a.body, a.local).unwrap_or(NO_SPOT) } else { NO_SPOT };
            s.parkable[i] = parkable;
            let still = parked
                || (!asleep
                    && footing == Footing::Grounded
                    && length(a.vel) < STILL_SPEED
                    && length(a.ang_vel) < STILL_SPIN
                    && t.saturating_sub(s.last_fired[i]) >= STILL_QUIET_TICKS
                    && s.melee[i].phase == MeleePhase::Idle);
            s.still.set(i, still);
            match footing {
                Footing::Grounded => grounded += 1,
                Footing::Aloft => aloft += 1,
                Footing::Free => {}
            }
            if self.concealment(i).sig == 0.0 {
                hidden += 1;
                hidden_asleep += u32::from(asleep);
            }
        }
        (self.n_grounded, self.n_aloft, self.n_hidden) = (grounded, aloft, hidden);
        self.n_hidden_asleep = hidden_asleep;
        self.cover_bits = used;
    }
}
