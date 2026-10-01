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

fn geometry(document: &ProjectDocument, intent: SourceTrimIntent) -> SourceTrimGeometry {
    document
        .source_trim_geometry(&id("root"), &id("left"), Some(&id("right")), intent)
        .unwrap()
}

fn intent(i: i64, o: i64, s: i64, r: i64) -> SourceTrimIntent {
    SourceTrimIntent {
        in_frames: i,
        out_frames: o,
        slip_frames: s,
        roll_frames: r,
        policy: SourceTrimPolicy::Ripple,
    }
}

fn adjust(
    document: &ProjectDocument,
    accepted: SourceTrimIntent,
    mode: SourceTrimControl,
    step: i64,
) -> SourceTrimAdjustment {
    document
        .adjust_source_trim_geometry(
            &id("root"),
            &id("left"),
            Some(&id("right")),
            accepted,
            mode,
            step,
        )
        .unwrap()
}

#[test]
fn joint_geometry_preserves_exact_padding_affine_clocks_and_one_entry_snapshot() {
    let before = modify(&fixture(Some((0, 147000)), 17), |v| {
        v["nodes"]["prefix"] = serde_json::to_value(hold(3)).unwrap();
        v["nodes"]["suffix"] = serde_json::to_value(hold(10)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "left", "right", "suffix"]);
    });
    let entry = before.to_json().unwrap();
    let result = geometry(&before, intent(2, 5, 3, -1));
    assert_eq!(result.project_duration_before, frames(33));
    assert_eq!(result.project_duration_after, frames(36));
    assert_eq!(result.duration_delta_frames, 3);
    assert_eq!(result.target.output_after, range(3, 15));
    assert_eq!(result.target.allocation_after, range(2, 14));
    assert_eq!(
        result.target.effective_after,
        SourceEditWindow::new(ratio(7, 3), ratio(41, 3)).unwrap()
    );
    assert_eq!(
        result.target.after.video_mapping.start_frames(),
        ratio(-13, 1)
    );
    assert_eq!(
        result.target.after.audio_mapping.start_frames(),
        ratio(-13, 1)
    );
    assert_eq!(result.target.after.audio_offset, AudioSample(17));
    let audio = result
        .target
        .after
        .audio_mapping
        .selection_frames(result.target.after.duration)
        .unwrap();
    assert_eq!(
        audio.start,
        ratio(7, 3).checked_sub(ratio(17, 1600)).unwrap()
    );
    assert_eq!(
        audio.end,
        ratio(41, 3).checked_sub(ratio(17, 1600)).unwrap()
    );
    let right = result.right.unwrap();
    assert_eq!(right.output_after, range(15, 26));
    assert_eq!(right.physical_prefix, frames(1));
    assert_eq!(right.allocation_after, range(0, 11));
    assert_eq!(
        right.effective_after,
        SourceEditWindow::new(ratio(1, 3), ratio(32, 3)).unwrap()
    );
    assert_eq!(right.after.video_mapping.start_frames(), ratio(-9, 1));
    assert_eq!(right.before.video, right.after.video);
    assert_eq!(right.before.audio, right.after.audio);
    assert_eq!(before.to_json().unwrap(), entry);
}

