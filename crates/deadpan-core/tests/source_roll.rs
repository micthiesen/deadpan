use deadpan_core::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

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
        "nodes":{"root":BeatNode::sequence("Root",vec![id("source")]),"source":BeatNode {label:"Source".into(),framing:None,audio_treatments:Default::default(),audio_editorial_edges: Default::default(), audio_edges:Default::default(),kind:NodeKind::Source {source}}},
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
    }
}
fn revision(document: &ProjectDocument) -> RevisionId {
    RevisionId::new(format!("{}x", document.revision_id())).unwrap()
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(document),
        command,
    }
}
fn roll(document: &ProjectDocument, left: &str, right: &str, delta: i64) -> Command {
    let resolution = document
        .source_roll(&id("root"), &id(left), &id(right), delta)
        .unwrap();
    Command::RollSources {
        parent: id("root"),
        left: id(left),
        right: id(right),
        delta_frames: delta,
        left_wrapper: resolution.left.needs_wrapper.then(|| id("left-crop")),
        right_wrapper: resolution.right.needs_wrapper.then(|| id("right-crop")),
        timing: AudioTimingId {
            allocation: revision(document),
            ordinal: 0,
        },
    }
}
fn edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(document, command);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let tx = apply(document, &request).unwrap();
    let tx: EditTransaction = serde_json::from_str(&serde_json::to_string(&tx).unwrap()).unwrap();
    let after = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    (after, tx)
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

#[test]
fn roll_uses_one_delta_and_preserves_fractional_padding_in_all_source_view_pairs() {
    for left_view in [false, true] {
        for right_view in [false, true] {
            for delta in [-3, 3] {
                let before = modify(&fixture(Some((0, 147000)), 17), |wire| {
                    let left = if left_view { "left-view" } else { "left" };
                    let right = if right_view { "right-view" } else { "right" };
                    if left_view {
                        wire["nodes"][left] = serde_json::to_value(crop("left", 0, 10)).unwrap();
                    }
                    if right_view {
                        wire["nodes"][right] = serde_json::to_value(crop("right", 0, 10)).unwrap();
                    }
                    wire["nodes"]["root"]["kind"]["children"] = json!([left, right]);
                });
                let left = if left_view { "left-view" } else { "left" };
                let right = if right_view { "right-view" } else { "right" };
                let resolution = before
                    .source_roll(&id("root"), &id(left), &id(right), delta)
                    .unwrap();
                assert_eq!(resolution.applied_delta_frames, delta);
                assert_eq!(resolution.pair_output, range(0, 20));
                assert_eq!(resolution.seam_after, ProjectFrame(10 + delta));
                assert_eq!(resolution.left.output_after, range(0, 10 + delta));
                assert_eq!(resolution.right.output_after, range(10 + delta, 20));
                assert_eq!(resolution.left.physical_prefix, frames(0));
                assert_eq!(resolution.right.physical_prefix, frames((-delta).max(0)));
                for side in [&resolution.left, &resolution.right] {
                    assert_eq!(
                        side.effective_after
                            .start()
                            .checked_sub(ExactRatio::integer(side.allocation_after.start().0))
                            .unwrap(),
                        ratio(1, 3)
                    );
                    assert_eq!(
                        ExactRatio::integer(side.allocation_after.end().0)
                            .checked_sub(side.effective_after.end())
                            .unwrap(),
                        ratio(1, 3)
                    );
                    assert_eq!(side.before.video, side.after.video);
                    assert_eq!(side.before.audio, side.after.audio);
                    assert_eq!(side.after.audio_offset, AudioSample(17));
                }
                let (after, tx) = edit(&before, roll(&before, left, right, delta));
                assert_eq!(tx.duration_delta, 0);
                assert_eq!(after.duration().unwrap(), frames(20));
                assert_eq!(source(&after, "left"), &resolution.left.after);
                assert_eq!(source(&after, "right"), &resolution.right.after);
                assert_eq!(
                    after.nodes().len() - before.nodes().len(),
                    usize::from(!left_view && delta < 0) + usize::from(!right_view && delta > 0)
                );
                assert_eq!(after.audio_bindings().timings().len(), 1);
                for binding in after.audio_bindings().bindings().values() {
                    assert!(binding.reanchors.is_empty());
                }
                let left_final = if resolution.left.needs_wrapper {
                    "left-crop"
                } else {
                    left
                };
                let right_final = if resolution.right.needs_wrapper {
                    "right-crop"
                } else {
                    right
                };
                assert_eq!(
                    after.nodes()[&id(left_final)].audio_editorial_edges,
                    AudioEditorialEdges {
                        start: false,
                        end: true
                    }
                );
                assert_eq!(
                    after.nodes()[&id(right_final)].audio_editorial_edges,
                    AudioEditorialEdges {
                        start: true,
                        end: false
                    }
                );
                assert!(after.nodes()[&id("root")].audio_editorial_edges.is_empty());
            }
        }
    }
}

