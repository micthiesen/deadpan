//! Public Core counterfactual for GroupSelection endpoint Splits.
//!
//! The AssetRecord is copied from an actual qualified cfr-bframes import.
//! The graph, event and treatments below are synthetic authored recipes.
//! This structural test neither decodes media nor grants host admission.
//! Positive controls precede the desired acceptance assertion, which is
//! expected to fail on the old code at the exact endpoint-Split guard.

use deadpan_core::*;
use std::collections::BTreeMap;

const OLD_REFUSAL: &str = "group endpoint splits cannot yet preserve beat-owned sound clocks";

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
        "Synthetic silent Hold",
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
    // Exact AssetRecord from history[1].edit.forward.assets[asset].after in
    // the retained transcript-search-portable.deadpan project. Provenance is
    // recorded alongside this draft in qualified-asset-metadata.json.
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

fn fixture(source_owner: bool) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("sound-group-endpoints-public-control").unwrap(),
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
    let mut owner = hold(12);
    owner.label = "Synthetic treated complete owner".into();
    owner.audio_treatments = treatment([3000, -1500, 1000]);
    if source_owner {
        // Exact first twelve qualified pictures, plus an independent finite
        // ten-frame audio recipe. The Source duration remains twelve frames.
        let audio_span = span(0, 16_016, 48_000);
        owner.kind = NodeKind::Source {
            source: SourceNode {
                duration: frames(12),
                edit_window: None,
                video: SourceVideo::Stream {
                    asset: asset(),
                    span: span(0, 12_012, 30_000),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: asset(),
                    span: audio_span,
                }),
                audio_mapping: SourceAudioMapping::natural_rate(audio_span, rate).unwrap(),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        };
    }
    let audio_span = span(0, 16_016, 48_000);
    let event = BeatSound {
        label: "Synthetic finite independent event".into(),
        source: SourceAudio {
            asset: asset(),
            span: audio_span,
        },
        mapping: SourceAudioMapping::natural_rate(audio_span, rate).unwrap(),
        offset: AudioSample(37),
        treatments: treatment([6000, -6000, 3000]),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    assert_eq!(
        event.mapping.duration_frames(frames(12)).unwrap(),
        ExactRatio::integer(10)
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence(
                "Ordinary root",
                vec![node("lead"), node("owner"), node("tail")],
            ),
        ),
        (node("lead"), hold(2)),
        (node("owner"), owner),
        (node("tail"), hold(2)),
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

fn apply_control(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let frozen = document.to_json().unwrap();
    let request = request(document, name, command);
    let decoded: CommandRequest =
        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(decoded, request);
    let transaction = apply(document, &decoded).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    let restored = transaction.inverse.apply(&after).unwrap();
    assert_eq!(restored, *document);
    assert_eq!(restored.to_json().unwrap(), frozen);
    assert_eq!(document.to_json().unwrap(), frozen);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

fn group(document: &ProjectDocument, name: &str, selection: SliceCaptureSelection) -> Command {
    let plan = document.group_selection(&node("root"), &selection).unwrap();
    Command::GroupSelection {
        parent: node("root"),
        selection,
        label: "Synthetic neutral group".into(),
        identities: GroupSelectionIdentities {
            group: node(&format!("{name}-group")),
            split: SplitIdentities {
                nodes: (0..plan.required_split_ids)
                    .map(|ordinal| node(&format!("{name}-split-{ordinal}")))
                    .collect(),
            },
        },
        timing: timing(name),
    }
}

fn witness(source_owner: bool) {
    let initial = fixture(source_owner);
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    let before = apply_control(
        &initial,
        "prefix",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: node("prefix"),
            identities: SplitIdentities::default(),
            timing: timing("prefix"),
        },
    );
    assert_eq!(before.duration().unwrap(), frames(17));
    assert_eq!(
        before.source_splice_boundary(&node("root"), 2).unwrap(),
        ProjectFrame(3)
    );
    assert_eq!(before.node_duration(&node("owner")).unwrap(), frames(12));
    assert_eq!(sample_boundary(3), 4805);
    assert_eq!(sample_boundary(5), 8008);
    assert_eq!(sample_boundary(11), 17_618);
    assert_eq!(sample_boundary(17), 27_227);
    assert_eq!(sample_boundary(4) - sample_boundary(3), 1601);
    assert_eq!(sample_boundary(3) - sample_boundary(2), 1602);
    let journal = &before.audio_bindings().sound_clocks()[&node("owner")][&sound()];
    assert_eq!(journal.scope(), &node("owner"));
    assert_ne!(journal.scope(), before.root());
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(journal.clocks()[0].timing(), &timing("prefix"));
    assert_eq!(journal.clocks()[0].scope(), &node("owner"));
    assert_eq!(journal.clocks()[0].owner(), &node("owner"));

    // Independent old-code positive control: one plain Split, with a public
    // caller-owned identity for each partition and the cloned complete owner.
    let split = apply_control(
        &before,
        "plain-split",
        Command::Split {
            node: node("owner"),
            at: frames(4),
            identities: SplitIdentities {
                nodes: vec![
                    node("plain-left"),
                    node("plain-right"),
                    node("plain-owner-copy"),
                ],
            },
        },
    );
    assert_eq!(split.duration().unwrap(), frames(17));
    assert_eq!(split.beat_sounds().len(), 2);
    for (owner, events) in split.beat_sounds() {
        assert_eq!(events, &before.beat_sounds()[&node("owner")]);
        let clocks = &split.audio_bindings().sound_clocks()[owner][&sound()];
        assert_eq!(clocks.clocks().len(), 2);
        assert_eq!(clocks.clocks()[0], journal.clocks()[0]);
        assert_eq!(clocks.clocks()[1].timing(), &timing("plain-split"));
    }
    println!("CONTROL plain Split passed, source_owner={source_owner}; inverse bytes identical");

    // Independent old-code control: aligned grouping at root, outside the
    // owner journal's scope, must not capture or change any timing table.
    let aligned = SliceCaptureSelection::Child {
        node: node("owner"),
    };
    assert_eq!(
        before
            .group_selection(before.root(), &aligned)
            .unwrap()
            .required_split_ids,
        0
    );
    let grouped = apply_control(&before, "aligned", group(&before, "aligned", aligned));
    assert_eq!(grouped.audio_bindings(), before.audio_bindings());
    assert_eq!(grouped.beat_sounds(), before.beat_sounds());
    assert_eq!(grouped.duration().unwrap(), frames(17));
    let ungrouped = apply_control(
        &grouped,
        "aligned-ungroup",
        Command::Ungroup {
            node: node("aligned-group"),
        },
    );
    assert_eq!(ungrouped.nodes(), before.nodes());
    assert_eq!(ungrouped.audio_bindings(), before.audio_bindings());
    assert_eq!(ungrouped.beat_sounds(), before.beat_sounds());
    println!("CONTROL aligned GroupSelection passed, source_owner={source_owner}; no new clocks");

    // The owner occupies absolute [3,15). Both [5,11) endpoints lie strictly
    // inside it, while the ordinary root remains outside every saved scope.
    let selection = SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(5), ProjectFrame(11)).unwrap(),
    };
    let plan = before.group_selection(before.root(), &selection).unwrap();
    assert_eq!(
        plan.range,
        FrameRange::new(ProjectFrame(5), ProjectFrame(11)).unwrap()
    );
    assert_eq!(
        plan.required_split_ids, 5,
        "one leaf Split and one refinement"
    );
    let frozen = before.to_json().unwrap();
    let command = group(&before, "endpoints", selection);
    let result = apply(&before, &request(&before, "endpoints", command));
    assert_eq!(before.to_json().unwrap(), frozen);
    if let Err(error) = &result {
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert_eq!(
            error.message, OLD_REFUSAL,
            "the old-code failure must be the endpoint guard"
        );
    }
    assert!(
        result.is_ok(),
        "two-endpoint GroupSelection should preserve the independently proven sound Split transport; source_owner={source_owner}, error={:?}",
        result.as_ref().err(),
    );

    // These become reachable only after the desired acceptance is implemented.
    let transaction = result.unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(after.duration().unwrap(), frames(17));
    assert_eq!(
        after.node_duration(&node("endpoints-group")).unwrap(),
        frames(6)
    );
    assert_eq!(after.beat_sounds().len(), 3);
    for events in after.beat_sounds().values() {
        assert_eq!(events, &before.beat_sounds()[&node("owner")]);
    }
    assert_eq!(
        transaction
            .inverse
            .apply(&after)
            .unwrap()
            .to_json()
            .unwrap(),
        frozen
    );
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

