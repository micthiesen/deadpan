use deadpan_core::*;
use serde_json::{Value, json};

fn id(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn range(a: i64, b: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap()
}
fn exact(a: ExactRatio, b: ExactRatio) -> ExactFrameRange {
    ExactFrameRange::new(a, b).unwrap()
}
fn window(a: ExactRatio, b: ExactRatio) -> SourceEditWindow {
    SourceEditWindow::new(a, b).unwrap()
}
fn span(a: i64, b: i64, rate: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, rate).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: a,
            time_base,
        },
        SourceTimestamp {
            ticks: b,
            time_base,
        },
    )
    .unwrap()
}
fn fixture() -> ProjectDocument {
    let asset = AssetId::new("original").unwrap();
    let picture = span(0, 100, 30);
    let audio = span(0, 147_000, 44_100);
    let w = window(ratio(1, 3), ratio(29, 3));
    let source = SourceNode {
        duration: frames(10),
        edit_window: Some(w),
        video: SourceVideo::Stream {
            asset: asset.clone(),
            span: picture,
        },
        video_mapping: SourceVideoMapping::SelectedPlacement {
            start: ExactRatio::integer(-10),
            frames: ExactRatio::integer(100),
            selection: exact(w.start(), w.end()),
            endpoints: EndpointPolicy::HoldAdjacent,
        },
        audio: Some(SourceAudio { asset, span: audio }),
        audio_mapping: SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::integer(-10),
            frames: ExactRatio::integer(100),
            selection: exact(
                w.start().checked_sub(ratio(17, 1600)).unwrap(),
                w.end().checked_sub(ratio(17, 1600)).unwrap(),
            ),
        },
        audio_offset: AudioSample(17),
        link: LinkRelation::Linked,
    };
    ProjectDocument::from_json(&json!({
        "schema_version":DOCUMENT_SCHEMA_VERSION,"project_id":"trim","revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root","marks":{},"overrides":{},
        "assets":{"original":AssetRecord {label:"Original".into(),content_hash:"a".repeat(64),video:Some(picture),audio:Some(audio),still_image:false,frame_count:Some(frames(100)),source_qualification:Some(SourceQualificationId::new("b".repeat(64)).unwrap())}},
        "nodes":{"root":BeatNode::sequence("Root",vec![id("source")]),"source":BeatNode {label:"Source".into(),framing:None,audio_treatments:Default::default(),audio_editorial_edges: Default::default(), audio_edges:Default::default(),kind:NodeKind::Source {source}, cutaways: Vec::new() }},
    }).to_string()).unwrap()
}
fn modify(d: &ProjectDocument, f: impl FnOnce(&mut Value)) -> ProjectDocument {
    let mut value = serde_json::to_value(d).unwrap();
    f(&mut value);
    ProjectDocument::from_json(&value.to_string()).unwrap()
}
fn source(d: &ProjectDocument) -> &SourceNode {
    let NodeKind::Source { source } = &d.nodes()[&id("source")].kind else {
        panic!()
    };
    source
}
fn crop(a: i64, b: i64) -> BeatNode {
    BeatNode {
        label: "View".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("source"),
            duration: frames(b - a),
            mapping: range(a, b),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
        cutaways: Vec::new(),
    }
}
fn partition(d: &ProjectDocument, a: i64, b: i64) -> ProjectDocument {
    modify(d, |v| {
        v["nodes"]["root"]["kind"]["children"] = json!(["view"]);
        v["nodes"]["view"] = serde_json::to_value(crop(a, b)).unwrap();
    })
}
fn trim(
    d: &ProjectDocument,
    target: &str,
    edge: SourceTrimEdge,
    delta: i64,
) -> SourceTrimResolution {
    let before = d.clone();
    let r = d
        .source_trim(
            &id("root"),
            &id(target),
            edge,
            delta,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(*d, before);
    assert_eq!(r.before, *source(d));
    // Validate the candidate media/crop independently of the authored reducer.
    modify(d, |v| {
        v["nodes"]["source"]["kind"]["source"] = serde_json::to_value(&r.after).unwrap();
        if target == "view" || r.needs_wrapper {
            v["nodes"]["view"] = serde_json::to_value(crop(
                r.allocation_after.start().0,
                r.allocation_after.end().0,
            ))
            .unwrap();
            v["nodes"]["root"]["kind"]["children"] = json!(["view"]);
        }
    });
    r
}

#[test]
fn fractional_padding_moves_with_either_edge_and_owner_only_grows() {
    let d = fixture();
    let i = trim(&d, "source", SourceTrimEdge::In, 3);
    assert_eq!(i.allocation_after, range(3, 10));
    assert_eq!(i.effective_after, window(ratio(10, 3), ratio(29, 3)));
    assert_eq!(i.window_after, i.effective_after);
    assert_eq!(i.after.duration, frames(10));
    assert!(i.needs_wrapper);
    assert_eq!(i.duration_delta_frames, -3);
    assert_eq!(i.physical_prefix, FrameDuration::ZERO);
    assert_eq!(
        i.root_operation,
        Some(RootSoundOperation::Delete { range: range(0, 3) })
    );
    assert_eq!(
        i.target_timing_window,
        Some(exact(ExactRatio::integer(3), ExactRatio::integer(10)))
    );
    assert_eq!(
        i.after.video_mapping.start_frames(),
        ExactRatio::integer(-10)
    );

    let i = trim(&d, "source", SourceTrimEdge::In, -4);
    assert_eq!(i.allocation_after, range(0, 14));
    assert_eq!(i.effective_after, window(ratio(1, 3), ratio(41, 3)));
    assert_eq!(i.physical_prefix, frames(4));
    assert_eq!(i.after.duration, frames(14));
    assert!(!i.needs_wrapper);
    assert_eq!(
        i.after.video_mapping.start_frames(),
        ExactRatio::integer(-6)
    );
    assert_eq!(
        i.after.audio_mapping.start_frames(),
        ExactRatio::integer(-6)
    );
    assert_eq!(i.after.audio_offset, AudioSample(17));
    assert_eq!(
        i.root_operation,
        Some(RootSoundOperation::Insert {
            at: ProjectFrame(0),
            duration: frames(4)
        })
    );
    assert_eq!(
        i.target_timing_window,
        Some(exact(ExactRatio::ZERO, ExactRatio::integer(10)))
    );

    let o = trim(&d, "source", SourceTrimEdge::Out, 5);
    assert_eq!(o.allocation_after, range(0, 15));
    assert_eq!(o.effective_after, window(ratio(1, 3), ratio(44, 3)));
    assert_eq!(o.after.duration, frames(15));
    assert_eq!(o.physical_prefix, FrameDuration::ZERO);
    assert!(!o.needs_wrapper);
    assert_eq!(
        o.root_operation,
        Some(RootSoundOperation::Insert {
            at: ProjectFrame(10),
            duration: frames(5)
        })
    );
    assert_eq!(o.target_timing_window, None);
    let o = trim(&d, "source", SourceTrimEdge::Out, -3);
    assert_eq!(o.allocation_after, range(0, 7));
    assert_eq!(o.effective_after, window(ratio(1, 3), ratio(20, 3)));
    assert_eq!(o.after.duration, frames(10));
    assert!(o.needs_wrapper);
    assert_eq!(
        o.root_operation,
        Some(RootSoundOperation::Delete {
            range: range(7, 10)
        })
    );
}

#[test]
fn exact_handle_reports_distinguish_exclusive_selected_and_inclusive_output_limits() {
    let d = fixture();
    let i = trim(&d, "source", SourceTrimEdge::In, -100);
    assert_eq!(i.minimum_delta.delta, ratio(-31, 3));
    assert!(i.minimum_delta.inclusive);
    assert_eq!((i.minimum_delta_frames, i.maximum_delta_frames), (-10, 9));
    assert_eq!(
        i.maximum_delta.reason,
        SourceTrimClamp::MinimumOutputDuration
    );
    assert_eq!(i.applied_delta_frames, -10);
    assert_eq!(i.clamp, Some(SourceTrimClamp::PictureStart));
    let o = trim(&d, "source", SourceTrimEdge::Out, 100);
    assert_eq!(o.maximum_delta.delta, ratio(241, 3));
    assert_eq!((o.minimum_delta_frames, o.maximum_delta_frames), (-9, 80));
    assert_eq!(o.applied_delta_frames, 80);
    assert_eq!(o.clamp, Some(SourceTrimClamp::PictureEnd));
    // Six selected frames surrounded by two frames of padding at either end.
    let narrow = modify(&d, |v| {
        let s = &mut v["nodes"]["source"]["kind"]["source"];
        s["edit_window"] =
            serde_json::to_value(window(ExactRatio::integer(2), ExactRatio::integer(8))).unwrap();
        s["video_mapping"]["selection"] =
            serde_json::to_value(exact(ExactRatio::integer(2), ExactRatio::integer(8))).unwrap();
        s["audio_mapping"]["selection"] = serde_json::to_value(exact(
            ExactRatio::integer(2).checked_sub(ratio(17, 1600)).unwrap(),
            ExactRatio::integer(8).checked_sub(ratio(17, 1600)).unwrap(),
        ))
        .unwrap();
    });
    let i = trim(&narrow, "source", SourceTrimEdge::In, 99);
    assert_eq!(i.maximum_delta.delta, ExactRatio::integer(6));
    assert!(!i.maximum_delta.inclusive);
    assert_eq!(i.maximum_delta_frames, 5);
    assert_eq!(i.applied_delta_frames, 5);
    assert_eq!(i.clamp, Some(SourceTrimClamp::MinimumSelectedDuration));
    let o = trim(&narrow, "source", SourceTrimEdge::Out, -99);
    assert_eq!(o.minimum_delta.delta, ExactRatio::integer(-6));
    assert!(!o.minimum_delta.inclusive);
    assert_eq!(o.minimum_delta_frames, -5);
    assert_eq!(o.clamp, Some(SourceTrimClamp::MinimumSelectedDuration));
}

#[test]
fn neutral_crop_retains_hidden_window_and_filters_until_an_edge_reaches_them() {
    let d = partition(&fixture(), 4, 8);
    for (edge, delta, allocation) in [
        (SourceTrimEdge::In, 2, range(6, 8)),
        (SourceTrimEdge::In, -2, range(2, 8)),
        (SourceTrimEdge::Out, -2, range(4, 6)),
        (SourceTrimEdge::Out, 1, range(4, 9)),
    ] {
        let r = trim(&d, "view", edge, delta);
        assert_eq!(r.allocation_after, allocation);
        assert_eq!(r.before, r.after);
        assert!(!r.needs_wrapper);
        assert_eq!(r.physical_source, id("source"));
        assert_eq!(r.target, id("view"));
    }
    let i = trim(&d, "view", SourceTrimEdge::In, -5);
    assert_eq!(i.physical_prefix, frames(1));
    assert_eq!(i.allocation_after, range(0, 9));
    assert_eq!(i.window_after, window(ExactRatio::ZERO, ratio(32, 3)));
    assert_eq!(i.after.duration, frames(11));
    assert!(!i.needs_wrapper);
    let o = trim(&d, "view", SourceTrimEdge::Out, 3);
    assert_eq!(o.window_after, window(ratio(1, 3), ExactRatio::integer(11)));
    assert_eq!(o.allocation_after, range(4, 11));
    assert_eq!(o.after.duration, frames(11));
}

#[test]
fn zero_resolution_preserves_mapping_variants_and_carries_no_authored_work() {
    let d = modify(&fixture(), |v| {
        let s = &mut v["nodes"]["source"]["kind"]["source"];
        s["duration"] = json!(100);
        s["edit_window"] =
            serde_json::to_value(window(ExactRatio::ZERO, ExactRatio::integer(100))).unwrap();
        s["video_mapping"] = serde_json::to_value(SourceVideoMapping::Duration {
            frames: ExactRatio::integer(100),
            endpoints: EndpointPolicy::HoldAdjacent,
        })
        .unwrap();
        s["audio"] = Value::Null;
        s["audio_mapping"] = serde_json::to_value(SourceAudioMapping::FitBeat).unwrap();
        s["link"] = json!("independent");
    });
    for (edge, wanted, clamp) in [
        (SourceTrimEdge::In, -1, Some(SourceTrimClamp::PictureStart)),
        (SourceTrimEdge::Out, 1, Some(SourceTrimClamp::PictureEnd)),
        (SourceTrimEdge::In, 0, None),
    ] {
        let r = trim(&d, "source", edge, wanted);
        assert_eq!(r.applied_delta_frames, 0);
        assert_eq!(r.before, r.after);
        assert_eq!(r.allocation_before, r.allocation_after);
        assert_eq!(r.output_before, r.output_after);
        assert_eq!(r.clamp, clamp);
        assert!(!r.needs_wrapper);
        assert_eq!(r.root_operation, None);
        assert_eq!(r.target_timing_window, None);
        assert_eq!(r.suffix_timing_window, None);
    }
    assert!(serde_json::from_str::<SourceTrimMode>("\"overwrite\"").is_err());
}

#[test]
fn linked_non_natural_affine_clock_and_independent_offset_survive_prefix_and_extension() {
    let d = modify(&fixture(), |v| {
        let s = &mut v["nodes"]["source"]["kind"]["source"];
        for name in ["video_mapping", "audio_mapping"] {
            s[name]["start"] = serde_json::to_value(ExactRatio::integer(-20)).unwrap();
            s[name]["frames"] = serde_json::to_value(ExactRatio::integer(200)).unwrap();
        }
    });
    let r = trim(&d, "source", SourceTrimEdge::In, -4);
    let SourceVideo::Stream { span, .. } = r.after.video else {
        panic!()
    };
    let picture = r
        .after
        .video_mapping
        .selection_in_source(span, r.after.duration)
        .unwrap();
    // The explicit map has two output frames per original video tick.
    assert_eq!(picture.start().ticks, ratio(49, 6));
    assert_eq!(picture.end().ticks, ratio(89, 6));
    assert_eq!(
        r.after
            .video_mapping
            .duration_frames(r.after.duration)
            .unwrap(),
        ExactRatio::integer(200)
    );
    let audio = r
        .after
        .audio_mapping
        .selection_frames(r.after.duration)
        .unwrap();
    assert_eq!(
        audio.start,
        ratio(1, 3).checked_sub(ratio(17, 1600)).unwrap()
    );
    assert_eq!(
        audio.end,
        ratio(41, 3).checked_sub(ratio(17, 1600)).unwrap()
    );
    // Exact original sample at the new selected In, not a rounded frame/sample conversion.
    let selected_sample = audio
        .start
        .checked_sub(ExactRatio::integer(-16))
        .unwrap()
        .checked_mul(ExactRatio::integer(735))
        .unwrap();
    assert_eq!(selected_sample, ratio(3_839_101, 320));
}

#[test]
fn extension_activates_dormant_linked_context_on_either_side_but_never_invents_audio() {
    for (a, b, edge, delta, expected_start, expected_end) in [
        (
            44_100,
            88_200,
            SourceTrimEdge::Out,
            15,
            ExactRatio::integer(20),
            ratio(74, 3).checked_sub(ratio(17, 1600)).unwrap(),
        ),
        (
            0,
            14_700,
            SourceTrimEdge::In,
            -3,
            ratio(1, 3).checked_sub(ratio(17, 1600)).unwrap(),
            ExactRatio::integer(3),
        ),
    ] {
        let d = modify(&fixture(), |v| {
            let audio = span(a, b, 44_100);
            v["assets"]["original"]["audio"] = serde_json::to_value(audio).unwrap();
            let s = &mut v["nodes"]["source"]["kind"]["source"];
            s["audio"]["span"] = serde_json::to_value(audio).unwrap();
            let start = ratio(i128::from(a), 1470)
                .checked_sub(ExactRatio::integer(10))
                .unwrap();
            let extent = ratio(i128::from(b - a), 1470);
            let point = if a == 0 { ExactRatio::ZERO } else { start };
            s["audio_mapping"] = serde_json::to_value(SourceAudioMapping::SelectedPlacement {
                start,
                frames: extent,
                selection: ExactFrameRange {
                    start: point,
                    end: point,
                },
            })
            .unwrap();
        });
        let r = trim(&d, "source", edge, delta);
        assert!(r.after.audio.is_some());
        assert_eq!(r.after.link, LinkRelation::Linked);
        assert_eq!(
            r.after
                .audio_mapping
                .selection_frames(r.after.duration)
                .unwrap(),
            exact(expected_start, expected_end)
        );
    }
    let absent = modify(&fixture(), |v| {
        let s = &mut v["nodes"]["source"]["kind"]["source"];
        s["audio"] = Value::Null;
        s["link"] = json!("independent");
        s["audio_mapping"] = serde_json::to_value(SourceAudioMapping::FitBeat).unwrap();
    });
    let r = trim(&absent, "source", SourceTrimEdge::In, -3);
    assert!(r.after.audio.is_none());
    assert_eq!(r.after.audio_mapping, SourceAudioMapping::FitBeat);
}

#[test]
fn nested_sequence_reports_old_project_windows_and_exact_root_ripple_once() {
    let d = modify(&fixture(), |v| {
        v["nodes"]["prefix"] = v["nodes"]["source"].clone();
        v["nodes"]["suffix"] = v["nodes"]["source"].clone();
        v["nodes"]["group"] =
            serde_json::to_value(BeatNode::sequence("Group", vec![id("source")])).unwrap();
        v["nodes"]["root"]["kind"]["children"] = json!(["prefix", "group", "suffix"]);
    });
    for (edge, delta, new_end, operation, target) in [
        (
            SourceTrimEdge::In,
            3,
            17,
            RootSoundOperation::Delete {
                range: range(10, 13),
            },
            Some(exact(ExactRatio::integer(13), ExactRatio::integer(20))),
        ),
        (
            SourceTrimEdge::In,
            -3,
            23,
            RootSoundOperation::Insert {
                at: ProjectFrame(10),
                duration: frames(3),
            },
            Some(exact(ExactRatio::integer(10), ExactRatio::integer(20))),
        ),
        (
            SourceTrimEdge::Out,
            3,
            23,
            RootSoundOperation::Insert {
                at: ProjectFrame(20),
                duration: frames(3),
            },
            None,
        ),
        (
            SourceTrimEdge::Out,
            -3,
            17,
            RootSoundOperation::Delete {
                range: range(17, 20),
            },
            None,
        ),
    ] {
        let r = d
            .source_trim(
                &id("group"),
                &id("source"),
                edge,
                delta,
                SourceTrimMode::Ripple,
            )
            .unwrap();
        assert_eq!(r.output_before, range(10, 20));
        assert_eq!(r.output_after, range(10, new_end));
        assert_eq!(r.slot, 0);
        assert_eq!(r.root_operation, Some(operation));
        assert_eq!(r.target_timing_window, target);
        assert_eq!(
            r.suffix_timing_window,
            Some(exact(ExactRatio::integer(20), ExactRatio::integer(30)))
        );
    }
}

#[test]
fn rejects_missing_qualification_generic_or_incoherent_clocks_and_held_picture_windows() {
    for case in 0..6 {
        let d = modify(&fixture(), |v| {
            if case == 0 {
                v["assets"]["original"]["source_qualification"] = Value::Null;
                return;
            }
            let s = &mut v["nodes"]["source"]["kind"]["source"];
            match case {
                1 => s["edit_window"] = Value::Null,
                2 => {
                    s["video_mapping"] = serde_json::to_value(SourceVideoMapping::FitBeat).unwrap()
                }
                3 => {
                    s["audio_mapping"]["start"] =
                        serde_json::to_value(ExactRatio::integer(-9)).unwrap()
                }
                4 => s["link"] = json!("independent"),
                _ => {
                    s["video_mapping"]["start"] =
                        serde_json::to_value(ExactRatio::integer(2)).unwrap();
                    s["video_mapping"]["selection"] =
                        serde_json::to_value(exact(ExactRatio::integer(2), ratio(29, 3))).unwrap();
                }
            }
        });
        let snapshot = d.clone();
        let error = d
            .source_trim(
                &id("root"),
                &id("source"),
                SourceTrimEdge::In,
                1,
                SourceTrimMode::Ripple,
            )
            .unwrap_err();
        assert_eq!(
            error.code,
            EditErrorCode::SourceRangeInvalid,
            "case {case}: {error:?}"
        );
        assert_eq!(d, snapshot);
    }
}

#[test]
fn rejects_treated_partition_and_composite_scope_including_zero_queries() {
    let d = partition(&fixture(), 4, 8);
    let treated = modify(&d, |v| {
        v["nodes"]["view"]["framing"] =
            serde_json::to_value(Framing::static_pose(FramingPose::identity()).unwrap()).unwrap();
    });
    assert_eq!(
        treated
            .source_trim(
                &id("root"),
                &id("view"),
                SourceTrimEdge::Out,
                0,
                SourceTrimMode::Ripple
            )
            .unwrap_err()
            .code,
        EditErrorCode::WrongNodeKind
    );
    let authored = modify(&d, |v| {
        v["nodes"]["view"]["kind"]["purpose"] = json!("edit")
    });
    assert_eq!(
        authored
            .source_trim(
                &id("root"),
                &id("view"),
                SourceTrimEdge::Out,
                1,
                SourceTrimMode::Ripple
            )
            .unwrap_err()
            .code,
        EditErrorCode::WrongNodeKind
    );
    assert_eq!(
        d.source_trim(
            &id("root"),
            &id("source"),
            SourceTrimEdge::Out,
            1,
            SourceTrimMode::Ripple
        )
        .unwrap_err()
        .code,
        EditErrorCode::SelectionUnavailable
    );
}

#[test]
fn prefix_overflow_in_dormant_context_is_an_error_without_clamping_or_mutation() {
    let end = i64::try_from((i128::from(i64::MAX) + 19) / 2).unwrap();
    let audio = span(end - 100, end, 30);
    let start = ratio(2 * i128::from(end - 100) - 20, 1);
    let d = modify(&fixture(), |v| {
        v["assets"]["original"]["audio"] = serde_json::to_value(audio).unwrap();
        let s = &mut v["nodes"]["source"]["kind"]["source"];
        s["audio_offset"] = json!(0);
        s["audio"]["span"] = serde_json::to_value(audio).unwrap();
        s["video_mapping"]["start"] = serde_json::to_value(ExactRatio::integer(-20)).unwrap();
        s["video_mapping"]["frames"] = serde_json::to_value(ExactRatio::integer(200)).unwrap();
        s["audio_mapping"] = serde_json::to_value(SourceAudioMapping::SelectedPlacement {
            start,
            frames: ExactRatio::integer(200),
            selection: ExactFrameRange { start, end: start },
        })
        .unwrap();
    });
    let snapshot = d.clone();
    let error = d
        .source_trim(
            &id("root"),
            &id("source"),
            SourceTrimEdge::In,
            -20,
            SourceTrimMode::Ripple,
        )
        .unwrap_err();
    assert_eq!(error.code, EditErrorCode::TimingOverflow);
    assert_eq!(d, snapshot);
}
