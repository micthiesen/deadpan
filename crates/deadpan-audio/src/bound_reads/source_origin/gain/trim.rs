//! Independent decoded PCM for ripple edge trims. Video uses a declared common
//! affine clock; these tests do not qualify video decoding or store admission.

use super::*;

fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
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

fn linked(
    duration: i64,
    audio_start: ExactRatio,
    offset: i64,
    video: SourceSpan,
) -> (ProjectDocument, Provider) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut voice = source(rate, 0, 8197, duration);
    let (base, provider) = document(
        rate,
        &["lead", "a"],
        vec![("lead", hold(1)), ("a", voice.clone())],
    );
    voice.audio_edges.node_start = AudioEdgePolicy::Hard;
    voice.audio_edges.node_end = AudioEdgePolicy::Hard;
    voice.audio_edges.source_placement_start = AudioEdgePolicy::Hard;
    voice.audio_edges.source_placement_end = AudioEdgePolicy::Hard;
    let NodeKind::Source { source } = &mut voice.kind else {
        unreachable!()
    };
    let audio_frames = ratio(40985, 8008);
    let offset_frames = ratio(i128::from(offset) * 5, 8008);
    let audio_end = audio_start.checked_add(audio_frames).unwrap();
    let selected_start =
        ExactRatio::ZERO.max_exact(audio_start.checked_add(offset_frames).unwrap());
    let selected_end =
        ExactRatio::integer(duration).min_exact(audio_end.checked_add(offset_frames).unwrap());
    let selection = if selected_start.compare(selected_end).is_lt() {
        ExactFrameRange::new(
            selected_start.checked_sub(offset_frames).unwrap(),
            selected_end.checked_sub(offset_frames).unwrap(),
        )
        .unwrap()
    } else {
        let point = if audio_end
            .checked_add(offset_frames)
            .unwrap()
            .compare_integer(0)
            .is_le()
        {
            audio_end
        } else {
            audio_start
        };
        ExactFrameRange {
            start: point,
            end: point,
        }
    };
    source.video = SourceVideo::Stream {
        asset: AssetId::new("media").unwrap(),
        span: video,
    };
    source.video_mapping = SourceVideoMapping::SelectedPlacement {
        start: audio_start
            .checked_add(ratio(i128::from(video.start().ticks) * 5, 8008))
            .unwrap(),
        frames: ratio(
            i128::from(video.end().ticks - video.start().ticks) * 5,
            8008,
        ),
        selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(duration)).unwrap(),
        endpoints: EndpointPolicy::HoldAdjacent,
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: audio_start,
        frames: audio_frames,
        selection,
    };
    source.audio_offset = AudioSample(offset);
    source.edit_window =
        Some(SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(duration)).unwrap());
    source.link = LinkRelation::Linked;
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["a"] = serde_json::to_value(voice).unwrap();
    let mut asset = serde_json::from_value::<AssetRecord>(wire["assets"]["media"].clone()).unwrap();
    asset.video = Some(video);
    asset.still_image = false;
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        provider,
    )
}

// Keep comparisons exact without depending on floating-point Ord support.
trait ExactExt {
    fn min_exact(self, rhs: Self) -> Self;
    fn max_exact(self, rhs: Self) -> Self;
}
impl ExactExt for ExactRatio {
    fn min_exact(self, rhs: Self) -> Self {
        if self.compare(rhs).is_lt() { self } else { rhs }
    }
    fn max_exact(self, rhs: Self) -> Self {
        if self.compare(rhs).is_gt() { self } else { rhs }
    }
}

fn captured(document: &ProjectDocument, resume: Option<i64>) -> ProjectDocument {
    let state = capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: revision("trim-captured"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut bindings = state.bindings().clone();
    if let Some(samples) = resume {
        bindings.get_mut(&id("a")).unwrap().resume = Some(AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant: ratio(i128::from(samples) * 5, 8008),
                terms: vec![],
            },
        });
    }
    let state = AudioBindingState::new_with_gaps(
        state
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
        state.gap_bindings().clone(),
    )
    .unwrap();
    with_bindings(document, &state)
}