#[test]
fn every_single_control_matches_scalar_source_geometry_and_clamp() {
    for audio in [None, Some((0, 147000)), Some((0, 100))] {
        for partition in [false, true] {
            let before = modify(&fixture(audio, 17), |v| {
                if partition {
                    v["nodes"]["view"] = serde_json::to_value(crop("left", 0, 10)).unwrap();
                    v["nodes"]["root"]["kind"]["children"] = json!(["view", "right"]);
                }
            });
            let target = id(if partition { "view" } else { "left" });
            for delta in [-100, -2, 0, 2, 100] {
                for (mode, edge) in [
                    (SourceTrimControl::In, SourceTrimEdge::In),
                    (SourceTrimControl::Out, SourceTrimEdge::Out),
                ] {
                    let scalar = before
                        .source_trim(&id("root"), &target, edge, delta, SourceTrimMode::Ripple)
                        .unwrap();
                    let combined = before
                        .adjust_source_trim_geometry(
                            &id("root"),
                            &target,
                            Some(&id("right")),
                            SourceTrimIntent::default(),
                            mode,
                            delta,
                        )
                        .unwrap();
                    assert_eq!(combined.applied_value, scalar.applied_delta_frames);
                    assert_eq!(combined.geometry.target.after, scalar.after);
                    assert_eq!(
                        combined.geometry.target.allocation_after,
                        scalar.allocation_after
                    );
                    assert_eq!(combined.geometry.target.output_after, scalar.output_after);
                    assert_eq!(combined.geometry.target.needs_wrapper, scalar.needs_wrapper);
                    assert_eq!(
                        combined.geometry.duration_delta_frames,
                        scalar.duration_delta_frames
                    );
                }
                let slip = before.source_slip(&id("root"), &target, delta).unwrap();
                let combined = before
                    .adjust_source_trim_geometry(
                        &id("root"),
                        &target,
                        Some(&id("right")),
                        SourceTrimIntent::default(),
                        SourceTrimControl::Slip,
                        delta,
                    )
                    .unwrap();
                assert_eq!(combined.applied_value, slip.applied_delta_frames);
                assert_eq!(combined.geometry.target.after, slip.after);
                assert_eq!(combined.geometry.duration_delta_frames, 0);
                let roll = before
                    .source_roll(&id("root"), &target, &id("right"), delta)
                    .unwrap();
                let combined = before
                    .adjust_source_trim_geometry(
                        &id("root"),
                        &target,
                        Some(&id("right")),
                        SourceTrimIntent::default(),
                        SourceTrimControl::Roll,
                        delta,
                    )
                    .unwrap();
                assert_eq!(combined.applied_value, roll.applied_delta_frames);
                assert_eq!(combined.geometry.target.after, roll.left.after);
                assert_eq!(
                    combined.geometry.target.allocation_after,
                    roll.left.allocation_after
                );
                let right = combined.geometry.right.unwrap();
                assert_eq!(right.after, roll.right.after);
                assert_eq!(right.allocation_after, roll.right.allocation_after);
                assert_eq!(right.physical_prefix, roll.right.physical_prefix);
            }
        }
    }
}

#[test]
fn one_active_clamp_discards_overshoot_and_other_controls_remain_accepted() {
    let before = fixture(None, 0);
    let accepted = intent(2, 1, 0, 1);
    let last = adjust(&before, accepted, SourceTrimControl::Slip, 100);
    assert_eq!(last.applied_value, 78);
    assert_eq!(last.maximum.value, ratio(235, 3));
    assert_eq!(
        last.clamp.unwrap().constraint,
        SourceTrimGeometryConstraint::PictureEnd
    );
    assert_eq!(last.geometry.intent, intent(2, 1, 78, 1));
    let reverse = adjust(&before, last.geometry.intent, SourceTrimControl::Slip, -1);
    assert_eq!(reverse.previous_value, 78);
    assert_eq!(reverse.requested_value, 77);
    assert_eq!(reverse.applied_value, 77);
    assert!(reverse.clamp.is_none());
    assert_eq!(reverse.geometry.intent, intent(2, 1, 77, 1));
    assert_eq!(geometry(&before, reverse.geometry.intent), reverse.geometry);
    let first = adjust(
        &before,
        SourceTrimIntent::default(),
        SourceTrimControl::Out,
        100,
    );
    let then_slip = adjust(&before, first.geometry.intent, SourceTrimControl::Slip, 2);
    let slipped = adjust(
        &before,
        SourceTrimIntent::default(),
        SourceTrimControl::Slip,
        2,
    );
    let then_out = adjust(
        &before,
        slipped.geometry.intent,
        SourceTrimControl::Out,
        100,
    );
    assert_ne!(then_slip.geometry.intent, then_out.geometry.intent);
    assert_eq!(then_slip.geometry.intent.slip_frames, 0);
    assert_eq!(then_out.geometry.intent.slip_frames, 2);
}

