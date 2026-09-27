use deadpan_core::*;
use serde_json::json;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn fixture() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("retime").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = json!(["a", "suffix"]);
    for (name, duration) in [("a", 12), ("suffix", 4)] {
        wire["nodes"][name] = json!({"label":name,"kind":{"type":"hold","recipe":{"duration":duration,"video":{"type":"background"},"audio":{"type":"silence"}}}});
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn request(document: &ProjectDocument, command: Command, revision: &str) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}
fn edit(
    document: &ProjectDocument,
    command: Command,
    revision: &str,
) -> (ProjectDocument, EditTransaction) {
    let tx = apply(document, &request(document, command, revision)).unwrap();
    let after = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    (after, tx)
}
fn wrap(duration: i64, pitch: PitchPolicy) -> Command {
    Command::WrapRetime {
        node: id("a"),
        id: id("rate"),
        duration: frames(duration),
        pitch,
    }
}
fn set(duration: i64, pitch: PitchPolicy) -> Command {
    Command::SetRetime {
        node: id("rate"),
        duration: frames(duration),
        pitch,
    }
}
fn capture(document: &ProjectDocument) -> ProjectDocument {
    let state = capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: document.revision_id().clone(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn wrap_and_update_retain_child_framing_marks_and_exact_mapping() {
    let framing = Framing::creep(
        FramingPose::identity(),
        FramingPose::new(
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(2),
        )
        .unwrap(),
        FramingCurve::Smoothstep,
    )
    .unwrap();
    let (framed, _) = edit(
        &fixture(),
        Command::SetFraming {
            node: id("a"),
            framing: Some(framing.clone()),
        },
        "framed",
    );
    let (marked, _) = edit(
        &framed,
        Command::SetMark {
            id: MarkId::new("content").unwrap(),
            owner: id("a"),
            label: "content".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: id("root"),
                    position: ExactRatio::integer(9),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "marked",
    );
    let (wrapped, tx) = edit(&marked, wrap(8, PitchPolicy::Preserve), "wrapped");
    assert_eq!(tx.duration_delta, -4);
    assert_eq!(wrapped.nodes()[&id("a")], marked.nodes()[&id("a")]);
    assert!(
        matches!(&wrapped.nodes()[&id("rate")].kind, NodeKind::Retime { child, duration, mapping, pitch: PitchPolicy::Preserve, purpose: RetimePurpose::Edit } if child == &id("a") && *duration == frames(8) && mapping.start() == ProjectFrame(0) && mapping.end() == ProjectFrame(12))
    );
    assert_eq!(
        wrapped.marks()[&MarkId::new("content").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ExactRatio::integer(6)
        }
    );
    let (wrapped, _) = edit(
        &wrapped,
        Command::SetFraming {
            node: id("rate"),
            framing: Some(framing.clone()),
        },
        "wrapper-framed",
    );
    let (updated, _) = edit(&wrapped, set(5, PitchPolicy::FollowSpeed), "updated");
    assert_eq!(
        updated.marks()[&MarkId::new("content").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ExactRatio::new(15, 4).unwrap()
        }
    );
    assert_eq!(updated.nodes()[&id("a")], marked.nodes()[&id("a")]);
    assert_eq!(updated.nodes()[&id("rate")].framing, Some(framing));
}

#[test]
fn changed_output_releases_only_its_binding_and_noop_keeps_all_clocks() {
    let (retimed, _) = edit(&fixture(), wrap(8, PitchPolicy::Preserve), "retimed");
    let bound = capture(&retimed);
    assert!(bound.audio_bindings().bindings().contains_key(&id("rate")));
    let (noop, _) = edit(&bound, set(8, PitchPolicy::Preserve), "noop");
    assert_eq!(noop.audio_bindings(), bound.audio_bindings());
    for (duration, pitch) in [
        (6, PitchPolicy::Preserve),
        (8, PitchPolicy::FollowSpeed),
        (12, PitchPolicy::Preserve),
    ] {
        let (changed, tx) = edit(&bound, set(duration, pitch), "changed");
        assert!(
            !changed
                .audio_bindings()
                .bindings()
                .contains_key(&id("rate"))
        );
        for owner in ["a", "suffix"] {
            assert_eq!(
                changed.audio_bindings().bindings()[&id(owner)],
                bound.audio_bindings().bindings()[&id(owner)]
            );
        }
        assert!(tx.changed_ids.contains(&id("rate")));
    }
    let bound = capture(&fixture());
    let (wrapped, _) = edit(&bound, wrap(18, PitchPolicy::Preserve), "wrapped");
    assert_eq!(wrapped.audio_bindings(), bound.audio_bindings());
}

#[test]
fn update_preserves_a_nonzero_child_trim_and_occurrence_update_keeps_other_plays() {
    let (retimed, _) = edit(&fixture(), wrap(8, PitchPolicy::Preserve), "trim-base");
    let mut wire = serde_json::to_value(&retimed).unwrap();
    wire["nodes"]["rate"]["kind"]["mapping"] = json!({"start":2,"end":10});
    let trimmed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let (repeated, _) = edit(
        &trimmed,
        Command::WrapRepeat {
            node: id("rate"),
            id: id("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
        "trim-repeat",
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let selected = iterations.at(1).unwrap();
    let (changed, _) = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("rate"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: selected.clone(),
                }],
            },
            edit: OccurrenceEdit::SetRetime {
                duration: frames(5),
                pitch: PitchPolicy::FollowSpeed,
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated-rate"), id("isolated-child")],
                marks: vec![],
            },
        },
        "trim-changed",
    );
    assert_eq!(changed.nodes()[&id("rate")], repeated.nodes()[&id("rate")]);
    assert!(matches!(&changed.nodes()[&id("isolated-rate")].kind,
        NodeKind::Retime { child, duration, mapping, pitch: PitchPolicy::FollowSpeed, .. }
        if child == &id("isolated-child") && *duration == frames(5)
            && mapping.start() == ProjectFrame(2) && mapping.end() == ProjectFrame(10)));
    assert_eq!(
        changed.overrides()[&id("repeat")].get(&selected),
        Some(&id("isolated-rate"))
    );
}

#[test]
fn partitions_are_wrapped_never_converted_and_bad_requests_are_atomic() {
    let (retimed, _) = edit(&fixture(), wrap(12, PitchPolicy::FollowSpeed), "retimed");
    let mut wire = serde_json::to_value(&retimed).unwrap();
    wire["nodes"]["rate"]["kind"]["purpose"] = json!("partition");
    wire["nodes"]["rate"]["kind"]["mapping"] = json!({"start":3,"end":9});
    wire["nodes"]["rate"]["kind"]["duration"] = json!(6);
    let partition = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        apply(
            &partition,
            &request(&partition, set(3, PitchPolicy::Preserve), "bad")
        )
        .unwrap_err()
        .code,
        EditErrorCode::WrongNodeKind
    );
    let (wrapped, _) = edit(
        &partition,
        Command::WrapRetime {
            node: id("rate"),
            id: id("outer"),
            duration: frames(4),
            pitch: PitchPolicy::Preserve,
        },
        "outer",
    );
    assert_eq!(wrapped.nodes()[&id("rate")], partition.nodes()[&id("rate")]);
    assert!(
        matches!(&wrapped.nodes()[&id("outer")].kind, NodeKind::Retime { mapping, .. } if mapping.duration() == frames(6))
    );
    for command in [
        wrap(0, PitchPolicy::Preserve),
        wrap(i64::MAX, PitchPolicy::FollowSpeed),
        Command::WrapRetime {
            node: id("root"),
            id: id("rate"),
            duration: frames(4),
            pitch: PitchPolicy::Preserve,
        },
        Command::WrapRetime {
            node: id("a"),
            id: id("suffix"),
            duration: frames(4),
            pitch: PitchPolicy::Preserve,
        },
        Command::SetRetime {
            node: id("a"),
            duration: frames(4),
            pitch: PitchPolicy::Preserve,
        },
    ] {
        let before = fixture();
        assert!(apply(&before, &request(&before, command, "bad")).is_err());
        assert_eq!(before, fixture());
    }
}

#[test]
fn occurrence_retime_isolates_only_the_selected_repeat_play() {
    let (repeated, _) = edit(
        &fixture(),
        Command::WrapRepeat {
            node: id("a"),
            id: id("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
        "repeat",
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let selected = iterations.at(1).unwrap();
    let (changed, tx) = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("a"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: selected.clone(),
                }],
            },
            edit: OccurrenceEdit::WrapRetime {
                id: id("rate"),
                duration: frames(5),
                pitch: PitchPolicy::FollowSpeed,
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated")],
                marks: vec![],
            },
        },
        "isolated-rate",
    );
    assert_eq!(tx.duration_delta, -7);
    assert_eq!(changed.nodes()[&id("a")], repeated.nodes()[&id("a")]);
    assert_eq!(
        changed.overrides()[&id("repeat")].get(&selected),
        Some(&id("rate"))
    );
}

