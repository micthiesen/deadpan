//! Adapter contract tests, intended as a child of policy_prediction_tests.rs.
//! Cached-mask validation does not authenticate a corrupted derived block.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use super::*;
use crate::stages::controlled_reads::{Provider, fixture_document_at_rate};
use deadpan_core::{
    AudioTimingId, ExactFrameRange, FrameRate, NodeId, NodeKind, ProjectDocument,
    SourceAudioMapping, capture_unbound_audio_bindings,
};

fn fixture(inactive_support: bool) -> (Arc<RenderPlan>, Provider) {
    let (document, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    if !inactive_support {
        return (Arc::new(RenderPlan::compile(&document).unwrap()), provider);
    }
    // Use the same real selected-placement/binding construction as the
    // authored-bus exhausted-bound regression, retaining actual owner support.
    let mut wire = serde_json::to_value(&document).unwrap();
    let mut leaf = document.nodes()[&NodeId::new("a").unwrap()].clone();
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!("fixture a is a real WAV-backed Source")
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ExactRatio::integer(512),
        selection: ExactFrameRange::new(ExactRatio::integer(100), ExactRatio::integer(150))
            .unwrap(),
    };
    wire["nodes"]["a"] = serde_json::to_value(leaf).unwrap();
    let selected = ProjectDocument::from_json(&wire.to_string()).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            &selected,
            AudioTimingId {
                allocation: RevisionId::new("cached-mask-support").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&bound).unwrap()), provider)
}

fn control<'work>(
    cancelled: &'work AtomicBool,
    work: &'work RefCell<ReadWork>,
) -> WorkControl<'work> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}

fn copy_block(block: &ReadBlock) -> ReadBlock {
    ReadBlock {
        start: block.start,
        samples: block.samples.clone(),
        dependencies: block.dependencies.clone(),
        relative_depth: block.relative_depth,
        suppressed: block.suppressed.clone(),
        exhausted: block.exhausted.clone(),
    }
}

type Mixed = (Vec<[f64; 2]>, Vec<Range<AudioSample>>);

/// Use actual owner clocks/routes. Expected observations are supplied
/// independently, so success is stronger than replaying the adapter's result.
fn evaluate<'plan>(
    renderer: &mut StageAudio,
    owners: &AudioOwnerQuery<'plan>,
    block: &ReadBlock,
    policy: OriginalPolicy<'_, 'plan>,
    selected: bool,
    observed_allowed: &[bool],
    control: WorkControl<'_>,
) -> Result<Mixed, StageAudioError> {
    let range = owners.samples();
    let frames = count(&range)?;
    assert_eq!(observed_allowed.len(), frames as usize);
    let mut mix = SharedMix::new(range.start, frames, None, None, true, None, &[]);
    let routes = owners
        .spans()
        .iter()
        .map(|span| {
            mix.route(
                span.owners(),
                span.samples(),
                (0, Contribution::Original),
                control,
            )
            .map(Some)
        })
        .collect::<Result<Vec<_>, _>>()?;
    mix.begin_prediction()?;
    let reservation = mix
        .resident_frames()?
        .checked_add(u64::from(frames) * 3)
        .unwrap();
    renderer.make_room(reservation, false, control)?;
    renderer.active_frames = renderer.active_frames.checked_add(reservation).unwrap();
    let result = (|| {
        mix.allocate_policy();
        populate(
            &mut mix,
            Inputs {
                original: Some(Original {
                    policy,
                    owners,
                    routes: &routes,
                    id: 0,
                }),
                roots: &[],
                root_range: range.clone(),
                root_ids: &[],
                root_routes: &[],
                beats: &[],
                beat_routes: &[],
            },
            |_| selected,
            control,
        )?;
        mix.allocate();
        for (span, route) in owners.spans().iter().zip(&routes) {
            for at in span.samples().start.0..span.samples().end.0 {
                let offset = usize::try_from(at - range.start.0).unwrap();
                mix.add(
                    route.as_ref().unwrap(),
                    offset,
                    block.samples[offset],
                    0,
                    observed_allowed[offset].then_some(0),
                    control,
                )?;
            }
        }
        mix.verify_prediction(control)?;
        let result = mix.finish(control, |_, _, _, _, _| {
            panic!("fixture has no temporal owner effect")
        })?;
        Ok((result.samples, result.suppressed))
    })();
    // Population may have partially written rows before rejecting a block;
    // the caller discards the complete mix rather than treating it as valid.
    renderer.active_frames -= reservation;
    assert_eq!(renderer.active_frames, 0);
    result
}

