use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn sound() -> SoundId {
    SoundId::new("sound").unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn range(a: i64, b: i64) -> ExactFrameRange {
    ExactFrameRange {
        start: ExactRatio::integer(a),
        end: ExactRatio::integer(b),
    }
}
fn request(doc: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", doc.revision_id())).unwrap(),
        command,
    }
}
fn edit(doc: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let transaction = apply(doc, &request(doc, command)).unwrap();
    let result = transaction.forward.apply(doc).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *doc);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    (result, transaction)
}
fn pause(n: i64) -> HoldRecipe {
    HoldRecipe {
        duration: frames(n),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn fixture() -> ProjectDocument {
    let mut doc = ProjectDocument::new(
        ProjectId::new("routing").unwrap(),
        RevisionId::new("r").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 480_000,
            time_base,
        },
    )
    .unwrap();
    doc = edit(
        &doc,
        Command::AddAsset {
            id: AssetId::new("catalog").unwrap(),
            asset: AssetRecord {
                label: "catalog".into(),
                content_hash: "a".repeat(64),
                audio: Some(span),
                video: None,
                frame_count: None,
                still_image: false,
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        },
    )
    .0;
    doc = edit(
        &doc,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence("Group", vec![node("a"), node("b"), node("c")]),
                    ),
                    (node("a"), BeatNode::hold("a", pause(10))),
                    (node("b"), BeatNode::hold("b", pause(20))),
                    (node("c"), BeatNode::hold("c", pause(70))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    let source = SourceAudio {
        asset: AssetId::new("catalog").unwrap(),
        span,
    };
    let duration = SourceAudioMapping::natural_rate(span, doc.presentation_basis().frame_rate)
        .unwrap()
        .duration_frames(frames(100))
        .unwrap();
    edit(
        &doc,
        Command::SetSound {
            id: sound(),
            event: SoundEvent {
                owner: node("root"),
                label: "Sound".into(),
                source,
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: duration,
                    selection: range(0, 80),
                },
                offset: AudioSample(0),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Automatic,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    )
    .0
}
fn insert(doc: &ProjectDocument, at: i64, name: &str) -> Command {
    Command::InsertTime {
        at: ProjectFrame(at),
        hold: pause(1),
        id: node(name),
        identities: SplitIdentities {
            nodes: (0..10).map(|i| node(&format!("{name}-{i}"))).collect(),
        },
        timing: AudioTimingId {
            allocation: request(doc, Command::DeleteSound { id: sound() }).new_revision,
            ordinal: 0,
        },
    }
}

#[test]
fn nested_pause_captures_once_and_more_than_twenty_one_edits_keep_the_recipe() {
    let original = fixture();
    let recipe = original.sounds()[&sound()].clone();
    let (mut doc, first) = edit(&original, insert(&original, 1, "pause0"));
    assert_eq!(
        doc.sound_routes()[&sound()].edits.len(),
        1,
        "internal Split must not route twice"
    );
    assert_eq!(first.duration_delta, 1);
    assert!(first.changed_ids.contains(&node("root")));
    for n in 1..32 {
        let at = doc.duration().unwrap().frames();
        doc = edit(&doc, insert(&doc, at, &format!("pause{n}"))).0;
    }
    let journal = &doc.sound_routes()[&sound()];
    assert_eq!(journal.edits.len(), 32);
    assert_eq!(journal.recipe_extent, frames(100));
    assert_eq!(doc.sounds()[&sound()], recipe);
    assert_eq!(
        journal.compile().unwrap().output_extent(),
        ExactRatio::integer(132)
    );
    assert!(FrozenAudioContext::capture(&doc).is_err());
    assert!(
        capture_unbound_audio_bindings(
            &doc,
            AudioTimingId {
                allocation: RevisionId::new("blocked").unwrap(),
                ordinal: 0
            }
        )
        .is_err()
    );
    let (removed, _) = edit(
        &doc,
        Command::Delete {
            node: node("group"),
        },
    );
    assert!(removed.sounds().is_empty());
    assert!(removed.sound_routes().is_empty());
    assert_eq!(removed.duration().unwrap(), frames(31));
}

#[test]
fn delete_onset_keeps_recipe_suffix_and_whole_deletion_removes_bus() {
    let original = fixture();
    let (doc, transaction) = edit(&original, Command::Delete { node: node("a") });
    assert_eq!(transaction.duration_delta, -10);
    assert_eq!(doc.sounds(), original.sounds());
    let journal = &doc.sound_routes()[&sound()];
    let route = journal.compile().unwrap();
    let query = route.query(range(0, 1), Default::default()).unwrap();
    assert_eq!(query.slices[0].recipe, Some(range(10, 11)));
    assert_eq!(journal.edits[0].cuts, RootSoundCutEdges::default());
    let (empty, _) = edit(
        &doc,
        Command::Delete {
            node: node("group"),
        },
    );
    assert_eq!(empty.duration().unwrap(), frames(0));
    assert!(empty.sounds().is_empty());
    assert!(empty.sound_routes().is_empty());

    let mut event = original.sounds()[&sound()].clone();
    let SourceAudioMapping::SelectedPlacement { selection, .. } = &mut event.mapping else {
        unreachable!()
    };
    *selection = range(10, 30);
    let (selected, _) = edit(&original, Command::SetSound { id: sound(), event });
    let (removed, _) = edit(&selected, Command::Delete { node: node("b") });
    assert!(
        removed.sounds().is_empty(),
        "fully removed selected support must retire its event"
    );
}

fn sample_clock_fixture(selection: ExactFrameRange) -> ProjectDocument {
    let original = fixture();
    let rate = FrameRate::new(32_000, 1).unwrap();
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["presentation_basis"]["frame_rate"] = json!(rate);
    wire["nodes"]["group"] = json!(BeatNode::sequence("Group", vec![node("a"), node("b")]));
    wire["nodes"]["a"] = json!(BeatNode::hold("a", pause(1)));
    wire["nodes"]["b"] = json!(BeatNode::hold("b", pause(2)));
    wire["nodes"].as_object_mut().unwrap().remove("c");
    let mut event = original.sounds()[&sound()].clone();
    event.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: SourceAudioMapping::natural_rate(event.source.span, rate)
            .unwrap()
            .duration_frames(FrameDuration::ZERO)
            .unwrap(),
        selection,
    };
    wire["sounds"]["sound"] = json!(event);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn splice_one_frame(doc: &ProjectDocument, name: &str) -> Command {
    Command::SpliceSource {
        parent: node("group"),
        index: 1,
        id: node(name),
        label: name.into(),
        source: SourceNode {
            duration: frames(1),
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(doc.sounds()[&sound()].source.clone()),
            audio_mapping: SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: doc.sounds()[&sound()]
                    .mapping
                    .duration_frames(FrameDuration::ZERO)
                    .unwrap(),
                selection: range(0, 1),
            },
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
        timing: AudioTimingId {
            allocation: request(doc, Command::DeleteSound { id: sound() }).new_revision,
            ordinal: 0,
        },
    }
}

#[test]
fn nested_interior_source_splice_routes_the_absolute_boundary_once() {
    let original = fixture();
    let Command::SpliceSource { source, timing, .. } = splice_one_frame(&original, "moment") else {
        panic!("source recipe")
    };
    let (inserted, transaction) = edit(
        &original,
        Command::SpliceSourceAt {
            parent: node("group"),
            target: node("b"),
            at: frames(3),
            source,
            id: node("moment"),
            label: "Original moment".into(),
            identities: SplitIdentities {
                nodes: (0..3).map(|i| node(&format!("split-{i}"))).collect(),
            },
            timing,
        },
    );
    assert_eq!(transaction.duration_delta, 1);
    assert_eq!(inserted.sounds(), original.sounds());
    let journal = &inserted.sound_routes()[&sound()];
    assert_eq!(journal.edits.len(), 1);
    assert_eq!(
        journal.edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(13),
            duration: frames(1),
        }
    );
    assert_eq!(journal.recipe_extent, frames(100));
    assert_eq!(
        journal.compile().unwrap().output_extent(),
        ExactRatio::integer(101)
    );
}

#[test]
fn deletion_keeps_a_physical_sample_outside_its_transported_exact_selection() {
    // RoundEven at 32kfps allocates selection [2,8/3) as sample [3,4).
    // Each insert at frame1 moves that sample by B2-B1 = 3-2 = 1.
    let original = sample_clock_fixture(ExactFrameRange {
        start: ExactRatio::integer(2),
        end: ExactRatio::new(8, 3).unwrap(),
    });
    let event = original.sounds()[&sound()].clone();
    let mut doc = original;
    for index in 0..4 {
        doc = edit(&doc, splice_one_frame(&doc, &format!("splice{index}"))).0;
    }
    // The original b now occupies frames[5,7), physical samples[8,10).
    // Semantic selection[6,20/3) lies inside it, but retained sample7 does not.
    let (retained, _) = edit(&doc, Command::Delete { node: node("b") });
    assert_eq!(retained.duration().unwrap(), frames(5));
    assert_eq!(retained.sounds()[&sound()], event);
    assert_eq!(retained.sound_routes()[&sound()].edits.len(), 5);
    assert_eq!(
        retained.sound_routes()[&sound()].edits[4].operation,
        RootSoundOperation::Delete {
            range: FrameRange::new(ProjectFrame(5), ProjectFrame(7)).unwrap()
        }
    );
    let (removed, _) = edit(
        &retained,
        Command::Delete {
            node: node("group"),
        },
    );
    assert!(removed.sounds().is_empty());
}

#[test]
fn initially_sampleless_selection_keeps_intent_until_logically_deleted() {
    // Both endpoints round to sample3, but the exact nonempty event is accepted
    // authored intent. An unrelated splice must not remove it incidentally.
    let original = sample_clock_fixture(ExactFrameRange {
        start: ExactRatio::new(21, 10).unwrap(),
        end: ExactRatio::new(11, 5).unwrap(),
    });
    let event = original.sounds()[&sound()].clone();
    let (shifted, _) = edit(&original, splice_one_frame(&original, "splice"));
    assert_eq!(shifted.sounds()[&sound()], event);
    assert_eq!(shifted.sound_routes()[&sound()].edits.len(), 1);
    let (removed, _) = edit(&shifted, Command::Delete { node: node("b") });
    assert!(removed.sounds().is_empty());
    assert!(removed.sound_routes().is_empty());
}

#[test]
fn translated_support_clips_before_narrowing_extreme_sample_labels() {
    let rate = FrameRate::new(240_000, 7).unwrap();
    let end = 6_588_122_883_467_697_005;
    let old_end = end - 1;
    assert_eq!(
        rate.audio_boundary(ProjectFrame(end)).unwrap(),
        AudioSample(i64::MAX)
    );
    assert_eq!(
        rate.audio_boundary(ProjectFrame(old_end)).unwrap(),
        AudioSample(i64::MAX - 1)
    );
    let original = fixture();
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["presentation_basis"]["frame_rate"] = json!(rate);
    wire["nodes"] = json!({
        "root": BeatNode::sequence("Root", vec![node("whole")]),
        "whole": BeatNode::hold("Whole", pause(end)),
    });
    let mut event = original.sounds()[&sound()].clone();
    event.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::integer(old_end - 1),
        frames: SourceAudioMapping::natural_rate(event.source.span, rate)
            .unwrap()
            .duration_frames(FrameDuration::ZERO)
            .unwrap(),
        selection: range(old_end - 1, old_end),
    };
    wire["sounds"]["sound"] = json!(event);
    wire["sound_routes"] = json!({"sound": RootSoundRoute {
        recipe_extent: frames(old_end),
        recipe_grid: RootSoundGrid::root(rate),
        edits: vec![RootSoundEdit {
            grid: RootSoundGrid::root(rate),
            operation: RootSoundOperation::Insert { at: ProjectFrame(1), duration: frames(1) },
            cuts: Default::default(),
        }],
    }});
    // The suffix shift is B2-B1 = 2 samples. Its selected endpoint becomes
    // i64::MAX+1 before clipping, but the surviving final sample is valid.
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(doc.duration().unwrap(), frames(end));
    assert!(doc.sounds().contains_key(&sound()));
    assert_eq!(
        ProjectDocument::from_json(&doc.to_json().unwrap()).unwrap(),
        doc
    );
}

#[test]
fn splice_and_nonroot_split_preserve_routing_while_root_split_stays_closed() {
    let original = fixture();
    let command = Command::SpliceSource {
        parent: node("group"),
        index: 1,
        id: node("pasted"),
        label: "Pasted".into(),
        source: SourceNode {
            duration: frames(3),
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(original.sounds()[&sound()].source.clone()),
            audio_mapping: SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: original.sounds()[&sound()]
                    .mapping
                    .duration_frames(frames(100))
                    .unwrap(),
                selection: range(0, 3),
            },
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
        timing: AudioTimingId {
            allocation: request(&original, Command::DeleteSound { id: sound() }).new_revision,
            ordinal: 0,
        },
    };
    let (doc, _) = edit(&original, command);
    assert_eq!(
        doc.sound_routes()[&sound()].edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(10),
            duration: frames(3)
        }
    );
    let (split, _) = edit(
        &doc,
        Command::Split {
            node: node("b"),
            at: frames(2),
            identities: SplitIdentities {
                nodes: vec![node("left"), node("right"), node("context")],
            },
        },
    );
    assert_eq!(split.sound_routes(), doc.sound_routes());
    assert_eq!(split.sounds(), doc.sounds());
    assert!(
        apply(
            &doc,
            &request(
                &doc,
                Command::Split {
                    node: node("root"),
                    at: frames(2),
                    identities: Default::default()
                }
            )
        )
        .is_err()
    );
}

#[test]
fn parameters_preserve_history_replacement_is_explicit_and_route_patches_are_guarded() {
    let original = fixture();
    let (doc, _) = edit(&original, insert(&original, 1, "pause"));
    let mut event = doc.sounds()[&sound()].clone();
    event.label = "Gain edit".into();
    event.gain_millidecibels = -3000;
    event.start_edge = AudioEdgePolicy::Hard;
    let (changed, _) = edit(
        &doc,
        Command::SetSound {
            id: sound(),
            event: event.clone(),
        },
    );
    assert_eq!(changed.sound_routes(), doc.sound_routes());
    event.offset = AudioSample(1);
    assert!(
        apply(
            &changed,
            &request(
                &changed,
                Command::SetSound {
                    id: sound(),
                    event: event.clone()
                }
            )
        )
        .is_err()
    );
    let (replaced, transaction) = edit(&changed, Command::ReplaceSound { id: sound(), event });
    assert!(replaced.sound_routes().is_empty());
    assert!(transaction.forward.sound_routes.contains_key(&sound()));
    let mut bad = transaction.forward.clone();
    bad.sound_routes.get_mut(&sound()).unwrap().before = None;
    assert_eq!(
        bad.apply(&changed).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
}

#[test]
fn routed_wire_admission_checks_clocks_ownership_capacity_and_frozen_context() {
    let original = fixture();
    let (doc, _) = edit(&original, insert(&original, 1, "pause"));
    for mutation in ["rate", "origin", "extent", "orphan"] {
        let mut wire = serde_json::to_value(&doc).unwrap();
        match mutation {
            "rate" => {
                wire["sound_routes"]["sound"]["edits"][0]["grid"]["frame_rate"] =
                    json!({"numerator": 24, "denominator": 1})
            }
            "origin" => {
                wire["sound_routes"]["sound"]["recipe_grid"]["frame_origin"] =
                    serde_json::to_value(ExactRatio::ONE).unwrap()
            }
            "extent" => {
                wire["sound_routes"]["sound"]["recipe_extent"] =
                    serde_json::to_value(frames(101)).unwrap()
            }
            "orphan" => {
                wire["sounds"].as_object_mut().unwrap().clear();
            }
            _ => unreachable!(),
        }
        assert!(
            ProjectDocument::from_json(&wire.to_string()).is_err(),
            "accepted {mutation}"
        );
    }
    let mut journal = doc.sound_routes()[&sound()].clone();
    journal
        .edits
        .resize(MAX_ROOT_SOUND_EDITS + 1, journal.edits[0]);
    assert!(journal.compile().is_err());
    assert!(
        serde_json::from_value::<RootSoundRoute>(serde_json::to_value(journal).unwrap()).is_err()
    );

    let mut old_wire = serde_json::to_value(&original).unwrap();
    old_wire["schema_version"] = json!(29);
    let old = legacy_v29::Document::from_json(&old_wire.to_string()).unwrap();
    assert!(old.matches(&original));
    for command in [
        Command::Delete { node: node("a") },
        insert(&original, 1, "injected"),
        Command::Split {
            node: node("a"),
            at: frames(1),
            identities: Default::default(),
        },
    ] {
        let request = request(&original, command);
        let upgraded =
            legacy_v29::upgrade_request(&serde_json::to_string(&request).unwrap()).unwrap();
        assert!(legacy_v29::validate_request_context(&original, &upgraded).is_err());
    }
    let replacement = request(
        &original,
        Command::ReplaceSound {
            id: sound(),
            event: original.sounds()[&sound()].clone(),
        },
    );
    assert!(legacy_v29::upgrade_request(&serde_json::to_string(&replacement).unwrap()).is_err());
}

#[test]
fn deep_history_and_large_repeat_queries_use_bounded_iterative_work() {
    std::thread::Builder::new()
        .stack_size(96 * 1024)
        .spawn(|| {
            let mut nodes = vec![SoundRouteNode::Recipe {}];
            for input in 0..2048 {
                nodes.push(SoundRouteNode::Window {
                    input,
                    selection: range(0, 1),
                });
            }
            let route = SoundRoute::new(ExactRatio::ONE, 2048, nodes).unwrap();
            assert_eq!(
                route.query(range(0, 1), Default::default()).unwrap().slices[0].recipe,
                Some(range(0, 1))
            );
            assert!(
                route
                    .query(
                        range(0, 1),
                        SoundRouteQueryLimits {
                            maximum_work: 100,
                            maximum_spans: 1
                        }
                    )
                    .is_err()
            );
            let map = SoundRippleMap::new(
                ExactRatio::integer(1_000_000_000),
                1,
                vec![
                    SoundRippleNode::Keep { range: range(0, 1) },
                    SoundRippleNode::Repeat {
                        body: 0,
                        count: 1_000_000_000,
                        input_stride: ExactRatio::ONE,
                    },
                ],
            )
            .unwrap();
            let route = SoundRoute::identity(ExactRatio::integer(1_000_000_000))
                .unwrap()
                .ripple(map)
                .unwrap();
            assert!(
                route
                    .query(
                        range(0, 1_000_000_000),
                        SoundRouteQueryLimits {
                            maximum_work: 50,
                            maximum_spans: 5
                        }
                    )
                    .is_err()
            );
            assert_eq!(
                route
                    .query(range(999_999_999, 1_000_000_000), Default::default())
                    .unwrap()
                    .slices[0]
                    .recipe,
                Some(range(999_999_999, 1_000_000_000))
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
