//! A fight on foot as the pilot's own screen sees it (`bc_sim::colony::brawl`; the server judges,
//! `bc-server`'s `brawl`): who a blow from where they stand would land on (the server asks the very
//! same of the poses it took), who E helps up, what E does when more than one thing is in reach,
//! and their own health and time down as the server last said (`Update::Hurt`).

use bc_proto::presence::{PersonPose, VISITOR_BASE};
use bc_sim::colony::brawl::{self, MAX_HP};

/// The pilot's fight on foot, as the server last said it (`Update::Hurt`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hurt {
    pub hp: u8,
    /// How long till they're up, s, when it was said (0: up).
    pub down_s: u16,
    /// Who last hurt them.
    pub by: Option<String>,
    /// Who's lifting them up, while somebody is.
    pub helper: Option<String>,
    /// How many times it's been said (a screen counts the time down from the latest).
    pub heard: u64,
}

impl Hurt {
    /// Down, lying where they fell.
    pub fn down(&self) -> bool {
        self.down_s > 0
    }

    /// What the bay line says of their health: nothing while they're whole.
    pub fn tag(&self) -> Option<String> {
        (self.hp < MAX_HP).then(|| format!("HP {}", self.hp))
    }
}

/// Whether someone in the plaza could be struck, helped or run down at all: another pilot (never
/// one of the store's traders on the Exchange floor), on foot (not driving, riding or seated).
fn on_foot(id: u16, p: &PersonPose) -> bool {
    id < VISITOR_BASE && !p.driving() && p.riding().is_none() && !p.seated()
}

/// Who a blow thrown from `me` would land on, of `people` (each by the id the plaza knows them by,
/// drawn where this screen has them), and what it would take (`brawl::lands`): of those it lands
/// on, anyone standing before anyone down (a blow on somebody down knocks them out), then the
/// nearest.
pub fn target(me: &PersonPose, people: &[(u16, PersonPose)]) -> Option<(u16, u8)> {
    people
        .iter()
        .filter(|(id, p)| on_foot(*id, p))
        .filter_map(|(id, p)| {
            let damage = brawl::lands(me, p)?;
            let far = (p.x - me.x).hypot(p.s - me.s);
            Some((p.down(), far, *id, damage))
        })
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, id, damage)| (id, damage))
}

/// The pilot who's down that E beside them would help up, of `people`: within
/// `brawl::HELP_REACH`, the nearest.
pub fn down_near(me: &PersonPose, people: &[(u16, PersonPose)]) -> Option<u16> {
    people
        .iter()
        .filter(|(id, p)| *id < VISITOR_BASE && p.down() && brawl::helps(me, p))
        .min_by(|a, b| {
            let far = |p: &PersonPose| (p.x - me.x).hypot(p.s - me.s);
            far(&a.1).total_cmp(&far(&b.1))
        })
        .map(|(id, _)| *id)
}

/// What E does on foot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use<T> {
    /// Help the pilot who's down there up (held).
    Help(u16),
    /// Whatever else E would take of what's in reach (a car, a lift's door), as picked.
    Thing(T),
}

/// What E takes on foot: somebody down beside the pilot comes first (somebody on the ground beats a
/// car at the kerb or a lift's door); else the thing in reach that was picked.
pub fn use_key<T>(down: Option<u16>, thing: Option<T>) -> Option<Use<T>> {
    down.map(Use::Help).or(thing.map(Use::Thing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::presence::{RIDE_CAR, RIDE_DOWN, RING_DECK};
    use bc_sim::colony::brawl::{BEHIND, BLOW, HELP_REACH};
    use core::f32::consts::FRAC_PI_2;

    /// Somewhere open in Hub Gate's square.
    fn open() -> (f32, f32) {
        use bc_sim::colony::{deck, town};
        let (x0, t) = town::out_of_the_lift(0);
        let s0 = town::deck_s(t);
        let clear = |x: f32, s: f32| {
            let (min, max) =
                (glam::Vec3::new(x - 4.0, 0.5, -s - 4.0), glam::Vec3::new(x + 4.0, 1.8, -s + 4.0));
            !deck::solid(deck::RING, min, max)
        };
        (0..3_600)
            .map(|k| (x0 - 60.0 + 2.0 * (k / 60) as f32, s0 - 60.0 + 2.0 * (k % 60) as f32))
            .find(|&(x, s)| clear(x, s))
            .expect("open ground")
    }

    fn at(x: f32, s: f32, yaw: f32) -> PersonPose {
        PersonPose { strip: RING_DECK, x, s, yaw, grounded: true, ..PersonPose::default() }
    }

    #[test]
    fn a_blow_goes_to_whoever_stands_in_front_before_anyone_down() {
        let (x, s) = open();
        let me = at(x, s, FRAC_PI_2);
        let ahead = at(x + 1.6, s, FRAC_PI_2);
        let near_down = PersonPose { ride: RIDE_DOWN, ..at(x + 1.0, s, 0.0) };
        let driving = PersonPose { ride: RIDE_CAR, ..at(x + 0.8, s, 0.0) };
        let trader = at(x + 0.5, s, 0.0);
        let people = [(4, near_down), (5, ahead), (6, driving), (VISITOR_BASE + 2, trader)];
        // Their back to them: twice a blow.
        assert_eq!(target(&me, &people), Some((5, BLOW * BEHIND)));
        assert_eq!(target(&me, &[(4, near_down)]), Some((4, BLOW)), "somebody down, if nobody's up");
        assert_eq!(target(&me, &[(6, driving), (VISITOR_BASE + 2, trader)]), None);
        assert_eq!(target(&PersonPose { yaw: -FRAC_PI_2, ..me }, &people), None, "facing away");
    }

    #[test]
    fn e_helps_somebody_down_before_a_car_or_a_lift() {
        let (x, s) = open();
        let me = at(x, s, 0.0);
        let down = PersonPose { ride: RIDE_DOWN, ..at(x + 2.0, s, 0.0) };
        let far_down = PersonPose { ride: RIDE_DOWN, ..at(x + HELP_REACH + 0.5, s, 0.0) };
        let up = at(x + 1.0, s, 0.0);
        assert_eq!(down_near(&me, &[(3, up), (4, far_down), (5, down)]), Some(5));
        assert_eq!(down_near(&me, &[(3, up), (4, far_down)]), None);
        // With a car at the kerb (or a lift's door) in reach too, E helps.
        #[derive(Debug, PartialEq)]
        enum Thing {
            Car,
            Lift,
        }
        assert_eq!(use_key(Some(5), Some(Thing::Car)), Some(Use::Help(5)));
        assert_eq!(use_key(Some(5), Some(Thing::Lift)), Some(Use::Help(5)));
        assert_eq!(use_key(None, Some(Thing::Lift)), Some(Use::Thing(Thing::Lift)));
        assert_eq!(use_key::<Thing>(None, None), None);
        let hurt = Hurt { hp: 60, ..Hurt::default() };
        assert_eq!(hurt.tag().as_deref(), Some("HP 60"));
        assert_eq!(Hurt { hp: MAX_HP, ..hurt }.tag(), None);
    }
}
