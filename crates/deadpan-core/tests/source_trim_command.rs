use deadpan_core::*;
use serde_json::{Value, json};

#[path = "source_trim_command/editorial.rs"]
mod editorial;

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
fn fixture(audio: Option<(i64, i64)>, offset: i64) -> ProjectDocument {
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
        "nodes":{"root":BeatNode::sequence("Root",vec![id("source")]),"source":BeatNode {label:"Source".into(),framing:None,audio_treatments:Default::default(),audio_editorial_edges: Default::default(), audio_edges:Default::default(),kind:NodeKind::Source {source}}},
    }).to_string()).unwrap()
}
fn source(document: &ProjectDocument) -> &SourceNode {
    let NodeKind::Source { source } = &document.nodes()[&id("source")].kind else {
        panic!()
    };
    source
}
fn modify(document: &ProjectDocument, update: impl FnOnce(&mut Value)) -> ProjectDocument {
    let mut value = serde_json::to_value(document).unwrap();
    update(&mut value);
    ProjectDocument::from_json(&value.to_string()).unwrap()
}

fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
        command,
    }
}
fn trim(document: &ProjectDocument, target: &str, edge: SourceTrimEdge, delta: i64) -> Command {
    let resolved = document
        .source_trim(
            &id("root"),
            &id(target),
            edge,
            delta,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    Command::TrimSource {
        parent: id("root"),
        node: id(target),
        edge,
        delta_frames: delta,
        mode: SourceTrimMode::Ripple,
        wrapper: resolved
            .needs_wrapper
            .then(|| id(&format!("{}-crop", document.revision_id()))),
        timing: AudioTimingId {
            allocation: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
            ordinal: 0,
        },
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let request = request(document, command);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let transaction: EditTransaction =
        serde_json::from_str(&serde_json::to_string(&transaction).unwrap()).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        transaction.duration_delta,
        after.duration().unwrap().frames() - document.duration().unwrap().frames()
    );
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn only_child(document: &ProjectDocument) -> NodeId {
    let NodeKind::Sequence { children } = &document.nodes()[&id("root")].kind else {
        panic!()
    };
    assert_eq!(children.len(), 1);
    children[0].clone()
}

#[test]
fn trim_both_edges_grows_only_the_physical_owner_and_round_trips_exactly() {
    for edge in [SourceTrimEdge::In, SourceTrimEdge::Out] {
        for delta in [-3, 3] {
            let before = fixture(Some((0, 147000)), 7);
            let command = trim(&before, "source", edge, delta);
            let after = edit(&before, command);
            let extends = matches!(
                (edge, delta),
                (SourceTrimEdge::In, -3) | (SourceTrimEdge::Out, 3)
            );
            assert_eq!(
                after.duration().unwrap(),
                frames(if extends { 13 } else { 7 })
            );
            assert_eq!(
                source(&after).duration,
                frames(if extends { 13 } else { 10 })
            );
            assert_eq!(source(&after).video, source(&before).video);
            assert_eq!(source(&after).audio, source(&before).audio);
            assert_eq!(source(&after).audio_offset, AudioSample(7));
            assert_eq!(after.assets(), before.assets());
            if extends {
                assert_eq!(only_child(&after), id("source"));
                assert_eq!(
                    source(&after).edit_window,
                    Some(SourceEditWindow::new(ratio(1, 3), ratio(38, 3)).unwrap())
                );
                assert_eq!(
                    source(&after).video_mapping.start_frames(),
                    ExactRatio::integer(if edge == SourceTrimEdge::In { -7 } else { -10 })
                );
            } else {
                let crop = only_child(&after);
                assert_ne!(crop, id("source"));
                let NodeKind::Retime {
                    child,
                    mapping,
                    duration,
                    purpose,
                    ..
                } = &after.nodes()[&crop].kind
                else {
                    panic!()
                };
                assert_eq!(child, &id("source"));
                assert_eq!(*purpose, RetimePurpose::Partition);
                assert_eq!(*duration, frames(7));
                assert_eq!(
                    *mapping,
                    FrameRange::new(
                        ProjectFrame(if edge == SourceTrimEdge::In { 3 } else { 0 }),
                        ProjectFrame(if edge == SourceTrimEdge::In { 10 } else { 7 })
                    )
                    .unwrap()
                );
                let restored = edit(&after, trim(&after, crop.as_str(), edge, -delta));
                assert_eq!(
                    only_child(&restored),
                    crop,
                    "existing wrapper identity remains"
                );
                assert_eq!(restored.duration().unwrap(), frames(10));
                assert_eq!(source(&restored), source(&before));
            }
        }
    }
}

#[test]
fn trim_capture_precedes_growth_and_retains_effect_clocks_and_root_sound_once() {
    let base = fixture(Some((0, 147000)), 0);
    let old_timing = AudioTimingId {
        allocation: RevisionId::new("old-clock").unwrap(),
        ordinal: 0,
    };
    let bindings = capture_unbound_audio_bindings(&base, old_timing.clone()).unwrap();
    let before = modify(&base, |v| {
        v["audio_bindings"] = serde_json::to_value(&bindings).unwrap();
        v["audio_lineage"] = json!({"source":{"allocation":"initial","origin":"old-source"}});
        v["nodes"]["source"]["framing"] = serde_json::to_value(
            Framing::creep(
                FramingPose::identity(),
                FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(2)).unwrap(),
                FramingCurve::Linear,
            )
            .unwrap(),
        )
        .unwrap();
        v["nodes"]["root"]["framing"] = v["nodes"]["source"]["framing"].clone();
        v["nodes"]["source"]["audio_treatments"] =
            serde_json::to_value(AudioTreatments::from_clip_gain(
                ClipGain::new(
                    GainDb::new(-3000).unwrap(),
                    false,
                    vec![],
                    vec![GainRange::new(ExactRatio::integer(2), ExactRatio::integer(3)).unwrap()],
                )
                .unwrap(),
            ))
            .unwrap();
        v["sounds"] = json!({"sound":SoundEvent {
            owner:id("root"),label:"Independent sound".into(),source:source(&base).audio.clone().unwrap(),
            mapping:SourceAudioMapping::SelectedPlacement{start:ExactRatio::ZERO,frames:ExactRatio::integer(100),selection:ExactFrameRange::new(ExactRatio::integer(1),ExactRatio::integer(2)).unwrap()},
            offset:AudioSample(0),gain_millidecibels:0,start_edge:AudioEdgePolicy::Hard,end_edge:AudioEdgePolicy::Hard,overflow:SoundOverflowPolicy::Reject,
        }});
    });
    let after = edit(&before, trim(&before, "source", SourceTrimEdge::In, -3));
    assert_eq!(
        after.audio_bindings().timings()[&old_timing],
        before.audio_bindings().timings()[&old_timing]
    );
    let old = &before.audio_bindings().bindings()[&id("source")];
    let new = &after.audio_bindings().bindings()[&id("source")];
    assert_eq!(
        new.lattice,
        old.rebase_local(ExactRatio::integer(3)).unwrap().lattice
    );
    assert_eq!(new.reanchors.len(), old.reanchors.len() + 1);
    assert!(after.audio_lineage().is_empty());
    let physical = &after.nodes()[&id("source")];
    assert_eq!(
        physical.framing,
        Some(
            before.nodes()[&id("source")]
                .framing
                .as_ref()
                .unwrap()
                .prepend_owner_frames(frames(3), frames(10))
                .unwrap()
        )
    );
    assert_eq!(
        physical.audio_treatments,
        before.nodes()[&id("source")]
            .audio_treatments
            .with_owner_prefix(frames(3))
            .unwrap()
    );
    assert_eq!(
        after.nodes()[&id("root")].framing,
        before.nodes()[&id("root")].framing
    );
    assert_eq!(after.sounds(), before.sounds());
    let route = &after.sound_routes()[&SoundId::new("sound").unwrap()];
    assert_eq!(route.edits.len(), 1);
    assert_eq!(
        route.edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(0),
            duration: frames(3)
        }
    );
}

