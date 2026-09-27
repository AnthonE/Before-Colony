//! A pilot's view of the battlefield: itself plus up to [`MAX_CONTACTS`] sensor contacts.
//!
//! The same structure feeds Mobile Doll AI on the server, the ZERO System, bots (built from
//! snapshots in `bc-client-core`) and the browser autopilot. Every brain sees the world the same way.

use bc_proto::{Faction, FrameId, NO_SLOT, Part, PilotKind};
use glam::{Quat, Vec3};

/// Contacts kept per perception (the nearest ones).
pub const MAX_CONTACTS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub slot: u16,
    pub frame: FrameId,
    pub faction: Faction,
    pub pilot: PilotKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub aim: Vec3,
    /// Estimated acceleration (zero if unknown).
    pub accel: Vec3,
    /// Torso armour fraction.
    pub hull: f32,
    pub dist: f32,
    pub firing: bool,
    /// Its weapon is pointed within a few degrees of us.
    pub aiming_at_me: bool,
    pub locked_on_me: bool,
    pub hostile: bool,
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            slot: NO_SLOT,
            frame: FrameId::Leo,
            faction: Faction::Oz,
            pilot: PilotKind::MobileDoll,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: Quat::IDENTITY,
            aim: Vec3::Z,
            accel: Vec3::ZERO,
            hull: 1.0,
            dist: f32::MAX,
            firing: false,
            aiming_at_me: false,
            locked_on_me: false,
            hostile: false,
        }
    }
}

/// What a kit-aware brain knows of its own suit beyond the basics.
#[derive(Clone, Copy, Debug, Default)]
pub struct KitView {
    /// Its missile lock on its designation is acquired.
    pub lock_acquired: bool,
    /// A guided missile is tracking it.
    pub missile_incoming: bool,
    /// Its special can be used now, and whether it's engaged (the jammer on, Full Open).
    pub special_ready: bool,
    pub special_active: bool,
    /// It's changing form.
    pub transforming: bool,
}

/// The perceiving suit itself.
#[derive(Clone, Copy, Debug)]
pub struct SelfView {
    pub slot: u16,
    pub frame: FrameId,
    pub faction: Faction,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub aim: Vec3,
    /// Armour fraction per [`Part`].
    pub parts: [f32; Part::COUNT],
    /// Fractions 0..1.
    pub heat: f32,
    pub energy: f32,
    pub propellant: f32,
    pub g_strain: f32,
    /// Primary, secondary, melee ready to fire.
    pub ready: [bool; 3],
    pub overheated: bool,
    pub kit: KitView,
}

impl Default for SelfView {
    fn default() -> Self {
        Self {
            slot: NO_SLOT,
            frame: FrameId::Leo,
            faction: Faction::Oz,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: Quat::IDENTITY,
            aim: Vec3::Z,
            parts: [1.0; Part::COUNT],
            heat: 0.0,
            energy: 1.0,
            propellant: 1.0,
            g_strain: 0.0,
            ready: [true; 3],
            overheated: false,
            kit: KitView::default(),
        }
    }
}

impl SelfView {
    pub fn hull(&self) -> f32 {
        self.parts[Part::Torso as usize]
    }
    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::Z
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Perception {
    pub me: SelfView,
    contacts: [Contact; MAX_CONTACTS],
    n: usize,
    /// Index of the farthest kept contact (valid when full).
    far: usize,
}

impl Default for Perception {
    fn default() -> Self {
        Self { me: SelfView::default(), contacts: [Contact::default(); MAX_CONTACTS], n: 0, far: 0 }
    }
}

impl Perception {
    pub fn reset(&mut self, me: SelfView) {
        self.me = me;
        self.n = 0;
        self.far = 0;
    }

    /// Whether a contact at `dist` would be kept. Lets callers skip building contacts that
    /// [`offer`](Self::offer) would throw away.
    #[inline]
    pub fn would_keep(&self, dist: f32) -> bool {
        self.n < MAX_CONTACTS || dist < self.contacts[self.far].dist
    }

    fn refresh_far(&mut self) {
        let mut far = 0;
        for i in 1..self.n {
            if self.contacts[i].dist > self.contacts[far].dist {
                far = i;
            }
        }
        self.far = far;
    }

    /// Adds a contact, keeping only the nearest [`MAX_CONTACTS`].
    pub fn offer(&mut self, c: Contact) {
        if self.n < MAX_CONTACTS {
            self.contacts[self.n] = c;
            self.n += 1;
            if self.n == MAX_CONTACTS {
                self.refresh_far();
            }
            return;
        }
        if c.dist < self.contacts[self.far].dist {
            self.contacts[self.far] = c;
            self.refresh_far();
        }
    }

    pub fn contacts(&self) -> &[Contact] {
        &self.contacts[..self.n]
    }

    pub fn hostiles(&self) -> impl Iterator<Item = &Contact> {
        self.contacts().iter().filter(|c| c.hostile)
    }

    pub fn get(&self, slot: u16) -> Option<&Contact> {
        self.contacts().iter().find(|c| c.slot == slot)
    }

    pub fn nearest_hostile(&self) -> Option<&Contact> {
        self.hostiles().min_by(|a, b| a.dist.total_cmp(&b.dist))
    }
}
