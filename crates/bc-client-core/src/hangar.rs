//! The pilot's hangar as the server tells it (survival rules): where they are, their bay (credits,
//! stores, the suit, the stations' jobs), the exchange, and what they've been told. The server
//! decides everything; this keeps its latest word, and turns what the pilot asks into frames.

use bc_econ::Debrief;
use bc_econ::charter::CharterView;
use bc_econ::exchange::Depth;
use bc_econ::proving::BoardView;
use bc_econ::wire::{self, HangarView, MarketView, Outcome, Place, Request, Update};
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
            Update::Place { place, bay, strip, trainer } => {
                let back = (self.place == Some(Place::Space) && place == Place::Hangar)
                    || (self.place == Some(Place::City) && place != Place::City);
                // Out of a trainer onto the hall's floor, it's still where they came from.
                let from_trainer = self.trainer && place == Place::City;
                (self.place, self.bay, self.strip) = (Some(place), bay, strip);
                self.trainer = trainer || from_trainer;
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
        }
        false
    }

    /// Whether the pilot came into the city out of a trainer (at the Blast Hall's gantry, not down
    /// the lift at Hub Gate), and is still on foot there: what puts them at the gantry's hatch.
    pub fn off_a_trainer(&self) -> bool {
        self.trainer && self.place == Some(Place::City)
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
        let place = |place, strip, trainer| Update::Place { place, bay: 7, strip, trainer };
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
}
