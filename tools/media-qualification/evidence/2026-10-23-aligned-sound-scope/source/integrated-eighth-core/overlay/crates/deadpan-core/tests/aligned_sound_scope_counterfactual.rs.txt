//! Public Core controls for aligned grouping inside retained sound scopes.
//!
//! Exact AssetRecord copied from the retained qualified cfr-bframes public
//! endpoint oracle. Synthetic authored graph; no receipt/media admission claim.
//! Independent pre-capture and outside-scope controls precede target acceptance.

use deadpan_core::*;
use std::collections::BTreeMap;

const RETAINED_REFUSAL: &str =
    "group ownership changes inside a retained beat sound scope are not yet supported";

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn sound() -> SoundId {
    SoundId::new("effect").unwrap()
}
fn asset() -> AssetId {
    AssetId::new("5ced4d7d-272f-46f0-a901-b0c82f67fd2b").unwrap()
}
fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn sample_boundary(frame: i64) -> i64 {
    i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)
            .unwrap()
            .round_even()
            .unwrap(),
    )
    .unwrap()
}
fn span(start: i64, end: i64, denominator: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, denominator).unwrap();
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
fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Silent background",
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn treatment(gains: [i32; 3]) -> AudioTreatments {
    AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(gains.map(|gain| GainDb::new(gain).unwrap()), None, None).unwrap(),
        ))
        .unwrap()
}
fn qualified_asset() -> AssetRecord {
    // Byte-for-byte metadata seed from the existing qualified public oracle:
    // /tmp/deadpan-sound-group-endpoints-2026-10-22/sound_group_endpoints_counterfactual.rs
    serde_json::from_str(
        r#"{
        "label":"original",
        "content_hash":"blake3:16614ec12d06396d3c0f0e7b91cc902c6dc30ad89c28d74b805cf64dc60d8aa1",
        "video":{
            "start":{"ticks":0,"time_base":{"numerator":1,"denominator":30000}},
            "end":{"ticks":120120,"time_base":{"numerator":1,"denominator":30000}}
        },
        "audio":{
            "start":{"ticks":0,"time_base":{"numerator":1,"denominator":48000}},
            "end":{"ticks":192192,"time_base":{"numerator":1,"denominator":48000}}
        },
        "still_image":false,
        "frame_count":120,
        "source_qualification":"c6bc3e81ab69906267164469d11739cf1363eea546436761ddd219069ed496b6"
    }"#,
    )
    .unwrap()
}
#[derive(Clone, Copy, Debug)]
enum Owner {
    Source,
    RoomTone,
    Silence,
}

fn fixture(kind: Owner) -> ProjectDocument {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let original_audio = SourceAudio {
        asset: asset(),
        span: span(0, 16016, 48000),
    };
    let mut owner = hold(12);
    owner.label = "Complete independent owner".into();
    owner.audio_treatments = treatment([3000, -1500, 1000]);
    owner.audio_edges.node_start = AudioEdgePolicy::Hard;
    owner.audio_edges.node_end = AudioEdgePolicy::Hard;
    match kind {
        Owner::Source => {
            owner.kind = NodeKind::Source {
                source: SourceNode {
                    duration: frames(12),
                    edit_window: None,
                    video: SourceVideo::Stream {
                        asset: asset(),
                        span: span(0, 12012, 30000),
                    },
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio: Some(original_audio.clone()),
                    audio_mapping: SourceAudioMapping::natural_rate(original_audio.span, rate)
                        .unwrap(),
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            }
        }
        Owner::RoomTone => {
            let NodeKind::Hold { recipe } = &mut owner.kind else {
                unreachable!()
            };
            recipe.audio = HoldAudio::RoomTone {
                source: original_audio,
            };
        }
        Owner::Silence => {}
    }
    let event_audio = SourceAudio {
        asset: asset(),
        span: span(0, 8197, 48000),
    };
    let event = BeatSound {
        label: "Finite event with separate EQ".into(),
        source: event_audio.clone(),
        mapping: SourceAudioMapping::natural_rate(event_audio.span, rate).unwrap(),
        offset: AudioSample(37),
        treatments: treatment([6000, -6000, 3000]),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let document = ProjectDocument::new(
        ProjectId::new("aligned-sound-scope-public-control").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("top")]),
        ),
        (node("lead"), hold(2)),
        (
            node("top"),
            BeatNode::sequence(
                "Ordinary placement scope",
                vec![node("owner"), node("sibling")],
            ),
        ),
        (node("owner"), owner),
        (node("sibling"), hold(5)),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(asset(), qualified_asset())])).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        node("owner"),
        BTreeMap::from([(sound(), event)]),
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}
fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let bytes = document.to_json().unwrap();
    let request = request(document, name, command);
    let decoded: CommandRequest =
        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(decoded, request);
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(
        transaction
            .inverse
            .apply(&after)
            .unwrap()
            .to_json()
            .unwrap(),
        bytes
    );
    assert_eq!(document.to_json().unwrap(), bytes);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn prefix(name: &str) -> Command {
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    Command::InsertTime {
        at: ProjectFrame(0),
        hold: recipe,
        id: node(name),
        identities: SplitIdentities::default(),
        timing: timing(name),
    }
}
fn group(
    document: &ProjectDocument,
    parent: &str,
    selection: SliceCaptureSelection,
    name: &str,
) -> Command {
    let plan = document.group_selection(&node(parent), &selection).unwrap();
    Command::GroupSelection {
        parent: node(parent),
        selection,
        label: "Aligned independent owner".into(),
        identities: GroupSelectionIdentities {
            group: node(&format!("{name}-group")),
            split: SplitIdentities {
                nodes: (0..plan.required_split_ids)
                    .map(|i| node(&format!("{name}-split-{i}")))
                    .collect(),
            },
        },
        timing: timing(name),
    }
}
fn owner_selection() -> SliceCaptureSelection {
    SliceCaptureSelection::Child {
        node: node("owner"),
    }
}
fn journal(document: &ProjectDocument) -> &SoundClockJournal {
    &document.audio_bindings().sound_clocks()[&node("owner")][&sound()]
}
fn refuse(document: &ProjectDocument, name: &str, command: Command, message: &str) {
    let bytes = document.to_json().unwrap();
    let error = apply(document, &request(document, name, command)).unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains(message), "{error}");
    assert_eq!(document.to_json().unwrap(), bytes);
}

