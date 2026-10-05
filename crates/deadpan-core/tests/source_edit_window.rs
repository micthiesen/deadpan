use deadpan_core::*;
use serde_json::json;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}
fn window() -> SourceEditWindow {
    SourceEditWindow::new(ratio(1, 3), ratio(89, 3)).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}
fn fixture() -> ProjectDocument {
    let asset = AssetId::new("original").unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
        SourceTimestamp {
            ticks: 30,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
    )
    .unwrap();
    let selected = ExactFrameRange::new(window().start(), window().end()).unwrap();
    let source = SourceNode {
        duration: frames(30),
        edit_window: Some(window()),
        video: SourceVideo::Stream {
            asset: asset.clone(),
            span,
        },
        video_mapping: SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ExactRatio::integer(30),
            selection: selected,
            endpoints: EndpointPolicy::HoldAdjacent,
        },
        audio: Some(SourceAudio { asset, span }),
        audio_mapping: SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ExactRatio::integer(30),
            selection: selected,
        },
        audio_offset: AudioSample(0),
        link: LinkRelation::Linked,
    };
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION,
        "project_id":"window", "revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "marks":{}, "overrides":{},
        "assets":{"original":AssetRecord {label:"Original".into(),content_hash:"a".repeat(64),video:Some(span),audio:Some(span),still_image:false,frame_count:Some(frames(30)),source_qualification:None}},
        "nodes":{
            "root":BeatNode::sequence("Root",vec![id("source")]),
            "source":BeatNode {label:"Source".into(),framing:None,audio_treatments:Default::default(),audio_editorial_edges: Default::default(), audio_edges:Default::default(),kind:NodeKind::Source {source}, cutaways: Vec::new(), captions: Vec::new() },
        },
    }).to_string()).unwrap()
}
fn source<'a>(document: &'a ProjectDocument, name: &str) -> &'a SourceNode {
    let NodeKind::Source { source } = &document.nodes()[&id(name)].kind else {
        panic!("expected Source")
    };
    source
}
fn sources(document: &ProjectDocument) -> Vec<&SourceNode> {
    document
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            NodeKind::Source { source } => Some(source),
            _ => None,
        })
        .collect()
}
fn edit(document: &ProjectDocument, next: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    };
    let tx = apply(document, &request).unwrap();
    let wire = serde_json::to_string(&tx).unwrap();
    let retained: EditTransaction = serde_json::from_str(&wire).unwrap();
    assert_eq!(retained, tx);
    let after = retained.forward.apply(document).unwrap();
    assert_eq!(retained.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

#[test]
fn window_is_optional_and_document_checks_its_exact_owner_extent() {
    let before = fixture();
    assert_eq!(source(&before, "source").edit_window, Some(window()));
    let mut wire = serde_json::to_value(&before).unwrap();
    let entry = &mut wire["nodes"]["source"]["kind"]["source"];
    entry.as_object_mut().unwrap().remove("edit_window");
    let generic = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(source(&generic, "source").edit_window, None);
    let generic_wire = serde_json::to_value(&generic).unwrap();
    assert!(
        generic_wire["nodes"]["source"]["kind"]["source"]
            .get("edit_window")
            .is_none()
    );
    wire["nodes"]["source"]["kind"]["source"]["edit_window"] = serde_json::Value::Null;
    assert_eq!(
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        generic
    );

    // The interval is valid on its own, but exceeds this typed Source's owner.
    let mut invalid = source(&before, "source").clone();
    invalid.duration = frames(29);
    assert!(
        before
            .source_view(
                &AssetId::new("original").unwrap(),
                invalid,
                id("view"),
                id("media")
            )
            .is_err()
    );
    wire["nodes"]["source"]["kind"]["source"]["edit_window"] =
        serde_json::to_value(SourceEditWindow::new(ExactRatio::ZERO, ratio(91, 3)).unwrap())
            .unwrap();
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
}

#[test]
fn audio_only_and_dormant_sources_keep_window_distinct_from_render_support() {
    let before = fixture();
    let mut audio_only = source(&before, "source").clone();
    audio_only.video = SourceVideo::Blank;
    audio_only.video_mapping = SourceVideoMapping::FitBeat;
    audio_only.link = LinkRelation::Independent;
    let audio_view = before
        .source_view(
            &AssetId::new("original").unwrap(),
            audio_only,
            id("view"),
            id("media"),
        )
        .unwrap();
    assert_eq!(source(&audio_view, "media").edit_window, Some(window()));

    let mut dormant = source(&before, "source").clone();
    dormant.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::integer(40),
        frames: ExactRatio::integer(30),
        selection: ExactFrameRange {
            start: ExactRatio::integer(40),
            end: ExactRatio::integer(40),
        },
    };
    let dormant_view = before
        .source_view(
            &AssetId::new("original").unwrap(),
            dormant,
            id("view"),
            id("media"),
        )
        .unwrap();
    let retained = source(&dormant_view, "media");
    assert_eq!(retained.edit_window, Some(window()));
    assert!(retained.audio.is_some());
    assert_eq!(retained.link, LinkRelation::Linked);
    assert_eq!(
        retained
            .audio_mapping
            .selection_frames(retained.duration)
            .unwrap()
            .start,
        ExactRatio::integer(40)
    );
}