fn add_mark(
    document: &ProjectDocument,
    name: &str,
    coordinate: Anchor,
    bias: InsertionBias,
) -> ProjectDocument {
    edit(
        document,
        Command::SetMark {
            id: MarkId::new(name).unwrap(),
            owner: id("source"),
            label: name.into(),
            boundary: BoundaryAnchor { coordinate, bias },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}
#[test]
fn trim_marks_distinguish_stored_source_coordinates_from_visible_occurrences() {
    let mut before = fixture(None, 0);
    for (name, coordinate) in [
        (
            "physical",
            Anchor::Local {
                node: id("source"),
                position: ExactRatio::integer(8),
            },
        ),
        (
            "ancestor",
            Anchor::Local {
                node: id("root"),
                position: ExactRatio::integer(8),
            },
        ),
        (
            "occurrence",
            Anchor::Occurrence {
                instance: InstancePath {
                    node: id("source"),
                    repeats: vec![],
                },
                position: ExactRatio::integer(8),
            },
        ),
        (
            "pts",
            Anchor::Source {
                asset: AssetId::new("original").unwrap(),
                moment: SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp: SourceTimestamp {
                        ticks: 18,
                        time_base: SourceTimeBase::new(1, 30).unwrap(),
                    },
                },
            },
        ),
    ] {
        before = add_mark(&before, name, coordinate, InsertionBias::Right);
    }
    let after = edit(&before, trim(&before, "source", SourceTrimEdge::Out, -4));
    let index = AnchorIndex::new(&after).unwrap();
    for name in ["physical", "pts"] {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(
            mark,
            before.marks().get(&MarkId::new(name).unwrap()).unwrap()
        );
        assert_eq!(mark.state, MarkState::Bound);
        assert_eq!(
            index
                .resolve_target(&AnchorTarget {
                    boundary: mark.boundary.clone(),
                    occurrence: Some(InstancePath {
                        node: id("source"),
                        repeats: vec![]
                    })
                })
                .unwrap_err()
                .code,
            AnchorErrorCode::OutsideMapping
        );
    }
    for name in ["ancestor", "occurrence"] {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert!(matches!(mark.state, MarkState::Unresolved { .. }));
        assert_eq!(
            mark.boundary,
            before.marks()[&MarkId::new(name).unwrap()].boundary
        );
    }
    let crop = only_child(&after);
    let extended = edit(&after, trim(&after, crop.as_str(), SourceTrimEdge::Out, 4));
    for name in ["ancestor", "occurrence"] {
        assert_eq!(
            extended.marks()[&MarkId::new(name).unwrap()],
            after.marks()[&MarkId::new(name).unwrap()],
            "extension cannot silently revive a mark"
        );
    }
}

#[test]
fn trim_prefix_moves_physical_content_marks_but_keeps_host_edge_sentinels() {
    let mut before = fixture(None, 0);
    for (name, node, position, bias) in [
        ("physical", "source", 4, InsertionBias::Right),
        ("ancestor", "root", 4, InsertionBias::Right),
        ("leading", "source", 0, InsertionBias::Left),
        ("first-content", "source", 0, InsertionBias::Right),
        ("trailing", "source", 10, InsertionBias::Right),
    ] {
        before = add_mark(
            &before,
            name,
            Anchor::Local {
                node: id(node),
                position: ExactRatio::integer(position),
            },
            bias,
        );
    }
    let after = edit(&before, trim(&before, "source", SourceTrimEdge::In, -3));
    for (name, expected) in [
        ("physical", 7),
        ("ancestor", 7),
        ("leading", 0),
        ("first-content", 3),
        ("trailing", 13),
    ] {
        let Anchor::Local { position, .. } = after.marks()[&MarkId::new(name).unwrap()]
            .boundary
            .coordinate
        else {
            panic!()
        };
        assert_eq!(position, ExactRatio::integer(expected), "{name}");
    }
}

#[test]
fn trim_rejects_missing_extra_colliding_or_stale_command_metadata_atomically() {
    let before = fixture(None, 0);
    let good = trim(&before, "source", SourceTrimEdge::Out, -3);
    let mut cases = vec![];
    let mut command = good.clone();
    if let Command::TrimSource { wrapper, .. } = &mut command {
        *wrapper = None;
    }
    cases.push(command);
    let mut command = good.clone();
    if let Command::TrimSource { wrapper, .. } = &mut command {
        *wrapper = Some(id("root"));
    }
    cases.push(command);
    let mut command = good.clone();
    if let Command::TrimSource { timing, .. } = &mut command {
        timing.allocation = RevisionId::new("wrong").unwrap();
    }
    cases.push(command);
    let mut command = trim(&before, "source", SourceTrimEdge::Out, 3);
    if let Command::TrimSource { wrapper, .. } = &mut command {
        *wrapper = Some(id("unused"));
    }
    cases.push(command);
    cases.push(trim(&before, "source", SourceTrimEdge::Out, 0));
    let bytes = before.to_json().unwrap();
    for command in cases {
        assert!(apply(&before, &request(&before, command)).is_err());
        assert_eq!(before.to_json().unwrap(), bytes);
    }
    let mut stale = request(&before, good);
    stale.expected_revision = RevisionId::new("stale").unwrap();
    assert_eq!(
        apply(&before, &stale).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
}

#[test]
fn frozen_command_grammars_reject_trim_including_escaped_variant() {
    type Upgrade = fn(&str) -> Result<CommandRequest, DocumentError>;
    let adapters: [Upgrade; 32] = [
        legacy_v1::upgrade_request,
        legacy_v2::upgrade_request,
        legacy_v3::upgrade_request,
        legacy_v4::upgrade_request,
        legacy_v5::upgrade_request,
        legacy_v6::upgrade_request,
        legacy_v7::upgrade_request,
        legacy_v8::upgrade_request,
        legacy_v9::upgrade_request,
        legacy_v10::upgrade_request,
        legacy_v11::upgrade_request,
        legacy_v12::upgrade_request,
        legacy_v13::upgrade_request,
        legacy_v14::upgrade_request,
        legacy_v15::upgrade_request,
        legacy_v16::upgrade_request,
        legacy_v17::upgrade_request,
        legacy_v18::upgrade_request,
        legacy_v19::upgrade_request,
        legacy_v20::upgrade_request,
        legacy_v21::upgrade_request,
        legacy_v22::upgrade_request,
        legacy_v23::upgrade_request,
        legacy_v24::upgrade_request,
        legacy_v25::upgrade_request,
        legacy_v26::upgrade_request,
        legacy_v27::upgrade_request,
        legacy_v28::upgrade_request,
        legacy_v29::upgrade_request,
        legacy_v30::upgrade_request,
        legacy_v31::upgrade_request,
        legacy_v32::upgrade_request,
    ];
    let document = fixture(None, 0);
    let old = serde_json::to_string(&request(
        &document,
        Command::Rename {
            node: id("source"),
            label: "Old".into(),
        },
    ))
    .unwrap();
    let new = request(&document, trim(&document, "source", SourceTrimEdge::Out, 3));
    let json = serde_json::to_string(&new).unwrap();
    for (version, upgrade) in adapters.iter().enumerate() {
        upgrade(&old).unwrap();
        assert!(upgrade(&json).is_err(), "schema {}", version + 1);
        assert!(upgrade(&json.replace("trim_source", "trim_\\u0073ource")).is_err());
    }
    for validate in [
        legacy_v29::validate_request_context,
        legacy_v30::validate_request_context,
        legacy_v31::validate_request_context,
        legacy_v32::validate_request_context,
    ] {
        assert!(validate(&document, &new).is_err());
    }
}

#[test]
fn trim_refuses_a_full_reanchor_budget_without_replacing_the_old_clock() {
    let base = fixture(Some((0, 147000)), 0);
    let captured = capture_unbound_audio_bindings(
        &base,
        AudioTimingId {
            allocation: RevisionId::new("full-budget").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut bindings = captured.bindings().clone();
    let binding = bindings.get_mut(&id("source")).unwrap();
    binding.reanchors = vec![
        AudioReanchorStep {
            placement: binding.lattice.clone(),
            window: None,
        };
        MAX_AUDIO_BINDING_TERMS
    ];
    let state = AudioBindingState::new_with_gaps(
        captured
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
        captured.gap_bindings().clone(),
    )
    .unwrap();
    let before = modify(&base, |wire| {
        wire["audio_bindings"] = serde_json::to_value(state).unwrap()
    });
    let bytes = before.to_json().unwrap();
    let error = apply(
        &before,
        &request(&before, trim(&before, "source", SourceTrimEdge::In, -1)),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert_eq!(before.to_json().unwrap(), bytes);
}

fn resolve_mark(
    document: &ProjectDocument,
    name: &str,
    occurrence: Option<InstancePath>,
) -> Result<ResolvedBoundary, AnchorError> {
    let resolved = AnchorIndex::new(document)
        .unwrap()
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id: MarkId::new(name).unwrap(),
                    occurrence,
                },
            },
        })?;
    let ResolvedSelectionKind::Point { point } = resolved.selection else {
        panic!("mark must resolve to a boundary")
    };
    Ok(point)
}

#[test]
fn trim_crop_boundary_bias_changes_visibility_without_erasing_physical_bindings() {
    for (edge, delta, local, project, visible_bias) in [
        (SourceTrimEdge::In, 3, 3, 0, InsertionBias::Right),
        (SourceTrimEdge::Out, -4, 6, 6, InsertionBias::Left),
    ] {
        // Integer editorial endpoints isolate crop bias from fractional padding.
        let mut before = modify(&fixture(None, 0), |wire| {
            let source = &mut wire["nodes"]["source"]["kind"]["source"];
            source["edit_window"] = serde_json::to_value(
                SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(10)).unwrap(),
            )
            .unwrap();
            source["video_mapping"]["selection"] = serde_json::to_value(
                ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(10)).unwrap(),
            )
            .unwrap();
        });
        for bias in [InsertionBias::Left, InsertionBias::Right] {
            for (kind, coordinate) in [
                (
                    "physical",
                    Anchor::Local {
                        node: id("source"),
                        position: ExactRatio::integer(local),
                    },
                ),
                (
                    "occurrence",
                    Anchor::Occurrence {
                        instance: InstancePath {
                            node: id("source"),
                            repeats: vec![],
                        },
                        position: ExactRatio::integer(local),
                    },
                ),
                (
                    "pts",
                    Anchor::Source {
                        asset: AssetId::new("original").unwrap(),
                        moment: SourceMoment::Timestamp {
                            stream: SourceStream::Video,
                            timestamp: SourceTimestamp {
                                ticks: local + 10,
                                time_base: SourceTimeBase::new(1, 30).unwrap(),
                            },
                        },
                    },
                ),
                (
                    "ancestor",
                    Anchor::Local {
                        node: id("root"),
                        position: ExactRatio::integer(local),
                    },
                ),
            ] {
                before = add_mark(&before, &format!("{kind}-{bias:?}"), coordinate, bias);
            }
        }
        let after = edit(&before, trim(&before, "source", edge, delta));
        for bias in [InsertionBias::Left, InsertionBias::Right] {
            for kind in ["physical", "occurrence", "pts"] {
                let name = format!("{kind}-{bias:?}");
                let mark_id = MarkId::new(&name).unwrap();
                assert_eq!(after.marks()[&mark_id], before.marks()[&mark_id]);
                assert_eq!(after.marks()[&mark_id].state, MarkState::Bound);
                let result = resolve_mark(
                    &after,
                    &name,
                    (kind == "pts").then(|| InstancePath {
                        node: id("source"),
                        repeats: vec![],
                    }),
                );
                if bias == visible_bias {
                    assert_eq!(result.unwrap().exact_frame, ExactRatio::integer(project));
                } else {
                    assert_eq!(result.unwrap_err().code, AnchorErrorCode::OutsideMapping);
                }
            }
            let name = format!("ancestor-{bias:?}");
            let mark_id = MarkId::new(&name).unwrap();
            if bias == visible_bias {
                assert_eq!(after.marks()[&mark_id].state, MarkState::Bound);
                assert_eq!(
                    resolve_mark(&after, &name, None).unwrap().exact_frame,
                    ExactRatio::integer(project)
                );
            } else {
                assert_eq!(
                    after.marks()[&mark_id].state,
                    MarkState::Unresolved {
                        reason: MarkLossReason::OutsideMapping
                    }
                );
                assert_eq!(
                    after.marks()[&mark_id].boundary,
                    before.marks()[&mark_id].boundary
                );
                assert_eq!(
                    resolve_mark(&after, &name, None).unwrap_err().code,
                    AnchorErrorCode::MarkUnresolved
                );
            }
        }
    }
}

