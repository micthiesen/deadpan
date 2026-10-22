//! Public-only synthetic fixture for frozen controls and positive union tests.

#![allow(dead_code)]

use deadpan_core::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
pub fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
pub fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
pub fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
pub fn timing(name: &str, ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal,
    }
}
pub fn span(start: i64, end: i64) -> SourceSpan {
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
pub fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Temporary anchor host",
        HoldRecipe {
            duration: frames(length),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
pub fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
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
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
pub fn fixture() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("range-set-public-control").unwrap(),
        revision("range-set-initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let asset = AssetId::new("range-set-synthetic-original").unwrap();
    let document = edit(
        &document,
        "range-set-asset",
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Synthetic 24f metadata; no actual media".into(),
                content_hash: "a".repeat(64),
                video: Some(span(0, 24)),
                audio: None,
                still_image: false,
                frame_count: Some(frames(24)),
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        },
    );
    let mut source = hold(24);
    source.label = "One immutable complete 24f Source recipe".into();
    source.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(24),
            edit_window: None,
            video: SourceVideo::Stream {
                asset,
                span: span(0, 24),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    edit(
        &document,
        "range-set-source",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("source"),
                nodes: BTreeMap::from([(node("source"), source)]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
}
pub fn capture(
    document: &ProjectDocument,
    selected: FrameRange,
    ordinal: u32,
) -> CapturedEditSlice {
    let value = CapturedEditSlice::capture(
        document,
        &node("root"),
        selected,
        timing("range-set-capture", ordinal),
    )
    .unwrap();
    value.validate_capture(document).unwrap();
    assert_eq!(value.range(), selected);
    assert_eq!(value.duration(), selected.duration());
    assert_eq!(
        value.selection(),
        &SliceCaptureSelection::Range { range: selected }
    );
    assert_eq!(value.attachments(), SliceAttachments::Owned);
    assert_eq!(
        CapturedEditSlice::from_json(&value.to_json().unwrap()).unwrap(),
        value
    );
    assert_eq!(value.project_id(), document.project_id());
    assert_eq!(value.revision_id(), document.revision_id());
    let wire = serde_json::to_value(&value).unwrap();
    assert_eq!(wire["nodes"].as_object().unwrap().len(), 1);
    assert_eq!(
        wire["nodes"]["source"],
        serde_json::to_value(&document.nodes()[&node("source")]).unwrap()
    );
    assert_eq!(
        wire["parts"],
        serde_json::json!([{
            "root": "source", "mapping": selected, "source_start": 0,
        }])
    );
    value
}
pub fn destination(source: &ProjectDocument, name: &str) -> ProjectDocument {
    // Separate private empty same-project document for the public paste oracle.
    ProjectDocument::new(
        source.project_id().clone(),
        revision(name),
        source.presentation_basis().clone(),
        node("root"),
    )
    .unwrap()
}
pub fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    index: usize,
) -> ProjectDocument {
    let required = slice.identity_requirements().unwrap();
    edit(
        document,
        name,
        Command::SpliceSlice {
            parent: node("root"),
            index,
            slice: slice.clone(),
            timing: timing(name, 0),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| node(&format!("{name}-node-{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| node(&format!("{name}-alias-{n}")))
                    .collect(),
            },
        },
    )
}
pub fn set_pools(set: &CapturedEditSet, name: &str) -> EditSetPasteIdentities {
    let required = set.identity_requirements().unwrap();
    EditSetPasteIdentities {
        group: node(&format!("{name}-group")),
        components: required
            .components
            .iter()
            .enumerate()
            .map(|(member, required)| EditSetComponentIdentities {
                nodes: (0..required.nodes)
                    .map(|n| node(&format!("{name}-{member}-node-{n}")))
                    .collect(),
                aliases: (0..required.aliases)
                    .map(|n| node(&format!("{name}-{member}-alias-{n}")))
                    .collect(),
            })
            .collect(),
        marks: (0..required.marks)
            .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
            .collect(),
    }
}
pub fn paste_set(document: &ProjectDocument, set: &CapturedEditSet, name: &str) -> ProjectDocument {
    edit(
        document,
        name,
        Command::SpliceEditSet {
            parent: node("root"),
            index: 0,
            set: set.clone(),
            identities: set_pools(set, name),
            timing: timing(name, 0),
        },
    )
}
pub fn check_oracle(
    document: &ProjectDocument,
    source: &ProjectDocument,
    expected: &[FrameRange],
) -> BTreeSet<NodeId> {
    let groups = document
        .children(&node("root"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(groups.len(), expected.len());
    let mut owners = BTreeSet::new();
    let mut output_start = 0;
    for (group, mapping) in groups.iter().zip(expected) {
        let children = document.children(group).cloned().collect::<Vec<_>>();
        assert_eq!(children.len(), 1);
        let NodeKind::Retime {
            child,
            duration,
            mapping: actual,
            pitch,
            purpose,
        } = &document.nodes()[&children[0]].kind
        else {
            panic!("each partial paste retains one exact transparent partition");
        };
        assert_eq!(*purpose, RetimePurpose::Partition);
        assert_eq!(*pitch, PitchPolicy::Preserve);
        assert_eq!(actual, mapping);
        assert_eq!(*duration, mapping.duration());
        assert_eq!(document.nodes()[child], source.nodes()[&node("source")]);
        assert!(
            owners.insert(child.clone()),
            "each copied component owns a distinct physical Source"
        );
        let geometry = document
            .sequence_children(&node("root"), group, group)
            .unwrap();
        assert_eq!(
            geometry.range,
            range(output_start, output_start + mapping.duration().frames())
        );
        output_start += mapping.duration().frames();
    }
    assert_eq!(document.duration().unwrap(), frames(output_start));
    owners
}
pub fn captured_marks(slice: &CapturedEditSlice) -> BTreeMap<MarkId, Mark> {
    serde_json::from_value(serde_json::to_value(slice).unwrap()["marks"].clone()).unwrap()
}
pub fn unresolved_fixture() -> ProjectDocument {
    let document = fixture();
    let document = edit(
        &document,
        "range-mark-host",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("temporary-host"),
                nodes: BTreeMap::from([(node("temporary-host"), hold(1))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let document = edit(
        &document,
        "range-mark-create",
        Command::SetMark {
            id: MarkId::new("one-logical-cue").unwrap(),
            owner: node("source"),
            label: "One logical cue whose separate host was deleted".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("temporary-host"),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    assert_eq!(document.marks().len(), 1);
    assert_eq!(
        document.marks().values().next().unwrap().state,
        MarkState::Bound
    );
    let document = edit(
        &document,
        "range-mark-delete-host",
        Command::DeleteRipple {
            node: node("temporary-host"),
            timing: timing("range-mark-delete-host", 0),
        },
    );
    let mark = document.marks().values().next().unwrap();
    assert_eq!(mark.owner, node("source"));
    assert_eq!(mark.binding_count(), 1);
    assert!(matches!(mark.state, MarkState::Unresolved { .. }));
    assert!(!document.nodes().contains_key(&node("temporary-host")));
    assert_eq!(document.duration().unwrap(), frames(24));
    document
}
