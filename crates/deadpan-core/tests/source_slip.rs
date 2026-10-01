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
fn command(target: &str, delta: i64) -> Command {
    Command::SlipSource {
        parent: id("root"),
        node: id(target),
        delta_frames: delta,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let is_slip = matches!(command, Command::SlipSource { .. });
    let request = request(document, command);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let tx = apply(document, &request).unwrap();
    let tx: EditTransaction = serde_json::from_str(&serde_json::to_string(&tx).unwrap()).unwrap();
    let after = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *document);
    if is_slip {
        assert_eq!(tx.duration_delta, 0);
    }
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

#[test]
fn exact_fractional_handles_clamp_inward_and_zero_never_authors() {
    let before = fixture(Some((4410, 176400)), 0);
    for (wanted, applied, reason) in [
        (-100, -10, Some(SourceSlipClamp::PictureStart)),
        (-10, -10, None),
        (5, 5, None),
        (80, 80, None),
        (100, 80, Some(SourceSlipClamp::PictureEnd)),
    ] {
        let r = before
            .source_slip(&id("root"), &id("source"), wanted)
            .unwrap();
        assert_eq!(r.minimum_delta, ratio(-31, 3));
        assert_eq!(r.maximum_delta, ratio(241, 3));
        assert_eq!((r.minimum_delta_frames, r.maximum_delta_frames), (-10, 80));
        assert_eq!(r.requested_delta_frames, wanted);
        assert_eq!(r.applied_delta_frames, applied);
        assert_eq!(r.clamp, reason);
        assert_eq!(r.physical_source, id("source"));
        assert_eq!(r.asset, AssetId::new("original").unwrap());
        let after = edit(&before, command("source", wanted));
        assert_eq!(source(&after), &r.after);
        assert_eq!(
            source(&after).video_mapping.start_frames(),
            ExactRatio::integer(-10 - applied)
        );
        assert_eq!(source(&after).edit_window, Some(window()));
        let SourceVideo::Stream { span, .. } = source(&after).video else {
            panic!()
        };
        let selected = source(&after)
            .video_mapping
            .selection_in_source(span, frames(10))
            .unwrap();
        assert_eq!(
            selected.start().ticks,
            ratio(31 + 3 * i128::from(applied), 3)
        );
    }
    let at_end = edit(&before, command("source", 80));
    let r = at_end.source_slip(&id("root"), &id("source"), 1).unwrap();
    assert_eq!(r.applied_delta_frames, 0);
    assert_eq!(r.maximum_delta, ratio(1, 3));
    assert_eq!(r.clamp, Some(SourceSlipClamp::PictureEnd));
    assert_eq!(r.after, r.before);
    let error = apply(&at_end, &request(&at_end, command("source", 1))).unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert_eq!(error.message, "source slip resolves to no change");
    assert!(apply(&before, &request(&before, command("source", 0))).is_err());
}

#[test]
fn linked_audio_offset_is_subtracted_once_and_dormant_audio_can_activate() {
    let before = fixture(Some((4410, 176400)), 7);
    let after = edit(&before, command("source", 5));
    let audio = source(&after).audio_mapping;
    assert_eq!(audio.start_frames(), ExactRatio::integer(-12));
    let selection = audio.selection_frames(frames(10)).unwrap();
    assert_eq!(
        selection.start,
        ratio(1, 3).checked_sub(ratio(7, 1600)).unwrap()
    );
    assert_eq!(
        selection.end,
        ratio(29, 3).checked_sub(ratio(7, 1600)).unwrap()
    );
    assert_eq!(
        audio
            .selection_frames_with_offset(
                frames(10),
                AudioSample(7),
                FrameRate::new(30, 1).unwrap()
            )
            .unwrap(),
        ExactFrameRange::new(window().start(), window().end()).unwrap()
    );
    assert_eq!(source(&after).audio_offset, AudioSample(7));
    // Independent source-sample oracle: output x maps to (x+15-o)*1470.
    let x = ratio(5, 2);
    let original_sample = x
        .checked_sub(ExactRatio::integer(-12))
        .unwrap()
        .checked_sub(ratio(7, 1600))
        .unwrap()
        .checked_div(ExactRatio::integer(117))
        .unwrap()
        .checked_mul(ExactRatio::integer(171990))
        .unwrap()
        .checked_add(ExactRatio::integer(4410))
        .unwrap();
    assert_eq!(
        original_sample,
        x.checked_add(ExactRatio::integer(15))
            .unwrap()
            .checked_sub(ratio(7, 1600))
            .unwrap()
            .checked_mul(ExactRatio::integer(1470))
            .unwrap()
    );

    let dormant = fixture(Some((88200, 176400)), 7);
    let quiet = source(&dormant)
        .audio_mapping
        .selection_frames(frames(10))
        .unwrap();
    assert_eq!(quiet.start, quiet.end);
    let awake = edit(&dormant, command("source", 50));
    let audible = source(&awake)
        .audio_mapping
        .selection_frames_with_offset(frames(10), AudioSample(7), FrameRate::new(30, 1).unwrap())
        .unwrap();
    assert_eq!(
        audible,
        ExactFrameRange::new(window().start(), window().end()).unwrap()
    );
    assert!(source(&awake).audio.is_some());
    assert_eq!(source(&awake).link, LinkRelation::Linked);
    let absent = edit(&fixture(None, 0), command("source", 5));
    assert!(source(&absent).audio.is_none());
}

#[test]
fn neutral_partition_uses_visible_handles_and_preserves_hidden_owner_context() {
    let before = modify(&fixture(Some((4410, 176400)), 0), |v| {
        v["nodes"]["root"]["kind"]["children"] = json!(["view"]);
        v["nodes"]["view"] = serde_json::to_value(BeatNode {
            label: "View".into(),
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: id("source"),
                duration: frames(4),
                mapping: FrameRange::new(ProjectFrame(4), ProjectFrame(8)).unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                purpose: RetimePurpose::Partition,
            },
        })
        .unwrap();
    });
    let r = before.source_slip(&id("root"), &id("view"), 100).unwrap();
    assert_eq!((r.minimum_delta_frames, r.maximum_delta_frames), (-14, 82));
    assert_eq!(
        r.effective_window,
        SourceEditWindow::new(ExactRatio::integer(4), ExactRatio::integer(8)).unwrap()
    );
    let after = edit(&before, command("view", 100));
    let mut marked_view = before.nodes()[&id("view")].clone();
    marked_view.audio_editorial_edges = AudioEditorialEdges {
        start: true,
        end: true,
    };
    assert_eq!(after.nodes()[&id("view")], marked_view);
    assert_eq!(source(&after).duration, frames(10));
    assert_eq!(source(&after).edit_window, Some(window()));
    assert_eq!(
        source(&after)
            .video_mapping
            .selection_frames(frames(10))
            .unwrap(),
        ExactFrameRange::new(window().start(), ExactRatio::integer(8)).unwrap()
    );
    assert_eq!(after.duration().unwrap(), frames(4));
}