#[test]
fn actual_generic_mapping_changes_clear_window_but_noops_and_undo_preserve_it() {
    let before = fixture();
    let original = source(&before, "source");
    for (name, command, expected) in [
        (
            "video-noop",
            Command::SetSourceVideoMapping {
                node: id("source"),
                mapping: original.video_mapping,
            },
            Some(window()),
        ),
        (
            "audio-noop",
            Command::SetSourceAudioMapping {
                node: id("source"),
                mapping: original.audio_mapping,
                offset: original.audio_offset,
            },
            Some(window()),
        ),
        (
            "video-change",
            Command::SetSourceVideoMapping {
                node: id("source"),
                mapping: SourceVideoMapping::FitBeat,
            },
            None,
        ),
        (
            "audio-change",
            Command::SetSourceAudioMapping {
                node: id("source"),
                mapping: SourceAudioMapping::FitBeat,
                offset: original.audio_offset,
            },
            None,
        ),
        (
            "offset-change",
            Command::SetSourceAudioMapping {
                node: id("source"),
                mapping: original.audio_mapping,
                offset: AudioSample(1),
            },
            None,
        ),
    ] {
        let after = edit(&before, name, command);
        let changed = source(&after, "source");
        assert_eq!(changed.edit_window, expected, "{name}");
        assert_eq!(changed.duration, original.duration);
        assert_eq!(changed.video, original.video);
        assert_eq!(changed.audio, original.audio);
    }
    let renamed = edit(
        &before,
        "renamed",
        Command::Rename {
            node: id("source"),
            label: "Retained".into(),
        },
    );
    assert_eq!(source(&renamed, "source").edit_window, Some(window()));
}

#[test]
fn occurrence_mapping_changes_clear_only_the_isolated_play_window() {
    let before = fixture();
    let repeated = edit(
        &before,
        "repeated",
        Command::WrapRepeat {
            node: id("source"),
            id: id("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        panic!()
    };
    let instance = InstancePath {
        node: id("source"),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iterations.at(1).unwrap(),
        }],
    };
    for (name, operation, expected) in [
        (
            "video",
            OccurrenceEdit::SetSourceVideoMapping {
                mapping: SourceVideoMapping::FitBeat,
            },
            None,
        ),
        (
            "offset",
            OccurrenceEdit::SetSourceAudioMapping {
                mapping: source(&repeated, "source").audio_mapping,
                offset: AudioSample(1),
            },
            None,
        ),
        (
            "noop",
            OccurrenceEdit::SetSourceVideoMapping {
                mapping: source(&repeated, "source").video_mapping,
            },
            Some(window()),
        ),
    ] {
        let after = edit(
            &repeated,
            name,
            Command::EditOccurrence {
                instance: instance.clone(),
                edit: operation,
                identities: OccurrenceIdentities {
                    nodes: vec![id("isolated")],
                    marks: vec![],
                },
            },
        );
        assert_eq!(source(&after, "source"), source(&repeated, "source"));
        assert_eq!(source(&after, "isolated").edit_window, expected);
        assert_eq!(sources(&after).len(), 2);
        assert_eq!(after.duration().unwrap(), repeated.duration().unwrap());
        assert_eq!(
            after.overrides()[&id("repeat")].get(&iterations.at(1).unwrap()),
            Some(&id("isolated"))
        );
    }
}

#[test]
fn split_and_partial_capture_preserve_complete_physical_window_through_paste() {
    let before = fixture();
    let split = edit(
        &before,
        "split",
        Command::Split {
            node: id("source"),
            at: frames(11),
            identities: SplitIdentities {
                nodes: (0..8).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    assert_eq!(split.duration().unwrap(), frames(30));
    assert_eq!(sources(&split).len(), 2);
    for owner in sources(&split) {
        assert_eq!(owner.edit_window, Some(window()));
        assert_eq!(owner.duration, frames(30));
    }
    let slice =
        CapturedEditSlice::capture(&split, &id("root"), range(2, 20), timing("capture")).unwrap();
    let slice = CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap();
    slice.validate_capture(&split).unwrap();
    let required = slice.identity_requirements().unwrap();
    let pasted = edit(
        &split,
        "pasted",
        Command::SpliceSlice {
            parent: id("root"),
            index: 2,
            slice,
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| id(&format!("paste-{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("paste-mark-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| id(&format!("paste-alias-{n}")))
                    .collect(),
            },
            timing: timing("pasted"),
        },
    );
    assert_eq!(pasted.duration().unwrap(), frames(48));
    assert_eq!(sources(&pasted).len(), 4);
    for owner in sources(&pasted) {
        assert_eq!(owner.edit_window, Some(window()));
        assert_eq!(owner.duration, frames(30));
    }
}
