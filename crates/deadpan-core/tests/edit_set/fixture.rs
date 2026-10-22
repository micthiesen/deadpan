//! Public SetMark/Split fixture shared by union regressions.

use deadpan_core::*;
use std::collections::BTreeMap;

pub(super) fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

pub(super) fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

pub(super) fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

pub(super) fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

pub(super) fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}

pub(super) fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1001, 30_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

pub(super) fn hold() -> BeatNode {
    BeatNode::hold(
        "Unselected inserted gap",
        HoldRecipe {
            duration: frames(2),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

pub(super) fn source() -> BeatNode {
    let mut result = hold();
    result.label = "Complete Source context".into();
    result.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(12),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: AssetId::new("shared-mark-synthetic-original").unwrap(),
                span: span(0, 12),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    result
}

pub(super) fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    };
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    result
}

pub(super) fn fixture() -> (ProjectDocument, ProjectDocument) {
    let document = ProjectDocument::new(
        ProjectId::new("shared-mark-control").unwrap(),
        revision("shared-mark-initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let document = edit(
        &document,
        "shared-mark-asset",
        Command::AddAsset {
            id: AssetId::new("shared-mark-synthetic-original").unwrap(),
            asset: AssetRecord {
                label: "Synthetic structural metadata, no actual media".into(),
                content_hash: "a".repeat(64),
                video: Some(span(0, 32)),
                audio: None,
                still_image: false,
                frame_count: Some(frames(32)),
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        },
    );
    let document = edit(
        &document,
        "shared-mark-source",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("source"),
                nodes: BTreeMap::from([(node("source"), source())]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let document = edit(
        &document,
        "shared-mark-set",
        Command::SetMark {
            id: MarkId::new("logical-shared-cue").unwrap(),
            owner: node("source"),
            label: "One logical shared cue".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Source {
                    asset: AssetId::new("shared-mark-synthetic-original").unwrap(),
                    moment: SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        timestamp: SourceTimestamp {
                            ticks: 9,
                            time_base: SourceTimeBase::new(1001, 30_000).unwrap(),
                        },
                    },
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    assert_eq!(stored(&document).binding_count(), 1);
    // A direct non-root one-node Source subtree needs two partition identities
    // and one right-context clone. Assert the concrete resulting graph below.
    let split = edit(
        &document,
        "shared-mark-split",
        Command::Split {
            node: node("source"),
            at: frames(5),
            identities: SplitIdentities {
                nodes: vec![
                    node("left-fragment"),
                    node("right-fragment"),
                    node("right-source-copy"),
                ],
            },
        },
    );
    assert_eq!(split.duration().unwrap(), frames(12));
    let separated = edit(
        &split,
        "shared-mark-gap",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("gap"),
                nodes: BTreeMap::from([(node("gap"), hold())]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    (split, separated)
}

pub(super) fn stored(document: &ProjectDocument) -> &Mark {
    &document.marks()[&MarkId::new("logical-shared-cue").unwrap()]
}

pub(super) fn capture(
    document: &ProjectDocument,
    selection: SliceCaptureSelection,
    name: &str,
) -> CapturedEditSlice {
    let captured =
        CapturedEditSlice::capture_selection(document, &node("root"), &selection, timing(name))
            .unwrap();
    captured.validate_capture(document).unwrap();
    assert_eq!(captured.selection(), &selection);
    assert_eq!(captured.attachments(), SliceAttachments::Owned);
    assert_eq!(
        CapturedEditSlice::from_json(&captured.to_json().unwrap()).unwrap(),
        captured
    );
    captured
}

pub(super) fn captured_marks(captured: &CapturedEditSlice) -> BTreeMap<MarkId, Mark> {
    let wire = serde_json::to_value(captured).unwrap();
    serde_json::from_value(wire["marks"].clone()).unwrap()
}

pub(super) fn destination(source: &ProjectDocument, name: &str) -> ProjectDocument {
    // Current closed slices permit only same-project/same-basis placement.
    // This is a separate private empty document of that project, not a new
    // authored revision installed into a store or the source's live timeline.
    ProjectDocument::new(
        source.project_id().clone(),
        revision(name),
        source.presentation_basis().clone(),
        node("root"),
    )
    .unwrap()
}

pub(super) fn paste(
    document: &ProjectDocument,
    captured: &CapturedEditSlice,
    prefix: &str,
    index: usize,
) -> ProjectDocument {
    let required = captured.identity_requirements().unwrap();
    edit(
        document,
        prefix,
        Command::SpliceSlice {
            parent: node("root"),
            index,
            slice: captured.clone(),
            timing: timing(prefix),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|index| node(&format!("{prefix}-node-{index}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|index| MarkId::new(format!("{prefix}-mark-{index}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|index| node(&format!("{prefix}-alias-{index}")))
                    .collect(),
            },
        },
    )
}