#[test]
fn roll_shared_limits_keep_strict_fractional_rules_and_report_the_controlling_side() {
    let base = fixture(None, 0);
    let r = base
        .source_roll(&id("root"), &id("left"), &id("right"), i64::MAX)
        .unwrap();
    assert_eq!((r.minimum_delta_frames, r.maximum_delta_frames), (-9, 9));
    assert_eq!(r.applied_delta_frames, 9);
    assert_eq!(r.clamp.unwrap().side, SourceRollSide::Right);
    assert_eq!(
        r.clamp.unwrap().reason,
        SourceTrimClamp::MinimumOutputDuration
    );
    let strict = modify(&base, |v| {
        // Left's inclusive picture limit ties right's exclusive width limit.
        v["nodes"]["left"]["kind"]["source"]["video_mapping"]["start"] =
            serde_json::to_value(ratio(-244, 3)).unwrap();
        let source = &mut v["nodes"]["right"]["kind"]["source"];
        source["edit_window"] =
            serde_json::to_value(SourceEditWindow::new(ratio(1, 3), ratio(28, 3)).unwrap())
                .unwrap();
        source["video_mapping"]["selection"] =
            serde_json::to_value(ExactFrameRange::new(ratio(1, 3), ratio(28, 3)).unwrap()).unwrap();
    });
    let r = strict
        .source_roll(&id("root"), &id("left"), &id("right"), 99)
        .unwrap();
    assert_eq!(r.maximum_delta.delta, ExactRatio::integer(9));
    assert!(!r.maximum_delta.inclusive);
    assert_eq!(r.applied_delta_frames, 8);
    assert_eq!(r.maximum_delta.side, SourceRollSide::Right);
    assert_eq!(
        r.clamp.unwrap().reason,
        SourceTrimClamp::MinimumSelectedDuration
    );
    let no_tail = modify(&base, |v| {
        v["nodes"]["left"]["kind"]["source"]["video_mapping"]["start"] =
            serde_json::to_value(ExactRatio::integer(-90)).unwrap();
    });
    let r = no_tail
        .source_roll(&id("root"), &id("left"), &id("right"), 2)
        .unwrap();
    assert_eq!(r.maximum_delta.delta, ratio(1, 3));
    assert_eq!(r.applied_delta_frames, 0);
    assert_eq!(r.clamp.unwrap().side, SourceRollSide::Left);
    assert_eq!(r.clamp.unwrap().reason, SourceTrimClamp::PictureEnd);
    for side in [&r.left, &r.right] {
        assert_eq!(side.before, side.after);
        assert_eq!(side.allocation_before, side.allocation_after);
        assert!(!side.needs_wrapper);
    }
    let r = base
        .source_roll(&id("root"), &id("left"), &id("right"), i64::MIN)
        .unwrap();
    assert_eq!(r.applied_delta_frames, -9);
    assert_eq!(r.clamp.unwrap().side, SourceRollSide::Left);
}

#[test]
fn roll_preserves_hidden_windows_until_the_edge_crosses_them() {
    let before = modify(&fixture(None, 0), |wire| {
        wire["nodes"]["left-view"] = serde_json::to_value(crop("left", 2, 7)).unwrap();
        wire["nodes"]["right-view"] = serde_json::to_value(crop("right", 3, 8)).unwrap();
        wire["nodes"]["root"]["kind"]["children"] = json!(["left-view", "right-view"]);
    });
    let r = before
        .source_roll(&id("root"), &id("left-view"), &id("right-view"), 2)
        .unwrap();
    assert_eq!(r.left.window_after, r.left.window_before);
    assert_eq!(r.right.window_after, r.right.window_before);
    assert_eq!(
        r.left.effective_after,
        SourceEditWindow::new(ExactRatio::integer(2), ExactRatio::integer(9)).unwrap()
    );
    assert_eq!(
        r.right.effective_after,
        SourceEditWindow::new(ExactRatio::integer(5), ExactRatio::integer(8)).unwrap()
    );
    assert_eq!(r.left.before, r.left.after);
    assert_eq!(r.right.before, r.right.after);
    let (after, _) = edit(&before, roll(&before, "left-view", "right-view", 2));
    assert_eq!(after.nodes().len(), before.nodes().len());
}