#[test]
fn hold_owner_group_endpoints_follow_plain_split_and_aligned_group_controls() {
    witness(false);
}

#[test]
fn source_owner_group_endpoints_follow_plain_split_and_aligned_group_controls() {
    witness(true);
}

#[test]
fn root_owned_attachment_keeps_aligned_groups_and_refuses_endpoint_transport() {
    let initial = fixture(false);
    let event = initial.beat_sounds()[&node("owner")][&sound()].clone();
    let mut wire = serde_json::to_value(&initial).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        initial.root().clone(),
        BTreeMap::from([(sound(), event)]),
    )]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let aligned = apply_control(
        &before,
        "root-aligned",
        group(
            &before,
            "root-aligned",
            SliceCaptureSelection::Child {
                node: node("owner"),
            },
        ),
    );
    assert_eq!(aligned.beat_sounds(), before.beat_sounds());
    assert_eq!(aligned.audio_bindings(), before.audio_bindings());
    let command = group(
        &before,
        "root-endpoints",
        SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(5), ProjectFrame(11)).unwrap(),
        },
    );
    let frozen = before.to_json().unwrap();
    let error = apply(&before, &request(&before, "root-endpoints", command)).unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert_eq!(
        error.message,
        "root-owned beat sounds cannot yet be transported"
    );
    assert_eq!(before.to_json().unwrap(), frozen);
}

#[test]
fn endpoint_sound_alias_limit_refuses_without_changing_the_entry_document() {
    let initial = fixture(true);
    let event = initial.beat_sounds()[&node("owner")][&sound()].clone();
    let mut wire = serde_json::to_value(&initial).unwrap();
    wire["beat_sounds"] = serde_json::to_value(BTreeMap::from([(
        node("owner"),
        (0..64)
            .map(|index| {
                (
                    SoundId::new(format!("event-{index}")).unwrap(),
                    event.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let command = group(
        &before,
        "limit-endpoints",
        SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(5), ProjectFrame(11)).unwrap(),
        },
    );
    let frozen = before.to_json().unwrap();
    let error = apply(&before, &request(&before, "limit-endpoints", command)).unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("sound"), "{error}");
    assert_eq!(before.to_json().unwrap(), frozen);
}
