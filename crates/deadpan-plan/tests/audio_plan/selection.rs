use super::*;
use deadpan_plan::{AudioDefinitionSelector, AudioRootPlacement, AudioSignalContent, SignalSample};

fn selected_document() -> ProjectDocument {
    document(
        FrameRate::new(48_000, 1).unwrap(),
        vec![id("source")],
        BTreeMap::from([(
            id("source"),
            source(
                500,
                SourceAudioMapping::SelectedPlacement {
                    start: ratio(-11, 7),
                    frames: ExactRatio::integer(48_000),
                    selection: ExactFrameRange::new(ratio(101, 7), ratio(2001, 7)).unwrap(),
                },
                -3,
            ),
        )]),
        BTreeMap::new(),
    )
}

#[test]
fn source_selection_separates_affine_phase_filter_support_and_allocation() {
    let doc = selected_document();
    let plan = RenderPlan::compile(&doc).unwrap();
    let q = query(&plan, 0, 500);
    assert_eq!(q.spans.len(), 3);
    assert_eq!(q.spans[1].allocated_samples, samples(11, 283));
    assert_eq!(q.spans[1].envelope_extent, ratio(80, 7)..ratio(1980, 7));
    assert!(matches!(
        q.spans[0].content,
        AudioContent::Silence {
            reason: SilenceReason::OutsideSourceSelection
        }
    ));
    let AudioContent::Source {
        start,
        duration,
        support,
        ..
    } = &q.spans[1].content
    else {
        panic!("source")
    };
    assert_eq!(*start, ratio(-32, 7));
    assert_eq!(*duration, ExactRatio::integer(48_000));
    assert_eq!(support.start.ticks, ExactRatio::integer(-47_984));
    assert_eq!(support.end.ticks, ratio(-333_988, 7));
    assert_eq!(
        q.spans[1].source_point(AudioSample(11)).unwrap().ticks,
        ratio(-335_891, 7)
    );
    assert!(
        q.spans[1]
            .boundaries
            .start
            .iter()
            .any(|b| b.kind == AudioBoundaryKind::SourcePlacementStart)
    );
    assert!(
        q.spans[1]
            .boundaries
            .end
            .iter()
            .any(|b| b.kind == AudioBoundaryKind::SourcePlacementEnd)
    );
    let context = FrozenAudioContext::capture(&doc).unwrap();
    let retained = RenderPlan::compile_audio_context(&context).unwrap();
    assert_eq!(query(&retained, 0, 500).spans, q.spans);
}

#[test]
fn source_selection_uses_each_root_or_point_grid_without_rerounding() {
    let doc = selected_document();
    let plan = RenderPlan::compile(&doc).unwrap();
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("source") })
        .unwrap();
    let signal = definition.signal();
    let q = signal
        .query(SignalSample(0)..SignalSample(500), Default::default())
        .unwrap();
    assert_eq!(q.spans[1].samples, SignalSample(12)..SignalSample(283));
    assert!(matches!(
        q.spans[1].content,
        AudioSignalContent::Leaf(AudioContent::Source { .. })
    ));
    let placement = AudioRootPlacement::new(
        ratio(-1, 5),
        ratio(3, 2),
        ExactRatio::ZERO..ExactRatio::integer(500),
    )
    .unwrap();
    let root = definition.in_root_clock(placement.clone()).unwrap();
    let q = root.audio(root.root_samples(), Default::default()).unwrap();
    assert_eq!(q.spans[1].samples, samples(17, 424));
    let point = definition.in_point_clock(placement, ratio(1, 3)).unwrap();
    let point_signal = point.signal();
    let q = point_signal
        .query(
            SignalSample(0)..point_signal.sample_count().unwrap(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(q.spans[1].samples, SignalSample(17)..SignalSample(424));
}
