use deadpan_core::*;
use deadpan_plan::{AudioQueryLimits, RenderPlan};

#[test]
fn dense_bound_repeat_inventory_uses_shared_work_not_output_span_capacity() {
    let node = |name: &str| NodeId::new(name).unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("dense-policy").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![node("repeat")])).unwrap();
    wire["nodes"]["voice"] = serde_json::to_value(BeatNode::hold(
        "Silence",
        HoldRecipe {
            duration: FrameDuration::new(1).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    wire["nodes"]["repeat"] = serde_json::to_value(BeatNode {
        label: "Dense repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: node("voice"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 256).unwrap(),
            gap: None,
        },
    })
    .unwrap();
    let unbound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bindings = capture_unbound_audio_bindings(
        &unbound,
        AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let limits = AudioQueryLimits {
        maximum_spans: 256,
        maximum_work: 65_536,
    };
    let unbound = RenderPlan::compile(&unbound)
        .unwrap()
        .audio_policy(AudioSample(0)..AudioSample(256), limits)
        .unwrap();
    let bound = RenderPlan::compile(&bound).unwrap();
    let retained = bound
        .audio_policy(AudioSample(0)..AudioSample(256), limits)
        .unwrap();
    assert_eq!(retained.suppressed, unbound.suppressed);
    assert_eq!(retained.suppressed, vec![AudioSample(0)..AudioSample(256)]);
    assert!(retained.work > unbound.work);
    assert!(retained.work <= limits.maximum_work);
    assert!(
        bound
            .audio_policy(
                AudioSample(0)..AudioSample(256),
                AudioQueryLimits {
                    maximum_work: 16,
                    ..limits
                },
            )
            .is_err()
    );
}
