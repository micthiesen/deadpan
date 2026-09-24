use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{PlanError, RenderPlan};

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
    assert!(matches!(
        RenderPlan::compile(&bound),
        Err(PlanError::UnsupportedAudioBindings)
    ));
    assert!(FrozenAudioContext::capture(&bound).is_err());
}
