use deadpan_core::*;

fn asset() -> AssetId {
    AssetId::new("original").unwrap()
}

fn fixture() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("interpretation").unwrap(),
        RevisionId::new("empty").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let time_base = SourceTimeBase::new(1, 24).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: -12,
            time_base,
        },
        SourceTimestamp {
            ticks: 12,
            time_base,
        },
    )
    .unwrap();
    let command = Command::ImportSource {
        id: asset(),
        asset: AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(span),
            audio: None,
            still_image: false,
            frame_count: Some(FrameDuration::new(24).unwrap()),
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
        primary: None,
        insertion: None,
    };
    apply_with_result(&document, &request(&document, command, "import"))
        .unwrap()
        .1
}

fn request(document: &ProjectDocument, command: Command, revision: &str) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn choice() -> SourceInterpretation {
    SourceInterpretation {
        range: Some(SourceColorRange::Full),
        chroma_location: Some(SourceChromaLocation::Center),
    }
}

fn set(interpretation: Option<SourceInterpretation>) -> Command {
    Command::SetSourceInterpretation {
        asset: asset(),
        interpretation,
    }
}

#[test]
fn correction_and_reset_are_reversible_without_replacing_assets_or_clocks() {
    let before = fixture();
    assert!(!before.to_json().unwrap().contains("source_interpretations"));
    let request = request(&before, set(Some(choice())), "corrected");
    let (edit, after) = apply_with_result(&before, &request).unwrap();
    let validated = ValidatedDocument::new(before.clone().into()).unwrap();
    let (fast_edit, fast_after) = apply_validated(&validated, &request).unwrap();
    assert_eq!(fast_edit, edit);
    assert_eq!(fast_after.document().as_ref(), &after);
    assert_eq!(after.assets(), before.assets());
    assert_eq!(after.nodes(), before.nodes());
    assert_eq!(after.presentation_basis(), before.presentation_basis());
    assert_eq!(after.basis_state(), before.basis_state());
    assert_eq!(
        after.source_interpretations().get(&asset()),
        Some(&choice())
    );
    assert_eq!(edit.duration_delta, 0);
    assert!(edit.changed_ids.is_empty());
    assert!(edit.forward.assets.is_empty());
    assert_eq!(edit.inverse.apply(&after).unwrap(), before);
    assert!(edit.forward.restores(&before, &after).unwrap());
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    let json = serde_json::to_string(&edit).unwrap();
    assert!(!json.contains("\"inverse\""));
    let retained: EditTransaction = serde_json::from_str(&json).unwrap();
    assert_eq!(retained, edit);
    assert_eq!(retained.inverse.apply(&after).unwrap(), before);

    let (reset, restored) =
        apply_with_result(&after, &crate::request(&after, set(None), "reset")).unwrap();
    assert!(restored.source_interpretations().is_empty());
    assert_eq!(reset.inverse.apply(&restored).unwrap(), after);
    assert!(reset.forward.restores(&after, &restored).unwrap());
    assert!(!edit.forward.restores(&before, &restored).unwrap());
}

#[test]
fn unknown_unqualified_audio_and_empty_interpretations_reject_without_changes() {
    let original = fixture();
    let before_json = original.to_json().unwrap();
    for bad in [
        Command::SetSourceInterpretation {
            asset: AssetId::new("absent").unwrap(),
            interpretation: None,
        },
        set(Some(SourceInterpretation {
            range: None,
            chroma_location: None,
        })),
    ] {
        assert!(apply(&original, &request(&original, bad, "bad")).is_err());
    }
    for field in ["source_qualification", "video"] {
        let mut json = serde_json::to_value(&original).unwrap();
        json["assets"]["original"][field] = serde_json::Value::Null;
        if field == "video" {
            json["assets"]["original"]["frame_count"] = serde_json::Value::Null;
            json["assets"]["original"]["audio"] =
                serde_json::to_value(original.assets()[&asset()].video).unwrap();
        }
        let unqualified = ProjectDocument::from_json(&json.to_string()).unwrap();
        assert!(
            apply(
                &unqualified,
                &request(&unqualified, set(Some(choice())), "bad")
            )
            .is_err()
        );
        json["source_interpretations"] = serde_json::json!({"original": choice()});
        assert!(ProjectDocument::from_json(&json.to_string()).is_err());
    }
    assert_eq!(original.to_json().unwrap(), before_json);
    let duplicate = before_json.replacen(
        '{',
        "{\"source_interpretations\":{\"original\":{\"range\":\"full\"},\"original\":{\"range\":\"limited\"}},",
        1,
    );
    assert!(ProjectDocument::from_json(&duplicate).is_err());
}