#[test]
fn trim_prefix_reconstructs_wrapper_and_ancestor_points_on_their_own_clocks() {
    let mut before = modify(&fixture(None, 0), |wire| {
        wire["nodes"]["view"] = serde_json::to_value(BeatNode {
            label: "Crop".into(),
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: id("source"),
                duration: frames(6),
                mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(8)).unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                purpose: RetimePurpose::Partition,
            },
        })
        .unwrap();
        for (name, duration) in [("prefix", 5), ("suffix", 2)] {
            wire["nodes"][name] = serde_json::to_value(BeatNode::hold(
                name,
                HoldRecipe {
                    picture_context: None,
                    duration: frames(duration),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ))
            .unwrap();
        }
        wire["nodes"]["root"]["kind"]["children"] = json!(["prefix", "view", "suffix"]);
    });
    let points = [
        ("wrapper-interior", "view", 2, InsertionBias::Right, 7, 12),
        ("wrapper-leading", "view", 0, InsertionBias::Left, 0, 5),
        (
            "wrapper-first-content",
            "view",
            0,
            InsertionBias::Right,
            5,
            10,
        ),
        (
            "wrapper-last-content",
            "view",
            6,
            InsertionBias::Left,
            11,
            16,
        ),
        ("wrapper-trailing", "view", 6, InsertionBias::Right, 11, 16),
        ("ancestor-interior", "root", 7, InsertionBias::Right, 12, 12),
        ("ancestor-before", "root", 5, InsertionBias::Left, 5, 5),
        (
            "ancestor-first-content",
            "root",
            5,
            InsertionBias::Right,
            10,
            10,
        ),
        (
            "ancestor-last-content",
            "root",
            11,
            InsertionBias::Left,
            16,
            16,
        ),
        ("ancestor-suffix", "root", 11, InsertionBias::Right, 16, 16),
        ("ancestor-leading", "root", 0, InsertionBias::Left, 0, 0),
        (
            "ancestor-trailing",
            "root",
            13,
            InsertionBias::Right,
            18,
            18,
        ),
    ];
    for (name, host, position, bias, _, _) in points {
        before = add_mark(
            &before,
            name,
            Anchor::Local {
                node: id(host),
                position: ExactRatio::integer(position),
            },
            bias,
        );
    }
    let resolved = before
        .source_trim(
            &id("root"),
            &id("view"),
            SourceTrimEdge::In,
            -5,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(resolved.physical_prefix, frames(3));
    let after = edit(&before, trim(&before, "view", SourceTrimEdge::In, -5));
    assert_eq!(after.duration().unwrap(), frames(18));
    assert_eq!(source(&after).duration, frames(13));
    for (name, host, _, _, position, project) in points {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.state, MarkState::Bound, "{name}");
        assert_eq!(
            mark.boundary.coordinate,
            Anchor::Local {
                node: id(host),
                position: ExactRatio::integer(position),
            },
            "{name}"
        );
        assert_eq!(
            resolve_mark(&after, name, None).unwrap().exact_frame,
            ExactRatio::integer(project),
            "{name}"
        );
    }
}