#[test]
fn cached_original_policy_rejects_mismatched_window_and_malformed_typed_masks() {
    let (plan, mut provider) = fixture(false);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let owners = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(64), Default::default())
        .unwrap();
    let raw = renderer
        .read_controlled(&mut provider, AudioSample(0), 64, control, true, 0)
        .unwrap();
    assert!(provider.calls > 0);
    assert!(raw.suppressed.is_empty() && raw.exhausted.is_empty());
    let observations = [true; 64];
    for shifted_start in [true, false] {
        let mut invalid = copy_block(&raw);
        if shifted_start {
            invalid.start = AudioSample(1);
        } else {
            invalid.samples.pop();
        }
        let result = evaluate(
            &mut renderer,
            &owners,
            &invalid,
            OriginalPolicy::Cached(&invalid),
            true,
            &observations,
            control,
        );
        assert!(matches!(
            result,
            Err(StageAudioError::Plan(PlanError::InvalidPlan(
                "cached Original prediction range differs"
            )))
        ));
    }
    for exhausted in [false, true] {
        for invalid_range in [
            AudioSample(-1)..AudioSample(1),
            AudioSample(63)..AudioSample(65),
            AudioSample(12)..AudioSample(12),
            AudioSample(20)..AudioSample(19),
            AudioSample(i64::MIN)..AudioSample(i64::MAX),
        ] {
            let mut invalid = copy_block(&raw);
            // A valid interval precedes the malformed one. No partial result
            // may be consumed, regardless of which mask vector rejects it.
            let masks = vec![AudioSample(4)..AudioSample(8), invalid_range];
            if exhausted {
                invalid.exhausted = masks;
            } else {
                invalid.suppressed = masks;
            }
            let result = evaluate(
                &mut renderer,
                &owners,
                &invalid,
                OriginalPolicy::Cached(&invalid),
                true,
                &observations,
                control,
            );
            assert!(matches!(result, Err(StageAudioError::Range)));
        }
    }
}

#[test]
fn cached_original_policy_uses_mask_union_and_exhaustion_independently() {
    let (plan, mut provider) = fixture(false);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let owners = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(64), Default::default())
        .unwrap();
    assert!(
        owners
            .spans()
            .iter()
            .all(|span| span.support() == AudioOwnerSupport::Active)
    );
    let raw = renderer
        .read_controlled(&mut provider, AudioSample(0), 64, control, true, 0)
        .unwrap();
    for include_suppressed in [false, true] {
        let mut block = copy_block(&raw);
        block.suppressed = if include_suppressed {
            vec![
                AudioSample(8)..AudioSample(16),
                AudioSample(12)..AudioSample(24),
                AudioSample(24)..AudioSample(32),
                AudioSample(8)..AudioSample(16),
            ]
        } else {
            Vec::new()
        };
        block.exhausted = vec![AudioSample(40)..AudioSample(48)];
        // Literal expected union, independent of adapter traversal/merging.
        let allowed: Vec<_> = (0..64)
            .map(|at| !(40..48).contains(&at) && !(include_suppressed && (8..32).contains(&at)))
            .collect();
        for (sample, allowed) in block.samples.iter_mut().zip(&allowed) {
            if !allowed {
                *sample = [0.0; 2];
            }
        }
        let (samples, suppressed) = evaluate(
            &mut renderer,
            &owners,
            &block,
            OriginalPolicy::Cached(&block),
            true,
            &allowed,
            control,
        )
        .unwrap();
        assert_eq!(
            samples,
            block
                .samples
                .iter()
                .map(|sample| sample.map(f64::from))
                .collect::<Vec<_>>()
        );
        let expected = if include_suppressed {
            vec![
                AudioSample(8)..AudioSample(32),
                AudioSample(40)..AudioSample(48),
            ]
        } else {
            vec![AudioSample(40)..AudioSample(48)]
        };
        assert_eq!(suppressed, expected);
    }
}

