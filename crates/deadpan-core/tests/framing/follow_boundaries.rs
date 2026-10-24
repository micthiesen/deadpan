//! Raw and typed structural ingress, separate from Source-only Zoom admission.
use super::*;

#[path = "follow_aggregate.rs"]
mod aggregate;

fn follow() -> Framing {
    Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("speaker").unwrap(),
            scale: ratio(2, 1),
            fallback: FramingPose::new(ratio(1, 3), ratio(2, 7), ratio(1, 2)).unwrap(),
            scale_transition: Some(FollowScaleTransition {
                factor: ratio(2, 1),
                curve: framing_zoom::ZoomCurve::Linear,
            }),
        },
    }
}
fn targeted() -> ProjectDocument {
    let mut value = serde_json::to_value(document()).unwrap();
    let time_base = SourceTimeBase::new(1, 1000).unwrap();
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
    value["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            label: "Video".into(),
            content_hash: "a".repeat(64),
            video: Some(span),
            audio: None,
            still_image: false,
            frame_count: Some(duration(30)),
            source_qualification: None,
        },
    )]))
    .unwrap();
    value["targets"] = serde_json::to_value(BTreeMap::from([(
        TargetId::new("speaker").unwrap(),
        AttentionTarget {
            label: "Speaker".into(),
            asset: AssetId::new("video").unwrap(),
            span,
            region: TargetRegion {
                center: [200_000, 500_000],
                size: [100_000, 100_000],
            },
            samples: Vec::new(),
            corrections: Vec::new(),
            provenance: None,
            display_aspect: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}
fn invalid_follow() -> Framing {
    let mut value = follow();
    if let FramingValue::Follow { fallback, .. } = &mut value.value {
        fallback.scale = ratio(40, 1); // Live 2→4 is valid; fallback 40→80 is not.
    }
    value
}
fn assert_framing_refusal(error: impl std::fmt::Display) {
    let message = error.to_string();
    assert!(message.contains("framing"), "{message}");
}

#[test]
fn raw_command_subtree_slice_and_stored_patch_validate_transition_before_publication() {
    let before = targeted();
    let invalid = invalid_follow();
    let command = Command::SetFraming {
        node: id("hold"),
        framing: Some(invalid.clone()),
    };
    let raw = serde_json::to_string(&command).unwrap();
    let command: Command = serde_json::from_str(&raw).unwrap();
    assert_framing_refusal(apply(&before, &request(&before, command)).unwrap_err());
    let mut child = hold(2);
    child.framing = Some(invalid.clone());
    let subtree = Subtree {
        root: id("new-hold"),
        nodes: BTreeMap::from([(id("new-hold"), child)]),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    };
    let raw = serde_json::to_string(&Command::Insert {
        parent: id("root"),
        index: 1,
        subtree,
    })
    .unwrap();
    let command: Command = serde_json::from_str(&raw).unwrap();
    assert_framing_refusal(apply(&before, &request(&before, command)).unwrap_err());
    // A fully valid raw Hold Follow demonstrates generic owner support.
    let framed = edit(
        &before,
        Command::SetFraming {
            node: id("hold"),
            framing: Some(follow()),
        },
    );
    let slice = CapturedEditSlice::capture(
        &framed,
        &id("root"),
        FrameRange::new(ProjectFrame(0), ProjectFrame(8)).unwrap(),
        AudioTimingId {
            allocation: RevisionId::new("capture-follow-boundary").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut raw = serde_json::to_value(&slice).unwrap();
    raw["nodes"]["hold"]["framing"] = serde_json::to_value(&invalid).unwrap();
    assert_framing_refusal(CapturedEditSlice::from_json(&raw.to_string()).unwrap_err());
    let tx = apply(
        &before,
        &request(
            &before,
            Command::SetFraming {
                node: id("hold"),
                framing: Some(follow()),
            },
        ),
    )
    .unwrap();
    let mut patch = tx.forward.clone();
    patch
        .nodes
        .get_mut(&id("hold"))
        .unwrap()
        .after
        .as_mut()
        .unwrap()
        .framing = Some(invalid.clone());
    let patch: DocumentPatch =
        serde_json::from_str(&serde_json::to_string(&patch).unwrap()).unwrap();
    assert_framing_refusal(patch.apply(&before).unwrap_err());
    // Undo's stored before image is independently validated too.
    let mut patch = tx.forward;
    patch
        .nodes
        .get_mut(&id("hold"))
        .unwrap()
        .before
        .as_mut()
        .unwrap()
        .framing = Some(invalid);
    assert_framing_refusal(patch.apply(&before).unwrap_err());
    assert_eq!(before, targeted());
}

#[test]
fn mixed_and_escaped_follow_transition_documents_keep_wire_counts_and_closed_fields() {
    let before = edit(
        &targeted(),
        Command::SetFraming {
            node: id("hold"),
            framing: Some(follow()),
        },
    );
    let raw = before.to_json().unwrap();
    let escaped = raw.replace("scale_transition", r"sc\u0061le_transition");
    assert_eq!(ProjectDocument::from_json(&escaped).unwrap(), before);
    for field in ["segments", "control1"] {
        let mut raw = serde_json::to_value(&before).unwrap();
        raw["nodes"]["hold"]["framing"]["value"]["scale_transition"][field] = json!([]);
        assert!(ProjectDocument::from_json(&raw.to_string()).is_err());
    }
}

#[test]
fn inserted_subtree_rejects_real_aggregate_transition_record_overflow_before_clone() {
    let before = targeted();
    let mut child = hold(1);
    child.framing = Some(follow());
    let count = MAX_FRAMING_RECORDS / 2 + 1;
    let mut nodes: BTreeMap<_, _> = (0..count)
        .map(|n| (id(&format!("incoming-{n}")), child.clone()))
        .collect();
    let children = nodes.keys().cloned().collect();
    nodes.insert(
        id("incoming-root"),
        BeatNode::sequence("Incoming", children),
    );
    let command = Command::Insert {
        parent: id("root"),
        index: 1,
        subtree: Subtree {
            root: id("incoming-root"),
            nodes,
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    };
    let error = apply(&before, &request(&before, command)).unwrap_err();
    assert_framing_refusal(error);
    assert_eq!(before, targeted());
}