fn witness(kind: Owner) {
    let initial = fixture(kind);
    assert_eq!(initial.duration().unwrap(), frames(19));

    // Independently construct the same final graph before any broad journal
    // exists; prefixing after that is already accepted by the baseline.
    let neutral = edit(
        &initial,
        "aligned",
        group(&initial, "top", owner_selection(), "aligned"),
    );
    assert_eq!(neutral.audio_bindings(), initial.audio_bindings());
    assert_eq!(neutral.beat_sounds(), initial.beat_sounds());
    let control = edit(&neutral, "prefix", prefix("prefix"));
    assert_eq!(control.duration().unwrap(), frames(20));
    assert_eq!(journal(&control).scope(), &node("top"));
    println!("CONTROL group-before-prefix passed: {kind:?}; inverse bytes identical");

    let before = edit(&initial, "prefix", prefix("prefix"));
    assert_eq!(before.duration().unwrap(), frames(20));
    assert_eq!(sample_boundary(3), 4805);
    assert_eq!(sample_boundary(15), 24024);
    assert_eq!(sample_boundary(20), 32032);
    assert_eq!(sample_boundary(3) + 37, 4842);
    assert_eq!(sample_boundary(3) + 37 + 8197, 13039);
    assert_eq!(journal(&before).scope(), &node("top"));
    assert_eq!(journal(&before).clocks().len(), 1);
    assert_eq!(journal(&before).clocks()[0].timing(), &timing("prefix"));
    assert_eq!(journal(&before).clocks()[0].scope(), &node("top"));
    assert_eq!(journal(&before).clocks()[0].owner(), &node("owner"));

    let outside = edit(
        &before,
        "outside",
        group(
            &before,
            "root",
            SliceCaptureSelection::Child { node: node("top") },
            "outside",
        ),
    );
    assert_eq!(outside.audio_bindings(), before.audio_bindings());
    assert_eq!(outside.beat_sounds(), before.beat_sounds());
    assert_eq!(outside.duration().unwrap(), frames(20));
    println!("CONTROL group-from-outside-scope passed: {kind:?}; all bindings unchanged");

    // The endpoint candidate remains accepted when grouping from the root,
    // outside the current broad top scope. Do not weaken this transport.
    let endpoints = SliceCaptureSelection::Range {
        range: range(5, 11),
    };
    assert!(
        before
            .group_selection(before.root(), &endpoints)
            .unwrap()
            .required_split_ids
            > 0
    );
    let split_group = edit(
        &before,
        "outside-endpoints",
        group(&before, "root", endpoints, "outside-endpoints"),
    );
    assert_eq!(split_group.duration().unwrap(), frames(20));
    assert_eq!(split_group.beat_sounds().len(), 3);
    assert!(
        split_group
            .beat_sounds()
            .values()
            .all(|events| events == &before.beat_sounds()[&node("owner")])
    );
    println!("CONTROL outside endpoint grouping passed: {kind:?}");

    let plain = edit(
        &before,
        "plain-split",
        Command::Split {
            node: node("owner"),
            at: frames(4),
            identities: SplitIdentities {
                nodes: vec![node("plain-left"), node("plain-right"), node("plain-copy")],
            },
        },
    );
    assert_eq!(plain.duration().unwrap(), frames(20));
    assert_eq!(plain.beat_sounds().len(), 2);
    assert!(
        plain
            .beat_sounds()
            .values()
            .all(|events| events == &before.beat_sounds()[&node("owner")])
    );
    println!("CONTROL plain Source/Hold Split passed: {kind:?}");

    let selection = owner_selection();
    let plan = before.group_selection(&node("top"), &selection).unwrap();
    assert_eq!(plan.required_split_ids, 0);
    let bytes = before.to_json().unwrap();
    let command = group(&before, "top", selection, "aligned");
    let result = apply(&before, &request(&before, "aligned", command));
    assert_eq!(before.to_json().unwrap(), bytes);
    assert!(
        result.is_ok(),
        "aligned unchanged {kind:?} owner must retain its broad scope; error={:?}",
        result.as_ref().err()
    );
    let transaction = result.unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert_preserved(&before, &after);
    assert_eq!(
        after.nodes(),
        control.nodes(),
        "independently constructed final graph"
    );
    assert_eq!(after.beat_sounds(), control.beat_sounds());
    assert_eq!(
        after.node_duration(&node("aligned-group")).unwrap(),
        frames(12)
    );
    assert_eq!(
        transaction
            .inverse
            .apply(&after)
            .unwrap()
            .to_json()
            .unwrap(),
        bytes
    );
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

// A no-time/no-crop group adds only the explicit introduced-group relation.
// The complete binding equality catches accidental changes to physical Original
// references, timing layouts, repeat maps, reference order, or capture count.
fn assert_preserved(before: &ProjectDocument, after: &ProjectDocument) {
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.beat_sounds(), before.beat_sounds());
    assert_eq!(
        after.nodes()[&node("owner")],
        before.nodes()[&node("owner")]
    );
    assert_eq!(
        after.nodes()[&node("sibling")],
        before.nodes()[&node("sibling")]
    );
    assert_eq!(
        after.audio_bindings().timings(),
        before.audio_bindings().timings()
    );
    let old = journal(before);
    let new = journal(after);
    assert_eq!(new.scope(), old.scope());
    assert_eq!(new.clocks().len(), old.clocks().len());
    for (old, new) in old.clocks().iter().zip(new.clocks()) {
        assert_eq!(new.timing(), old.timing());
        assert_eq!(new.owner(), old.owner());
        assert_eq!(new.scope(), old.scope());
        assert_eq!(new.repeats(), old.repeats());
        let mut groups = old.introduced_groups().clone();
        groups.insert(node("aligned-group"));
        assert_eq!(new.introduced_groups(), &groups);
        let proof = after.audio_bindings().timings()[new.timing()]
            .sound_owner_clock_correspondence_with_groups(
                &FrozenAudioLayout::capture(after).unwrap(),
                (new.scope(), journal(after).scope()),
                (new.owner(), &node("owner")),
                new.repeats(),
                new.introduced_groups(),
                MAX_DOCUMENT_NODES,
            )
            .unwrap();
        assert_eq!(proof.historical_node(&node("top")), Some(old.scope()));
        assert_eq!(proof.historical_node(&node("aligned-group")), None);
    }
    let expected_journal = SoundClockJournal::new(
        old.scope().clone(),
        old.clocks()
            .iter()
            .map(|reference| {
                let mut groups = reference.introduced_groups().clone();
                groups.insert(node("aligned-group"));
                reference.clone().with_introduced_groups(groups).unwrap()
            })
            .collect(),
    )
    .unwrap();
    let mut expected = serde_json::to_value(before.audio_bindings()).unwrap();
    expected["sound_clocks"] = serde_json::to_value(BTreeMap::from([(
        node("owner"),
        BTreeMap::from([(sound(), expected_journal)]),
    )]))
    .unwrap();
    assert_eq!(
        serde_json::to_value(after.audio_bindings()).unwrap(),
        expected
    );
}

