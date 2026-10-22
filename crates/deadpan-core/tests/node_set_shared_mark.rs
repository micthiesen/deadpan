//! Public control for preserving one logical mark across disjoint components.
//! Uses established single-capture and placement APIs only.
//! Synthetic structural metadata; this is not decoder or PCM evidence.

use deadpan_core::*;
use std::collections::{BTreeMap, BTreeSet};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
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

fn span(start: i64, end: i64) -> SourceSpan {
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

fn hold() -> BeatNode {
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

fn source() -> BeatNode {
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

fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
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

fn fixture() -> (ProjectDocument, ProjectDocument) {
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

fn stored(document: &ProjectDocument) -> &Mark {
    &document.marks()[&MarkId::new("logical-shared-cue").unwrap()]
}

fn capture(
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

fn captured_marks(captured: &CapturedEditSlice) -> BTreeMap<MarkId, Mark> {
    let wire = serde_json::to_value(captured).unwrap();
    serde_json::from_value(wire["marks"].clone()).unwrap()
}

fn destination(source: &ProjectDocument, name: &str) -> ProjectDocument {
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

fn paste(
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

fn check_whole_fragment(
    document: &ProjectDocument,
    id: &str,
    source_id: &str,
    mapping: FrameRange,
) {
    let NodeKind::Retime {
        child,
        duration,
        mapping: actual_mapping,
        pitch,
        purpose,
    } = &document.nodes()[&node(id)].kind
    else {
        panic!("the selected whole fragment is an ordinary Split Partition owner");
    };
    assert_eq!(child, &node(source_id));
    assert_eq!(*duration, mapping.duration());
    assert_eq!(*actual_mapping, mapping);
    assert_eq!(*pitch, PitchPolicy::Preserve);
    assert_eq!(*purpose, RetimePurpose::Partition);
    assert!(matches!(
        document.nodes()[child].kind,
        NodeKind::Source { .. }
    ));
}

fn check_copied_binding(
    document: &ProjectDocument,
    binding: &MarkFragment,
    expected_coordinate: &Anchor,
) {
    assert_eq!(&binding.coordinate, expected_coordinate);
    assert_eq!(binding.state, MarkState::Bound);
    assert!(matches!(
        document.nodes()[&binding.owner].kind,
        NodeKind::Source { .. }
    ));
}

#[test]
fn separate_component_pastes_split_one_logical_mark_while_existing_whole_capture_preserves_it() {
    let (adjacent, separated) = fixture();
    let frozen_adjacent = adjacent.to_json().unwrap();
    let frozen_separated = separated.to_json().unwrap();
    assert_eq!(adjacent.marks().len(), 1);
    assert_eq!(separated.marks().len(), 1);
    assert_eq!(stored(&adjacent).binding_count(), 2);
    assert_eq!(stored(&separated), stored(&adjacent));
    assert_eq!(
        stored(&adjacent)
            .bindings()
            .map(|binding| binding.owner)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([node("source"), node("right-source-copy")])
    );
    assert!(
        stored(&adjacent)
            .bindings()
            .all(|binding| binding.state == MarkState::Bound)
    );
    check_whole_fragment(&separated, "left-fragment", "source", range(0, 5));
    check_whole_fragment(
        &separated,
        "right-fragment",
        "right-source-copy",
        range(5, 12),
    );
    assert_eq!(separated.duration().unwrap(), frames(14));
    assert_eq!(
        separated
            .children(&node("root"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![node("left-fragment"), node("gap"), node("right-fragment")]
    );
    assert_eq!(
        separated
            .sequence_children(
                &node("root"),
                &node("left-fragment"),
                &node("left-fragment")
            )
            .unwrap()
            .range,
        range(0, 5)
    );
    assert_eq!(
        separated
            .sequence_children(
                &node("root"),
                &node("right-fragment"),
                &node("right-fragment")
            )
            .unwrap()
            .range,
        range(7, 14)
    );

    // Public existing contiguous capture of the two adjacent whole fragments
    // is an oracle for the desired combined owned relationship, with no gap.
    let combined_control = capture(
        &adjacent,
        SliceCaptureSelection::Children {
            first: node("left-fragment"),
            last: node("right-fragment"),
        },
        "shared-combined-capture",
    );
    assert_eq!(combined_control.duration(), frames(12));
    assert_eq!(combined_control.identity_requirements().unwrap().marks, 1);
    let combined_marks = captured_marks(&combined_control);
    assert_eq!(combined_marks.len(), 1);
    assert_eq!(
        combined_marks[&MarkId::new("logical-shared-cue").unwrap()],
        *stored(&adjacent)
    );
    assert_eq!(combined_marks.values().next().unwrap().binding_count(), 2);

    // Two current closed capsules after separation correctly retain one
    // physical binding each, under the same original logical MarkId. They
    // are independent captures, not a union value or a union implementation.
    let left = capture(
        &separated,
        SliceCaptureSelection::Child {
            node: node("left-fragment"),
        },
        "shared-left-capture",
    );
    let right = capture(
        &separated,
        SliceCaptureSelection::Child {
            node: node("right-fragment"),
        },
        "shared-right-capture",
    );
    assert_eq!(left.duration(), frames(5));
    assert_eq!(right.duration(), frames(7));
    for captured in [&left, &right] {
        assert_eq!(captured.identity_requirements().unwrap().marks, 1);
        let marks = captured_marks(captured);
        assert_eq!(marks.len(), 1);
        let mark = &marks[&MarkId::new("logical-shared-cue").unwrap()];
        assert_eq!(mark.label, stored(&adjacent).label);
        assert_eq!(mark.loss_policy, stored(&adjacent).loss_policy);
        assert_eq!(mark.boundary.bias, stored(&adjacent).boundary.bias);
        assert_eq!(mark.binding_count(), 1);
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(
            mark.boundary.coordinate,
            stored(&adjacent).boundary.coordinate
        );
    }
    assert_ne!(
        captured_marks(&left).values().next().unwrap().owner,
        captured_marks(&right).values().next().unwrap().owner
    );

    // Capturing the separated envelope retains the one relationship too, but
    // also copies the unselected gap and produces14f rather than12f.
    let covering = capture(
        &separated,
        SliceCaptureSelection::Children {
            first: node("left-fragment"),
            last: node("right-fragment"),
        },
        "shared-covering-capture",
    );
    assert_eq!(covering.duration(), frames(14));
    assert_eq!(
        captured_marks(&covering)
            .values()
            .next()
            .unwrap()
            .binding_count(),
        2
    );
    assert!(
        serde_json::to_value(&covering).unwrap()["nodes"]
            .get("gap")
            .is_some()
    );

    let empty_oracle = destination(&adjacent, "shared-oracle-empty");
    let oracle = paste(&empty_oracle, &combined_control, "shared-oracle-paste", 0);
    assert_eq!(oracle.duration().unwrap(), frames(12));
    assert_eq!(oracle.marks().len(), 1);
    let oracle_mark = &oracle.marks()[&MarkId::new("shared-oracle-paste-mark-0").unwrap()];
    assert_eq!(oracle_mark.binding_count(), 2);
    assert_eq!(oracle_mark.label, stored(&adjacent).label);
    assert_eq!(oracle_mark.loss_policy, stored(&adjacent).loss_policy);
    assert_eq!(oracle_mark.boundary.bias, stored(&adjacent).boundary.bias);
    for binding in oracle_mark.bindings() {
        check_copied_binding(&oracle, &binding, &stored(&adjacent).boundary.coordinate);
    }

    let empty_independent = destination(&separated, "shared-independent-empty");
    let independent = paste(&empty_independent, &left, "shared-independent-left", 0);
    let independent = paste(&independent, &right, "shared-independent-right", 1);
    assert_eq!(independent.duration().unwrap(), frames(12));
    assert_eq!(independent.marks().len(), 2);
    for name in [
        "shared-independent-left-mark-0",
        "shared-independent-right-mark-0",
    ] {
        let mark = &independent.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.binding_count(), 1);
        assert_eq!(mark.label, oracle_mark.label);
        assert_eq!(mark.loss_policy, oracle_mark.loss_policy);
        assert_eq!(mark.boundary.bias, oracle_mark.boundary.bias);
        for binding in mark.bindings() {
            check_copied_binding(
                &independent,
                &binding,
                &stored(&adjacent).boundary.coordinate,
            );
        }
    }
    assert_eq!(
        independent
            .marks()
            .values()
            .map(Mark::binding_count)
            .sum::<usize>(),
        2
    );
    assert_ne!(independent.marks().len(), oracle.marks().len());
    assert_eq!(adjacent.to_json().unwrap(), frozen_adjacent);
    assert_eq!(separated.to_json().unwrap(), frozen_separated);
}