#[test]
fn out_and_roll_compensate_without_intermediate_negative_or_oversized_target() {
    let before = modify(&fixture(None, 0), |v| {
        v["nodes"]["right"]["kind"]["source"]["video_mapping"]["start"] = json!(ratio(-40, 1));
    });
    // Roll -15 alone would empty A. Out +20 makes the complete state valid.
    let result = geometry(&before, intent(0, 20, 0, -15));
    assert_eq!(result.target.output_after, range(0, 15));
    assert_eq!(result.right.unwrap().output_after, range(15, 40));
    assert_eq!(result.duration_delta_frames, 20);
    assert!(
        before
            .source_roll(&id("root"), &id("left"), &id("right"), -15)
            .unwrap()
            .clamp
            .is_some()
    );
    // Opposite contributions at A's Out do not erase B's independent Roll.
    let result = geometry(&before, intent(0, -3, 0, 3));
    assert_eq!(result.target.after, *source(&before, "left"));
    assert_eq!(result.target.output_after, range(0, 10));
    assert_eq!(result.right.unwrap().allocation_after, range(3, 10));
    assert_eq!(result.duration_delta_frames, -3);
    assert!(!result.intent.is_zero());
}

#[test]
fn disjoint_new_material_reports_closed_old_endpoints_without_handle_narrowing() {
    let before = fixture(Some((0, 147000)), 0);
    let later = geometry(&before, intent(20, 20, 0, 0));
    assert_eq!(later.target.allocation_after, range(20, 30));
    assert_eq!(later.target.output_after, range(0, 10));
    assert_eq!(later.target.phase_anchor, SourceTrimPhaseAnchor::SourceEnd);
    let before = modify(&before, |v| {
        // Shift both complete affine contexts earlier, retaining a longer head handle.
        for name in ["left", "right"] {
            v["nodes"][name]["kind"]["source"]["video_mapping"]["start"] = json!(ratio(-40, 1));
            v["nodes"][name]["kind"]["source"]["audio_mapping"]["start"] = json!(ratio(-40, 1));
        }
    });
    let earlier = geometry(&before, intent(-20, -20, 0, 0));
    assert_eq!(earlier.target.physical_prefix, frames(20));
    assert_eq!(earlier.target.allocation_after, range(0, 10));
    assert_eq!(earlier.target.after.duration, frames(30));
    assert_eq!(
        earlier.target.phase_anchor,
        SourceTrimPhaseAnchor::SourceStart
    );
    assert_eq!(earlier.target.effective_after, window());
    let partial = geometry(&before, intent(2, -2, 0, 0));
    assert_eq!(
        partial.target.phase_anchor,
        SourceTrimPhaseAnchor::Retained {
            allocation: range(2, 8)
        }
    );
}

#[test]
fn absent_ineligible_and_nonadjacent_neighbors_do_not_block_nonroll_values() {
    let before = modify(&fixture(None, 0), |v| {
        v["nodes"]["right"] = serde_json::to_value(hold(10)).unwrap();
    });
    for right in [None, Some(id("right")), Some(id("missing"))] {
        let result = before
            .source_trim_geometry(&id("root"), &id("left"), right.as_ref(), intent(1, 2, 3, 0))
            .unwrap();
        assert!(result.right.is_none());
        assert!(matches!(
            result.roll_availability,
            SourceTrimRollAvailability::Unavailable { .. }
        ));
        assert!(
            before
                .source_trim_geometry(&id("root"), &id("left"), right.as_ref(), intent(1, 2, 3, 1))
                .is_err()
        );
        assert!(
            before
                .adjust_source_trim_geometry(
                    &id("root"),
                    &id("left"),
                    right.as_ref(),
                    SourceTrimIntent::default(),
                    SourceTrimControl::Roll,
                    1
                )
                .is_err()
        );
    }
    let before = modify(&fixture(None, 0), |v| {
        v["nodes"]["empty"] = serde_json::to_value(BeatNode::sequence("Empty", vec![])).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "empty", "right"]);
    });
    assert!(matches!(
        geometry(&before, intent(0, 0, 1, 0)).roll_availability,
        SourceTrimRollAvailability::Unavailable { .. }
    ));
    assert!(
        before
            .source_trim_geometry(
                &id("root"),
                &id("left"),
                Some(&id("right")),
                intent(0, 0, 0, 1)
            )
            .is_err()
    );
}