#[test]
fn aligned_source_owner_group_preserves_preexisting_scope_after_controls() {
    witness(Owner::Source);
}
#[test]
fn aligned_room_tone_hold_group_preserves_preexisting_scope_after_controls() {
    witness(Owner::RoomTone);
}
#[test]
fn aligned_silence_hold_group_keeps_suppression_recipe_and_scope_history() {
    witness(Owner::Silence);
}

#[test]
fn chronological_multiple_reference_order_survives_aligned_group_introduction() {
    let initial = fixture(Owner::Source);
    let first = edit(&initial, "prefix", prefix("prefix"));
    let returned = edit(
        &first,
        "returned",
        Command::DeleteRipple {
            node: node("prefix"),
            timing: timing("returned"),
        },
    );
    let before = edit(&returned, "prefix-again", prefix("prefix-again"));
    assert_eq!(before.duration().unwrap(), frames(20));
    assert_eq!(
        journal(&before)
            .clocks()
            .iter()
            .map(|reference| reference.timing().clone())
            .collect::<Vec<_>>(),
        vec![timing("prefix"), timing("returned"), timing("prefix-again")]
    );
    let after = edit(
        &before,
        "aligned",
        group(&before, "top", owner_selection(), "aligned"),
    );
    assert_preserved(&before, &after);
}

