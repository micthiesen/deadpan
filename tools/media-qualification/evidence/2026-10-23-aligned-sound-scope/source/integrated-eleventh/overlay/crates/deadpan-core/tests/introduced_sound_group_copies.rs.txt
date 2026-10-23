//! Public copy lifecycle for explicit groups introduced after sound capture.
//! Synthetic recipes verify structural clocks, not decoded media or PCM.

use deadpan_core::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("effect").unwrap()
}

fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(duration),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn fixture(source: bool) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
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
    let asset = AssetId::new("effect").unwrap();
    let mut owner = hold(4);
    if source {
        owner.kind = NodeKind::Source {
            source: SourceNode {
                duration: frames(4),
                edit_window: None,
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: asset.clone(),
                    span,
                }),
                audio_mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        };
    }
    let nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("lead"), id("top"), id("tail")]),
        ),
        (id("lead"), hold(2)),
        (
            id("top"),
            BeatNode::sequence("Top", vec![id("owner"), id("other")]),
        ),
        (id("owner"), owner),
        (id("other"), hold(3)),
        (id("tail"), hold(2)),
    ]);
    let event = BeatSound {
        label: "Effect".into(),
        source: SourceAudio {
            asset: asset.clone(),
            span,
        },
        mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
        offset: AudioSample(37),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let assets = BTreeMap::from([(
        asset,
        AssetRecord {
            label: "Effect".into(),
            content_hash: "a".repeat(64),
            audio: Some(span),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]);
    ProjectDocument::from_json(
        &json!({
            "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"introduced-group-copy",
            "revision_id":"initial","root":"root","nodes":nodes,"assets":assets,
            "presentation_basis":{"width":16,"height":16,
                "frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
            "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
            "marks":{},"overrides":{},
            "beat_sounds":BTreeMap::from([(id("owner"), BTreeMap::from([(sound(),event)]))]),
        })
        .to_string(),
    )
    .unwrap()
}

fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let before = document.to_json().unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    };
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(document.to_json().unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

fn shifted(document: &ProjectDocument, name: &str) -> ProjectDocument {
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    edit(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: id(name),
            identities: SplitIdentities::default(),
            timing: timing(name),
        },
    )
}

fn grouped(source: bool) -> ProjectDocument {
    let before = shifted(&fixture(source), "first-placement");
    group_owner(&before, "owner", "fresh", "grouped")
}

fn group_owner(
    document: &ProjectDocument,
    owner: &str,
    group: &str,
    name: &str,
) -> ProjectDocument {
    let selection = SliceCaptureSelection::Child { node: id(owner) };
    let plan = document.group_selection(&id("top"), &selection).unwrap();
    assert_eq!(plan.required_split_ids, 0);
    edit(
        document,
        name,
        Command::GroupSelection {
            parent: id("top"),
            selection,
            label: "Fresh group".into(),
            identities: GroupSelectionIdentities {
                group: id(group),
                split: SplitIdentities::default(),
            },
            timing: timing(name),
        },
    )
}

fn capture(
    document: &ProjectDocument,
    parent: &NodeId,
    child: &NodeId,
    name: &str,
) -> CapturedEditSlice {
    let slice = CapturedEditSlice::capture_selection(
        document,
        parent,
        &SliceCaptureSelection::Child {
            node: child.clone(),
        },
        timing(name),
    )
    .unwrap();
    slice.validate_capture(document).unwrap();
    assert_eq!(
        CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap(),
        slice
    );
    slice
}

fn paste(document: &ProjectDocument, slice: &CapturedEditSlice, name: &str) -> ProjectDocument {
    let required = slice.identity_requirements().unwrap();
    edit(
        document,
        name,
        Command::SpliceSlice {
            parent: id("root"),
            index: 0,
            slice: slice.clone(),
            timing: timing(name),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| id(&format!("{name}-node-{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| id(&format!("{name}-alias-{n}")))
                    .collect(),
            },
        },
    )
}

fn copied_owner(document: &ProjectDocument, name: &str) -> NodeId {
    document
        .beat_sounds()
        .keys()
        .find(|owner| owner.as_str().starts_with(name))
        .unwrap()
        .clone()
}

fn proof(
    document: &ProjectDocument,
    owner: &NodeId,
    reference: &SoundClockReference,
) -> SoundClockCorrespondence {
    let journal = &document.audio_bindings().sound_clocks()[owner][&sound()];
    document.audio_bindings().timings()[reference.timing()]
        .sound_owner_clock_correspondence_with_groups(
            &FrozenAudioLayout::capture(document).unwrap(),
            (reference.scope(), journal.scope()),
            (reference.owner(), owner),
            reference.repeats(),
            reference.introduced_groups(),
            MAX_DOCUMENT_NODES,
        )
        .unwrap()
}

fn assert_unpaired_group(document: &ProjectDocument, owner: &NodeId, count: usize) {
    let journal = &document.audio_bindings().sound_clocks()[owner][&sound()];
    assert_ne!(journal.scope(), owner);
    assert_eq!(journal.clocks().len(), count);
    let oldest = &journal.clocks()[0];
    assert_eq!(oldest.scope(), oldest.owner());
    assert_eq!(
        oldest.introduced_groups(),
        &BTreeSet::from([journal.scope().clone()])
    );
    assert_eq!(
        proof(document, owner, oldest).historical_node(journal.scope()),
        None
    );
    assert_ne!(oldest.owner(), owner);
    assert!(!document.nodes().contains_key(oldest.owner()));
    for reference in &journal.clocks()[1..] {
        assert!(reference.introduced_groups().is_empty());
        assert_eq!(
            proof(document, owner, reference).historical_node(journal.scope()),
            Some(reference.scope())
        );
    }
}

#[test]
fn fresh_group_copy_and_recopy_keep_live_scope_and_distinct_capture_contexts() {
    for source in [false, true] {
        let before = grouped(source);
        let journal = &before.audio_bindings().sound_clocks()[&id("owner")][&sound()];
        assert_eq!(journal.scope(), &id("top"));
        assert_eq!(journal.clocks().len(), 1);
        let slice = capture(&before, &id("top"), &id("fresh"), "capture");
        let once = paste(&before, &slice, "first-copy");
        let owner = copied_owner(&once, "first-copy");
        assert_unpaired_group(&once, &owner, 2);
        let journal = &once.audio_bindings().sound_clocks()[&owner][&sound()];
        let copied_group = journal.scope().clone();
        let old_timing = journal.clocks()[0].timing().clone();
        let recopy = capture(&once, &id("first-copy-node-0"), &copied_group, "recapture");
        let twice = paste(&once, &recopy, "second-copy");
        let second = copied_owner(&twice, "second-copy");
        assert_unpaired_group(&twice, &second, 3);
        assert!(
            twice.audio_bindings().sound_clocks()[&second][&sound()]
                .clocks()
                .iter()
                .all(|reference| reference.timing() != &old_timing)
        );
        assert_eq!(
            before.beat_sounds()[&id("owner")],
            twice.beat_sounds()[&second]
        );
    }
}

#[test]
fn complete_top_copy_preserves_every_old_top_alias() {
    let before = grouped(false);
    let slice = capture(&before, &id("root"), &id("top"), "capture-top");
    let after = paste(&before, &slice, "top-copy");
    let owner = copied_owner(&after, "top-copy");
    let journal = &after.audio_bindings().sound_clocks()[&owner][&sound()];
    assert_eq!(journal.clocks().len(), 2);
    for reference in journal.clocks() {
        assert_eq!(
            proof(&after, &owner, reference).historical_node(journal.scope()),
            Some(reference.scope())
        );
    }
    let oldest = &journal.clocks()[0];
    assert_eq!(oldest.introduced_groups().len(), 1);
    let group = oldest.introduced_groups().first().unwrap();
    assert!(after.nodes().contains_key(group));
    assert!(
        !after.audio_bindings().timings()[oldest.timing()]
            .nodes()
            .contains_key(group)
    );
    assert_eq!(proof(&after, &owner, oldest).historical_node(group), None);
    let latest = journal.clocks().last().unwrap();
    assert!(
        proof(&after, &owner, latest)
            .historical_node(group)
            .is_some()
    );
}

#[test]
fn copying_owner_filters_exterior_introduced_group_relations() {
    let before = grouped(false);
    let slice = capture(&before, &id("fresh"), &id("owner"), "owner-capture");
    let after = paste(&before, &slice, "owner-copy");
    let owner = copied_owner(&after, "owner-copy");
    let journal = &after.audio_bindings().sound_clocks()[&owner][&sound()];
    assert_eq!(journal.scope(), &owner);
    for reference in journal.clocks() {
        assert_eq!(reference.scope(), reference.owner());
        assert!(reference.introduced_groups().is_empty());
    }
}

#[test]
fn sibling_part_copy_filters_relations_to_each_scope_not_selected_forest() {
    let mut wire = serde_json::to_value(fixture(false)).unwrap();
    wire["beat_sounds"]["other"] = wire["beat_sounds"]["owner"].clone();
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let before = shifted(&initial, "first-placement");
    let first = group_owner(&before, "owner", "fresh-a", "group-a");
    let grouped = group_owner(&first, "other", "fresh-b", "group-b");
    for owner in [id("owner"), id("other")] {
        let journal = &grouped.audio_bindings().sound_clocks()[&owner][&sound()];
        assert_eq!(
            journal.clocks()[0].introduced_groups(),
            &BTreeSet::from([id("fresh-a"), id("fresh-b")])
        );
    }
    let slice = CapturedEditSlice::capture_selection(
        &grouped,
        &id("top"),
        &SliceCaptureSelection::Children {
            first: id("fresh-a"),
            last: id("fresh-b"),
        },
        timing("siblings"),
    )
    .unwrap();
    slice.validate_capture(&grouped).unwrap();
    let after = paste(&grouped, &slice, "siblings-copy");
    let owners = after
        .beat_sounds()
        .keys()
        .filter(|owner| owner.as_str().starts_with("siblings-copy"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(owners.len(), 2);
    for owner in owners {
        assert_unpaired_group(&after, &owner, 2);
    }
}

#[test]
fn partial_group_copy_requires_entering_the_group_and_filters_outer_relation() {
    let before = grouped(false);
    let bytes = before.to_json().unwrap();
    let error =
        CapturedEditSlice::capture(&before, &id("top"), range(4, 6), timing("outside-partial"))
            .unwrap_err();
    assert!(
        error.message.contains("enter the intended group"),
        "{error}"
    );
    assert_eq!(before.to_json().unwrap(), bytes);
    let slice = CapturedEditSlice::capture(
        &before,
        &id("fresh"),
        range(4, 6),
        timing("partial-capture"),
    )
    .unwrap();
    slice.validate_capture(&before).unwrap();
    let after = paste(&before, &slice, "partial-copy");
    let owner = copied_owner(&after, "partial-copy");
    assert_eq!(after.durations().unwrap()[&owner], frames(4));
    assert_eq!(slice.duration(), frames(2));
    let journal = &after.audio_bindings().sound_clocks()[&owner][&sound()];
    assert_eq!(journal.scope(), &owner);
    assert_eq!(journal.clocks().len(), 2);
    assert!(
        journal
            .clocks()
            .iter()
            .all(|reference| reference.introduced_groups().is_empty())
    );
}

#[test]
fn malformed_copy_relations_never_treat_historical_ids_as_live_groups() {
    let before = grouped(false);
    let slice = capture(&before, &id("top"), &id("fresh"), "capture");
    let pasted = paste(&before, &slice, "copy");
    let owner = copied_owner(&pasted, "copy");
    let group = pasted.audio_bindings().sound_clocks()[&owner][&sound()].scope();
    let recopy = capture(&pasted, &id("copy-node-0"), group, "recopy");
    let good: serde_json::Value = serde_json::from_str(&recopy.to_json().unwrap()).unwrap();
    let old_owner =
        good["audio_bindings"]["sound_clocks"][owner.as_str()]["effect"]["clocks"][0]["owner"]
            .clone();
    for relation in [json!(["missing-live-group"]), json!([old_owner]), json!([])] {
        let mut forged = good.clone();
        forged["audio_bindings"]["sound_clocks"][owner.as_str()]["effect"]["clocks"][0]["introduced_groups"] =
            relation;
        assert!(CapturedEditSlice::from_json(&forged.to_string()).is_err());
    }
}