fn trimmed(
    document: &ProjectDocument,
    target: &str,
    edge: SourceTrimEdge,
    delta: i64,
    expected_prefix: i64,
) -> (ProjectDocument, EditTransaction) {
    let immutable = document.clone();
    let resolution = document
        .source_trim(
            &id("root"),
            &id(target),
            edge,
            delta,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(
        resolution.applied_delta_frames, delta,
        "fixture must not clamp"
    );
    assert_eq!(resolution.physical_prefix, frames(expected_prefix));
    let allocation = revision("trim-result");
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::TrimSource {
                parent: id("root"),
                node: id(target),
                edge,
                delta_frames: delta,
                mode: SourceTrimMode::Ripple,
                wrapper: resolution.needs_wrapper.then(|| id("trim-window")),
                timing: AudioTimingId {
                    allocation,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    assert_eq!(document, &immutable);
    let after = transaction.forward.apply(document).unwrap();
    let NodeKind::Source { source } = &after.nodes()[&id("a")].kind else {
        unreachable!()
    };
    assert_eq!(source, &resolution.after);
    assert_eq!(source.audio_offset, resolution.before.audio_offset);
    assert_eq!(source.audio, resolution.before.audio);
    for (id, layout) in document.audio_bindings().timings() {
        assert_eq!(after.audio_bindings().timings().get(id), Some(layout));
    }
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    (after, transaction)
}

#[track_caller]
fn same_pcm(actual: &[[f32; 2]], expected: &[[f32; 2]], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}");
    if let Some((at, (actual, expected))) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!("{label}: first mismatch {at}: actual {actual:?}, expected {expected:?}");
    }
}

#[test]
fn in_extension_exposes_one_and_three_frame_prefixes_on_the_retained_sample_grid() {
    // Full measured audio begins 6000 mix samples before physical zero. The
    // independent +7 offset makes selected Original support [5993,7594.6).
    let (original, mut provider) = linked(1, ratio(-30_000, 8008), 7, span(0, 8197));
    for (label, before, resume) in [
        ("plain", original.clone(), 0),
        ("captured", captured(&original, None), 0),
        ("resumed", captured(&original, Some(64)), 64),
    ] {
        let q = ratio(29_967 + i128::from(resume) * 5, 5);
        let baseline = source_oracle(&provider, 5993..7595, q, 256);
        same_pcm(
            &pcm(&before, &mut provider, 1602..1858, 193, false),
            &baseline,
            label,
        );
        for (prefix, join, distance, selected_start, total) in
            [(1, 3203, 1601, 4392, 4805), (3, 6406, 4804, 1189, 8008)]
        {
            let (after, tx) = trimmed(&before, "a", SourceTrimEdge::In, -prefix, prefix);
            assert_eq!(
                RenderPlan::compile(&after)
                    .unwrap()
                    .audio_duration()
                    .unwrap(),
                AudioSample(total)
            );
            let first_phase = q.checked_sub(ExactRatio::integer(distance)).unwrap();
            let prefix_pcm = source_oracle(&provider, selected_start..7595, first_phase, 256);
            let body_pcm = source_oracle(&provider, selected_start..7595, q, 256);
            assert_ne!(
                prefix_pcm,
                source_oracle(
                    &provider,
                    selected_start..7595,
                    first_phase.checked_add(ExactRatio::ONE).unwrap(),
                    256
                )
            );
            for chunk in [193, 239] {
                same_pcm(
                    &pcm(&after, &mut provider, 1602..1858, chunk, false),
                    &prefix_pcm,
                    &format!("{label}, prefix {prefix}"),
                );
                same_pcm(
                    &pcm(&after, &mut provider, join..join + 256, chunk, false),
                    &body_pcm,
                    "retained body entry",
                );
            }
            let restored = tx.inverse.apply(&after).unwrap();
            same_pcm(
                &pcm(&restored, &mut provider, 1602..1858, 251, false),
                &baseline,
                "inverse PCM",
            );
        }
    }
}

#[test]
fn in_extension_keeps_symbolic_resume_and_chronological_entries_before_physical_rebase() {
    let (base, mut provider) = linked(6, ExactRatio::ZERO, 7, span(-32032, 128128));
    let layout_at = |lead: i64| {
        let mut wire = serde_json::to_value(&base).unwrap();
        wire["nodes"]["lead"] = serde_json::to_value(hold(lead)).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let mut old_wire = serde_json::to_value(&base).unwrap();
    old_wire["nodes"].as_object_mut().unwrap().remove("lead");
    old_wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("a")])).unwrap();
    let old = ProjectDocument::from_json(&old_wire.to_string()).unwrap();
    let first = layout_at(1);
    let second = layout_at(2);
    let mut wire = serde_json::to_value(layout_at(4)).unwrap();
    wire["nodes"]["window"] = serde_json::to_value(partition("a", 2, 6)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("window")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let state = AudioBindingState::new(
        [&old, &first, &second]
            .into_iter()
            .enumerate()
            .map(|(ordinal, document)| AudioTimingRecord {
                id: historical_placement(u32::try_from(ordinal).unwrap())
                    .reference
                    .timing,
                layout: FrozenAudioLayout::capture(document).unwrap(),
            })
            .collect(),
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: historical_placement(0),
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ZERO,
                    phase: AudioLocalPhase {
                        constant: ratio(64 * 5, 8008),
                        terms: vec![AudioPhaseTerm {
                            placement: historical_placement(0),
                            from_local: ExactRatio::ZERO,
                            to_local: ExactRatio::ONE,
                        }],
                    },
                }),
                reanchors: [(1, 2, 7), (2, 4, 8)]
                    .into_iter()
                    .map(|(ordinal, start, end)| AudioReanchorStep {
                        placement: historical_placement(ordinal),
                        window: Some(
                            ExactFrameRange::new(
                                ExactRatio::integer(start),
                                ExactRatio::integer(end),
                            )
                            .unwrap(),
                        ),
                    })
                    .collect(),
            },
        )]),
    )
    .unwrap();
    let before = with_bindings(&current, &state);
    // 64+1602+1601+1601 = owner sample 4868; subtract offset 7 once.
    let body = source_oracle(&provider, 0..8197, ExactRatio::integer(4861), 256);
    same_pcm(
        &pcm(&before, &mut provider, 6406..6662, 193, false),
        &body,
        "chronological baseline",
    );
    let (after, tx) = trimmed(&before, "window", SourceTrimEdge::In, -3, 1);
    // Old body at frame 4 moves to frame 7. B(7)-B(4)=4805, so
    // the new prefix starts at 4861-4805=56, not 55 or a recaptured origin.
    let prefix = source_oracle(&provider, 0..8197, ExactRatio::integer(56), 256);
    assert_ne!(
        prefix,
        source_oracle(&provider, 0..8197, ExactRatio::integer(57), 256)
    );
    for chunk in [193, 239] {
        same_pcm(
            &pcm(&after, &mut provider, 6406..6662, chunk, false),
            &prefix,
            "chronological audible prefix",
        );
        same_pcm(
            &pcm(&after, &mut provider, 11211..11467, chunk, false),
            &body,
            "chronological body join",
        );
    }
    let restored = tx.inverse.apply(&after).unwrap();
    same_pcm(
        &pcm(&restored, &mut provider, 6406..6662, 251, false),
        &body,
        "chronological inverse",
    );
}