#[test]
fn endpoint_splits_direct_group_and_ungroup_within_retained_scope_still_refuse_atomically() {
    let before = edit(&fixture(Owner::Source), "prefix", prefix("prefix"));
    let selection = SliceCaptureSelection::Range {
        range: range(5, 11),
    };
    assert_eq!(
        before
            .group_selection(&node("top"), &selection)
            .unwrap()
            .required_split_ids,
        5
    );
    refuse(
        &before,
        "interior-endpoints",
        group(&before, "top", selection, "interior-endpoints"),
        RETAINED_REFUSAL,
    );
    refuse(
        &before,
        "direct-group",
        Command::Group {
            parent: node("top"),
            start: 0,
            end: 1,
            id: node("direct-group"),
            label: "Not in this slice".into(),
        },
        RETAINED_REFUSAL,
    );
    refuse(
        &before,
        "ungroup-scope",
        Command::Ungroup { node: node("top") },
        RETAINED_REFUSAL,
    );
}

#[derive(Clone, Copy, Debug)]
enum Placement {
    Retime,
    Repeat,
    Partition,
}

fn placement_fixture(placement: Placement) -> ProjectDocument {
    let initial = fixture(Owner::Source);
    let mut wire = serde_json::to_value(initial).unwrap();
    let mut wrapper = BeatNode::sequence("Actual placement history", vec![]);
    wrapper.kind = match placement {
        Placement::Repeat => NodeKind::Repeat {
            child: node("top"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        },
        Placement::Retime | Placement::Partition => NodeKind::Retime {
            child: node("top"),
            duration: frames(15),
            mapping: range(1, 16),
            pitch: PitchPolicy::FollowSpeed,
            purpose: if matches!(placement, Placement::Partition) {
                RetimePurpose::Partition
            } else {
                RetimePurpose::Edit
            },
        },
    };
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Root",
        vec![node("lead"), node("placement")],
    ))
    .unwrap();
    wire["nodes"]["placement"] = serde_json::to_value(wrapper).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn real_retime_repeat_and_partition_placement_histories_still_refuse_group_introduction() {
    for placement in [Placement::Retime, Placement::Repeat, Placement::Partition] {
        // Journals are created by a real public timing edit, never wire-forged.
        let initial = placement_fixture(placement);
        let before = edit(&initial, "prefix", prefix("prefix"));
        assert_eq!(journal(&before).scope(), &node("placement"));
        let expected = match before.group_selection(&node("top"), &owner_selection()) {
            Ok(plan) => {
                assert_eq!(plan.required_split_ids, 0);
                RETAINED_REFUSAL
            }
            Err(error) => {
                assert_eq!(error.code, EditErrorCode::InvalidCommand);
                assert_eq!(
                    error.message,
                    "source splice cannot change a Repeat or Retime ancestor"
                );
                "source splice cannot change a Repeat or Retime ancestor"
            }
        };
        let command = Command::GroupSelection {
            parent: node("top"),
            selection: owner_selection(),
            label: "Unsupported placement".into(),
            identities: GroupSelectionIdentities {
                group: node("aligned-group"),
                split: SplitIdentities { nodes: vec![] },
            },
            timing: timing("aligned"),
        };
        refuse(&before, "aligned", command, expected);
        println!("CONTROL retained {placement:?} placement refusal passed");
    }
}

#[test]
fn root_owned_attachment_transport_still_refuses_atomically() {
    let initial = fixture(Owner::Source);
    let events = initial.beat_sounds()[&node("owner")].clone();
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(node("root"), events)])).unwrap();
    let root_owned = ProjectDocument::from_json(&wire.to_string()).unwrap();
    refuse(
        &root_owned,
        "prefix",
        prefix("prefix"),
        "root-owned beat sounds cannot yet be transported",
    );
}

#[test]
fn missing_historical_owner_is_rejected_at_public_document_admission() {
    let before = edit(&fixture(Owner::Source), "prefix", prefix("prefix"));
    let bytes = before.to_json().unwrap();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["audio_bindings"]["sound_clocks"]["owner"]["effect"]["clocks"][0]["owner"] =
        serde_json::json!("missing-owner");
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    assert_eq!(before.to_json().unwrap(), bytes);
}