#[test]
fn frozen_schema_27_roundtrips_existing_documents_and_rejects_new_commands() {
    let before = fixture();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["schema_version"] = json!(27);
    let old = legacy_v27::Document::from_json(&wire.to_string()).unwrap();
    assert!(old.matches(&before));
    assert_eq!(old.upgrade().unwrap(), before);
    let readers = [
        legacy_v1::upgrade_request,
        legacy_v2::upgrade_request,
        legacy_v3::upgrade_request,
        legacy_v4::upgrade_request,
        legacy_v5::upgrade_request,
        legacy_v6::upgrade_request,
        legacy_v7::upgrade_request,
        legacy_v8::upgrade_request,
        legacy_v9::upgrade_request,
        legacy_v10::upgrade_request,
        legacy_v11::upgrade_request,
        legacy_v12::upgrade_request,
        legacy_v13::upgrade_request,
        legacy_v14::upgrade_request,
        legacy_v15::upgrade_request,
        legacy_v16::upgrade_request,
        legacy_v17::upgrade_request,
        legacy_v18::upgrade_request,
        legacy_v19::upgrade_request,
        legacy_v20::upgrade_request,
        legacy_v21::upgrade_request,
        legacy_v22::upgrade_request,
        legacy_v23::upgrade_request,
        legacy_v24::upgrade_request,
        legacy_v25::upgrade_request,
        legacy_v26::upgrade_request,
        legacy_v27::upgrade_request,
    ];
    for command in [
        wrap(8, PitchPolicy::Preserve),
        set(8, PitchPolicy::FollowSpeed),
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("a"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::WrapRetime {
                id: id("new"),
                duration: frames(5),
                pitch: PitchPolicy::Preserve,
            },
            identities: OccurrenceIdentities {
                nodes: vec![],
                marks: vec![],
            },
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("a"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::SetRetime {
                duration: frames(5),
                pitch: PitchPolicy::Preserve,
            },
            identities: OccurrenceIdentities {
                nodes: vec![],
                marks: vec![],
            },
        },
    ] {
        let json = serde_json::to_string(&request(&before, command, "future")).unwrap();
        for (index, reader) in readers.iter().enumerate() {
            assert!(
                reader(&json).is_err(),
                "legacy {} admitted future command",
                index + 1
            );
        }
    }
}