#[test]
fn in_extension_activates_audio_before_the_window_and_keeps_absence_silent() {
    // Audio ends half a frame before W before its +7 mix-sample offset.
    // The new one-frame prefix selects Original [7389.2,8197), whose
    // integer filter support is [7390,8197). At B(1) its phase is 7390.2.
    let (original, mut provider) = linked(1, ratio(-44_989, 8008), 7, span(0, 20_000));
    let mut absent = serde_json::to_value(&original).unwrap();
    // Serialize the typed node because enum wire shape is deliberately closed.
    let mut absent_node = original.nodes()[&id("a")].clone();
    let NodeKind::Source { source } = &mut absent_node.kind else {
        unreachable!()
    };
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    source.audio_offset = AudioSample(0);
    source.link = LinkRelation::Independent;
    absent["nodes"]["a"] = serde_json::to_value(absent_node).unwrap();
    let absent = ProjectDocument::from_json(&absent.to_string()).unwrap();
    for (label, before, audible) in [
        ("plain dormant", original.clone(), true),
        ("bound dormant", captured(&original, None), true),
        ("absent", absent, false),
    ] {
        provider.calls = 0;
        let baseline = pcm(&before, &mut provider, 0..3203, 193, false);
        assert!(
            baseline
                .iter()
                .flatten()
                .all(|sample| sample.to_bits() == 0),
            "{label}"
        );
        assert_eq!(provider.calls, 0, "{label} requested dormant media");
        let (after, tx) = trimmed(&before, "a", SourceTrimEdge::In, -1, 1);
        let expected = if audible {
            source_oracle(&provider, 7390..8197, ratio(36_951, 5), 256)
        } else {
            vec![[0.; 2]; 256]
        };
        if audible {
            assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
        }
        provider.calls = 0;
        for chunk in [193, 239] {
            same_pcm(
                &pcm(&after, &mut provider, 1602..1858, chunk, false),
                &expected,
                label,
            );
            assert!(
                pcm(&after, &mut provider, 3203..3459, chunk, false)
                    .iter()
                    .flatten()
                    .all(|sample| sample.to_bits() == 0)
            );
        }
        if !audible {
            assert_eq!(provider.calls, 0);
        }
        let restored = tx.inverse.apply(&after).unwrap();
        provider.calls = 0;
        same_pcm(
            &pcm(&restored, &mut provider, 0..3203, 251, false),
            &baseline,
            "dormant inverse",
        );
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn trim_preserves_filter_context_and_authors_only_the_new_entry_fade() {
    let (original, mut provider) = linked(4, ExactRatio::ZERO, 0, span(0, 8197));
    let mut wire = serde_json::to_value(&original).unwrap();
    let mut voice = original.nodes()[&id("a")].clone();
    voice.audio_edges = Default::default();
    wire["nodes"]["a"] = serde_json::to_value(voice).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let (after, tx) = trimmed(&before, "a", SourceTrimEdge::In, 1, 0);
    let old_source = &before.nodes()[&id("a")];
    assert_eq!(
        after.nodes()[&id("a")].kind,
        old_source.kind,
        "Partition-only crop keeps W and mapping variants"
    );
    assert_eq!(after.nodes()[&id("a")].audio_edges, old_source.audio_edges);
    let expected = source_oracle(&provider, 0..6407, ratio(8007, 5), 256);
    same_pcm(
        &pcm(&after, &mut provider, 1602..1858, 193, false),
        &expected,
        "cropped raw entry",
    );
    let faded: Vec<_> = expected
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            let gain = ((at as f64 + 0.5) / 96.0).min(1.0) as f32;
            [sample[0] * gain, sample[1] * gain]
        })
        .collect();
    assert_ne!(faded, expected);
    for chunk in [193, 239] {
        same_pcm(
            &pcm(&after, &mut provider, 1602..1858, chunk, true),
            &faded,
            "authored entry fade keeps independent raw sample phase",
        );
    }
    same_pcm(
        &pcm(&after, &mut provider, 1649..1702, 53, true),
        &faded[47..100],
        "mid-ramp query does not restart the envelope",
    );
    // The new allocation has 4804 samples, one fewer than old [2,5).
    // Its last 256 samples correspond to old [7751,8007), not [7752,8008).
    let old_tail = pcm(&before, &mut provider, 7751..8007, 193, true);
    same_pcm(
        &pcm(&after, &mut provider, 6150..6406, 239, true),
        &old_tail,
        "retained Source tail envelope",
    );
    let restored = tx.inverse.apply(&after).unwrap();
    same_pcm(
        &pcm(&restored, &mut provider, 7751..8007, 251, true),
        &old_tail,
        "edge inverse",
    );
}