#[test]
fn roll_activates_dormant_linked_audio_without_inventing_an_absent_voice() {
    for audio in [None, Some((0, 14700))] {
        let before = fixture(audio, 17);
        let r = before
            .source_roll(&id("root"), &id("left"), &id("right"), -3)
            .unwrap();
        if audio.is_some() {
            let old = r
                .right
                .before
                .audio_mapping
                .selection_frames(frames(10))
                .unwrap();
            assert_eq!(old.start, old.end);
            let selected = r
                .right
                .after
                .audio_mapping
                .selection_frames(r.right.after.duration)
                .unwrap();
            assert_eq!(
                selected.start,
                ratio(1, 3).checked_sub(ratio(17, 1600)).unwrap()
            );
            assert_eq!(selected.end, ExactRatio::integer(3));
            assert_eq!(r.right.before.audio, r.right.after.audio);
        } else {
            assert!(r.right.after.audio.is_none());
            assert_eq!(r.right.after.audio_mapping, SourceAudioMapping::FitBeat);
        }
        edit(&before, roll(&before, "left", "right", -3));
    }
}

#[test]
fn roll_keeps_independent_coherent_affine_rates_and_offsets_on_both_sides() {
    let before = modify(&fixture(Some((0, 147000)), 17), |wire| {
        for name in ["video_mapping", "audio_mapping"] {
            wire["nodes"]["right"]["kind"]["source"][name]["start"] =
                serde_json::to_value(ExactRatio::integer(-20)).unwrap();
            wire["nodes"]["right"]["kind"]["source"][name]["frames"] =
                serde_json::to_value(ExactRatio::integer(200)).unwrap();
        }
    });
    let r = before
        .source_roll(&id("root"), &id("left"), &id("right"), -3)
        .unwrap();
    assert_eq!(
        r.left
            .after
            .video_mapping
            .duration_frames(r.left.after.duration)
            .unwrap(),
        ExactRatio::integer(100)
    );
    assert_eq!(
        r.right
            .after
            .video_mapping
            .duration_frames(r.right.after.duration)
            .unwrap(),
        ExactRatio::integer(200)
    );
    assert_eq!(
        r.right.after.video_mapping.start_frames(),
        ExactRatio::integer(-17)
    );
    let a = r
        .right
        .after
        .audio_mapping
        .selection_frames(r.right.after.duration)
        .unwrap();
    assert_eq!(a.start, ratio(1, 3).checked_sub(ratio(17, 1600)).unwrap());
    assert_eq!(a.end, ratio(38, 3).checked_sub(ratio(17, 1600)).unwrap());
    edit(&before, roll(&before, "left", "right", -3));
}

#[test]
fn roll_refuses_nonadjacent_unqualified_incoherent_and_unsupported_targets() {
    let before = fixture(Some((0, 147000)), 0);
    for (left, right) in [("right", "left"), ("left", "left"), ("missing", "right")] {
        assert!(
            before
                .source_roll(&id("root"), &id(left), &id(right), 1)
                .is_err()
        );
    }
    let empty = modify(&before, |v| {
        v["nodes"]["empty"] = serde_json::to_value(BeatNode::sequence("Empty", vec![])).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "empty", "right"]);
    });
    assert!(
        empty
            .source_roll(&id("root"), &id("left"), &id("right"), 1)
            .is_err()
    );
    for broken in [
        modify(&before, |v| {
            v["assets"]["original"]["source_qualification"] = Value::Null;
        }),
        modify(&before, |v| {
            v["nodes"]["right"]["kind"]["source"]["audio_mapping"]["frames"] =
                serde_json::to_value(ExactRatio::integer(101)).unwrap();
        }),
        modify(&before, |v| {
            v["nodes"]["right-view"] = serde_json::to_value(crop("right", 0, 10)).unwrap();
            v["nodes"]["right-view"]["framing"] =
                serde_json::to_value(Framing::static_pose(FramingPose::identity()).unwrap())
                    .unwrap();
            v["nodes"]["root"]["kind"]["children"] = json!(["left", "right-view"]);
        }),
    ] {
        let right = if broken.nodes().contains_key(&id("right-view")) {
            "right-view"
        } else {
            "right"
        };
        assert!(
            broken
                .source_roll(&id("root"), &id("left"), &id(right), 0)
                .is_err()
        );
    }
    assert_eq!(before, fixture(Some((0, 147000)), 0));
    let held_picture = modify(&fixture(None, 0), |v| {
        let mapping = &mut v["nodes"]["left"]["kind"]["source"]["video_mapping"];
        mapping["start"] = serde_json::to_value(ExactRatio::ONE).unwrap();
        mapping["selection"] =
            serde_json::to_value(ExactFrameRange::new(ExactRatio::ONE, ratio(29, 3)).unwrap())
                .unwrap();
    });
    assert!(
        held_picture
            .source_roll(&id("root"), &id("left"), &id("right"), 0)
            .is_err()
    );
}

