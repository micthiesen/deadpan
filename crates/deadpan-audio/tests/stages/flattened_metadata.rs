use super::*;
use deadpan_plan::{AudioQueryLimits, AudioSoundContextAdmission, PlanError};
use std::alloc::Layout;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Observation {
    pub(super) retained_frames: u64,
    pub(super) peak_frames: u64,
}

#[derive(Default)]
struct Ledger {
    live: Cell<u64>,
    peak: Cell<u64>,
    work: Cell<usize>,
    reservations: Cell<usize>,
}

struct Lease<'a> {
    ledger: &'a Ledger,
    frames: u64,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        self.ledger
            .live
            .set(self.ledger.live.get().checked_sub(self.frames).unwrap());
    }
}

struct Admission<'a>(&'a Ledger);

impl<'a> AudioSoundContextAdmission for Admission<'a> {
    type Error = PlanError;
    type Issuer = *const Ledger;
    type Lease = Lease<'a>;

    fn issuer(&self) -> Self::Issuer {
        self.0
    }

    fn spend(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.0
            .work
            .set(self.0.work.get().checked_add(amount).unwrap());
        Ok(())
    }

    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error> {
        // Rounding each host reservation preserves simultaneous small charges.
        let frames = u64::try_from(layout.size()).unwrap().div_ceil(8);
        let next = self.0.live.get().checked_add(frames).unwrap();
        self.0.live.set(next);
        self.0.peak.set(self.0.peak.get().max(next));
        self.0
            .reservations
            .set(self.0.reservations.get().checked_add(1).unwrap());
        Ok(Lease {
            ledger: self.0,
            frames,
        })
    }

    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease) {
        assert!(std::ptr::eq(target.ledger, source.ledger));
        target.frames = target.frames.checked_add(source.frames).unwrap();
        source.frames = 0;
    }
}

pub(super) fn root(plan: &RenderPlan, samples: Range<AudioSample>) -> Observation {
    let ledger = Ledger::default();
    let query = plan
        .audio_admitted(
            samples.clone(),
            AudioQueryLimits::default(),
            &mut Admission(&ledger),
        )
        .unwrap();
    assert!(query.was_forwarded());
    assert_eq!(ledger.work.get(), query.work());
    assert_eq!(
        query.work(),
        query.canonical_units() + query.additional_units()
    );
    let processing = plan
        .audio_processing_admitted(
            samples,
            AudioQueryLimits::default(),
            &mut Admission(&ledger),
        )
        .unwrap();
    assert!(processing.was_forwarded());
    assert_eq!(ledger.work.get(), query.work() + processing.work());
    assert_eq!(
        processing.work(),
        processing.canonical_units() + processing.additional_units()
    );
    assert!(ledger.reservations.get() > 0);
    let observation = Observation {
        retained_frames: ledger.live.get(),
        peak_frames: ledger.peak.get(),
    };
    assert!(observation.retained_frames > 0);
    assert!(observation.peak_frames >= observation.retained_frames);
    drop((query, processing));
    assert_eq!(ledger.live.get(), 0);
    observation
}
