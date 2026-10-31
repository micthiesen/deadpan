//! Retained copied payloads overlap real side/origin and outer-span replacements.
use super::*;
use crate::plan::audio_fades::query::trace::{self as common, Event, Phase as Common};
use crate::plan::audio_owner_input::retained_fades::trace::{self as retained_trace, Phase};
use std::cell::Cell;

#[derive(Clone, Copy, Debug)]
enum Entry {
    Common {
        event: Event,
        live: usize,
    },
    Retained {
        phase: Phase,
        live: usize,
    },
    Reservation {
        size: usize,
        align: usize,
        before: usize,
        after: usize,
    },
}
struct Active {
    policy: Rc<RefCell<PolicyState>>,
    entries: Vec<Entry>,
    completed_spans: usize,
    processing_bytes: usize,
    editorial_bytes: usize,
}
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static HANDLES: Cell<usize> = const { Cell::new(0) };
    static COVERAGE: RefCell<Coverage> = const { RefCell::new(Coverage::new()) };
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}
#[derive(Debug, Default)]
struct Coverage {
    queries: usize,
    initial: usize,
    origins: usize,
    sides: usize,
    coincident: usize,
    outer_growth: usize,
    outer_prior_growth: usize,
    spare_push: usize,
    repeat_editorial_handles: usize,
}
impl Coverage {
    const fn new() -> Self {
        Self {
            queries: 0,
            initial: 0,
            origins: 0,
            sides: 0,
            coincident: 0,
            outer_growth: 0,
            outer_prior_growth: 0,
            spare_push: 0,
            repeat_editorial_handles: 0,
        }
    }
}
fn common_phase(event: Event) {
    diagnostic::record(event);
    ACTIVE.with(|slot| {
        let mut active = slot.borrow_mut();
        let active = active.as_mut().unwrap();
        let live = active.policy.borrow().live;
        let payload = (active.completed_spans + 1) * active.processing_bytes + active.editorial_bytes;
        assert!(
            live >= payload,
            "actual retained processing and editorial copies remain charged through common geometry/edge phases"
        );
        let origin_layout = |count| Layout::array::<crate::AudioBoundaryOrigin>(count).unwrap().size();
        match event.phase {
            Common::OriginGrowthBefore => assert!(live >= payload + origin_layout(event.old_len)),
            Common::SideGrowthBefore => assert!(
                live >= payload + Layout::array::<crate::AudioFadeEdge>(event.old_len).unwrap().size()
            ),
            Common::OriginMergeReady => assert!(
                live >= payload + origin_layout(event.old_len) + origin_layout(event.new_len)
            ),
            Common::InitialEndBefore => assert!(live >= payload + Layout::new::<crate::AudioFadeEdge>().size()),
            _ => {}
        }
        active.entries.push(Entry::Common { event, live });
    });
}
fn retained_phase(phase: Phase) {
    ACTIVE.with(|slot| {
        let mut active = slot.borrow_mut();
        let active = active.as_mut().unwrap();
        let live = active.policy.borrow().live;
        if let Phase::OuterPushBefore {
            old_len,
            old_capacity,
        } = phase
        {
            assert_eq!(old_len, active.completed_spans);
            let old = Layout::array::<crate::AudioFadeSpan>(old_capacity).unwrap();
            assert!(
                live >= old.size() + (old_len + 1) * active.processing_bytes,
                "old outer capacity, prior copied processing payloads and current span coexist"
            );
        }
        if let Phase::OuterPushAfter {
            new_len,
            new_capacity,
        } = phase
        {
            assert_eq!(new_len, active.completed_spans + 1);
            active.completed_spans = new_len;
            let capacity = Layout::array::<crate::AudioFadeSpan>(new_capacity).unwrap();
            assert!(live >= capacity.size() + new_len * active.processing_bytes);
        }
        active.entries.push(Entry::Retained { phase, live });
    });
}
struct Recording<'a>(&'a mut Policy<true>);
impl AudioSoundContextAdmission for Recording<'_> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.0.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.0.spend(units)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let before = self.0.0.borrow().live;
        let lease = self.0.reserve(layout)?;
        let after = self.0.0.borrow().live;
        ACTIVE.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .unwrap()
                .entries
                .push(Entry::Reservation {
                    size: layout.size(),
                    align: layout.align(),
                    before,
                    after,
                })
        });
        Ok(lease)
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.0.merge(target, source)
    }
}
struct Session;
impl Session {
    fn finish(self) -> Vec<Entry> {
        ACTIVE.with(|slot| slot.borrow_mut().take().unwrap().entries)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        ACTIVE.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
fn reservations(entries: &[Entry]) -> Vec<(usize, usize, usize, usize)> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Reservation {
                size,
                align,
                before,
                after,
            } => Some((*size, *align, *before, *after)),
            _ => None,
        })
        .collect()
}
fn one_layout(entries: &[Entry], layout: Layout, live: usize) {
    assert_eq!(
        reservations(entries),
        vec![(layout.size(), layout.align(), live, live + layout.size())]
    );
}
fn common_pair(entries: &[Entry], index: usize, next: Common) -> (Event, usize, &[Entry]) {
    let end = entries[index + 1..]
        .iter()
        .position(|entry| matches!(entry, Entry::Common { event, .. } if event.phase == next))
        .map(|offset| index + 1 + offset)
        .expect("actual common after phase");
    let Entry::Common { event, live } = entries[end] else {
        unreachable!()
    };
    (event, live, &entries[index + 1..end])
}
fn assert_entries(entries: &[Entry], peak: usize) {
    for (index, entry) in entries.iter().enumerate() {
        match *entry {
            Entry::Common { event, live } => match event.phase {
                Common::InitialStartBefore | Common::InitialEndBefore => {
                    let next = if event.phase == Common::InitialStartBefore {
                        Common::InitialStartAfter
                    } else {
                        Common::InitialEndAfter
                    };
                    let (after, after_live, between) = common_pair(entries, index, next);
                    let layout = Layout::array::<crate::AudioFadeEdge>(1).unwrap();
                    assert_eq!((event.old_len, after.new_len), (0, 1));
                    one_layout(between, layout, live);
                    assert_eq!(after_live, live + layout.size());
                    if event.phase == Common::InitialEndBefore {
                        let prior = entries[..index]
                            .iter()
                            .rev()
                            .find_map(|entry| match entry {
                                Entry::Common { event, live } => Some((event.phase, *live)),
                                _ => None,
                            })
                            .unwrap();
                        assert_eq!(prior, (Common::InitialStartAfter, live));
                    }
                    COVERAGE.with(|counts| counts.borrow_mut().initial += 1);
                }
                Common::OriginGrowthBefore | Common::SideGrowthBefore => {
                    let origin = event.phase == Common::OriginGrowthBefore;
                    let next = if origin {
                        Common::OriginGrowthAfter
                    } else {
                        Common::SideGrowthAfter
                    };
                    let (after, after_live, between) = common_pair(entries, index, next);
                    let (old, new) = if origin {
                        (
                            Layout::array::<crate::AudioBoundaryOrigin>(event.old_len).unwrap(),
                            Layout::array::<crate::AudioBoundaryOrigin>(after.new_len).unwrap(),
                        )
                    } else {
                        (
                            Layout::array::<crate::AudioFadeEdge>(event.old_len).unwrap(),
                            Layout::array::<crate::AudioFadeEdge>(after.new_len).unwrap(),
                        )
                    };
                    assert_eq!(after.old_len, event.old_len);
                    assert!(after.new_len > event.old_len);
                    one_layout(between, new, live);
                    assert_eq!(after_live, live + new.size() - old.size());
                    assert!(peak >= live + new.size());
                    COVERAGE.with(|counts| {
                        let mut counts = counts.borrow_mut();
                        if origin {
                            counts.origins += 1;
                        } else {
                            counts.sides += 1;
                        }
                    });
                }
                Common::OriginMergeBefore => {
                    let (ready, ready_live, _) =
                        common_pair(entries, index, Common::OriginMergeReady);
                    let (_, after_live, _) = common_pair(entries, index, Common::OriginMergeAfter);
                    assert_eq!(ready.old_len, event.old_len);
                    assert!(ready.new_len >= event.new_len);
                    assert!(ready_live >= live);
                    assert_eq!(after_live, ready_live);
                    if event.old_len > 0 && ready.new_len > 0 {
                        let old =
                            Layout::array::<crate::AudioBoundaryOrigin>(event.old_len).unwrap();
                        let new =
                            Layout::array::<crate::AudioBoundaryOrigin>(ready.new_len).unwrap();
                        assert!(ready_live >= old.size() + new.size());
                        COVERAGE.with(|counts| counts.borrow_mut().coincident += 1);
                    }
                }
                _ => {}
            },
            Entry::Retained {
                phase:
                    Phase::OuterPushBefore {
                        old_len,
                        old_capacity,
                    },
                live,
            } => {
                let end = entries[index + 1..]
                    .iter()
                    .position(|entry| {
                        matches!(
                            entry,
                            Entry::Retained {
                                phase: Phase::OuterPushAfter { .. },
                                ..
                            }
                        )
                    })
                    .map(|offset| index + 1 + offset)
                    .unwrap();
                let Entry::Retained {
                    phase:
                        Phase::OuterPushAfter {
                            new_len,
                            new_capacity,
                        },
                    live: after_live,
                } = entries[end]
                else {
                    unreachable!()
                };
                assert_eq!(new_len, old_len + 1);
                let between = &entries[index + 1..end];
                if new_len > old_capacity {
                    assert_eq!(new_capacity, new_len.max(old_capacity * 2).max(1));
                    let old = Layout::array::<crate::AudioFadeSpan>(old_capacity).unwrap();
                    let new = Layout::array::<crate::AudioFadeSpan>(new_capacity).unwrap();
                    one_layout(between, new, live);
                    assert_eq!(after_live, live + new.size() - old.size());
                    assert!(peak >= live + new.size());
                    COVERAGE.with(|counts| {
                        let mut counts = counts.borrow_mut();
                        counts.outer_growth += 1;
                        counts.outer_prior_growth += usize::from(old_len > 0);
                    });
                } else {
                    assert_eq!(new_capacity, old_capacity);
                    assert_eq!(reservations(between), Vec::new());
                    assert_eq!(after_live, live);
                    COVERAGE.with(|counts| counts.borrow_mut().spare_push += 1);
                }
            }
            _ => {}
        }
    }
}
pub(super) fn observe(
    input: &AudioOwnerInput<'_>,
    retained: &AudioOwnerRetainedInput<'_>,
    rows: &[Value],
) {
    if !ENABLED.get() {
        return;
    }
    HANDLES.set(HANDLES.get() + 1);
    let snapshot = retained.capture_baseline_snapshot(input.context().plan);
    let (processing_bytes, editorial_bytes) = copy_tests::expected(&snapshot);
    assert!(processing_bytes > 0);
    if !snapshot["processing"]["instance"]["repeats"]
        .as_array()
        .unwrap()
        .is_empty()
        && !snapshot["editorial"]["editorial"]
            .as_array()
            .unwrap()
            .is_empty()
    {
        COVERAGE.with(|counts| counts.borrow_mut().repeat_editorial_handles += 1);
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in rows.iter().filter(|row| {
        row["name"].as_str().unwrap().ends_with("/default")
            && row["output"]["result"] == "Ok"
            && !row["output"]["query"]["spans"]
                .as_array()
                .unwrap()
                .is_empty()
    }) {
        let range = &row["request"]["samples"];
        let start = range["start"].as_i64().unwrap();
        let end = range["end"].as_i64().unwrap();
        if !seen.insert((start, end)) {
            continue;
        }
        let mut policy = Policy::<true>::new(usize::MAX);
        ACTIVE.with(|slot| {
            assert!(
                slot.replace(Some(Active {
                    policy: Rc::clone(&policy.0),
                    entries: Vec::new(),
                    completed_spans: 0,
                    processing_bytes,
                    editorial_bytes,
                }))
                .is_none()
            )
        });
        let session = Session;
        let query = retained_trace::with_observer(retained_phase, || {
            common::with_observer(common_phase, || {
                retained.fades_admitted(
                    AudioSample(start)..AudioSample(end),
                    Default::default(),
                    &mut Recording(&mut policy),
                )
            })
        })
        .unwrap();
        let entries = session.finish();
        let actual = fade_query_baseline::query_snapshot(query.query());
        for (name, expected) in row["output"]["query"].as_object().unwrap() {
            if name != "work" {
                assert_eq!(&actual[name], expected);
            }
        }
        assert_eq!(
            query.canonical_units(),
            row["output"]["query"]["work"].as_u64().unwrap() as usize
        );
        assert_eq!(
            query.work(),
            query.canonical_units() + query.additional_units()
        );
        assert_eq!(query.work(), policy.0.borrow().forwarded);
        assert_entries(&entries, policy.0.borrow().peak);
        drop(query);
        assert_eq!(policy.0.borrow().live, 0);
        COVERAGE.with(|counts| counts.borrow_mut().queries += 1);
    }
    assert_eq!(
        retained.capture_baseline_snapshot(input.context().plan),
        snapshot
    );
}
#[test]
fn retained_copied_payloads_overlap_real_side_origin_and_outer_replacements() {
    assert!(!ENABLED.replace(true));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ENABLED.set(false);
        }
    }
    let _reset = Reset;
    HANDLES.set(0);
    COVERAGE.with(|counts| *counts.borrow_mut() = Coverage::new());
    let original: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix(), original);
    let supplement: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(super::supplement::supplement_matrix(), supplement);
    assert_eq!(HANDLES.get(), 50);
    COVERAGE.with(|counts| assert_eq!(counts.borrow().queries, 594));
    let additional = diagnostic::ordinary_case();
    assert_eq!(additional, diagnostic::frozen_case());
    assert!(
        additional["handles"]
            .as_array()
            .unwrap()
            .iter()
            .all(|handle| handle["result"] == "Some")
    );
    assert_eq!(HANDLES.get(), 54);
    COVERAGE.with(|counts| {
        let counts = counts.borrow();
        assert_eq!(counts.queries, 638);
        assert!(
            counts.initial > 0 && counts.origins > 0 && counts.sides > 0 && counts.coincident > 0,
            "{counts:?}"
        );
        assert!(counts.outer_growth > 0 && counts.outer_prior_growth > 0 && counts.spare_push > 0);
        assert!(counts.repeat_editorial_handles > 0);
    });
}

#[path = "retained_fade_coincident_diagnostic.rs"]
pub(in crate::plan) mod diagnostic;
