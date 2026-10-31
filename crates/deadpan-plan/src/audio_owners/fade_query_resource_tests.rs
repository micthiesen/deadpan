//! Actual phase brackets supply independent requested-Layout overlap checks.
use super::*;
use crate::plan::audio_fades::query::trace::{self, Event, Phase};
use std::cell::RefCell;

#[derive(Clone, Copy, Debug)]
enum Entry {
    Event {
        event: Event,
        live: usize,
        forwarded: usize,
    },
    Reservation {
        size: usize,
        align: usize,
        before: usize,
        after: usize,
    },
}

#[derive(Clone, Copy)]
enum Fault {
    RefuseEnd,
    RotateEnd,
}

struct Active {
    state: Rc<RefCell<PolicyState>>,
    entries: Vec<Entry>,
    fault: Option<Fault>,
    armed: bool,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

struct Session;
impl Session {
    fn start(state: Rc<RefCell<PolicyState>>, fault: Option<Fault>) -> Self {
        ACTIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none());
            *slot = Some(Active {
                state,
                entries: Vec::new(),
                fault,
                armed: false,
            });
        });
        Self
    }

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

fn observe(event: Event) {
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let active = slot.as_mut().unwrap();
        let (live, forwarded) = {
            let state = active.state.borrow();
            (state.live, state.forwarded)
        };
        active.entries.push(Entry::Event {
            event,
            live,
            forwarded,
        });
        if event.phase == Phase::InitialEndBefore
            && !active.armed
            && let Some(fault) = active.fault
        {
            active.armed = true;
            let mut state = active.state.borrow_mut();
            let index = state.reservations + 1;
            match fault {
                Fault::RefuseEnd => state.refuse_reservation = Some(index),
                Fault::RotateEnd => state.rotate_reservation = Some(index),
            }
        }
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
        let state = Rc::clone(&self.0.0);
        let before = state.borrow().live;
        let lease = self.0.reserve(layout)?;
        let after = state.borrow().live;
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
                });
        });
        Ok(lease)
    }

    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.0.merge(target, source);
    }
}

fn traced(
    parent: Parent<'_, '_>,
    row: &Value,
    policy: &mut Policy<true>,
    fault: Option<Fault>,
) -> (
    Result<AdmittedAudioFadeQuery<usize, Lease>, PlanError>,
    Vec<Entry>,
) {
    let session = Session::start(Rc::clone(&policy.0), fault);
    let result = trace::with_observer(observe, || {
        admitted(
            parent,
            samples(&row["request"]["samples"]),
            Default::default(),
            &mut Recording(policy),
        )
    });
    (result, session.finish())
}

fn pair(entries: &[Entry], index: usize, after: Phase) -> (Event, usize, &[Entry]) {
    let end = entries[index + 1..]
        .iter()
        .position(|entry| matches!(entry, Entry::Event { event, .. } if event.phase == after))
        .map(|offset| index + 1 + offset)
        .expect("actual after phase");
    let Entry::Event { event, live, .. } = entries[end] else {
        unreachable!()
    };
    (event, live, &entries[index + 1..end])
}

fn one_layout(entries: &[Entry], layout: Layout, live: usize) {
    let allocations: Vec<_> = entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Reservation {
                size,
                align,
                before,
                after,
            } => Some((*size, *align, *before, *after)),
            Entry::Event { .. } => None,
        })
        .collect();
    assert_eq!(
        allocations,
        vec![(layout.size(), layout.align(), live, live + layout.size())]
    );
}

#[test]
fn frozen_full_parents_reach_actual_leaf_stage_bound_and_both_retained_grids() {
    let mut phases = std::collections::BTreeSet::new();
    visit(|parent, case| {
        let mut policy = Policy::<true>::new(usize::MAX);
        let (result, entries) = traced(parent, full(case), &mut policy, None);
        let value = result.unwrap();
        assert_true(&value, &full(case)["output"]["query"], &policy);
        for entry in entries {
            if let Entry::Event { event, .. } = entry {
                phases.insert(format!("{:?}", event.phase));
            }
        }
        drop(value);
        assert_eq!(policy.0.borrow().live, 0);
    });
    for expected in [
        "PrimaryLeaf",
        "PrimaryStage",
        "PrimaryBound",
        "EnvelopeRootRoundEven",
        "EnvelopePointCeil",
        "RetainedGeometryLeaf",
        "OriginMergeReady",
    ] {
        assert!(phases.contains(expected), "missing {expected}: {phases:?}");
    }
}