#[test]
fn roll_avoids_unary_total_overflow_but_reports_real_physical_prefix_overflow() {
    let huge = modify(&fixture(None, 0), |v| {
        v["presentation_basis"]["frame_rate"] = json!({"numerator":48000,"denominator":1});
        v["nodes"]["tail"] = serde_json::to_value(hold(i64::MAX - 20)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "right", "tail"]);
    });
    assert_eq!(huge.duration().unwrap().frames(), i64::MAX);
    assert!(
        huge.source_trim(
            &id("root"),
            &id("left"),
            SourceTrimEdge::Out,
            1,
            SourceTrimMode::Ripple
        )
        .is_err()
    );
    let r = huge
        .source_roll(&id("root"), &id("left"), &id("right"), 1)
        .unwrap();
    assert_eq!(r.applied_delta_frames, 1);
    assert_eq!(r.pair_output, range(0, 20));
    let overflow = modify(&fixture(None, 0), |v| {
        v["nodes"]["right"]["kind"]["source"]["duration"] = json!(i64::MAX);
        v["nodes"]["right-view"] = serde_json::to_value(crop("right", 0, 10)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "right-view"]);
    });
    let error = overflow
        .source_roll(&id("root"), &id("left"), &id("right-view"), -3)
        .unwrap_err();
    assert_eq!(error.code, EditErrorCode::TimingOverflow);
}

#[test]
fn roll_rejects_bad_identities_metadata_and_zero_without_mutation() {
    let before = fixture(None, 0);
    let good = roll(&before, "left", "right", 3);
    let mut invalid = Vec::new();
    for kind in 0..5 {
        let mut command = good.clone();
        let Command::RollSources {
            left_wrapper,
            right_wrapper,
            timing,
            left,
            right,
            ..
        } = &mut command
        else {
            unreachable!()
        };
        match kind {
            0 => *right_wrapper = None,
            1 => *right_wrapper = Some(id("left")),
            2 => *left_wrapper = Some(id("unused")),
            3 => timing.allocation = RevisionId::new("stale-clock").unwrap(),
            _ => std::mem::swap(left, right),
        }
        invalid.push(command);
    }
    invalid.push(roll(&before, "left", "right", 0));
    for command in invalid {
        assert!(apply(&before, &request(&before, command)).is_err());
    }
    let mut stale = request(&before, good);
    stale.expected_revision = RevisionId::new("stale").unwrap();
    assert!(apply(&before, &stale).is_err());
    assert_eq!(before, fixture(None, 0));
    let wire = serde_json::to_value(request(&before, roll(&before, "left", "right", 3))).unwrap();
    let mut extra = wire.clone();
    extra["command"]["mode"] = json!("ripple");
    assert!(serde_json::from_value::<CommandRequest>(extra).is_err());
    let mut extra = wire;
    extra["command"]["delta_frames"] = json!(1.5);
    assert!(serde_json::from_value::<CommandRequest>(extra).is_err());
}

