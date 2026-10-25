//! The pilot's hangar as the server tells it (survival rules): where they are, their bay (credits,
//! stores, the suit, the stations' jobs), the exchange, and what they've been told. The server
//! decides everything; this keeps its latest word, and turns what the pilot asks into frames.

use bc_econ::charter::CharterView;
use bc_econ::exchange::Depth;
use bc_econ::proving::{BoardView, Trainer};
use bc_econ::wire::{self, HangarView, HomesView, MarketView, Outcome, Place, Request, Update};
use bc_econ::{Bay, Debrief, Item, Material};
use bc_proto::control::encode_hangar;

#[derive(Clone, Debug, Default)]
pub struct HangarState {
    /// Where the pilot is (`None`: arcade rules, or not told yet).
    pub place: Option<Place>,
    /// Their bay's number.
    pub bay: u8,
    /// In the city: the land strip they're on.
    pub strip: Option<u8>,
    /// Flying one of the Charter Board's trainers, from the Blast Hall's gantry (and, just docked
    /// it, on foot there).
    pub trainer: bool,
    pub view: Option<HangarView>,
    pub market: Option<MarketView>,
    /// The Charter Board (while the pilot is looking at it).
    pub charter: Option<CharterView>,
    /// The Proving Ground's board (in the colony, where the server keeps it up to date: forgotten
    /// back in the bay).
    pub proving: Option<BoardView>,
    /// The held homes of the buildings near the pilot in the city (`docs/OPEN_CITY.md`): the rest
    /// are free.
    pub homes: HomesView,
    /// Into the city by waking at home (H3): where they get up, until the city's started there.
    pub wake: Option<bc_econ::wire::Wake>,
    /// The pilot's body (`bc_econ::body`): how fed they are, and for how long.
    pub body: Option<bc_econ::body::BodyView>,
    /// The book (and price history) of the item the pilot is watching.
    pub book: Option<(Depth, Vec<u64>)>,
    /// What to tell the pilot (the text, and whether it was done or refused), oldest first. The
    /// UI takes them.
    pub notes: Vec<(String, bool)>,
    /// Sorties that ended, oldest first. The UI takes them.
    pub sorties: Vec<(Outcome, String)>,
    /// Their payout sheets (`bc_econ::debrief`), oldest first: the UI takes them...
    pub debriefs: Vec<Debrief>,
    /// ...and the last one is kept.
    pub last_debrief: Option<Debrief>,
    /// News, oldest first. The UI takes it.
    pub news: Vec<String>,
    /// In the city: the names of the people seen there, by the slot the plaza knows them by.
    pub people: std::collections::HashMap<u16, String>,
    /// The colony's radio: who said what, oldest first (any rules). The UI takes it.
    pub said: Vec<(String, String)>,
    /// Sales filled on the Exchange this session (its `SOLD …` notes; `objectives`).
    pub sales: u32,
    /// Bumped by every update (the UI redraws when it moves).
    pub version: u64,
}

impl HangarState {
    /// Takes in an update. Whether the pilot just came back from the sector into the hangar, or
    /// left the city (where they watched the suits inside the colony): what they saw is done with.
    pub fn apply(&mut self, update: Update) -> bool {
        self.version += 1;
        match update {
            Update::Place { place, bay, strip, trainer, wake } => {
                let back = (self.place == Some(Place::Space) && place == Place::Hangar)
                    || (self.place == Some(Place::City) && place != Place::City);
                // Out of a trainer onto the hall's floor, it's still where they came from.
                let from_trainer = self.trainer && place == Place::City;
                (self.place, self.bay, self.strip) = (Some(place), bay, strip);
                self.trainer = trainer || from_trainer;
                self.wake = wake;
                if place == Place::Hangar {
                    self.proving = None;
                }
                return back;
            }
            Update::Hangar(view) => self.view = Some(view),
            Update::Market(market) => self.market = Some(market),
            Update::Charter(view) => self.charter = Some(view),
            Update::Book { depth, history } => self.book = Some((depth, history)),
            Update::Note { text, ok } => {
                // The Exchange's word of a sale filled (`bc_econ::exchange`).
                if ok && text.starts_with("SOLD ") {
                    self.sales += 1;
                }
                self.notes.push((text, ok));
            }
            Update::Sortie { outcome, text, debrief } => {
                self.sorties.push((outcome, text));
                if let Some(d) = debrief {
                    self.debriefs.push(d.clone());
                    self.last_debrief = Some(d);
                }
            }
            Update::News { text } => self.news.push(text),
            Update::People { people } => {
                for p in people {
                    self.people.insert(p.id, p.name);
                }
            }
            Update::Said { from, text } => self.said.push((from, text)),
            Update::Proving(view) => self.proving = Some(view),
            Update::Homes(view) => self.homes = view,
            Update::Body(view) => self.body = Some(view),
        }
        false
    }