#[test]
fn cached_original_policy_keeps_selection_and_actual_owner_support_separate() {
    let (plan, mut provider) = fixture(true);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let range = AudioSample(96)..AudioSample(160);
    let owners = plan
        .audio_gain_owners(range.clone(), Default::default())
        .unwrap();
    assert!(
        owners
            .spans()
            .iter()
            .any(|span| span.support() == AudioOwnerSupport::Inactive)
    );
    assert!(
        owners
            .spans()
            .iter()
            .any(|span| span.support() == AudioOwnerSupport::Active)
    );
    let mut raw = renderer
        .read_controlled(&mut provider, range.start, 64, control, true, 0)
        .unwrap();
    // Narrow private-fixture perturbation: remove cached exclusions so only
    // current owner support can reject inactive spans. This is not a claim
    // that a modified derived block could enter the authenticated cache.
    raw.suppressed.clear();
    raw.exhausted.clear();
    for selected in [false, true] {
        let allowed: Vec<_> = (96..160)
            .map(|at| {
                selected
                    && owners.spans().iter().any(|span| {
                        span.samples().contains(&AudioSample(at))
                            && span.support() == AudioOwnerSupport::Active
                    })
            })
            .collect();
        let (samples, suppressed) = evaluate(
            &mut renderer,
            &owners,
            &raw,
            OriginalPolicy::Cached(&raw),
            selected,
            &allowed,
            control,
        )
        .unwrap();
        if selected {
            assert!(allowed.iter().any(|bit| *bit));
            assert!(allowed.iter().any(|bit| !bit));
            assert!(samples.iter().any(|sample| *sample != [0.0; 2]));
        } else {
            assert_eq!(samples, vec![[0.0; 2]; 64]);
            assert_eq!(suppressed, vec![range.clone()]);
        }
    }
}

#[test]
fn identical_valid_mask_corruption_needs_an_independent_query_witness() {
    let (plan, mut provider) = fixture(false);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let request = renderer
        .prepare_original_root(&plan, AudioSample(0), 64, control, true)
        .unwrap();
    let owners = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(64), Default::default())
        .unwrap();
    let mut corrupt = renderer
        .read_controlled(&mut provider, AudioSample(0), 64, control, true, 0)
        .unwrap();
    assert!(corrupt.suppressed.is_empty() && corrupt.exhausted.is_empty());
    corrupt.suppressed.push(AudioSample(16)..AudioSample(24));
    corrupt.samples[16..24].fill([0.0; 2]);
    let observations: Vec<_> = (0..64).map(|at| !(16..24).contains(&at)).collect();
    // Both warm prediction and numeric producer observations now consume the
    // same wrong but well-formed mask. Their agreement cannot detect it.
    let (_, suppressed) = evaluate(
        &mut renderer,
        &owners,
        &corrupt,
        OriginalPolicy::Cached(&corrupt),
        true,
        &observations,
        control,
    )
    .unwrap();
    assert_eq!(suppressed, vec![AudioSample(16)..AudioSample(24)]);
    // Preserve the independently prepared OLD queries. Their policy still
    // permits these samples and rejects the identically corrupt observation.
    let independent = evaluate(
        &mut renderer,
        &owners,
        &corrupt,
        OriginalPolicy::Queries(request.queries()),
        true,
        &observations,
        control,
    );
    assert!(matches!(
        independent,
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "contribution permission prediction differs"
        )))
    ));
}