#[test]
fn roll_preserves_existing_clocks_effects_root_routes_and_hold_allowances() {
    let base = modify(&fixture(Some((0, 147000)), 0), |v| {
        v["nodes"]["tail"] = serde_json::to_value(hold(2)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "right", "tail"]);
    });
    let old_timing = AudioTimingId {
        allocation: RevisionId::new("old-clock").unwrap(),
        ordinal: 7,
    };
    let captured = capture_unbound_audio_bindings(&base, old_timing.clone()).unwrap();
    let mut owners = captured.bindings().clone();
    let right = owners.get_mut(&id("right")).unwrap();
    right.resume = Some(AudioResume {
        local_boundary: ExactRatio::integer(2),
        phase: AudioLocalPhase {
            constant: ratio(1, 1600),
            terms: vec![],
        },
    });
    right.reanchors.push(AudioReanchorStep {
        placement: right.lattice.clone(),
        window: Some(
            ExactFrameRange::new(ExactRatio::integer(10), ExactRatio::integer(20)).unwrap(),
        ),
    });
    let bindings = AudioBindingState::new(
        captured
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
    let sound_id = SoundId::new("sound").unwrap();
    let before = modify(&base, |v| {
        v["audio_bindings"] = serde_json::to_value(&bindings).unwrap();
        let framing = Framing::creep(
            FramingPose::identity(),
            FramingPose::new(ratio(1, 2), ratio(1, 2), ExactRatio::integer(2)).unwrap(),
            FramingCurve::Linear,
        )
        .unwrap();
        v["nodes"]["right"]["framing"] = serde_json::to_value(&framing).unwrap();
        v["nodes"]["root"]["framing"] = serde_json::to_value(&framing).unwrap();
        let treatments = AudioTreatments::from_clip_gain(
            ClipGain::new(
                GainDb::new(-3000).unwrap(),
                false,
                vec![],
                vec![GainRange::new(ExactRatio::integer(2), ExactRatio::integer(3)).unwrap()],
            )
            .unwrap(),
        );
        v["nodes"]["right"]["audio_treatments"] = serde_json::to_value(treatments).unwrap();
        v["nodes"]["right"]["audio_edges"] = serde_json::to_value(AudioEdgePolicies {
            node_start: AudioEdgePolicy::Hard,
            ..Default::default()
        })
        .unwrap();
        v["sounds"] = serde_json::to_value(BTreeMap::from([(
            sound_id.clone(),
            SoundEvent {
                owner: id("root"),
                label: "Crossing root sound".into(),
                source: source(&base, "left").audio.clone().unwrap(),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: ExactRatio::integer(100),
                    selection: ExactFrameRange::new(
                        ExactRatio::integer(1),
                        ExactRatio::integer(22),
                    )
                    .unwrap(),
                },
                offset: AudioSample(0),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Automatic,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]))
        .unwrap();
        v["sound_routes"] = serde_json::to_value(BTreeMap::from([(
            sound_id.clone(),
            RootSoundRoute::identity(frames(22), FrameRate::new(30, 1).unwrap()),
        )]))
        .unwrap();
        v["sound_allowances"] = serde_json::to_value(BTreeMap::from([(
            sound_id.clone(),
            SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: id("tail"),
                    repeats: vec![],
                },
            }])
            .unwrap(),
        )]))
        .unwrap();
    });
    let (after, tx) = edit(&before, roll(&before, "left", "right", -3));
    assert_eq!(tx.duration_delta, 0);
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    assert_eq!(after.sound_allowances(), before.sound_allowances());
    assert_eq!(
        after.audio_bindings().timings(),
        before.audio_bindings().timings()
    );
    for node in ["left", "tail"] {
        assert_eq!(
            after.audio_bindings().bindings()[&id(node)],
            before.audio_bindings().bindings()[&id(node)]
        );
    }
    assert_eq!(
        after.audio_bindings().bindings()[&id("right")],
        before.audio_bindings().bindings()[&id("right")]
            .rebase_local(ExactRatio::integer(3))
            .unwrap()
    );
    assert_eq!(
        after.nodes()[&id("right")].framing,
        Some(
            before.nodes()[&id("right")]
                .framing
                .as_ref()
                .unwrap()
                .prepend_owner_frames(frames(3), frames(10))
                .unwrap()
        )
    );
    assert_eq!(
        after.nodes()[&id("right")].audio_treatments,
        before.nodes()[&id("right")]
            .audio_treatments
            .with_owner_prefix(frames(3))
            .unwrap()
    );
    assert_eq!(
        after.nodes()[&id("root")].framing,
        before.nodes()[&id("root")].framing
    );
    assert_eq!(
        after.nodes()[&id("right")].audio_edges,
        before.nodes()[&id("right")].audio_edges
    );
    assert_eq!(after.nodes()[&id("tail")], before.nodes()[&id("tail")]);
}