#[test]
fn trim_fades_both_new_join_sides_but_keeps_an_untouched_split_join_continuous() {
    let (original, mut provider) = linked(4, ExactRatio::ZERO, 0, span(0, 8197));
    let before = captured(&original, None);
    let split = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("split-for-trim-edges"),
            command: Command::Split {
                node: id("a"),
                at: frames(2),
                identities: SplitIdentities {
                    nodes: vec![id("left"), id("right"), id("copy")],
                },
            },
        },
    )
    .unwrap()
    .forward
    .apply(&before)
    .unwrap();
    let raw_join = pcm(&split, &mut provider, 4709..4901, 193, false);
    same_pcm(
        &pcm(&split, &mut provider, 4709..4901, 239, true),
        &raw_join,
        "transparent Split has no creative seam",
    );

    let (cut, tx) = trimmed(&split, "left", SourceTrimEdge::Out, -1, 0);
    // Join moves from frame3/B=4805 to frame2/B=3203. The left
    // tail starts at 3107-1601.6=1505.4; the right retained entry
    // remains at old 4805-1601.6=3203.4. Offset is applied once.
    let outgoing = source_oracle(&provider, 0..6407, ratio(7527, 5), 96);
    let incoming = source_oracle(&provider, 0..6407, ratio(16017, 5), 96);
    for (range, raw, start_edge) in [(3107..3203, outgoing, false), (3203..3299, incoming, true)] {
        let faded: Vec<_> = raw
            .iter()
            .enumerate()
            .map(|(at, sample)| {
                let distance = if start_edge { at } else { 95 - at };
                let gain = ((distance as f64 + 0.5) / 96.0) as f32;
                [sample[0] * gain, sample[1] * gain]
            })
            .collect();
        for chunk in [31, 73] {
            same_pcm(
                &pcm(&cut, &mut provider, range.clone(), chunk, false),
                &raw,
                "join raw phase/filter support",
            );
            same_pcm(
                &pcm(&cut, &mut provider, range.clone(), chunk, true),
                &faded,
                "both incident sides receive one default ramp",
            );
        }
    }
    let restored = tx.inverse.apply(&cut).unwrap();
    same_pcm(
        &pcm(&restored, &mut provider, 4709..4901, 97, true),
        &raw_join,
        "inverse restores a continuous Split seam",
    );

    let (other, _) = trimmed(&split, "left", SourceTrimEdge::In, 1, 0);
    let raw_join = pcm(&other, &mut provider, 3107..3299, 193, false);
    same_pcm(
        &pcm(&other, &mut provider, 3107..3299, 239, true),
        &raw_join,
        "editing In must not mark the untouched Out Split seam",
    );
}