#[test]
fn slip_marks_both_incident_joins_across_groups_without_changing_clocks_or_policies() {
    for partition in [false, true] {
        let before = modify(&fixture(Some((0, 147000)), 7), |wire| {
            let source = wire["nodes"]["source"].clone();
            wire["nodes"]["previous"] = source.clone();
            wire["nodes"]["next"] = source;
            for node in ["previous", "source", "next"] {
                wire["nodes"][node]["audio_edges"] = serde_json::to_value(AudioEdgePolicies {
                    node_start: AudioEdgePolicy::Hard,
                    node_end: AudioEdgePolicy::Hard,
                    ..Default::default()
                })
                .unwrap();
            }
            let target = if partition {
                wire["nodes"]["view"] = serde_json::to_value(BeatNode {
                    label: "View".into(),
                    framing: None,
                    audio_treatments: Default::default(),
                    audio_edges: Default::default(),
                    audio_editorial_edges: Default::default(),
                    kind: NodeKind::Retime {
                        child: id("source"),
                        duration: frames(4),
                        mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(6)).unwrap(),
                        pitch: PitchPolicy::FollowSpeed,
                        purpose: RetimePurpose::Partition,
                    },
                })
                .unwrap();
                "view"
            } else {
                "source"
            };
            wire["nodes"]["zero_before"] =
                serde_json::to_value(BeatNode::sequence("Empty", vec![])).unwrap();
            wire["nodes"]["zero_after"] = wire["nodes"]["zero_before"].clone();
            wire["nodes"]["group"] = serde_json::to_value(BeatNode::sequence(
                "Group",
                vec![id("zero_before"), id(target), id("zero_after")],
            ))
            .unwrap();
            wire["nodes"]["root"]["kind"]["children"] = json!(["previous", "group", "next"]);
        });
        let target = if partition { "view" } else { "source" };
        let command = Command::SlipSource {
            parent: id("group"),
            node: id(target),
            delta_frames: 3,
        };
        let after = edit(&before, command);
        assert_eq!(
            after.nodes()[&id(target)].audio_editorial_edges,
            AudioEditorialEdges {
                start: true,
                end: true
            }
        );
        assert_eq!(
            after.nodes()[&id("previous")].audio_editorial_edges,
            AudioEditorialEdges {
                start: false,
                end: true
            }
        );
        assert_eq!(
            after.nodes()[&id("next")].audio_editorial_edges,
            AudioEditorialEdges {
                start: true,
                end: false
            }
        );
        for node in ["root", "group", "zero_before", "zero_after"] {
            assert!(after.nodes()[&id(node)].audio_editorial_edges.is_empty());
        }
        for node in ["previous", "source", "next"] {
            assert_eq!(
                after.nodes()[&id(node)].audio_edges,
                before.nodes()[&id(node)].audio_edges
            );
        }
        assert_eq!(before.nodes().len(), after.nodes().len());
        assert_eq!(before.duration().unwrap(), after.duration().unwrap());
        assert_eq!(before.audio_bindings(), after.audio_bindings());
        assert_eq!(before.sounds(), after.sounds());
        assert_eq!(before.sound_routes(), after.sound_routes());
        if partition {
            assert!(
                after.nodes()[&id("source")]
                    .audio_editorial_edges
                    .is_empty()
            );
        }
        let snapshot = before.clone();
        let noop = Command::SlipSource {
            parent: id("group"),
            node: id(target),
            delta_frames: 0,
        };
        assert!(apply(&before, &request(&before, noop)).is_err());
        assert_eq!(before, snapshot);
    }
}

