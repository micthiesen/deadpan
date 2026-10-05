use deadpan_core::*;
use serde_json::{Value, json};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn span(start: i64, end: i64, rate: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, rate).unwrap();
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
fn window() -> SourceEditWindow {
    SourceEditWindow::new(ratio(1, 3), ratio(29, 3)).unwrap()
}
fn single_fixture(audio: Option<(i64, i64)>, offset: i64) -> ProjectDocument {
    let picture = span(0, 100, 30);
    let asset = AssetId::new("original").unwrap();
    let sound = audio.map(|(a, b)| span(a, b, 44_100));
    let audio_mapping = audio.map_or(SourceAudioMapping::FitBeat, |(a, b)| {
        let start = ratio(i128::from(a), 1470)
            .checked_sub(ExactRatio::integer(10))
            .unwrap();
        let extent = ratio(i128::from(b - a), 1470);
        let shift = ratio(i128::from(offset), 1600);
        let end = start.checked_add(extent).unwrap();
        let lo = start.checked_add(shift).unwrap();
        let hi = end.checked_add(shift).unwrap();
        let first = if lo.compare(window().start()).is_gt() {
            lo
        } else {
            window().start()
        };
        let last = if hi.compare(window().end()).is_lt() {
            hi
        } else {
            window().end()
        };
        let selection = if first.compare(last).is_lt() {
            ExactFrameRange {
                start: first.checked_sub(shift).unwrap(),
                end: last.checked_sub(shift).unwrap(),
            }
        } else {
            let point = if hi.compare(window().start()).is_le() {
                end
            } else {
                start
            };
            ExactFrameRange {
                start: point,
                end: point,
            }
        };
        SourceAudioMapping::SelectedPlacement {
            start,
            frames: extent,
            selection,
        }
    });
    let source = SourceNode {
        duration: frames(10),
        edit_window: Some(window()),
        video: SourceVideo::Stream {
            asset: asset.clone(),
            span: picture,
        },
        video_mapping: SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::integer(-10),
            frames: ExactRatio::integer(100),
            selection: ExactFrameRange::new(window().start(), window().end()).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        },
        audio: sound.map(|span| SourceAudio { asset, span }),
        audio_mapping,
        audio_offset: AudioSample(offset),
        link: if audio.is_some() {
            LinkRelation::Linked
        } else {
            LinkRelation::Independent
        },
    };
    ProjectDocument::from_json(&json!({
        "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"slip","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","marks":{},"overrides":{},
        "assets":{"original":AssetRecord {label:"Original".into(),content_hash:"a".repeat(64),video:Some(picture),audio:sound,still_image:false,frame_count:Some(frames(100)),source_qualification:Some(SourceQualificationId::new("b".repeat(64)).unwrap())}},
        "nodes":{"root":BeatNode::sequence("Root",vec![id("source")]),"source":BeatNode {label:"Source".into(),framing:None,audio_treatments:Default::default(),audio_editorial_edges: Default::default(), audio_edges:Default::default(),kind:NodeKind::Source {source}, cutaways: Vec::new(), captions: Vec::new() }},
    }).to_string()).unwrap()
}

