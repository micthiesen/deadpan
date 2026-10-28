use super::*;
use deadpan_core::{AudioEdgePolicy, NodeId};
use deadpan_plan::{
    AudioBoundaries, AudioBoundaryKind, AudioBoundaryOrigin, AudioBoundaryRule, AudioEnvelope,
    AudioFadeEdge, AudioFadeSpan, AudioSampleGrid, AudioSampleMap, AudioTransform,
};
use std::sync::atomic::Ordering;

fn span(start: i64, end: i64, length: u64, progress: i128, silent_hold: bool) -> AudioSpan {
    let instance = InstancePath {
        node: NodeId::new("voice").unwrap(),
        repeats: Vec::new(),
    };
    let boundary = |kind| AudioBoundaryOrigin {
        instance: instance.clone(),
        gap_after: None,
        kind,
        policy: AudioEdgePolicy::Automatic,
        placement_support: false,
    };
    AudioSpan {
        definition: None,
        samples: AudioSample(start)..AudioSample(end),
        allocated_samples: AudioSample(start)..AudioSample(end),
        project_extent: ExactRatio::integer(start)..ExactRatio::integer(end),
        envelope_extent: ExactRatio::ZERO..ExactRatio::integer(length as i64),
        envelope_samples: AudioSample(0)..AudioSample(length as i64),
        envelope: AudioEnvelope::new(length, progress, AudioSample(start)).unwrap(),
        boundaries: AudioBoundaries {
            start: vec![boundary(AudioBoundaryKind::NodeStart)],
            end: vec![boundary(AudioBoundaryKind::NodeEnd)],
        },
        instance,
        gap_after: None,
        transform: AudioTransform {
            project_origin: ExactRatio::ZERO,
            project_frames_per_local_frame: ExactRatio::ONE,
            project_frames_per_sample: ExactRatio::ONE,
        },
        grid: AudioSampleGrid::new(
            ExactRatio::ZERO,
            ExactRatio::ONE,
            AudioBoundaryRule::RoundEven,
        )
        .unwrap(),
        sampling: AudioSampleMap::new(AudioSample(0), ExactRatio::ZERO, ExactRatio::ONE).unwrap(),
        retimes: Vec::new(),
        content: AudioContent::Silence {
            reason: if silent_hold {
                SilenceReason::SilentHold
            } else {
                SilenceReason::NoSourceAudio
            },
        },
    }
}

fn queries(
    start: i64,
    end: i64,
    spans: Vec<AudioSpan>,
    suppressed: Vec<Range<AudioSample>>,
) -> RootReadQueries<'static> {
    let project_id = ProjectId::new("raw-policy").unwrap();
    let revision_id = RevisionId::new("before").unwrap();
    RootReadQueries {
        flattened: AudioQuery {
            project_id: project_id.clone(),
            revision_id: revision_id.clone(),
            samples: AudioSample(start)..AudioSample(end),
            spans,
            lookup: Default::default(),
            work: 0,
        },
        processing: AudioProcessingQuery {
            project_id,
            revision_id,
            samples: AudioSample(start)..AudioSample(end),
            spans: Vec::new(),
            lookup: Default::default(),
            work: 0,
        },
        policy: AudioPolicyQuery {
            suppressed,
            contents: Vec::new(),
            lookup: Default::default(),
            work: 0,
        },
        fades: None,
    }
}

fn control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

#[derive(Debug, PartialEq)]
enum Seen {
    Hold(Range<AudioSample>),
    Envelope(Range<AudioSample>),
    Exhausted(Range<AudioSample>),
    Policy(Range<AudioSample>),
}

fn observe(event: Event<'_>, out: &mut Vec<Seen>) {
    out.push(match event {
        Event::SilentHold { range, .. } => Seen::Hold(range),
        Event::RetainedEnvelope { span, .. } => Seen::Envelope(span.samples.clone()),
        Event::Exhausted(range) => Seen::Exhausted(range),
        Event::Suppressed { range, .. } => Seen::Policy(range),
    });
}