#[test]
fn roll_transforms_marks_once_preserving_physical_bindings_and_seam_bias() {
    let mut before = fixture(None, 0);
    for (name, node, position, bias, loss) in [
        (
            "physical-left",
            "left",
            8,
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "physical-right",
            "right",
            4,
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "ancestor-retained",
            "root",
            14,
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "seam-left",
            "root",
            10,
            InsertionBias::Left,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "seam-right",
            "root",
            10,
            InsertionBias::Right,
            AnchorLossPolicy::KeepUnresolved,
        ),
        (
            "drop",
            "root",
            8,
            InsertionBias::Right,
            AnchorLossPolicy::DeleteOwned,
        ),
    ] {
        before = edit(
            &before,
            Command::SetMark {
                id: MarkId::new(name).unwrap(),
                owner: id(node),
                label: name.into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: id(node),
                        position: ExactRatio::integer(position),
                    },
                    bias,
                },
                loss_policy: loss,
            },
        )
        .0;
    }
    before = edit(
        &before,
        Command::SetMark {
            id: MarkId::new("source-pts").unwrap(),
            owner: id("left"),
            label: "Source PTS".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Source {
                    asset: AssetId::new("original").unwrap(),
                    moment: SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        timestamp: SourceTimestamp {
                            ticks: 18,
                            time_base: SourceTimeBase::new(1, 30).unwrap(),
                        },
                    },
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
    .0;
    let (after, _) = edit(&before, roll(&before, "left", "right", -3));
    assert_eq!(
        after.marks()[&MarkId::new("source-pts").unwrap()],
        before.marks()[&MarkId::new("source-pts").unwrap()]
    );
    for (name, position) in [
        ("physical-left", 8),
        ("physical-right", 7),
        ("ancestor-retained", 14),
        ("seam-right", 10),
    ] {
        let mark = &after.marks()[&MarkId::new(name).unwrap()];
        assert_eq!(mark.state, MarkState::Bound);
        let Anchor::Local {
            position: actual, ..
        } = mark.boundary.coordinate
        else {
            panic!()
        };
        assert_eq!(actual, ExactRatio::integer(position), "{name}");
    }
    assert!(matches!(
        after.marks()[&MarkId::new("seam-left").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    assert!(!after.marks().contains_key(&MarkId::new("drop").unwrap()));
    let mark = &after.marks()[&MarkId::new("physical-left").unwrap()];
    assert_eq!(
        AnchorIndex::new(&after)
            .unwrap()
            .resolve_target(&AnchorTarget {
                boundary: mark.boundary.clone(),
                occurrence: Some(InstancePath {
                    node: id("left"),
                    repeats: vec![]
                })
            })
            .unwrap_err()
            .code,
        AnchorErrorCode::OutsideMapping
    );
}

#[test]
fn historical_command_grammars_refuse_roll_and_current_wire_is_closed() {
    let before = fixture(None, 0);
    let req = request(&before, roll(&before, "left", "right", 3));
    let json = serde_json::to_string(&req).unwrap();
    macro_rules! closed { ($($module:ident),+ $(,)?) => { $(assert!($module::upgrade_request(&json).is_err(),stringify!($module));)+ }; }
    closed!(
        legacy_v1, legacy_v2, legacy_v3, legacy_v4, legacy_v5, legacy_v6, legacy_v7, legacy_v8,
        legacy_v9, legacy_v10, legacy_v11, legacy_v12, legacy_v13, legacy_v14, legacy_v15,
        legacy_v16, legacy_v17, legacy_v18, legacy_v19, legacy_v20, legacy_v21, legacy_v22,
        legacy_v23, legacy_v24, legacy_v25, legacy_v26, legacy_v27, legacy_v28, legacy_v29,
        legacy_v30, legacy_v31, legacy_v32
    );
    for validate in [
        legacy_v29::validate_request_context,
        legacy_v30::validate_request_context,
        legacy_v31::validate_request_context,
        legacy_v32::validate_request_context,
    ] {
        assert!(validate(&before, &req).is_err());
    }
    assert_eq!(serde_json::from_str::<CommandRequest>(&json).unwrap(), req);
}