fn modify(document: &ProjectDocument, update: impl FnOnce(&mut Value)) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    update(&mut wire);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn fixture(audio: Option<(i64, i64)>, offset: i64) -> ProjectDocument {
    modify(&single_fixture(audio, offset), |wire| {
        let source = wire["nodes"]
            .as_object_mut()
            .unwrap()
            .remove("source")
            .unwrap();
        wire["nodes"]["left"] = source.clone();
        wire["nodes"]["right"] = source;
        wire["nodes"]["root"]["kind"]["children"] = json!(["left", "right"]);
    })
}
fn source<'a>(document: &'a ProjectDocument, node: &str) -> &'a SourceNode {
    let NodeKind::Source { source } = &document.nodes()[&id(node)].kind else {
        panic!()
    };
    source
}
fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}
fn crop(node: &str, a: i64, b: i64) -> BeatNode {
    BeatNode {
        label: "Retained view".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(node),
            duration: frames(b - a),
            mapping: range(a, b),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn hold(n: i64) -> BeatNode {
    BeatNode::hold(
        "Silent",
        HoldRecipe {
            duration: frames(n),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn revision(d: &ProjectDocument) -> RevisionId {
    RevisionId::new(format!("{}x", d.revision_id())).unwrap()
}
fn request(d: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: d.project_id().clone(),
        expected_revision: d.revision_id().clone(),
        new_revision: revision(d),
        command,
    }
}
fn intent(i: i64, o: i64, s: i64, r: i64, policy: SourceTrimPolicy) -> SourceTrimIntent {
    SourceTrimIntent {
        in_frames: i,
        out_frames: o,
        slip_frames: s,
        roll_frames: r,
        policy,
    }
}
fn resources(d: &ProjectDocument, r: &SourceTrimEditResolution) -> SourceTrimResources {
    SourceTrimResources {
        target_wrapper: r.required_target_wrapper.then(|| id("a-crop")),
        right_wrapper: r.required_right_wrapper.then(|| id("b-crop")),
        split: SplitIdentities {
            nodes: (0..r.required_split_nodes)
                .map(|n| id(&format!("split-{n}")))
                .collect(),
        },
        fillers: (0..r.required_filler_nodes)
            .map(|n| id(&format!("filler-{n}")))
            .collect(),
        timing: (r.capture != SourceTrimCapture::None).then(|| AudioTimingId {
            allocation: revision(d),
            ordinal: 0,
        }),
    }
}
fn command_for(
    d: &ProjectDocument,
    left: &str,
    right: Option<&str>,
    intent: SourceTrimIntent,
) -> Command {
    let right = right.map(id);
    let r = d
        .source_trim_edit(&id("root"), &id(left), right.as_ref(), intent)
        .unwrap();
    Command::ApplySourceTrim {
        parent: id("root"),
        node: id(left),
        right,
        intent,
        resources: resources(d, &r),
    }
}
fn combined(d: &ProjectDocument, intent: SourceTrimIntent) -> Command {
    command_for(d, "left", Some("right"), intent)
}
fn edit(d: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(d, command);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let tx = apply(d, &request).unwrap();
    let next = tx.forward.apply(d).unwrap();
    assert_eq!(tx.inverse.apply(&next).unwrap(), *d);
    assert_eq!(
        tx.forward.apply(&tx.inverse.apply(&next).unwrap()).unwrap(),
        next
    );
    assert_eq!(
        ProjectDocument::from_json(&next.to_json().unwrap()).unwrap(),
        next
    );
    (next, tx)
}
fn wide(audio: Option<(i64, i64)>) -> ProjectDocument {
    let d = fixture(audio, 0);
    modify(&d, |v| {
        for (name, n) in [("left", 10), ("right", 20)] {
            let mut source = source(&d, name).clone();
            source.duration = frames(n);
            let w = SourceEditWindow::new(ratio(1, 3), ratio(i128::from(n * 3 - 1), 3)).unwrap();
            source.edit_window = Some(w);
            source.video_mapping = SourceVideoMapping::SelectedPlacement {
                start: ratio(-40, 1),
                frames: ratio(100, 1),
                selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
                endpoints: EndpointPolicy::HoldAdjacent,
            };
            if source.audio.is_some() {
                source.audio_mapping = SourceAudioMapping::SelectedPlacement {
                    start: ratio(-40, 1),
                    frames: ratio(100, 1),
                    selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
                };
            }
            v["nodes"][name]["kind"]["source"] = serde_json::to_value(source).unwrap();
        }
        v["nodes"]["prefix"] = serde_json::to_value(hold(10)).unwrap();
        v["nodes"]["suffix"] = serde_json::to_value(hold(20)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "left", "right", "suffix"]);
    })
}
fn intervals(d: &ProjectDocument) -> Vec<(String, FrameRange)> {
    let NodeKind::Sequence { children } = &d.nodes()[&id("root")].kind else {
        panic!()
    };
    let durations = d.durations().unwrap();
    let mut cursor = 0;
    children
        .iter()
        .map(|node| {
            let start = cursor;
            cursor += durations[node].frames();
            (node.to_string(), range(start, cursor))
        })
        .collect()
}
fn mark(
    d: &ProjectDocument,
    name: &str,
    owner: &str,
    coordinate: Anchor,
    bias: InsertionBias,
    policy: AnchorLossPolicy,
) -> ProjectDocument {
    modify(d, |v| {
        v["marks"][name] = serde_json::to_value(Mark {
            owner: id(owner),
            label: name.into(),
            boundary: BoundaryAnchor { coordinate, bias },
            loss_policy: policy,
            state: MarkState::Bound,
            fragments: vec![],
        })
        .unwrap()
    })
}
fn bound(d: &ProjectDocument) -> ProjectDocument {
    let state = capture_unbound_audio_bindings(
        d,
        AudioTimingId {
            allocation: RevisionId::new("old").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    modify(d, |v| {
        v["audio_bindings"] = serde_json::to_value(state).unwrap()
    })
}

#[test]
fn complete_ripple_intent_has_disjoint_old_owner_groups_and_one_history_patch() {
    // Asserts the authored reference representation (every reanchor step
    // and complete timing tables). tests/timing_representation.rs proves the
    // compact storage resolves and renders identically.
    deadpan_core::with_reference_timing_representation(|| {
        let before = wide(Some((0, 147000)));
        let i = intent(2, 5, 3, -1, SourceTrimPolicy::Ripple);
        let resolution = before
            .source_trim_edit(&id("root"), &id("left"), Some(&id("right")), i)
            .unwrap();
        assert_eq!(resolution.geometry.target.output_after, range(10, 22));
        assert_eq!(
            resolution.right_after.as_ref().unwrap().output,
            range(22, 43)
        );
        assert_eq!(resolution.reanchors.len(), 3);
        assert_eq!(resolution.reanchors[0].window.unwrap().start, ratio(12, 1));
        assert_eq!(resolution.reanchors[1].window.unwrap().start, ratio(20, 1));
        assert_eq!(resolution.reanchors[2].window.unwrap().start, ratio(40, 1));
        let (after, tx) = edit(&before, combined(&before, i));
        assert_eq!(tx.duration_delta, 3);
        for owner in ["left", "right", "suffix"] {
            assert_eq!(
                after.audio_bindings().bindings()[&id(owner)]
                    .reanchors
                    .len(),
                1
            );
        }
        assert!(
            after.audio_bindings().bindings()[&id("prefix")]
                .reanchors
                .is_empty()
        );
        assert_eq!(after.audio_bindings().timings().len(), 1);
        assert_eq!(
            source(&after, "left").video_mapping.start_frames(),
            ratio(-43, 1)
        );
    })
}

#[test]
fn disjoint_and_exact_touch_use_chronological_source_endpoints_not_invalid_scalar_edits() {
    // Asserts the authored reference representation (every reanchor step
    // and complete timing tables). tests/timing_representation.rs proves the
    // compact storage resolves and renders identically.
    deadpan_core::with_reference_timing_representation(|| {
        let before = wide(None);
        for (i, o, r, expected) in [
            (15, 0, 9, AudioSourceEndpoint::End),
            (10, 10, 0, AudioSourceEndpoint::End),
            (-10, -10, 0, AudioSourceEndpoint::Start),
        ] {
            let accepted = intent(i, o, 0, r, SourceTrimPolicy::Ripple);
            let (after, _) = edit(&before, combined(&before, accepted));
            assert_eq!(
                after.audio_bindings().bindings()[&id("left")]
                    .reanchors
                    .last()
                    .unwrap()
                    .anchor,
                AudioReanchorAnchor::SourceEndpoint { endpoint: expected }
            );
            assert!(
                after.audio_bindings().bindings()[&id("left")]
                    .reanchors
                    .last()
                    .unwrap()
                    .window
                    .is_none()
            );
            if r == 9 {
                assert_eq!(
                    after.audio_bindings().bindings()[&id("right")]
                        .reanchors
                        .last()
                        .unwrap()
                        .window
                        .unwrap()
                        .start,
                    ratio(29, 1)
                );
            }
        }
    })
}

#[test]
fn overwrite_normal_form_handles_compensation_and_disjoint_swept_silence() {
    let before = wide(None);
    for (i, o, r, a, b, fillers) in [
        (
            5,
            20,
            -15,
            range(15, 25),
            Some(range(25, 40)),
            vec![range(10, 15)],
        ),
        (
            20,
            15,
            0,
            range(30, 35),
            Some(range(35, 40)),
            vec![range(10, 30)],
        ),
        (
            -10,
            -15,
            0,
            range(0, 5),
            Some(range(20, 40)),
            vec![range(5, 20)],
        ),
        (0, 0, 2, range(10, 22), Some(range(22, 40)), vec![]),
        (
            0,
            -5,
            5,
            range(10, 20),
            Some(range(25, 40)),
            vec![range(20, 25)],
        ),
        (
            2,
            -2,
            0,
            range(12, 18),
            Some(range(20, 40)),
            vec![range(10, 12), range(18, 20)],
        ),
        (35, 30, 0, range(45, 50), None, vec![range(10, 45)]),
    ] {
        let accepted = intent(i, o, 0, r, SourceTrimPolicy::Overwrite);
        let resolution = before
            .source_trim_edit(&id("root"), &id("left"), Some(&id("right")), accepted)
            .unwrap();
        assert_eq!(resolution.geometry.target.output_after, a);
        assert_eq!(resolution.right_after.as_ref().map(|s| s.output), b);
        assert_eq!(resolution.fillers, fillers);
        assert!(resolution.reanchors.is_empty());
        let ids = resources(&before, &resolution);
        let result_ids = resolution.result_identities(&ids).unwrap();
        let (after, tx) = edit(
            &before,
            Command::ApplySourceTrim {
                parent: id("root"),
                node: id("left"),
                right: Some(id("right")),
                intent: accepted,
                resources: ids,
            },
        );
        assert_eq!(tx.duration_delta, 0);
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        let placements = intervals(&after);
        assert!(placements.contains(&(result_ids.target.to_string(), a)));
        for (id, expected) in result_ids.fillers.iter().zip(&fillers) {
            assert!(placements.contains(&(id.to_string(), *expected)));
            assert!(
                after.nodes()[id].audio_editorial_edges.start
                    && after.nodes()[id].audio_editorial_edges.end
            );
        }
        if b.is_none() {
            assert!(!after.nodes().contains_key(&id("right")));
        }
    }
}

#[test]
fn all_scalar_paths_preserve_their_exact_source_candidates() {
    let before = fixture(Some((0, 147000)), 17);
    for delta in [-2, 2] {
        for edge in [SourceTrimEdge::In, SourceTrimEdge::Out] {
            let scalar = before
                .source_trim(
                    &id("root"),
                    &id("left"),
                    edge,
                    delta,
                    SourceTrimMode::Ripple,
                )
                .unwrap();
            let accepted = if edge == SourceTrimEdge::In {
                intent(delta, 0, 0, 0, SourceTrimPolicy::Ripple)
            } else {
                intent(0, delta, 0, 0, SourceTrimPolicy::Ripple)
            };
            let (after, _) = edit(&before, combined(&before, accepted));
            assert_eq!(source(&after, "left"), &scalar.after);
        }
        let scalar = before.source_slip(&id("root"), &id("left"), delta).unwrap();
        let (after, _) = edit(
            &before,
            combined(&before, intent(0, 0, delta, 0, SourceTrimPolicy::Ripple)),
        );
        assert_eq!(source(&after, "left"), &scalar.after);
        assert_eq!(after.audio_bindings(), before.audio_bindings());
        let scalar = before
            .source_roll(&id("root"), &id("left"), &id("right"), delta)
            .unwrap();
        let (after, _) = edit(
            &before,
            combined(&before, intent(0, 0, 0, delta, SourceTrimPolicy::Ripple)),
        );
        assert_eq!(source(&after, "left"), &scalar.left.after);
        assert_eq!(source(&after, "right"), &scalar.right.after);
    }
}

#[test]
fn both_physical_prefixes_translate_content_marks_and_retained_clocks_once() {
    let base = wide(Some((0, 147000)));
    let mut before = modify(&base, |v| {
        for name in ["left", "right"] {
            let mut s = source(&base, name).clone();
            s.duration = frames(30);
            let w = SourceEditWindow::new(ratio(1, 3), ratio(89, 3)).unwrap();
            s.edit_window = Some(w);
            s.video_mapping = SourceVideoMapping::SelectedPlacement {
                start: ratio(-40, 1),
                frames: ratio(100, 1),
                selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
                endpoints: EndpointPolicy::HoldAdjacent,
            };
            s.audio_mapping = SourceAudioMapping::SelectedPlacement {
                start: ratio(-40, 1),
                frames: ratio(100, 1),
                selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
            };
            v["nodes"][name]["kind"]["source"] = serde_json::to_value(s).unwrap();
            v["nodes"][name]["audio_treatments"] =
                serde_json::to_value(AudioTreatments::from_clip_gain(
                    ClipGain::new(
                        GainDb::new(-3000).unwrap(),
                        false,
                        vec![],
                        vec![GainRange::new(ratio(3, 1), ratio(4, 1)).unwrap()],
                    )
                    .unwrap(),
                ))
                .unwrap();
            v["nodes"][name]["framing"] = serde_json::to_value(
                Framing::creep(
                    FramingPose::identity(),
                    FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(2, 1)).unwrap(),
                    FramingCurve::Linear,
                )
                .unwrap(),
            )
            .unwrap();
        }
        v["nodes"]["a-view"] = serde_json::to_value(crop("left", 1, 11)).unwrap();
        v["nodes"]["b-view"] = serde_json::to_value(crop("right", 2, 22)).unwrap();
        v["nodes"]["prefix"] = serde_json::to_value(hold(4)).unwrap();
        v["nodes"]["suffix"] = serde_json::to_value(hold(5)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "a-view", "b-view", "suffix"]);
    });
    for name in ["left", "right"] {
        before = mark(
            &before,
            name,
            name,
            Anchor::Local {
                node: id(name),
                position: ratio(3, 1),
            },
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        );
    }
    before = mark(
        &before,
        "ancestor",
        "root",
        Anchor::Local {
            node: id("root"),
            position: ratio(6, 1),
        },
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    before = mark(
        &before,
        "source-pts",
        "root",
        Anchor::Source {
            asset: AssetId::new("original").unwrap(),
            moment: SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: SourceTimestamp {
                    ticks: 43,
                    time_base: SourceTimeBase::new(1, 30).unwrap(),
                },
            },
        },
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    before = mark(
        &before,
        "fixed",
        "root",
        Anchor::Sequence {
            frame: ProjectFrame(6),
        },
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    before = mark(
        &before,
        "leading",
        "left",
        Anchor::Local {
            node: id("left"),
            position: ExactRatio::ZERO,
        },
        InsertionBias::Left,
        AnchorLossPolicy::KeepUnresolved,
    );
    before = modify(&before, |v| {
        let mut frozen: Mark = serde_json::from_value(v["marks"]["left"].clone()).unwrap();
        frozen.state = MarkState::Unresolved {
            reason: MarkLossReason::OutOfRange,
        };
        v["marks"]["frozen"] = serde_json::to_value(frozen).unwrap();
    });
    before = bound(&before);
    let accepted = intent(-3, 3, 0, -5, SourceTrimPolicy::Ripple);
    let command = command_for(&before, "a-view", Some("b-view"), accepted);
    let (after, _) = edit(&before, command);
    for (name, prefix) in [("left", 2), ("right", 3)] {
        assert_eq!(
            after.marks()[&MarkId::new(name).unwrap()]
                .boundary
                .coordinate,
            Anchor::Local {
                node: id(name),
                position: ratio(i128::from(3 + prefix), 1)
            }
        );
        let old = &before.audio_bindings().bindings()[&id(name)];
        let new = &after.audio_bindings().bindings()[&id(name)];
        assert_eq!(
            new.lattice,
            old.rebase_local(ExactRatio::integer(prefix))
                .unwrap()
                .lattice
        );
        assert_eq!(new.reanchors.len(), 1);
        assert_eq!(
            after.nodes()[&id(name)].audio_treatments,
            before.nodes()[&id(name)]
                .audio_treatments
                .with_owner_prefix(frames(prefix))
                .unwrap()
        );
        assert_eq!(
            after.nodes()[&id(name)].framing,
            Some(
                before.nodes()[&id(name)]
                    .framing
                    .as_ref()
                    .unwrap()
                    .prepend_owner_frames(frames(prefix), frames(30))
                    .unwrap()
            )
        );
    }
    assert_eq!(
        after.marks()[&MarkId::new("ancestor").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ratio(9, 1)
        }
    );
    assert_eq!(after.audio_bindings().timings().len(), 2);
    for name in ["source-pts", "fixed", "leading", "frozen"] {
        assert_eq!(
            after.marks()[&MarkId::new(name).unwrap()],
            before.marks()[&MarkId::new(name).unwrap()]
        );
    }
}

#[test]
fn partial_composite_overwrite_retains_full_context_bindings_and_copied_lineage() {
    for flavor in [
        "sequence",
        "repeat",
        "preserve",
        "follow",
        "partition",
        "hold",
    ] {
        let base = fixture(Some((0, 147000)), 0);
        let before = modify(&base, |v| {
            let mut n = BeatNode::sequence(flavor, vec![id("right")]);
            n.kind = match flavor {
                "sequence" => n.kind,
                "repeat" => NodeKind::Repeat {
                    child: id("right"),
                    iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
                    gap: Some(HoldRecipe {
                        duration: frames(2),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    }),
                    escalation: None,
                },
                "preserve" | "follow" => NodeKind::Retime {
                    child: id("right"),
                    duration: frames(20),
                    mapping: range(0, 10),
                    pitch: if flavor == "preserve" {
                        PitchPolicy::Preserve
                    } else {
                        PitchPolicy::FollowSpeed
                    },
                    purpose: RetimePurpose::Edit,
                },
                "partition" => {
                    n.audio_editorial_edges.start = true;
                    n.framing = Some(Framing::static_pose(FramingPose::identity()).unwrap());
                    NodeKind::Retime {
                        child: id("right"),
                        duration: frames(10),
                        mapping: range(0, 10),
                        pitch: PitchPolicy::FollowSpeed,
                        purpose: RetimePurpose::Partition,
                    }
                }
                "hold" => {
                    v["nodes"].as_object_mut().unwrap().remove("right");
                    hold(10).kind
                }
                _ => unreachable!(),
            };
            v["nodes"]["neighbor"] = serde_json::to_value(n).unwrap();
            v["nodes"]["root"]["kind"]["children"] = json!(["left", "neighbor"]);
        });
        let before = bound(&before);
        let accepted = intent(0, 3, 0, 0, SourceTrimPolicy::Overwrite);
        let r = before
            .source_trim_edit(&id("root"), &id("left"), None, accepted)
            .unwrap();
        assert_eq!(r.splits.len(), 1);
        assert_eq!(r.capture, SourceTrimCapture::None);
        let (after, _) = edit(&before, command_for(&before, "left", None, accepted));
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        assert!(!after.nodes().contains_key(&id("neighbor")));
        let (copy_id, _) = after
            .nodes()
            .iter()
            .find(|(id, n)| n.label == flavor && after.audio_lineage().contains_key(*id))
            .unwrap();
        assert!(after.audio_lineage().contains_key(copy_id));
        assert_eq!(after.audio_lineage()[copy_id].origin, id("neighbor"));
        assert_eq!(
            after.audio_bindings().timings(),
            before.audio_bindings().timings()
        );
        assert!(
            after
                .audio_bindings()
                .bindings()
                .values()
                .all(|b| b.reanchors.is_empty())
        );
    }
}

#[test]
fn near_empty_siblings_move_with_a_while_far_endpoints_survive_and_interior_empties_retire() {
    let before = modify(&wide(None), |v| {
        for name in ["near-in", "near-out", "inside", "far"] {
            v["nodes"][name] = serde_json::to_value(BeatNode::sequence(name, vec![])).unwrap();
        }
        v["nodes"]["middle"] = serde_json::to_value(hold(3)).unwrap();
        v["nodes"]["tail"] = serde_json::to_value(hold(7)).unwrap();
        v["nodes"].as_object_mut().unwrap().remove("suffix");
        v["nodes"]["root"]["kind"]["children"] = json!([
            "prefix", "near-in", "left", "near-out", "middle", "inside", "tail", "far", "right"
        ]);
    });
    let accepted = intent(2, 10, 0, 0, SourceTrimPolicy::Overwrite);
    let r = before
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(
        r.empty_moves
            .iter()
            .map(|m| (m.node.to_string(), m.before.0, m.after.0))
            .collect::<Vec<_>>(),
        vec![("near-in".into(), 10, 12), ("near-out".into(), 20, 30)]
    );
    let (after, _) = edit(&before, command_for(&before, "left", None, accepted));
    let positions = intervals(&after);
    assert!(positions.contains(&("near-in".into(), range(12, 12))));
    assert!(positions.contains(&("near-out".into(), range(30, 30))));
    assert!(positions.contains(&("far".into(), range(30, 30))));
    assert!(!after.nodes().contains_key(&id("inside")));
    let at30: Vec<_> = positions
        .iter()
        .filter(|(_, r)| r.start().0 == 30)
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(at30, vec!["near-out", "far", "right"]);
}

#[test]
fn split_occurrence_fragments_receive_loss_only_after_the_final_survivor_exists() {
    let before = modify(&fixture(None, 0), |v| {
        v["nodes"]["neighbor"] =
            serde_json::to_value(BeatNode::sequence("neighbor", vec![id("right")])).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "neighbor"]);
    });
    let mut before = before;
    for (name, pos, bias, policy) in [
        (
            "kept",
            7,
            InsertionBias::Right,
            AnchorLossPolicy::DeleteOwned,
        ),
        (
            "lost",
            1,
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "cut-left",
            3,
            InsertionBias::Left,
            AnchorLossPolicy::DeleteOwned,
        ),
        (
            "cut-right",
            3,
            InsertionBias::Right,
            AnchorLossPolicy::DeleteOwned,
        ),
    ] {
        before = mark(
            &before,
            name,
            "right",
            Anchor::Occurrence {
                instance: InstancePath {
                    node: id("right"),
                    repeats: vec![],
                },
                position: ratio(pos, 1),
            },
            bias,
            policy,
        );
    }
    before = mark(
        &before,
        "ancestor",
        "root",
        Anchor::Local {
            node: id("root"),
            position: ratio(17, 1),
        },
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    before = mark(
        &before,
        "definition",
        "right",
        Anchor::Local {
            node: id("right"),
            position: ratio(1, 1),
        },
        InsertionBias::Right,
        AnchorLossPolicy::DeleteOwned,
    );
    let (after, _) = edit(
        &before,
        command_for(
            &before,
            "left",
            None,
            intent(0, 3, 0, 0, SourceTrimPolicy::Overwrite),
        ),
    );
    assert!(
        after.marks()[&MarkId::new("kept").unwrap()]
            .bindings()
            .all(|m| m.state == MarkState::Bound)
    );
    assert!(
        !after
            .marks()
            .contains_key(&MarkId::new("cut-left").unwrap())
    );
    assert_eq!(
        after.marks()[&MarkId::new("cut-right").unwrap()].state,
        MarkState::Bound
    );
    assert!(matches!(
        after.marks()[&MarkId::new("lost").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    assert_eq!(
        after.marks()[&MarkId::new("ancestor").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: id("root"),
            position: ratio(17, 1)
        }
    );
    let hidden = &after.marks()[&MarkId::new("definition").unwrap()];
    assert_eq!(hidden.state, MarkState::Bound);
    assert_eq!(hidden.binding_count(), 1);
    assert_ne!(hidden.owner, id("right"));
}

fn with_sound(d: &ProjectDocument, issuer: Option<SoundHoldIssuer>) -> ProjectDocument {
    let sound = SoundId::new("sound").unwrap();
    modify(d, |v| {
        v["sounds"] = json!({"sound":SoundEvent{owner:id("root"),label:"Root bus".into(),source:source(d,"left").audio.clone().unwrap(),mapping:SourceAudioMapping::SelectedPlacement{start:ratio(0,1),frames:ratio(100,1),selection:ExactFrameRange::new(ratio(0,1),ExactRatio::integer(d.duration().unwrap().frames())).unwrap()},offset:AudioSample(0),gain_millidecibels:0,start_edge:AudioEdgePolicy::Automatic,end_edge:AudioEdgePolicy::Hard,overflow:SoundOverflowPolicy::Reject}});
        v["sound_routes"] = serde_json::to_value(std::collections::BTreeMap::from([(
            sound.clone(),
            RootSoundRoute::identity(d.duration().unwrap(), d.presentation_basis().frame_rate),
        )]))
        .unwrap();
        if let Some(issuer) = issuer {
            v["sound_allowances"] = serde_json::to_value(std::collections::BTreeMap::from([(
                sound,
                SoundHoldAllowances::try_from(vec![issuer]).unwrap(),
            )]))
            .unwrap();
        }
    })
}
#[test]
fn overwrite_copies_existing_repeat_allowance_but_grants_nothing_to_filler() {
    let plays = IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap();
    let before = modify(&fixture(Some((0, 147000)), 0), |v| {
        v["nodes"]["right"] = serde_json::to_value(hold(10)).unwrap();
        v["nodes"]["neighbor"] = json!({"label":"Repeat","kind":{"type":"repeat","child":"right","iterations":plays,"gap":null}});
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "neighbor"]);
    });
    let before = with_sound(
        &bound(&before),
        Some(SoundHoldIssuer::Node {
            instance: InstancePath {
                node: id("right"),
                repeats: vec![RepeatInstance {
                    node: id("neighbor"),
                    iteration: plays.at(1).unwrap(),
                }],
            },
        }),
    );
    let (after, _) = edit(
        &before,
        command_for(
            &before,
            "left",
            None,
            intent(2, 3, 0, 0, SourceTrimPolicy::Overwrite),
        ),
    );
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    let allowance = &after.sound_allowances()[&SoundId::new("sound").unwrap()];
    assert_eq!(allowance.len(), 1);
    let issuer = allowance.iter().next().unwrap();
    assert_ne!(issuer.instance().node, id("right"));
    assert!(after.nodes().contains_key(&issuer.instance().node));
    assert_ne!(issuer.instance().node, id("filler-0"));
    assert_eq!(
        after.audio_bindings().timings(),
        before.audio_bindings().timings()
    );
}

#[test]
fn root_sound_mapping_uses_the_complete_in_out_intent_once_and_identity_restores_objects() {
    let before = with_sound(&wide(Some((0, 147000))), None);
    for accepted in [
        intent(0, 0, 2, 0, SourceTrimPolicy::Ripple),
        intent(0, 0, 0, 2, SourceTrimPolicy::Ripple),
    ] {
        let (after, _) = edit(&before, combined(&before, accepted));
        assert_eq!(after.sounds(), before.sounds());
        assert_eq!(after.sound_routes(), before.sound_routes());
    }
    for accepted in [
        intent(2, 2, 0, 0, SourceTrimPolicy::Ripple),
        intent(0, -3, 0, 3, SourceTrimPolicy::Ripple),
    ] {
        let (after, _) = edit(&before, combined(&before, accepted));
        let route = &after.sound_routes()[&SoundId::new("sound").unwrap()];
        assert_eq!(route.edits.len(), 1);
        assert_eq!(
            route.edits[0].operation,
            RootSoundOperation::Trim {
                range: range(10, 20),
                in_frames: accepted.in_frames,
                out_frames: accepted.out_frames
            }
        );
    }
}

#[test]
fn raw_zero_bad_resources_stale_targets_and_absence_never_author_or_retarget() {
    let before = wide(None);
    let bytes = before.to_json().unwrap();
    let zero = command_for(&before, "left", None, SourceTrimIntent::default());
    assert!(
        apply(&before, &request(&before, zero))
            .unwrap_err()
            .message
            .contains("no change")
    );
    let r = before
        .source_trim_edit(
            &id("root"),
            &id("left"),
            None,
            intent(0, 0, 1, 0, SourceTrimPolicy::Ripple),
        )
        .unwrap();
    assert!(matches!(
        r.geometry.roll_availability,
        SourceTrimRollAvailability::Unavailable { .. }
    ));
    assert!(
        before
            .source_trim_edit(
                &id("root"),
                &id("left"),
                None,
                intent(0, 0, 0, 1, SourceTrimPolicy::Ripple)
            )
            .is_err()
    );
    let good = combined(&before, intent(2, -2, 0, 0, SourceTrimPolicy::Overwrite));
    for mutate in 0..6 {
        let mut bad = good.clone();
        let Command::ApplySourceTrim {
            resources, right, ..
        } = &mut bad
        else {
            panic!()
        };
        match mutate {
            0 => resources.fillers.push(id("extra")),
            1 => resources.target_wrapper = None,
            2 => resources.fillers[0] = id("left"),
            3 => resources.fillers[1] = resources.fillers[0].clone(),
            4 => resources.timing.as_mut().unwrap().allocation = RevisionId::new("wrong").unwrap(),
            5 => *right = Some(id("prefix")),
            _ => unreachable!(),
        }
        // R=0 explicitly tolerates ineligible captured right; that mutation
        // changes the required generic neighbor treatment, not captured A.
        if mutate != 5 {
            assert!(apply(&before, &request(&before, bad)).is_err());
        } else {
            assert!(apply(&before, &request(&before, bad)).is_ok());
        }
    }
    let mut wrong_pair = combined(&before, intent(0, 0, 0, 1, SourceTrimPolicy::Ripple));
    let Command::ApplySourceTrim { right, .. } = &mut wrong_pair else {
        panic!()
    };
    *right = Some(id("prefix"));
    assert!(apply(&before, &request(&before, wrong_pair)).is_err());
    let mut stale = request(&before, good);
    stale.expected_revision = RevisionId::new("stale").unwrap();
    assert_eq!(
        apply(&before, &stale).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    assert_eq!(before.to_json().unwrap(), bytes);
}

#[test]
fn linked_dormant_support_can_activate_and_absent_audio_stays_absent() {
    for audio in [None, Some((60_000, 147_000))] {
        let before = fixture(audio, 0);
        let accepted = intent(25, 25, 0, 0, SourceTrimPolicy::Ripple);
        let old = source(&before, "left");
        if old.audio.is_some() {
            let w = old.audio_mapping.selection_frames(old.duration).unwrap();
            assert_eq!(w.start, w.end);
        }
        let (after, _) = edit(&before, combined(&before, accepted));
        let next = source(&after, "left");
        if audio.is_none() {
            assert!(next.audio.is_none())
        } else {
            let w = next.audio_mapping.selection_frames(next.duration).unwrap();
            assert!(w.start.compare(w.end).is_lt())
        }
    }
}

#[test]
fn closed_resources_reject_new_vocabulary() {
    let before = wide(None);
    let current = request(
        &before,
        combined(&before, intent(2, -2, 0, 0, SourceTrimPolicy::Overwrite)),
    );
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["command"]["resources"]["fillers"] = json!(["one", "two", "three"]);
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["command"]["resources"]["unexpected"] = json!(null);
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
}

#[test]
fn two_exterior_splits_retain_both_contexts_without_using_a_cached_target_slot() {
    let before = modify(&fixture(Some((0, 147000)), 0), |v| {
        v["nodes"]["lead"] = serde_json::to_value(hold(10)).unwrap();
        v["nodes"]["before"] =
            serde_json::to_value(BeatNode::sequence("before", vec![id("lead")])).unwrap();
        v["nodes"]["after"] =
            serde_json::to_value(BeatNode::sequence("after", vec![id("right")])).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["before", "left", "after"]);
    });
    let accepted = intent(-3, 3, 0, 0, SourceTrimPolicy::Overwrite);
    let r = before
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(r.splits.len(), 2);
    assert_eq!(r.required_split_nodes, 8);
    assert!(r.fillers.is_empty());
    let (after, _) = edit(&before, command_for(&before, "left", None, accepted));
    let placements = intervals(&after);
    assert_eq!(
        placements.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
        vec![range(0, 7), range(7, 23), range(23, 30)]
    );
    assert!(after.nodes().contains_key(&id("before")));
    assert!(!after.nodes().contains_key(&id("after")));
    assert_eq!(source(&after, "left").duration, frames(16));
    assert_eq!(after.audio_bindings().timings().len(), 1);
}

#[test]
fn nested_scope_ripple_captures_ancestor_suffix_but_overwrite_cannot_consume_it() {
    let before = modify(&wide(None), |v| {
        v["nodes"]["group"] =
            serde_json::to_value(BeatNode::sequence("Group", vec![id("left"), id("right")]))
                .unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "group", "suffix"]);
    });
    let accepted = intent(1, 4, 0, -1, SourceTrimPolicy::Ripple);
    let r = before
        .source_trim_edit(&id("group"), &id("left"), Some(&id("right")), accepted)
        .unwrap();
    assert_eq!(r.reanchors.len(), 3);
    assert!(r.reanchors[2].owners.contains(&id("suffix")));
    let command = Command::ApplySourceTrim {
        parent: id("group"),
        node: id("left"),
        right: Some(id("right")),
        intent: accepted,
        resources: resources(&before, &r),
    };
    let (after, _) = edit(&before, command);
    assert_eq!(
        after.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 3
    );
    assert!(
        before
            .source_trim_edit(
                &id("group"),
                &id("left"),
                Some(&id("right")),
                intent(-1, 0, 0, 0, SourceTrimPolicy::Overwrite)
            )
            .is_err()
    );
}

#[test]
fn reanchor_term_limit_fails_before_mutation_and_does_not_replace_the_old_clock() {
    let base = bound(&wide(None));
    let mut owners = base.audio_bindings().bindings().clone();
    let binding = owners.get_mut(&id("left")).unwrap();
    binding.reanchors = vec![
        AudioReanchorStep::for_allocation(
            binding.lattice.clone(),
            Some(ExactFrameRange::new(ratio(10, 1), ratio(20, 1)).unwrap())
        );
        MAX_AUDIO_BINDING_TERMS
    ];
    let state = AudioBindingState::new(
        base.audio_bindings()
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        owners,
    )
    .unwrap();
    let before = modify(&base, |v| {
        v["audio_bindings"] = serde_json::to_value(state).unwrap()
    });
    let bytes = before.to_json().unwrap();
    let error = apply(
        &before,
        &request(
            &before,
            combined(&before, intent(1, 0, 0, 0, SourceTrimPolicy::Ripple)),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert_eq!(before.to_json().unwrap(), bytes);
}

#[test]
fn retained_padding_only_b_tail_keeps_its_full_window_and_is_not_a_retired_owner() {
    let base = wide(None);
    let before = modify(&base, |v| {
        let mut s = source(&base, "right").clone();
        let w = SourceEditWindow::new(ratio(1, 3), ratio(4, 3)).unwrap();
        s.edit_window = Some(w);
        s.video_mapping = SourceVideoMapping::SelectedPlacement {
            start: ratio(-40, 1),
            frames: ratio(100, 1),
            selection: ExactFrameRange::new(w.start(), w.end()).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        v["nodes"]["right"]["kind"]["source"] = serde_json::to_value(s).unwrap();
    });
    let accepted = intent(0, 13, 0, 0, SourceTrimPolicy::Overwrite);
    let r = before
        .source_trim_edit(&id("root"), &id("left"), Some(&id("right")), accepted)
        .unwrap();
    let b = r.right_after.as_ref().unwrap();
    assert_eq!(b.output, range(33, 40));
    assert_eq!(b.allocation, range(13, 20));
    assert!(b.visible_selection.is_none());
    assert!(r.required_right_wrapper);
    let (after, _) = edit(&before, combined(&before, accepted));
    assert_eq!(source(&after, "right"), source(&before, "right"));
    assert!(after.nodes().contains_key(&id("right")));
    assert!(
        after
            .source_trim_geometry(
                &id("root"),
                &id("b-crop"),
                None,
                SourceTrimIntent::default()
            )
            .is_err()
    );
}

/// Empty Sequences add structural capacity without changing picture duration.
/// Callers use unbound fixtures: binding validation has a separate work budget
/// that includes all current nodes and therefore cannot admit a bound 100k tree.
fn at_node_capacity(document: &ProjectDocument, count: usize) -> ProjectDocument {
    assert!(document.nodes().len() <= count);
    assert!(count <= MAX_DOCUMENT_NODES);
    modify(document, |wire| {
        let extra = count - document.nodes().len();
        let ids: Vec<_> = (0..extra).map(|n| id(&format!("capacity-{n}"))).collect();
        for node in &ids {
            wire["nodes"][node.as_str()] =
                serde_json::to_value(BeatNode::sequence("", Vec::new())).unwrap();
        }
        wire["nodes"]["root"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .extend(ids.into_iter().map(|node| json!(node)));
    })
}

#[test]
fn full_capacity_geometry_defers_retired_b_wrapper_without_authorizing_capture() {
    // A[0,10), B[10,20), then only empty structures. The edit grows physical A
    // in place to [0,20) and retires B. No wrapper, filler or Split is needed.
    // The later unbound clock capture has its own limits; this is not authority
    // to bypass them or a claim that an exact-capacity bound tree can exist.
    let small = fixture(None, 0);
    assert!(small.audio_bindings().is_empty());
    let before = at_node_capacity(&small, MAX_DOCUMENT_NODES);
    let accepted = intent(0, 9, 0, 1, SourceTrimPolicy::Overwrite);
    let geometry = before
        .source_trim_geometry(&id("root"), &id("left"), Some(&id("right")), accepted)
        .unwrap();
    assert!(geometry.requires_overwrite_overlay);
    assert_eq!(geometry.required_source_wrappers, 1);
    assert!(!geometry.target.needs_wrapper);
    assert!(geometry.right.as_ref().unwrap().needs_wrapper);
    // The interactive resolver uses the same deferred overwrite admission. A
    // nudge may inspect the candidate even when its provisional B crop would
    // not fit, then reach this final valid no-allocation result immediately.
    let adjustment = before
        .adjust_source_trim_geometry(
            &id("root"),
            &id("left"),
            Some(&id("right")),
            intent(0, 9, 0, 0, SourceTrimPolicy::Overwrite),
            SourceTrimControl::Roll,
            1,
        )
        .unwrap();
    assert_eq!(adjustment.geometry, geometry);
    assert!(adjustment.clamp.is_none());
    let resolution = before
        .source_trim_edit(&id("root"), &id("left"), Some(&id("right")), accepted)
        .unwrap();
    assert_eq!(resolution.capture, SourceTrimCapture::Unbound);
    assert_eq!(resolution.temporary_nodes, MAX_DOCUMENT_NODES);
    assert_eq!(resolution.geometry.target.output_after, range(0, 20));
    assert!(resolution.right_after.is_none());
    assert!(!resolution.required_target_wrapper);
    assert!(!resolution.required_right_wrapper);
    assert_eq!(resolution.required_split_nodes, 0);
    assert_eq!(resolution.required_filler_nodes, 0);
    let original = before.clone();
    let error = apply(&before, &request(&before, combined(&before, accepted))).unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert_eq!(before, original);
}

#[test]
fn overwrite_rejects_real_temporary_split_peak_even_when_final_tree_would_fit() {
    // A[0,10), Sequence(B)[10,20). Keeping the final five frames of that
    // Sequence needs its two-node full copy plus two neutral crop wrappers.
    let small = modify(&fixture(None, 0), |wire| {
        wire["nodes"]["neighbor"] =
            serde_json::to_value(BeatNode::sequence("Neighbor", vec![id("right")])).unwrap();
        wire["nodes"]["root"]["kind"]["children"] = json!(["left", "neighbor"]);
    });
    let accepted = intent(0, 5, 0, 0, SourceTrimPolicy::Overwrite);
    let small_resolution = small
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(small_resolution.capture, SourceTrimCapture::Unbound);
    assert_eq!(small_resolution.required_split_nodes, 4);
    assert_eq!(small_resolution.splits.len(), 1);
    assert!(!small_resolution.required_target_wrapper);
    assert_eq!(small_resolution.required_filler_nodes, 0);
    let (small_after, _) = edit(&small, command_for(&small, "left", None, accepted));
    // The discarded left wrapper and its original two-node context are removed
    // only after the four temporary nodes exist. The final net growth is one.
    assert_eq!(small_after.nodes().len(), small.nodes().len() + 1);
    let before = at_node_capacity(&small, MAX_DOCUMENT_NODES - 3);
    let original = before.clone();
    let geometry = before
        .source_trim_geometry(&id("root"), &id("left"), None, accepted)
        .unwrap();
    assert_eq!(geometry.required_source_wrappers, 0);
    assert!(geometry.requires_overwrite_overlay);
    let error = before
        .source_trim_edit(&id("root"), &id("left"), None, accepted)
        .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("temporary node budget"));
    let authored = Command::ApplySourceTrim {
        parent: id("root"),
        node: id("left"),
        right: None,
        intent: accepted,
        resources: SourceTrimResources {
            split: SplitIdentities {
                nodes: (0..4).map(|n| id(&format!("peak-copy-{n}"))).collect(),
            },
            timing: Some(AudioTimingId {
                allocation: revision(&before),
                ordinal: 0,
            }),
            ..Default::default()
        },
    };
    let error = apply(&before, &request(&before, authored)).unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("temporary node budget"));
    assert_eq!(before, original);
}