#[test]
fn initial_sides_and_replacement_buffers_keep_actual_old_and_new_layouts_live() {
    let mut initial = 0;
    let mut origins = 0;
    let mut sides = 0;
    let mut coincident = 0;
    visit(|parent, case| {
        let mut policy = Policy::<true>::new(usize::MAX);
        let (result, entries) = traced(parent, full(case), &mut policy, None);
        let value = result.unwrap();
        assert_true(&value, &full(case)["output"]["query"], &policy);
        for (index, entry) in entries.iter().enumerate() {
            let Entry::Event { event, live, .. } = *entry else {
                continue;
            };
            match event.phase {
                Phase::InitialStartBefore | Phase::InitialEndBefore => {
                    let next = if event.phase == Phase::InitialStartBefore {
                        Phase::InitialStartAfter
                    } else {
                        Phase::InitialEndAfter
                    };
                    let (after, after_live, between) = pair(&entries, index, next);
                    let layout = Layout::array::<crate::AudioFadeEdge>(1).unwrap();
                    assert_eq!((event.old_len, after.new_len), (0, 1));
                    assert!(live > 0, "the actual primary walk remains charged");
                    one_layout(between, layout, live);
                    assert_eq!(after_live, live + layout.size());
                    if event.phase == Phase::InitialEndBefore {
                        let previous = entries[..index]
                            .iter()
                            .rev()
                            .find_map(|entry| match entry {
                                Entry::Event { event, live, .. } => Some((event.phase, *live)),
                                Entry::Reservation { .. } => None,
                            })
                            .unwrap();
                        assert_eq!(previous, (Phase::InitialStartAfter, live));
                    }
                    initial += 1;
                }
                Phase::OriginGrowthBefore | Phase::SideGrowthBefore => {
                    let origin = event.phase == Phase::OriginGrowthBefore;
                    let next = if origin {
                        Phase::OriginGrowthAfter
                    } else {
                        Phase::SideGrowthAfter
                    };
                    let (after, after_live, between) = pair(&entries, index, next);
                    let (old_layout, new_layout) = if origin {
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
                    one_layout(between, new_layout, live);
                    assert_eq!(after_live, live + new_layout.size() - old_layout.size());
                    assert!(policy.0.borrow().peak >= live + new_layout.size());
                    if origin {
                        origins += 1;
                    } else {
                        sides += 1;
                    }
                }
                Phase::OriginMergeBefore => {
                    let (ready, ready_live, _) = pair(&entries, index, Phase::OriginMergeReady);
                    let (_, after_live, _) = pair(&entries, index, Phase::OriginMergeAfter);
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
                        coincident += 1;
                    }
                }
                _ => {}
            }
        }
        drop(value);
        assert_eq!(policy.0.borrow().live, 0);
    });
    assert!(initial > 0 && origins > 0 && sides > 0 && coincident > 0);
}

#[test]
fn actual_second_side_refusal_and_rotation_keep_first_side_and_primary_until_failure() {
    let mut checked = 0;
    visit(|parent, case| {
        if case["name"] != "processing_fixture/leaf/source_fit_beat/root" {
            return;
        }
        for fault in [Fault::RefuseEnd, Fault::RotateEnd] {
            let mut policy = Policy::<true>::new(usize::MAX);
            let original = Rc::clone(&policy.0);
            let (result, entries) = traced(parent, full(case), &mut policy, Some(fault));
            match fault {
                Fault::RefuseEnd => assert!(matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
                )),
                Fault::RotateEnd => assert!(matches!(
                    result,
                    Err(PlanError::AudioMetadataIssuerMismatch)
                )),
            }
            let (live, forwarded) = entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::Event {
                        event,
                        live,
                        forwarded,
                    } if event.phase == Phase::InitialEndBefore => Some((*live, *forwarded)),
                    _ => None,
                })
                .unwrap();
            let start_live = entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::Event { event, live, .. }
                        if event.phase == Phase::InitialStartBefore =>
                    {
                        Some(*live)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                live,
                start_live + Layout::array::<crate::AudioFadeEdge>(1).unwrap().size()
            );
            assert!(start_live > 0);
            assert!(!entries.iter().any(|entry| matches!(entry, Entry::Event { event, .. } if event.phase == Phase::InitialEndAfter)));
            assert_eq!(original.borrow().forwarded, forwarded + 1);
            assert_eq!(original.borrow().live, 0);
            assert_eq!(policy.0.borrow().live, 0);
            if matches!(fault, Fault::RotateEnd) {
                assert!(!Rc::ptr_eq(&original, &policy.0));
                assert_eq!(policy.0.borrow().forwarded, 0);
            }
            checked += 1;
        }
    });
    assert_eq!(checked, 2);
}