#[test]
fn inverse_guards_cannot_erase_a_later_correction() {
    let before = fixture();
    let (first, after) =
        apply_with_result(&before, &request(&before, set(Some(choice())), "first")).unwrap();
    let different = SourceInterpretation {
        range: Some(SourceColorRange::Limited),
        chroma_location: None,
    };
    let (_, later) =
        apply_with_result(&after, &request(&after, set(Some(different)), "later")).unwrap();
    let stale = first.inverse.rebased(
        later.revision_id().clone(),
        RevisionId::new("undo").unwrap(),
    );
    assert_eq!(
        stale.apply(&later).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
    assert_eq!(
        stale.restores(&later, &before).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
}

#[test]
fn detached_original_view_retains_the_captured_interpretation() {
    let before = fixture();
    let (_, document) =
        apply_with_result(&before, &request(&before, set(Some(choice())), "corrected")).unwrap();
    let view = original_view(&document);
    assert_eq!(
        view.source_interpretations(),
        document.source_interpretations()
    );
    assert_eq!(view.assets(), document.assets());
}

fn original_view(document: &ProjectDocument) -> ProjectDocument {
    let source = SourceNode {
        duration: FrameDuration::new(24).unwrap(),
        edit_window: None,
        video: SourceVideo::Stream {
            asset: asset(),
            span: document.assets()[&asset()].video.unwrap(),
        },
        video_mapping: SourceVideoMapping::natural_rate(
            document.assets()[&asset()].video.unwrap(),
            document.presentation_basis().frame_rate,
            EndpointPolicy::HoldAdjacent,
        )
        .unwrap(),
        audio: None,
        audio_mapping: SourceAudioMapping::FitBeat,
        link: LinkRelation::Independent,
        audio_offset: AudioSample(0),
    };
    document
        .source_view(
            &asset(),
            source,
            NodeId::new("view").unwrap(),
            NodeId::new("source").unwrap(),
        )
        .unwrap()
}

#[test]
fn copied_edits_capture_choices_but_existing_destination_sources_keep_their_current_choice() {
    let before = original_view(&fixture());
    let (_, corrected) =
        apply_with_result(&before, &request(&before, set(Some(choice())), "corrected")).unwrap();
    let slice = CapturedEditSlice::capture(
        &corrected,
        corrected.root(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(24)).unwrap(),
        AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let slice = CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap();
    slice.validate_capture(&corrected).unwrap();
    let mut tampered = serde_json::to_value(&slice).unwrap();
    tampered["source_interpretations"] = serde_json::json!({});
    assert!(
        CapturedEditSlice::from_json(&tampered.to_string())
            .unwrap()
            .validate_capture(&corrected)
            .is_err()
    );
    let empty = ProjectDocument::new(
        before.project_id().clone(),
        RevisionId::new("empty-destination").unwrap(),
        before.presentation_basis().clone(),
        before.root().clone(),
    )
    .unwrap();
    let required = slice.identity_requirements().unwrap();
    for destination in [empty, before, corrected] {
        let expected = if destination.assets().contains_key(&asset()) {
            destination.source_interpretations().get(&asset()).copied()
        } else {
            Some(choice())
        };
        let (patch, pasted) = apply_with_result(
            &destination,
            &request(
                &destination,
                Command::SpliceSlice {
                    parent: destination.root().clone(),
                    index: 0,
                    slice: slice.clone(),
                    timing: AudioTimingId {
                        allocation: RevisionId::new("pasted").unwrap(),
                        ordinal: 0,
                    },
                    identities: SlicePasteIdentities {
                        authored: OccurrenceIdentities {
                            nodes: (0..required.nodes)
                                .map(|i| NodeId::new(format!("pasted-{i}")).unwrap())
                                .collect(),
                            marks: (0..required.marks)
                                .map(|i| MarkId::new(format!("mark-{i}")).unwrap())
                                .collect(),
                        },
                        aliases: (0..required.aliases)
                            .map(|i| NodeId::new(format!("alias-{i}")).unwrap())
                            .collect(),
                    },
                },
                "pasted",
            ),
        )
        .unwrap();
        assert_eq!(
            pasted.source_interpretations().get(&asset()).copied(),
            expected
        );
        assert_eq!(patch.duration_delta, 24);
        assert_eq!(patch.inverse.apply(&pasted).unwrap(), destination);
    }
}