#[test]
fn rejects_unqualified_incoherent_and_unsupported_sources_without_mutation() {
    let before = fixture(Some((4410, 176400)), 0);
    for variant in 0..11 {
        let invalid = modify(&before, |v| {
            let s = &mut v["nodes"]["source"]["kind"]["source"];
            match variant {
                0 => s["edit_window"] = Value::Null,
                1 => s["video_mapping"] = json!({"type":"fit_beat"}),
                2 => s["audio_mapping"] = json!({"type":"fit_beat"}),
                3 => s["link"] = json!("independent"),
                4 => {
                    s["video_mapping"]["selection"]["start"] =
                        serde_json::to_value(ExactRatio::integer(1)).unwrap()
                }
                5 => {
                    s["audio_mapping"]["start"] =
                        serde_json::to_value(ExactRatio::integer(-6)).unwrap()
                }
                6 => {
                    s["audio_mapping"]["frames"] =
                        serde_json::to_value(ExactRatio::integer(118)).unwrap()
                }
                7 => {
                    s["audio_mapping"]["selection"]["start"] =
                        serde_json::to_value(ExactRatio::integer(1)).unwrap()
                }
                8 => v["assets"]["original"]["source_qualification"] = Value::Null,
                9 => s["video"]["span"] = serde_json::to_value(span(0, 90, 30)).unwrap(),
                _ => {
                    s["audio"]["asset"] = json!("other");
                    v["assets"]["other"] = v["assets"]["original"].clone();
                }
            }
        });
        let snapshot = invalid.clone();
        assert!(
            invalid.source_slip(&id("root"), &id("source"), 5).is_err(),
            "variant {variant}"
        );
        assert!(apply(&invalid, &request(&invalid, command("source", 5))).is_err());
        assert_eq!(invalid, snapshot);
    }
    let repeated = edit(
        &before,
        Command::WrapRepeat {
            node: id("source"),
            id: id("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    assert!(repeated.source_slip(&id("root"), &id("repeat"), 1).is_err());
    assert!(
        repeated
            .source_slip(&id("repeat"), &id("source"), 1)
            .is_err()
    );
    let lead = modify(&before, |v| {
        let s = &mut v["nodes"]["source"]["kind"]["source"];
        s["video_mapping"]["start"] = serde_json::to_value(ExactRatio::integer(1)).unwrap();
        s["video_mapping"]["selection"]["start"] =
            serde_json::to_value(ExactRatio::integer(1)).unwrap();
    });
    assert!(
        lead.source_slip(&id("root"), &id("source"), 1)
            .unwrap_err()
            .message
            .contains("lead or tail")
    );
}

#[test]
fn fixed_clocks_effects_bindings_and_root_sounds_survive_slip() {
    let base = fixture(Some((4410, 176400)), 0);
    let bound = capture_unbound_audio_bindings(
        &base,
        AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let before = modify(&base, |v| {
        v["audio_bindings"] = serde_json::to_value(bound).unwrap();
        v["audio_lineage"] = json!({"source":{"allocation":"initial","origin":"old-source"},"root":{"allocation":"initial","origin":"old-root"}});
        v["nodes"]["source"]["framing"] = serde_json::to_value(
            Framing::static_pose(FramingPose::identity())
                .unwrap()
                .prepend_owner_frames(frames(2), frames(8))
                .unwrap(),
        )
        .unwrap();
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
        // Full context stays at its natural rate: (176400 - 4410) / 44100 * 30
        // is 117 frames. Only the selected [1, 2) interval is audible in root.
        let sound = SoundEvent {
            owner: id("root"),
            label: "Sound".into(),
            source: source(&base).audio.clone().unwrap(),
            mapping: SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: ExactRatio::integer(117),
                selection: ExactFrameRange::new(ExactRatio::integer(1), ExactRatio::integer(2))
                    .unwrap(),
            },
            offset: AudioSample(0),
            gain_millidecibels: 0,
            start_edge: AudioEdgePolicy::Automatic,
            end_edge: AudioEdgePolicy::Automatic,
            overflow: SoundOverflowPolicy::Reject,
        };
        v["sounds"] = json!({"sound": sound});
    });
    let after = edit(&before, command("source", 5));
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert!(after.audio_lineage().is_empty());
    assert_eq!(
        after.nodes()[&id("source")].framing,
        before.nodes()[&id("source")].framing
    );
    assert_eq!(
        after.nodes()[&id("source")].audio_treatments,
        before.nodes()[&id("source")].audio_treatments
    );
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    assert_eq!(after.sound_allowances(), before.sound_allowances());
}

#[test]
fn local_and_sequence_marks_stay_fixed_while_source_pts_resolves_or_leaves_selection() {
    let mut before = fixture(Some((4410, 176400)), 0);
    for (name, coordinate) in [
        (
            "local",
            Anchor::Local {
                node: id("source"),
                position: ExactRatio::integer(4),
            },
        ),
        (
            "occurrence",
            Anchor::Occurrence {
                instance: InstancePath {
                    node: id("source"),
                    repeats: vec![],
                },
                position: ExactRatio::integer(4),
            },
        ),
        (
            "sequence",
            Anchor::Sequence {
                frame: ProjectFrame(4),
            },
        ),
        (
            "inside",
            Anchor::Source {
                asset: AssetId::new("original").unwrap(),
                moment: SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp: SourceTimestamp {
                        ticks: 16,
                        time_base: SourceTimeBase::new(1, 30).unwrap(),
                    },
                },
            },
        ),
        (
            "outside",
            Anchor::Source {
                asset: AssetId::new("original").unwrap(),
                moment: SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp: SourceTimestamp {
                        ticks: 12,
                        time_base: SourceTimeBase::new(1, 30).unwrap(),
                    },
                },
            },
        ),
    ] {
        before = edit(
            &before,
            Command::SetMark {
                id: MarkId::new(name).unwrap(),
                owner: id("source"),
                label: name.into(),
                boundary: BoundaryAnchor {
                    coordinate,
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
            },
        );
    }
    let after = edit(&before, command("source", 5));
    assert_eq!(after.marks(), before.marks());
    let target = |name: &str| AnchorTarget {
        boundary: after.marks()[&MarkId::new(name).unwrap()].boundary.clone(),
        occurrence: Some(InstancePath {
            node: id("source"),
            repeats: vec![],
        }),
    };
    assert_eq!(
        AnchorIndex::new(&before)
            .unwrap()
            .resolve_target(&target("inside"))
            .unwrap()
            .exact_frame,
        ExactRatio::integer(6)
    );
    assert_eq!(
        AnchorIndex::new(&after)
            .unwrap()
            .resolve_target(&target("inside"))
            .unwrap()
            .exact_frame,
        ExactRatio::integer(1)
    );
    assert_eq!(
        AnchorIndex::new(&after)
            .unwrap()
            .resolve_target(&target("outside"))
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
    assert_eq!(
        after.marks()[&MarkId::new("outside").unwrap()].state,
        MarkState::Bound
    );
}

#[test]
fn common_nonnatural_slope_and_signed_original_origins_stay_exact() {
    let before = modify(&fixture(Some((4410, 176400)), 0), |v| {
        let picture = span(-300, -200, 30);
        let audio = span(4410 - 441000, 176400 - 441000, 44100);
        v["assets"]["original"]["video"] = serde_json::to_value(picture).unwrap();
        v["assets"]["original"]["audio"] = serde_json::to_value(audio).unwrap();
        let source = &mut v["nodes"]["source"]["kind"]["source"];
        source["video"]["span"] = serde_json::to_value(picture).unwrap();
        source["audio"]["span"] = serde_json::to_value(audio).unwrap();
        source["video_mapping"]["start"] = serde_json::to_value(ExactRatio::integer(-20)).unwrap();
        source["video_mapping"]["frames"] = serde_json::to_value(ExactRatio::integer(200)).unwrap();
        source["audio_mapping"]["start"] = serde_json::to_value(ExactRatio::integer(-14)).unwrap();
        source["audio_mapping"]["frames"] = serde_json::to_value(ExactRatio::integer(234)).unwrap();
    });
    let after = edit(&before, command("source", 5));
    let source = source(&after);
    assert_eq!(
        source
            .video_mapping
            .duration_frames(source.duration)
            .unwrap(),
        ExactRatio::integer(200)
    );
    assert_eq!(
        source
            .audio_mapping
            .duration_frames(source.duration)
            .unwrap(),
        ExactRatio::integer(234)
    );
    let SourceVideo::Stream { span, .. } = source.video else {
        panic!()
    };
    let selection = source
        .video_mapping
        .selection_in_source(span, source.duration)
        .unwrap();
    assert_eq!(selection.start().ticks, ratio(-862, 3));
    assert_eq!(
        source.audio_mapping.start_frames(),
        ExactRatio::integer(-19)
    );
}

#[test]
fn distant_linked_context_overflow_is_an_error_not_a_handle_clamp() {
    let end = i64::try_from((i128::from(i64::MAX) + 19) / 2).unwrap();
    let audio = span(end - 100, end, 30);
    let start = ExactRatio::new(2 * i128::from(end - 100) - 20, 1).unwrap();
    let before = modify(&fixture(Some((4410, 176400)), 0), |v| {
        v["assets"]["original"]["audio"] = serde_json::to_value(audio).unwrap();
        let source = &mut v["nodes"]["source"]["kind"]["source"];
        source["audio"]["span"] = serde_json::to_value(audio).unwrap();
        source["video_mapping"]["start"] = serde_json::to_value(ExactRatio::integer(-20)).unwrap();
        source["video_mapping"]["frames"] = serde_json::to_value(ExactRatio::integer(200)).unwrap();
        source["audio_mapping"] = serde_json::to_value(SourceAudioMapping::SelectedPlacement {
            start,
            frames: ExactRatio::integer(200),
            selection: ExactFrameRange { start, end: start },
        })
        .unwrap();
    });
    let snapshot = before.clone();
    assert_eq!(
        before
            .source_slip(&id("root"), &id("source"), -20)
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
    assert!(apply(&before, &request(&before, command("source", -20))).is_err());
    assert_eq!(before, snapshot);
}

#[test]
fn treated_nested_and_authored_retimes_are_explicitly_unsupported() {
    let base = fixture(Some((4410, 176400)), 0);
    let neutral = BeatNode {
        label: "View".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("source"),
            duration: frames(4),
            mapping: FrameRange::new(ProjectFrame(4), ProjectFrame(8)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    };
    for variant in 0..3 {
        let before = modify(&base, |v| {
            v["nodes"]["root"]["kind"]["children"] = json!(["view"]);
            v["nodes"]["view"] = serde_json::to_value(&neutral).unwrap();
            match variant {
                0 => {
                    v["nodes"]["view"]["framing"] =
                        serde_json::to_value(Framing::static_pose(FramingPose::identity()).unwrap())
                            .unwrap()
                }
                1 => v["nodes"]["view"]["kind"]["purpose"] = json!("edit"),
                _ => {
                    v["nodes"]["outer"] = serde_json::to_value(BeatNode {
                        label: "Outer".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind: NodeKind::Retime {
                            child: id("view"),
                            duration: frames(4),
                            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
                            pitch: PitchPolicy::FollowSpeed,
                            purpose: RetimePurpose::Partition,
                        },
                    })
                    .unwrap();
                    v["nodes"]["root"]["kind"]["children"] = json!(["outer"]);
                }
            }
        });
        let target = if variant == 2 { "outer" } else { "view" };
        assert_eq!(
            before
                .source_slip(&id("root"), &id(target), 1)
                .unwrap_err()
                .code,
            EditErrorCode::WrongNodeKind
        );
    }
}

#[test]
fn frozen_command_grammars_reject_slip_including_escaped_variant() {
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
    let new = request(&document, command("source", 5));
    let json = serde_json::to_string(&new).unwrap();
    for (version, upgrade) in adapters.iter().enumerate() {
        upgrade(&old).unwrap();
        assert!(upgrade(&json).is_err(), "schema {}", version + 1);
        assert!(upgrade(&json.replace("slip_source", "slip_\\u0073ource")).is_err());
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