    /// How fed the pilot is, once the server has said.
    pub fn fed(&self) -> Option<bc_sim::content::body::Fed> {
        self.body.map(|b| b.fed)
    }

    /// Whether the pilot came into the city out of a trainer (at the Blast Hall's gantry, not down
    /// the lift at Hub Gate), and is still on foot there: what puts them at the gantry's hatch.
    pub fn off_a_trainer(&self) -> bool {
        self.trainer && self.place == Some(Place::City)
    }

    /// What the Blast Hall's gantry readies for the pilot, by name: the test range's pick (the
    /// Proving Ground's board carries it), the Charter Board's Argo till they pick another.
    pub fn trainer_name(&self) -> String {
        let pick = self.proving.as_ref().map_or(Trainer::Board, |p| p.trainer);
        let frame = |line| bc_sim::content::frame_name(line).to_uppercase();
        match pick {
            Trainer::Board => "THE CHARTER BOARD'S ARGO".into(),
            Trainer::Bay => match self.view.as_ref().map(|v| &v.bay) {
                Some(Bay::Docked { suit }) => format!("YOUR {}, AS BUILT", frame(suit.line)),
                _ => "YOUR BAY'S BUILD".into(),
            },
            Trainer::Line { line } => format!("A NEW {}", frame(line)),
        }
    }

    /// On foot in the hangar bay.
    pub fn in_hangar(&self) -> bool {
        self.place == Some(Place::Hangar)
    }

    /// On foot in the colony's city.
    pub fn in_city(&self) -> bool {
        self.place == Some(Place::City)
    }

    /// Credits, as last told.
    pub fn credits(&self) -> u64 {
        self.view.as_ref().map_or(0, |v| v.credits)
    }

    /// Whether the Proving Ground's board holds a time of the pilot's own (the server's word:
    /// what earns the Charter Board's flight certificate).
    pub fn has_certificate(&self) -> bool {
        self.proving.as_ref().is_some_and(|p| p.mine.any())
    }

    /// Whether the bay's suit (standing in it, or out) has every weapon its frame carries fitted:
    /// the worn Argo an Arrival is handed hasn't its beam rifle.
    pub fn armed(&self) -> bool {
        match self.view.as_ref().map(|v| &v.bay) {
            Some(Bay::Docked { suit } | Bay::Out { suit }) => {
                (0..3).all(|m| suit.weapon_on(m).is_none() || suit.mounts[m])
            }
            _ => false,
        }
    }

    /// The pilot's standing with the colony, as the Charter Board last told it.
    pub fn standing(&self) -> u64 {
        self.charter.as_ref().map_or(0, |c| c.standing)
    }

    /// The Almagest's aces the pilot has downed, as the Charter Board last told it.
    pub fn aces_downed(&self) -> u32 {
        self.charter.as_ref().map_or(0, |c| c.mine_aces)
    }

    /// Whether the Charter Board holds something for the pilot at its desk (a certificate's rifle,
    /// a patrol's reward, an ace's bounty), as it last told it: the server tells it as it's put
    /// there and as it's handed over.
    pub fn board_holds(&self) -> bool {
        self.charter.as_ref().is_some_and(|c| c.waiting.is_some())
    }