#[test]
fn literal_event_order_masks_and_pcm_keep_exact_retained_boundaries() {
    let queries = queries(
        10,
        16,
        vec![span(10, 14, 2, -1, false), span(14, 16, 1, i128::MAX, true)],
        vec![AudioSample(11)..AudioSample(12)],
    );
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut seen = Vec::new();
    visit(
        queries.as_borrowed(),
        AudioSample(10),
        6,
        control,
        || control.spend_plan_work(1),
        |event| {
            observe(event, &mut seen);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        seen,
        vec![
            Seen::Envelope(AudioSample(10)..AudioSample(14)),
            Seen::Exhausted(AudioSample(10)..AudioSample(11)),
            Seen::Exhausted(AudioSample(13)..AudioSample(14)),
            Seen::Hold(AudioSample(14)..AudioSample(16)),
            Seen::Policy(AudioSample(11)..AudioSample(12)),
        ]
    );
    assert_eq!(work.borrow().plan_work, 8);
    let mut pcm = [[1.0, -0.0]; 6];
    let applied = apply(queries.as_borrowed(), AudioSample(10), &mut pcm, control).unwrap();
    assert_eq!(
        work.borrow().plan_work,
        8,
        "the production replacement adds no planning charges"
    );
    assert_eq!(
        applied.suppressed,
        vec![
            AudioSample(14)..AudioSample(16),
            AudioSample(11)..AudioSample(12)
        ]
    );
    assert_eq!(
        applied.exhausted,
        vec![
            AudioSample(10)..AudioSample(11),
            AudioSample(13)..AudioSample(14)
        ]
    );
    let expected = [
        [0.0, 0.0],
        [0.0, 0.0],
        [1.0, -0.0],
        [0.0, 0.0],
        [0.0, 0.0],
        [0.0, 0.0],
    ];
    for (actual, expected) in pcm.into_iter().zip(expected) {
        assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
    }
}

#[test]
fn later_invalid_policy_range_cannot_zero_an_earlier_valid_range() {
    let queries = queries(
        10,
        14,
        vec![span(10, 14, 4, 0, false)],
        vec![
            AudioSample(10)..AudioSample(11),
            AudioSample(9)..AudioSample(10),
        ],
    );
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut pcm = [[0.75, -0.5]; 4];
    let before = pcm;
    assert!(matches!(
        apply(
            queries.as_borrowed(),
            AudioSample(10),
            &mut pcm,
            control(&cancelled, &work)
        ),
        Err(StageAudioError::Range)
    ));
    assert_eq!(pcm, before);
    let mut seen = Vec::new();
    let control = control(&cancelled, &work);
    assert!(matches!(
        visit(
            queries.as_borrowed(),
            AudioSample(10),
            4,
            control,
            || control.spend_plan_work(1),
            |event| {
                observe(event, &mut seen);
                Ok(())
            },
        ),
        Err(StageAudioError::Range)
    ));
    assert_eq!(seen, vec![Seen::Envelope(AudioSample(10)..AudioSample(14))]);
    assert_eq!(
        work.borrow().plan_work,
        4,
        "both validations, including the refused record, stay billed"
    );
}

#[test]
fn retained_callback_error_precedes_exhaustion_math_and_policy_validation() {
    let queries = queries(
        0,
        2,
        vec![span(0, 2, 1, i128::MAX, false)],
        vec![AudioSample(-1)..AudioSample(0)],
    );
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut events = 0;
    let result = visit(
        queries.as_borrowed(),
        AudioSample(0),
        2,
        control(&cancelled, &work),
        || Ok(()),
        |event| {
            events += 1;
            assert!(matches!(event, Event::RetainedEnvelope { .. }));
            Err(StageAudioError::Preparation(PreparationError::Cancelled))
        },
    );
    assert!(matches!(
        result,
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    assert_eq!(events, 1);
    let mut pcm = [[1.0; 2]; 2];
    assert!(matches!(
        apply(
            queries.as_borrowed(),
            AudioSample(0),
            &mut pcm,
            control(&cancelled, &work)
        ),
        Err(StageAudioError::Range)
    ));
    assert_eq!(
        pcm, [[1.0; 2]; 2],
        "retained validation still precedes PCM mutation"
    );
}

#[test]
fn creative_zero_gain_and_policy_contents_do_not_invent_mask_bits() {
    let base = span(0, 2, 2, 0, false);
    let mut queries = queries(0, 2, vec![base.clone()], Vec::new());
    queries.policy.contents.push(AudioContent::Silence {
        reason: SilenceReason::SilentHold,
    });
    queries.fades = Some(AudioFadeQuery {
        spans: vec![AudioFadeSpan {
            samples: AudioSample(0)..AudioSample(2),
            start: vec![AudioFadeEdge {
                at: ExactRatio::ZERO,
                length: 2,
                distance_at_start: ExactRatio::integer(-1),
                origins: base.boundaries.start,
                editorial: true,
            }],
            end: Vec::new(),
        }],
        lookup: Default::default(),
        work: 0,
    });
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut pcm = [[1.0; 2]; 2];
    let applied = apply(
        queries.as_borrowed(),
        AudioSample(0),
        &mut pcm,
        control(&cancelled, &work),
    )
    .unwrap();
    assert!(applied.suppressed.is_empty() && applied.exhausted.is_empty());
    assert_eq!(pcm, [[1.0; 2]; 2]);
    crate::edges::apply_creative_fades(
        AudioSample(0),
        &mut pcm,
        &queries.fades.as_ref().unwrap().spans,
    )
    .unwrap();
    assert_eq!(
        pcm[0], [0.0; 2],
        "creative zero is applied later but is not a raw permission mask"
    );

    queries.flattened.spans.clear();
    let mut seen = Vec::new();
    visit(
        queries.as_borrowed(),
        AudioSample(0),
        2,
        control(&cancelled, &work),
        || Ok(()),
        |event| {
            observe(event, &mut seen);
            Ok(())
        },
    )
    .unwrap();
    assert!(
        seen.is_empty(),
        "an independent occurrence with no Original flattened spans inherits no Original endpoint mask"
    );
}

#[test]
fn cold_billing_cancellation_and_exact_work_refusals_propagate_without_refund() {
    let queries = queries(
        0,
        2,
        vec![span(0, 2, 2, 0, false)],
        vec![AudioSample(0)..AudioSample(1)],
    );
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - 4,
        ..Default::default()
    });
    let control = control(&cancelled, &work);
    visit(
        queries.as_borrowed(),
        AudioSample(0),
        2,
        control,
        || control.spend_plan_work(1),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 3;
    assert!(matches!(
        visit(
            queries.as_borrowed(),
            AudioSample(0),
            2,
            control,
            || control.spend_plan_work(1),
            |_| Ok(())
        ),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    cancelled.store(true, Ordering::Relaxed);
    let mut events = 0;
    assert!(matches!(
        visit(
            queries.as_borrowed(),
            AudioSample(0),
            2,
            control,
            || Ok(()),
            |_| {
                events += 1;
                Ok(())
            }
        ),
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    assert_eq!(events, 0);
}
