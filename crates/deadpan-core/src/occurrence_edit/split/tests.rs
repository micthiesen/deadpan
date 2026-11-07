use crate::*;
use std::collections::BTreeMap;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Picture",
        HoldRecipe {
            duration: frames(value),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn fixture() -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("occurrence-split-sound").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: node("group"),
        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    document.nodes = BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("repeat")]),
        ),
        (node("repeat"), repeated),
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("a"), node("b")]),
        ),
        (node("a"), hold(4)),
        (node("b"), hold(6)),
    ])
    .into();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 1000,
            time_base,
        },
    )
    .unwrap();
    let asset = AssetId::new("sound").unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let sound = BeatSound {
        label: "Exact onset".into(),
        source: SourceAudio { asset, span },
        mapping: SourceAudioMapping::natural_rate(span, document.presentation_basis.frame_rate)
            .unwrap(),
        offset: AudioSample(37),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    for owner in ["root", "group"] {
        document.beat_sounds.insert(
            node(owner),
            BTreeMap::from([(SoundId::new(owner).unwrap(), sound.clone())]),
        );
    }
    document.validate().unwrap();
    document
}
fn request(document: &ProjectDocument) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("split"),
        command: Command::EditOccurrence {
            instance: InstancePath {
                node: node("group"),
                repeats: vec![RepeatInstance {
                    node: node("repeat"),
                    iteration: IterationId {
                        allocation: revision("plays"),
                        ordinal: 1,
                    },
                }],
            },
            edit: OccurrenceEdit::Split {
                at: frames(5),
                identities: SplitIdentities {
                    nodes: (0..8).map(|index| node(&format!("cut-{index}"))).collect(),
                },
            },
            identities: OccurrenceIdentities {
                nodes: ["isolated-group", "isolated-a", "isolated-b"]
                    .into_iter()
                    .map(node)
                    .collect(),
                marks: vec![],
            },
        },
    }
}

#[test]
fn occurrence_split_uses_real_root_and_attached_sound_capture_after_isolation() {
    let before = fixture();
    let request = request(&before);
    let transaction = apply(&before, &request).unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    after.validate().unwrap();
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(
        after.nodes()[&node("group")],
        before.nodes()[&node("group")]
    );
    assert_eq!(after.nodes()[&node("a")], before.nodes()[&node("a")]);
    assert_eq!(after.nodes()[&node("b")], before.nodes()[&node("b")]);
    assert_eq!(
        after.beat_sounds()[&node("root")],
        before.beat_sounds()[&node("root")]
    );
    assert_eq!(
        after.beat_sounds()[&node("group")],
        before.beat_sounds()[&node("group")]
    );
    assert_eq!(after.beat_sounds().len(), 4);
    assert_eq!(after.overrides()[&node("repeat")].len(), 1);
    let proof = derive_scoped_isolation(&before, &request, &after).unwrap();
    assert_eq!(proof.record().steps().len(), 1);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

#[test]
fn occurrence_split_checks_both_identity_pools_before_any_isolation() {
    let before = fixture();
    let mut request = request(&before);
    let Command::EditOccurrence {
        edit: OccurrenceEdit::Split { identities, .. },
        ..
    } = &mut request.command
    else {
        unreachable!()
    };
    identities.nodes[0] = node("isolated-group");
    let snapshot = before.to_json().unwrap();
    let error = apply(&before, &request).unwrap_err();
    assert!(error.message.contains("fresh and distinct"), "{error}");
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert!(before.overrides().is_empty());
}

#[test]
fn occurrence_split_retains_shared_processor_history_and_rejects_forged_isolation() {
    let mut before = fixture();
    before.beat_sounds.remove(&node("root"));
    for name in ["prefix-one", "prefix-two"] {
        let transaction = apply(
            &before,
            &CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision(name),
                command: Command::InsertTime {
                    at: ProjectFrame(0),
                    hold: HoldRecipe {
                        duration: frames(1),
                        video: HoldVideo::Background,
                        picture_context: None,
                        audio: HoldAudio::Silence,
                    },
                    id: node(name),
                    identities: SplitIdentities::default(),
                    timing: AudioTimingId {
                        allocation: revision(name),
                        ordinal: 0,
                    },
                },
            },
        )
        .unwrap();
        before = transaction.forward.apply(&before).unwrap();
    }
    let transaction = apply(&before, &request(&before)).unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    after.validate().unwrap();
    let original =
        &after.audio_bindings.sound_clocks[&node("group")][&SoundId::new("group").unwrap()];
    assert_eq!(original.scope(), &node("repeat"));
    assert_eq!(original.clocks().len(), 2);
    for reference in original.clocks() {
        assert_eq!(reference.introduced_isolations().len(), 1);
        assert_eq!(reference.introduced_isolations()[0].iteration.ordinal, 1);
    }
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );

    for (field, value) in [
        ("live_repeat", serde_json::json!("group")),
        ("historical_repeat", serde_json::json!("group")),
        ("live_root", serde_json::json!("group")),
        (
            "iteration",
            serde_json::json!({"allocation":"plays", "ordinal":0}),
        ),
    ] {
        let mut forged = serde_json::to_value(&after).unwrap();
        forged["audio_bindings"]["sound_clocks"]["group"]["group"]["clocks"][0]["introduced_isolations"]
            [0][field] = value;
        assert!(
            ProjectDocument::from_json(&forged.to_string()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn occurrence_split_refuses_invalid_seams_without_consuming_identity_pools() {
    for seam in [0, 10, 11] {
        let before = fixture();
        let mut request = request(&before);
        let Command::EditOccurrence {
            edit: OccurrenceEdit::Split { at, .. },
            identities,
            ..
        } = &mut request.command
        else {
            unreachable!()
        };
        *at = frames(seam);
        identities.nodes.clear();
        let error = apply(&before, &request).unwrap_err();
        assert!(error.message.contains("strictly inside"), "{error}");
        assert!(before.overrides().is_empty());
    }
}
