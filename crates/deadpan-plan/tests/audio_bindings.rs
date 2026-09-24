use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{AudioBoundDomain, AudioQueryLimits, AudioSignalContent, RenderPlan};

#[test]
fn compilation_never_silently_discards_persisted_sampling_clocks() {
    let node = |value: &str| NodeId::new(value).unwrap();
    let mut wire = serde_json::to_value(
        ProjectDocument::new(
            ProjectId::new("binding-admission").unwrap(),
            RevisionId::new("r0").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(30_000, 1001).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            node("root"),
        )
        .unwrap(),
    )
    .unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![node("hold")])).unwrap();
    wire["nodes"]["hold"] = serde_json::to_value(BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: FrameDuration::new(4).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    let unbound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    RenderPlan::compile(&unbound).unwrap();
    let timing = AudioTimingId {
        allocation: RevisionId::new("retained").unwrap(),
        ordinal: 0,
    };
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(&unbound).unwrap(),
        }],
        BTreeMap::from([(
            node("hold"),
            OwnedAudioBinding {
                lattice: AudioPlacementTemplate {
                    reference: AudioReferenceClock {
                        timing,
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: node("hold"),
                    },
                    arguments: vec![],
                    births: vec![],
                },
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ONE,
                    phase: AudioLocalPhase {
                        constant: ExactRatio::new(3, 7).unwrap(),
                        terms: vec![],
                    },
                }),
            },
        )]),
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(&bindings).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(bound.audio_bindings(), &bindings);
    let plan = RenderPlan::compile(&bound).unwrap();
    assert!(plan.has_audio_bindings());
    let query = plan
        .audio_processing(
            AudioSample(1602)..AudioSample(1603),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let AudioSignalContent::Bound(operand) = &query.spans[0].content else {
        panic!("binding was dropped")
    };
    assert!(operand.belongs_to(&plan));
    assert_eq!(
        operand.reference_at_offset(1602).unwrap(),
        ExactRatio::new(3432, 5).unwrap()
    );
    assert!(matches!(
        operand.raw_domain().unwrap(),
        AudioBoundDomain::Root(_)
    ));
    assert!(query.work > operand.work());
    let policy = plan
        .audio_policy(
            AudioSample(1602)..AudioSample(1603),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        policy.suppressed,
        vec![AudioSample(1602)..AudioSample(1603)]
    );
    assert!(
        plan.audio_policy(
            AudioSample(1602)..AudioSample(1603),
            AudioQueryLimits {
                maximum_work: 2,
                maximum_spans: 8
            }
        )
        .is_err()
    );
    assert!(FrozenAudioContext::capture(&bound).is_err());
}