#[test]
fn trim_loss_policy_applies_per_binding_and_prefix_preserves_unresolved_coordinates() {
    let local = |host: &str, position| Anchor::Local {
        node: id(host),
        position: ExactRatio::integer(position),
    };
    for policy in [
        AnchorLossPolicy::DeleteOwned,
        AnchorLossPolicy::KeepUnresolved,
    ] {
        let before = modify(&fixture(None, 0), |wire| {
            let fragment = |coordinate| MarkFragment {
                owner: id("source"),
                coordinate,
                state: MarkState::Bound,
            };
            let logical = |fragments| Mark {
                owner: id("source"),
                label: "Logical cue".into(),
                boundary: BoundaryAnchor {
                    coordinate: local("root", 8),
                    bias: InsertionBias::Right,
                },
                loss_policy: policy,
                state: MarkState::Bound,
                fragments,
            };
            wire["marks"]["mixed"] = serde_json::to_value(logical(vec![
                fragment(local("source", 8)),
                fragment(local("root", 2)),
            ]))
            .unwrap();
            wire["marks"]["lost"] =
                serde_json::to_value(logical(vec![fragment(Anchor::Occurrence {
                    instance: InstancePath {
                        node: id("source"),
                        repeats: vec![],
                    },
                    position: ExactRatio::integer(8),
                })]))
                .unwrap();
        });
        let after = edit(&before, trim(&before, "source", SourceTrimEdge::Out, -4));
        let mixed = &after.marks()[&MarkId::new("mixed").unwrap()];
        let keep = policy == AnchorLossPolicy::KeepUnresolved;
        let bindings: Vec<_> = mixed.bindings().collect();
        let offset = usize::from(keep);
        assert_eq!(bindings.len(), 2 + offset);
        if keep {
            assert_eq!(bindings[0].coordinate, local("root", 8));
            assert_eq!(
                bindings[0].state,
                MarkState::Unresolved {
                    reason: MarkLossReason::OutsideMapping
                }
            );
            let lost = &after.marks()[&MarkId::new("lost").unwrap()];
            assert_eq!(lost.binding_count(), 2);
            assert!(
                lost.bindings()
                    .all(|binding| matches!(binding.state, MarkState::Unresolved { .. }))
            );
        } else {
            assert!(!after.marks().contains_key(&MarkId::new("lost").unwrap()));
        }
        assert_eq!(bindings[offset].coordinate, local("source", 8));
        assert_eq!(bindings[offset].state, MarkState::Bound);
        assert_eq!(bindings[offset + 1].coordinate, local("root", 2));
        assert_eq!(bindings[offset + 1].state, MarkState::Bound);
        let visible = resolve_mark(&after, "mixed", None).unwrap();
        assert_eq!(visible.exact_frame, ExactRatio::integer(2));
        assert_eq!(visible.mark.unwrap().bindings, [offset + 1]);

        let crop = only_child(&after);
        let prefixed = edit(&after, trim(&after, crop.as_str(), SourceTrimEdge::In, -3));
        let shifted: Vec<_> = prefixed.marks()[&MarkId::new("mixed").unwrap()]
            .bindings()
            .collect();
        if keep {
            assert_eq!(shifted[0], bindings[0]);
            assert_eq!(
                prefixed.marks()[&MarkId::new("lost").unwrap()],
                after.marks()[&MarkId::new("lost").unwrap()]
            );
        }
        assert_eq!(shifted[offset].coordinate, local("source", 11));
        assert_eq!(shifted[offset + 1].coordinate, local("root", 5));
        let visible = resolve_mark(&prefixed, "mixed", None).unwrap();
        assert_eq!(visible.exact_frame, ExactRatio::integer(5));
        assert_eq!(visible.mark.unwrap().bindings, [offset + 1]);
    }
}