#[test]
fn overwrite_uses_selected_scope_bounds_and_never_claims_overlay_admission() {
    let before = modify(&fixture(None, 0), |v| {
        v["nodes"]["prefix"] = serde_json::to_value(hold(7)).unwrap();
        v["nodes"]["suffix"] = serde_json::to_value(hold(8)).unwrap();
        v["nodes"]["group"] =
            serde_json::to_value(BeatNode::sequence("Group", vec![id("left"), id("right")]))
                .unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "group", "suffix"]);
    });
    let overwrite = SourceTrimIntent {
        policy: SourceTrimPolicy::Overwrite,
        ..Default::default()
    };
    let result = before
        .adjust_source_trim_geometry(
            &id("group"),
            &id("left"),
            Some(&id("right")),
            overwrite,
            SourceTrimControl::In,
            -100,
        )
        .unwrap();
    assert_eq!(result.applied_value, 0);
    assert_eq!(
        result.clamp.unwrap().constraint,
        SourceTrimGeometryConstraint::ScopeStart
    );
    assert_eq!(result.geometry.scope_before, range(7, 27));
    assert_eq!(result.geometry.scope_after, range(7, 27));
    let result = before
        .adjust_source_trim_geometry(
            &id("group"),
            &id("left"),
            Some(&id("right")),
            overwrite,
            SourceTrimControl::Out,
            100,
        )
        .unwrap();
    assert_eq!(result.applied_value, 10);
    assert_eq!(
        result.clamp.unwrap().constraint,
        SourceTrimGeometryConstraint::ScopeEnd
    );
    assert_eq!(result.geometry.target.output_after, range(7, 27));
    assert_eq!(result.geometry.right.unwrap().output_after, range(17, 27)); // pre-overlay
    assert_eq!(result.geometry.project_duration_after, frames(35));
    assert!(result.geometry.requires_overwrite_overlay);
    let ripple = intent(-1, 0, 0, 0);
    assert!(
        before
            .source_trim_geometry(&id("group"), &id("left"), Some(&id("right")), ripple)
            .is_ok()
    );
    assert!(
        before
            .source_trim_geometry(
                &id("group"),
                &id("left"),
                Some(&id("right")),
                SourceTrimIntent {
                    policy: SourceTrimPolicy::Overwrite,
                    ..ripple
                }
            )
            .is_err()
    );
    assert_eq!(ripple, intent(-1, 0, 0, 0));
}

#[test]
fn signed_origin_vfr_context_retains_fractional_exact_source_endpoints() {
    let before = modify(&fixture(None, 0), |v| {
        let span = span(-40, 60, 30);
        v["assets"]["original"]["video"] = json!(span);
        v["assets"]["original"]["frame_count"] = json!(8);
        for name in ["left", "right"] {
            v["nodes"][name]["kind"]["source"]["video"]["span"] = json!(span);
        }
    });
    let result = geometry(&before, intent(2, 5, 3, -1));
    let source = &result.target.after;
    let SourceVideo::Stream { asset, span } = &source.video else {
        unreachable!()
    };
    let selected = source
        .video_mapping
        .selection_in_source(*span, source.duration)
        .unwrap();
    assert_eq!(selected.start().ticks, ratio(-74, 3));
    assert_eq!(selected.end().ticks, ratio(-40, 3));
    let index = SourceFrameIndex::new(
        asset.clone(),
        span.start().time_base,
        [-40, -30, -24, -15, -5, 20, 40, 55]
            .into_iter()
            .enumerate()
            .map(|(i, pts)| IndexedSourceFrame {
                identity: SourceFrameId(u64::try_from(i).unwrap()),
                pts,
                reported_duration: None,
                keyframe: i == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            })
            .collect(),
        60,
        TerminalProvenance::Explicit,
    )
    .unwrap();
    assert_eq!(
        index
            .select_in_exact_span(selected.start(), selected, EndpointPolicy::Reject)
            .unwrap()
            .identity,
        SourceFrameId(1)
    );
    assert_eq!(
        index
            .select_in_exact_span(selected.end(), selected, EndpointPolicy::HoldAdjacent)
            .unwrap()
            .identity,
        SourceFrameId(3)
    );
}

