//! The pilot's hangar as the server tells it (survival rules): where they are, their bay (credits,
//! stores, the suit, the stations' jobs), the exchange, and what they've been told. The server
//! decides everything; this keeps its latest word, and turns what the pilot asks into frames.

use bc_econ::exchange::Depth;
use bc_econ::wire::{self, HangarView, MarketView, Outcome, Place, Request, Update};
use bc_proto::control::encode_hangar;

#[derive(Clone, Debug, Default)]
pub struct HangarState {
    /// Where the pilot is (`None`: arcade rules, or not told yet).
    pub place: Option<Place>,
    /// Their bay's number.
    pub bay: u8,
    pub view: Option<HangarView>,
    pub market: Option<MarketView>,
    /// The book (and price history) of the item the pilot is watching.
    pub book: Option<(Depth, Vec<u64>)>,
    /// What to tell the pilot (the text, and whether it was done or refused), oldest first. The
    /// UI takes them.
    pub notes: Vec<(String, bool)>,
    /// Sorties that ended, oldest first. The UI takes them.
    pub sorties: Vec<(Outcome, String)>,
    /// Bumped by every update (the UI redraws when it moves).
    pub version: u64,
}

impl HangarState {
    /// Takes in an update. Whether the pilot just came back from the sector into the hangar.
    pub fn apply(&mut self, update: Update) -> bool {
        self.version += 1;
        match update {
            Update::Place { place, bay } => {
                let back = self.place == Some(Place::Space) && place == Place::Hangar;
                (self.place, self.bay) = (Some(place), bay);
                return back;
            }
            Update::Hangar(view) => self.view = Some(view),
            Update::Market(market) => self.market = Some(market),
            Update::Book { depth, history } => self.book = Some((depth, history)),
            Update::Note { text, ok } => self.notes.push((text, ok)),
            Update::Sortie { outcome, text } => self.sorties.push((outcome, text)),
        }
        false
    }

    /// On foot in the hangar bay.
    pub fn in_hangar(&self) -> bool {
        self.place == Some(Place::Hangar)
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
        assert!(!h.apply(Update::Place { place: Place::Hangar, bay: 7 }));
        assert!(h.in_hangar());
        assert!(!h.apply(Update::Place { place: Place::Space, bay: 7 }));
        assert!(h.apply(Update::Place { place: Place::Hangar, bay: 7 }), "back from the sector");
        h.apply(Update::Note { text: "MADE 80 kg STEEL".into(), ok: true });
        assert_eq!(h.notes.len(), 1);
        assert_eq!(h.version, 4);
    }
}