    /// Whether the pilot has siderium in their stores, or a job making it.
    pub fn smelting(&self) -> bool {
        let siderium = Item::Material(Material::Siderium);
        self.view.as_ref().is_some_and(|v| {
            v.stock.iter().any(|&(item, q)| item == siderium && q > 0)
                || v.jobs.iter().any(|j| j.item == siderium)
        })
    }
}

/// A request as a control-stream frame.
pub fn frame(req: &Request) -> Vec<u8> {
    let json = wire::encode(req);
    let mut out = vec![0u8; json.len() + 3];
    let n = encode_hangar(&json, &mut out).unwrap_or(0);
    out.truncate(n);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::control::Frame;

    #[test]
    fn requests_frame_and_updates_land() {
        let bytes = frame(&Request::Launch);
        let Ok(Some((Frame::Hangar(p), used))) = Frame::decode(&bytes) else { panic!() };
        assert_eq!(used, bytes.len());
        assert_eq!(wire::decode::<Request>(p), Some(Request::Launch));

        let mut h = HangarState::default();
        let place = |place, strip, trainer| Update::Place { place, bay: 7, strip, trainer, wake: None };
        assert!(!h.apply(place(Place::Hangar, None, false)));
        assert!(h.in_hangar());
        assert!(!h.apply(place(Place::Space, None, false)));
        assert!(h.apply(place(Place::Hangar, None, false)), "back from the sector");
        assert!(!h.apply(place(Place::City, Some(2), false)));
        assert!(h.in_city() && h.strip == Some(2));
        assert!(h.apply(place(Place::Hangar, None, false)), "up from the city");
        assert!(!h.apply(place(Place::City, Some(2), false)));
        h.apply(Update::Note { text: "MADE 80 kg STEEL".into(), ok: true });
        assert_eq!(h.notes.len(), 1);
        assert_eq!(h.version, 7);
        // The body, once the server has said.
        assert_eq!(h.fed(), None);
        let peckish = bc_sim::content::body::Fed::Peckish;
        h.apply(Update::Body(bc_econ::body::BodyView { fed: peckish, food_s: 2_400, well_fed_s: 0 }));
        assert_eq!(h.fed(), Some(peckish));
        assert_eq!(h.body.map(|b| b.food_s), Some(2_400));
        // Into a trainer from the city, and out of it onto the hall's floor; then up the lift.
        assert!(!h.off_a_trainer());
        assert!(h.apply(place(Place::Space, None, true)), "the city's watching is done with");
        assert!(h.trainer && !h.off_a_trainer());
        assert!(!h.apply(place(Place::City, Some(0), false)));
        assert!(h.off_a_trainer(), "on foot at the gantry");
        h.apply(Update::Proving(bc_econ::proving::Board::default().view("", 0)));
        assert!(h.proving.is_some());
        h.apply(place(Place::Hangar, None, false));
        assert!(!h.trainer && !h.off_a_trainer());
        assert!(h.proving.is_none(), "the board's the colony's");
    }

    #[test]
    fn the_gantry_names_the_trainer_picked() {
        let mut h = HangarState::default();
        assert_eq!(h.trainer_name(), "THE CHARTER BOARD'S ARGO", "before the board's word");
        h.apply(Update::Hangar(HangarView::of(&bc_econ::Hangar::starter(), 0)));
        let mut board = bc_econ::proving::Board::default().view("", 0);
        h.apply(Update::Proving(board.clone()));
        assert_eq!(h.trainer_name(), "THE CHARTER BOARD'S ARGO");
        // The suits by their names.
        let name = |f| bc_sim::content::frame_name(f).to_uppercase();
        board.trainer = Trainer::Bay;
        h.apply(Update::Proving(board.clone()));
        assert_eq!(h.trainer_name(), format!("YOUR {}, AS BUILT", name(bc_proto::FrameId::Argo)));
        board.trainer = Trainer::Line { line: bc_proto::FrameId::Antares };
        h.apply(Update::Proving(board));
        assert_eq!(h.trainer_name(), format!("A NEW {}", name(bc_proto::FrameId::Antares)));
    }

    /// What the objectives read off the server's word: a certificate, a suit armed, standing, aces
    /// downed, siderium.
    #[test]
    fn the_objectives_read_the_servers_word() {
        let mut h = HangarState::default();
        assert!(!h.has_certificate() && !h.armed() && !h.smelting(), "nothing told yet");
        assert_eq!((h.standing(), h.aces_downed()), (0, 0));
        // The worn Argo an Arrival is handed hasn't its beam rifle; a complete one, docked or out, is
        // armed.
        let starter = bc_econ::Hangar::starter();
        h.apply(Update::Hangar(HangarView::of(&starter, 0)));
        assert!(!h.armed(), "the starter's Argo came without its rifle");
        let argo = bc_econ::Suit::complete(bc_proto::FrameId::Argo);
        let mut whole = bc_econ::Hangar { bay: Bay::Docked { suit: argo.clone() }, ..starter.clone() };
        h.apply(Update::Hangar(HangarView::of(&whole, 0)));
        assert!(h.armed());
        whole.bay = Bay::Out { suit: argo };
        h.apply(Update::Hangar(HangarView::of(&whole, 0)));
        assert!(h.armed(), "out on a sortie, as fitted");
        whole.bay = Bay::Empty;
        h.apply(Update::Hangar(HangarView::of(&whole, 0)));
        assert!(!h.armed(), "no suit in the bay");
        // A time of their own on the Proving Ground's board.
        let mut board = bc_econ::proving::Board::default().view("", 0);
        h.apply(Update::Proving(board.clone()));
        assert!(!h.has_certificate());
        board.mine.drill_ms = Some(60_000);
        h.apply(Update::Proving(board.clone()));
        assert!(h.has_certificate());
        board.mine = bc_econ::proving::Bests { course_ms: Some(150_000), drill_ms: None };
        h.apply(Update::Proving(board));
        assert!(h.has_certificate(), "the course counts as well");
        // The Charter Board's word.
        let mut charter = bc_econ::Board::new().view("a", 0);
        charter.standing = 120;
        charter.mine_aces = 2;
        h.apply(Update::Charter(charter.clone()));
        assert_eq!((h.standing(), h.aces_downed()), (120, 2));
        assert!(!h.board_holds());
        charter.waiting = Some(bc_econ::charter::Waiting {
            credits: 0,
            goods: vec![(Item::Weapon(bc_proto::WeaponKind::BeamRifle), 1)],
        });
        h.apply(Update::Charter(charter.clone()));
        assert!(h.board_holds(), "a certificate's rifle at its desk");
        charter.waiting = None;
        h.apply(Update::Charter(charter));
        assert!(!h.board_holds(), "handed over");
        // Siderium: a job making it, or some in the stores.
        let siderium = Item::Material(Material::Siderium);
        let mut view = HangarView::of(&starter, 0);
        view.jobs.push(wire::JobView {
            station: bc_econ::Station::Foundry,
            item: siderium,
            batches: 1,
            done: 0,
            secs: 600,
            secs_left: 600,
        });
        h.apply(Update::Hangar(view));
        assert!(h.smelting(), "queued at the foundry");
        let mut view = HangarView::of(&starter, 0);
        view.jobs.push(wire::JobView {
            station: bc_econ::Station::Fabricator,
            item: Item::Material(Material::Steel),
            batches: 1,
            done: 0,
            secs: 60,
            secs_left: 60,
        });
        h.apply(Update::Hangar(view.clone()));
        assert!(!h.smelting(), "steel's no siderium");
        view.stock.push((siderium, 0));
        h.apply(Update::Hangar(view.clone()));
        assert!(!h.smelting(), "none of it");
        view.stock.push((siderium, 4));
        h.apply(Update::Hangar(view));
        assert!(h.smelting(), "in the stores");
    }
}