#[test]
fn strict_fractional_width_and_real_storage_overflow_are_separate_limits() {
    let before = modify(&fixture(None, 0), |v| {
        for name in ["left", "right"] {
            let window = SourceEditWindow::new(ratio(2, 1), ratio(3, 1)).unwrap();
            v["nodes"][name]["kind"]["source"]["edit_window"] = json!(window);
            v["nodes"][name]["kind"]["source"]["video_mapping"]["selection"] =
                json!(ExactFrameRange::new(window.start(), window.end()).unwrap());
        }
    });
    let result = adjust(
        &before,
        SourceTrimIntent::default(),
        SourceTrimControl::In,
        1,
    );
    assert_eq!(result.applied_value, 0);
    assert_eq!(result.maximum.value, ratio(1, 1));
    assert!(!result.maximum.inclusive);
    assert_eq!(
        result.maximum.constraint,
        SourceTrimGeometryConstraint::MinimumSelectedDuration
    );
    let huge = modify(&fixture(None, 0), |v| {
        v["presentation_basis"]["frame_rate"] = json!({"numerator":48000,"denominator":1});
        v["nodes"]["tail"] = serde_json::to_value(hold(i64::MAX - 20)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "right", "tail"]);
    });
    assert_eq!(
        geometry(&huge, intent(0, 0, 0, 3)).project_duration_after,
        frames(i64::MAX)
    );
    assert_eq!(
        huge.source_trim_geometry(
            &id("root"),
            &id("left"),
            Some(&id("right")),
            intent(0, 1, 0, 0)
        )
        .unwrap_err()
        .code,
        EditErrorCode::TimingOverflow
    );
    assert_eq!(
        huge.adjust_source_trim_geometry(
            &id("root"),
            &id("left"),
            Some(&id("right")),
            SourceTrimIntent::default(),
            SourceTrimControl::Out,
            1
        )
        .unwrap_err()
        .code,
        EditErrorCode::TimingOverflow
    );
    let overflow = modify(&fixture(None, 0), |v| {
        v["nodes"]["right"]["kind"]["source"]["duration"] = json!(i64::MAX);
        v["nodes"]["right-view"] = serde_json::to_value(crop("right", 0, 10)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left", "right-view"]);
    });
    assert_eq!(
        overflow
            .source_trim_geometry(
                &id("root"),
                &id("left"),
                Some(&id("right-view")),
                intent(0, 0, 0, -3)
            )
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
    let before = fixture(None, 0);
    assert_eq!(
        before
            .adjust_source_trim_geometry(
                &id("root"),
                &id("left"),
                Some(&id("right")),
                intent(0, 0, 1, 0),
                SourceTrimControl::Slip,
                i64::MAX
            )
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
}

#[test]
fn wrapper_requirements_count_both_owners_and_reuse_retained_partitions() {
    let before = fixture(None, 0);
    let result = geometry(&before, intent(1, 0, 0, 1));
    assert!(result.target.needs_wrapper);
    assert!(result.right.as_ref().unwrap().needs_wrapper);
    assert_eq!(result.required_source_wrappers, 2);
    let partitioned = modify(&before, |v| {
        v["nodes"]["left-view"] = serde_json::to_value(crop("left", 0, 10)).unwrap();
        v["nodes"]["right-view"] = serde_json::to_value(crop("right", 0, 10)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["left-view", "right-view"]);
    });
    let retained = partitioned
        .source_trim_geometry(
            &id("root"),
            &id("left-view"),
            Some(&id("right-view")),
            intent(1, 0, 0, 1),
        )
        .unwrap();
    assert_eq!(retained.required_source_wrappers, 0);
    assert_eq!(retained.target.after, result.target.after);
    assert_eq!(retained.right.unwrap().after, result.right.unwrap().after);
}

#[test]
fn joint_partition_edges_retain_hidden_selected_context_independently() {
    let before = modify(&fixture(Some((0, 147000)), 17), |v| {
        v["nodes"]["view"] = serde_json::to_value(crop("left", 2, 7)).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["view", "right"]);
    });
    let result = before
        .source_trim_geometry(
            &id("root"),
            &id("view"),
            Some(&id("right")),
            intent(1, 2, 0, 0),
        )
        .unwrap();
    assert_eq!(result.target.allocation_before, range(2, 7));
    assert_eq!(result.target.allocation_after, range(3, 9));
    assert_eq!(result.target.window_after, window());
    assert_eq!(result.target.after, *source(&before, "left"));
    assert_eq!(result.target.output_after, range(0, 6));
    assert_eq!(
        result.target.effective_after,
        SourceEditWindow::new(ratio(3, 1), ratio(9, 1)).unwrap()
    );
    let extended = before
        .source_trim_geometry(
            &id("root"),
            &id("view"),
            Some(&id("right")),
            intent(-4, 0, 0, 0),
        )
        .unwrap();
    assert_eq!(extended.target.physical_prefix, frames(2));
    assert_eq!(extended.target.allocation_after, range(0, 9));
    assert_eq!(
        extended.target.window_after,
        SourceEditWindow::new(ratio(0, 1), ratio(35, 3)).unwrap()
    );
    assert_eq!(extended.target.after.duration, frames(12));
    assert_eq!(extended.target.after.audio_offset, AudioSample(17));
}

#[test]
fn dormant_audio_activates_and_absent_audio_remains_absent_in_complete_intent() {
    let before = fixture(Some((0, 1470)), 17);
    assert_eq!(
        source(&before, "left")
            .audio_mapping
            .selection_frames(frames(10))
            .unwrap()
            .start,
        source(&before, "left")
            .audio_mapping
            .selection_frames(frames(10))
            .unwrap()
            .end
    );
    let active = geometry(&before, intent(-10, 0, 0, 0));
    let audible = active
        .target
        .after
        .audio_mapping
        .selection_frames(active.target.after.duration)
        .unwrap();
    assert!(audible.start.compare(audible.end).is_lt());
    assert_eq!(active.target.after.audio, source(&before, "left").audio);
    assert_eq!(active.target.after.audio_offset, AudioSample(17));
    let absent = geometry(&fixture(None, 17), intent(-10, 0, 0, 0));
    assert!(absent.target.after.audio.is_none());
    assert_eq!(
        absent.target.after.audio_mapping,
        SourceAudioMapping::FitBeat
    );
}

#[test]
fn zero_and_reverse_restore_representation_without_conflating_audio_intent() {
    let before = modify(&fixture(None, 0), |v| {
        let source = &mut v["nodes"]["left"]["kind"]["source"];
        source["edit_window"] = json!(SourceEditWindow::new(ratio(0, 1), ratio(10, 1)).unwrap());
        source["video"] = json!(SourceVideo::Stream {
            asset: AssetId::new("short").unwrap(),
            span: span(0, 10, 30)
        });
        source["video_mapping"] = json!(SourceVideoMapping::Placement {
            start: ratio(0, 1),
            frames: ratio(10, 1),
            endpoints: EndpointPolicy::HoldAdjacent
        });
        v["assets"]["short"] = json!(AssetRecord {
            label: "Short".into(),
            content_hash: "c".repeat(64),
            video: Some(span(0, 10, 30)),
            audio: None,
            still_image: false,
            frame_count: Some(frames(10)),
            source_qualification: Some(SourceQualificationId::new("d".repeat(64)).unwrap())
        });
    });
    for policy in [SourceTrimPolicy::Ripple, SourceTrimPolicy::Overwrite] {
        let zero = geometry(
            &before,
            SourceTrimIntent {
                policy,
                ..Default::default()
            },
        );
        assert_eq!(zero.target.before, zero.target.after);
        assert!(matches!(
            zero.target.after.video_mapping,
            SourceVideoMapping::Placement { .. }
        ));
        assert_eq!(zero.required_source_wrappers, 0);
        assert!(!zero.requires_overwrite_overlay);
    }
    let before = fixture(None, 0);
    let pending = adjust(
        &before,
        SourceTrimIntent::default(),
        SourceTrimControl::In,
        -2,
    );
    assert_eq!(pending.geometry.target.physical_prefix, frames(2));
    let reversed = adjust(&before, pending.geometry.intent, SourceTrimControl::In, 2);
    assert!(reversed.geometry.intent.is_zero());
    assert_eq!(reversed.geometry.target.after, *source(&before, "left"));
    assert_eq!(reversed.geometry.required_source_wrappers, 0);
    let slipped = geometry(&before, intent(0, 0, 2, 0));
    let edged = geometry(&before, intent(2, 2, 0, 0));
    assert_eq!(slipped.target.output_after, edged.target.output_after);
    assert_ne!(slipped.intent, edged.intent);
    assert_ne!(slipped.target.after, edged.target.after);
    let value = serde_json::to_value(slipped.intent).unwrap();
    assert!(value.get("mode").is_none());
    assert_eq!(
        serde_json::from_value::<SourceTrimIntent>(value.clone()).unwrap(),
        slipped.intent
    );
    let mut unknown = value;
    unknown["mode"] = json!("out");
    assert!(serde_json::from_value::<SourceTrimIntent>(unknown).is_err());
}