#[test]
fn slip_fades_a_previously_continuous_split_seam_without_changing_raw_clocks() {
    let (original, mut provider) = linked(4, ExactRatio::ZERO, 0, span(0, 8197));
    let before = captured(&original, None);
    let split = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("split-for-slip-edges"),
            command: Command::Split {
                node: id("a"),
                at: frames(2),
                identities: SplitIdentities {
                    nodes: vec![id("left"), id("right"), id("copy")],
                },
            },
        },
    )
    .unwrap()
    .forward
    .apply(&before)
    .unwrap();
    let old_join = pcm(&split, &mut provider, 4709..4901, 193, false);
    same_pcm(
        &pcm(&split, &mut provider, 4709..4901, 239, true),
        &old_join,
        "pre-Slip transparent seam",
    );
    assert_eq!(
        split
            .source_slip(&id("root"), &id("left"), 1)
            .unwrap()
            .applied_delta_frames,
        1
    );
    let transaction = apply(
        &split,
        &CommandRequest {
            project_id: split.project_id().clone(),
            expected_revision: split.revision_id().clone(),
            new_revision: revision("slip-split-edges"),
            command: Command::SlipSource {
                parent: id("root"),
                node: id("left"),
                delta_frames: 1,
            },
        },
    )
    .unwrap();
    let slipped = transaction.forward.apply(&split).unwrap();
    assert_eq!(slipped.audio_bindings(), split.audio_bindings());
    // Output seam remains frame3/B=4805. Left advances by exactly1601.6
    // Original samples: 4709-1601.6+1601.6=4709. Its selected filter
    // operand moves to ceil([1601.6,8008))=[1602,8008); the right full
    // owner and phase4805-1601.6=3203.4 remain unchanged.
    let outgoing = source_oracle(&provider, 1602..8008, ExactRatio::integer(4709), 96);
    let incoming = source_oracle(&provider, 0..6407, ratio(16017, 5), 96);
    for (range, raw, start_edge) in [(4709..4805, outgoing, false), (4805..4901, incoming, true)] {
        let faded: Vec<_> = raw
            .iter()
            .enumerate()
            .map(|(at, sample)| {
                let distance = if start_edge { at } else { 95 - at };
                let gain = ((distance as f64 + 0.5) / 96.0) as f32;
                [sample[0] * gain, sample[1] * gain]
            })
            .collect();
        assert_ne!(faded, raw);
        for chunk in [31, 73] {
            same_pcm(
                &pcm(&slipped, &mut provider, range.clone(), chunk, false),
                &raw,
                "Slip raw seam phase and support",
            );
            same_pcm(
                &pcm(&slipped, &mut provider, range.clone(), chunk, true),
                &faded,
                "Slip authors both incident edge ramps once",
            );
        }
    }
    let restored = transaction.inverse.apply(&slipped).unwrap();
    assert_eq!(restored, split);
    same_pcm(
        &pcm(&restored, &mut provider, 4709..4901, 97, true),
        &old_join,
        "Slip inverse restores continuous Split seam",
    );
}

#[path = "trim/roll.rs"]
mod roll;
