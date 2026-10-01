//! Decoded PCM witnesses for Slip. Picture is a declared synthetic common clock;
//! this test does not claim a measured video receipt or native visual admission.

use super::*;

#[derive(Clone, Copy)]
enum Fixture {
    Interior,
    Dormant,
    Chronological,
}

fn span(start: i64, end: i64, hz: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, hz).unwrap();
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

fn linked_fixture(kind: Fixture) -> (ProjectDocument, Provider) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let duration = if matches!(kind, Fixture::Chronological) {
        6
    } else {
        1
    };
    let voice = source(rate, 0, 8197, duration);
    let (original, provider) = if matches!(kind, Fixture::Chronological) {
        document(rate, &["a"], vec![("a", voice)])
    } else {
        document(rate, &["lead", "a"], vec![("lead", hold(1)), ("a", voice)])
    };
    let mut voice = original.nodes()[&id("a")].clone();
    for edge in [
        &mut voice.audio_edges.node_start,
        &mut voice.audio_edges.node_end,
        &mut voice.audio_edges.source_placement_start,
        &mut voice.audio_edges.source_placement_end,
    ] {
        *edge = AudioEdgePolicy::Hard;
    }
    let NodeKind::Source { source } = &mut voice.kind else {
        unreachable!()
    };
    let audio_frames = ratio(40985, 8008); // 8197 original samples at 30000/1001 fps.
    let offset = ratio(35, 8008); // Seven independent mix samples, retained once.
    let window = SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(duration)).unwrap();
    let (video, video_start, video_frames, audio_start, selection) = match kind {
        Fixture::Interior => (
            span(0, 8197, 48_000),
            ratio(-2500, 1001),
            audio_frames,
            ratio(-2500, 1001),
            ExactFrameRange::new(
                ExactRatio::ZERO.checked_sub(offset).unwrap(),
                ExactRatio::ONE.checked_sub(offset).unwrap(),
            )
            .unwrap(),
        ),
        Fixture::Dormant => (
            // x(t)=rate*t+2 for both streams. Video begins four frames before
            // audio, so W=[0,1) has real picture while the retained audio sleeps.
            span(-32032, 40985, 240_000),
            ratio(-2, 1),
            ratio(73017, 8008),
            ratio(2, 1),
            ExactFrameRange {
                start: ratio(2, 1),
                end: ratio(2, 1),
            },
        ),
        Fixture::Chronological => (
            span(-16016, 64064, 240_000),
            ratio(-2, 1),
            ratio(10, 1),
            ExactRatio::ZERO,
            ExactFrameRange::new(ExactRatio::ZERO, audio_frames).unwrap(),
        ),
    };
    source.video = SourceVideo::Stream {
        asset: AssetId::new("media").unwrap(),
        span: video,
    };
    source.video_mapping = SourceVideoMapping::SelectedPlacement {
        start: video_start,
        frames: video_frames,
        selection: ExactFrameRange::new(window.start(), window.end()).unwrap(),
        endpoints: EndpointPolicy::HoldAdjacent,
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: audio_start,
        frames: audio_frames,
        selection,
    };
    source.audio_offset = AudioSample(7);
    source.edit_window = Some(window);
    source.link = LinkRelation::Linked;
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["nodes"]["a"] = serde_json::to_value(voice).unwrap();
    let mut asset = original.assets()[&AssetId::new("media").unwrap()].clone();
    asset.video = Some(video);
    asset.still_image = false;
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        provider,
    )
}

fn capture(document: &ProjectDocument) -> ProjectDocument {
    let state = capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: revision("slip-retained"),
            ordinal: 0,
        },
    )
    .unwrap();
    assert!(state.bindings().contains_key(&id("a")));
    with_bindings(document, &state)
}

// Complete the existing physical-prefix fixture for linked video and editorial
// metadata. This is test setup, not a second Slip implementation or Trim command.
fn rebased(document: &ProjectDocument) -> ProjectDocument {
    let mut patch = prefix_patch(document, "root", 1, document.audio_bindings());
    let before = &document.nodes()[&id("a")];
    let NodeKind::Source { source: old } = &before.kind else {
        unreachable!()
    };
    let node = patch
        .nodes
        .get_mut(&id("a"))
        .unwrap()
        .after
        .as_mut()
        .unwrap();
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    let selection = old.video_mapping.selection_frames(old.duration).unwrap();
    source.video_mapping = SourceVideoMapping::SelectedPlacement {
        start: old
            .video_mapping
            .start_frames()
            .checked_add(ExactRatio::ONE)
            .unwrap(),
        frames: old.video_mapping.duration_frames(old.duration).unwrap(),
        selection: ExactFrameRange::new(
            selection.start.checked_add(ExactRatio::ONE).unwrap(),
            selection.end.checked_add(ExactRatio::ONE).unwrap(),
        )
        .unwrap(),
        endpoints: EndpointPolicy::HoldAdjacent,
    };
    source.edit_window = Some(
        old.edit_window
            .unwrap()
            .prepend_owner_frames(frames(1))
            .unwrap(),
    );
    node.audio_treatments = before
        .audio_treatments
        .with_owner_prefix(frames(1))
        .unwrap();
    patch.apply(document).unwrap()
}

fn slipped(
    document: &ProjectDocument,
    target: &str,
    delta: i64,
) -> (ProjectDocument, EditTransaction) {
    let resolution = document
        .source_slip(&id("root"), &id(target), delta)
        .unwrap();
    assert_eq!(
        resolution.applied_delta_frames, delta,
        "oracle requires an unclamped Slip"
    );
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(if delta > 0 {
                "slip-later"
            } else {
                "slip-earlier"
            }),
            command: Command::SlipSource {
                parent: id("root"),
                node: id(target),
                delta_frames: delta,
            },
        },
    )
    .unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(after.duration().unwrap(), document.duration().unwrap());
    assert_eq!(after.audio_bindings(), document.audio_bindings());
    assert_eq!(after.sounds(), document.sounds());
    assert_eq!(after.sound_routes(), document.sound_routes());
    assert_eq!(after.sound_allowances(), document.sound_allowances());
    for owner in ["a", "root"] {
        assert_eq!(
            after.nodes()[&id(owner)].audio_treatments,
            document.nodes()[&id(owner)].audio_treatments
        );
    }
    let NodeKind::Source { source } = &after.nodes()[&id("a")].kind else {
        unreachable!()
    };
    assert_eq!(source.audio_offset, AudioSample(7));
    assert_eq!(source.audio.as_ref().unwrap().span, span(0, 8197, 48_000));
    assert_eq!(source.edit_window, resolution.before.edit_window);
    (after, transaction)
}

fn phase(resume: i64, delta: i64) -> ExactRatio {
    // Exact first lattice phase 2/5, 4000 samples of earlier full context,
    // minus the independent seven-sample offset. One project frame is 8008/5
    // original samples. Neither RenderPlan nor mapping getters feed this oracle.
    ratio(19967 + 5 * i128::from(resume) + 8008 * i128::from(delta), 5)
}

fn selected_source_support(delta: i64) -> std::ops::Range<i64> {
    // W=[0,1) selects Original [3993,5594.6) before Slip. Translate both
    // endpoints by delta*1601.6, then exclude integer filter taps outside
    // that half-open interval. A retained resume changes the read phase,
    // not this authored crop. No plan or mapping result feeds these bounds.
    let left = 19965 + 8008 * i128::from(delta);
    i64::try_from(ratio(left, 5).ceil().unwrap()).unwrap()
        ..i64::try_from(ratio(left + 8008, 5).ceil().unwrap()).unwrap()
}

fn assert_pcm_eq(actual: &[[f32; 2]], expected: &[[f32; 2]], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}: frame count");
    let mismatches: Vec<_> = actual
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .map(|(index, _)| index)
        .collect();
    if let Some(&first) = mismatches.first() {
        panic!(
            "{label}: {} exact PCM mismatches, first {first}, last {}; actual {:?}, expected {:?}",
            mismatches.len(),
            mismatches.last().unwrap(),
            actual[first],
            expected[first]
        );
    }
}

fn expected_bus(provider: &Provider, resume: i64, delta: i64) -> Vec<[f32; 2]> {
    let voice = source_oracle(
        provider,
        selected_source_support(delta),
        phase(resume, delta),
        256,
    );
    let sound = source_oracle(provider, 7000..7256, ExactRatio::integer(7000), 256);
    let q = i128::from(GAIN_NUMERIC_SCALE);
    (0..256)
        .map(|at| {
            // Owner motion stays fixed while media phase changes. Rebased fixtures
            // shift both old owner keys and their physical positions by one frame.
            let position = 2 + 5 * (i128::from(resume) + at as i128);
            let ramp = if position < 1280 {
                -6000.0
                    + 12_000.0 * ratio(position * q, 1280).round_even().unwrap() as f64
                        / GAIN_NUMERIC_SCALE as f64
            } else {
                0.0
            };
            let root = -3000.0
                - if (128..192).contains(&at) {
                    6000.0
                } else {
                    0.0
                };
            let voice_amplitude = if (517..597).contains(&position) {
                0.0
            } else {
                10_f64.powf((3000.0 + ramp + root) / 20_000.0)
            };
            let sound_amplitude = 10_f64.powf((-6000.0 + root) / 20_000.0);
            std::array::from_fn(|channel| {
                (f64::from(voice[at][channel]) * voice_amplitude
                    + f64::from(sound[at][channel]) * sound_amplitude) as f32
            })
        })
        .collect()
}

#[test]
fn slip_moves_decoded_media_phase_and_keeps_retained_owner_gain_mute_and_root_sound() {
    let (original, mut provider) = linked_fixture(Fixture::Interior);
    let captured = capture(&original);
    let bound = bound_case(&original, 0);
    let resumed = bound_case(&original, 64);
    for (label, raw, start, resume, prefix) in [
        ("unbound", original, 1602, 0, 0),
        ("captured", captured.clone(), 1602, 0, 0),
        ("moved-bound", bound, 3203, 0, 0),
        ("resumed", resumed.clone(), 3203, 64, 0),
        ("captured-rebased", captured, 1602, 0, 1),
        ("resumed-rebased", resumed, 3203, 64, 1),
    ] {
        let mut wire = serde_json::to_value(with_controls(&raw, start)).unwrap();
        // B(2)=3203 lies just before exact frame 2. A sound starting there
        // also meets the leading silent Hold's end gate, whose default edge
        // would add a 96-sample fade despite the sound's own Hard start.
        // Make this fixture's join explicitly Hard, including the new lead
        // made by bound_case, so the independent bus oracle has no gate fade.
        let mut lead = raw.nodes()[&id("lead")].clone();
        lead.audio_edges.node_end = AudioEdgePolicy::Hard;
        wire["nodes"]["lead"] = serde_json::to_value(lead).unwrap();
        let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let before = if prefix == 0 {
            before
        } else {
            rebased(&before)
        };
        let target = if prefix == 0 { "a" } else { "physical-window" };
        let baseline = expected_bus(&provider, resume, 0);
        assert_pcm_eq(
            &authored(&before, &mut provider, start, &[256]),
            &baseline,
            &format!("{label} baseline"),
        );
        assert_ne!(
            source_oracle(&provider, selected_source_support(0), phase(resume, 0), 256),
            source_oracle(
                &provider,
                selected_source_support(0),
                phase(resume, 0)
                    .checked_add(ExactRatio::integer(7))
                    .unwrap(),
                256
            ),
            "ignoring the independent offset must change PCM"
        );
        for delta in [-1, 1] {
            let (after, transaction) = slipped(&before, target, delta);
            let expected = expected_bus(&provider, resume, delta);
            assert_ne!(expected, baseline, "{label}: Slip must move media");
            for sample in [0, 17, 104, 128, 255] {
                let local = ratio(2 + 5 * i128::from(resume + sample), 8008)
                    .checked_add(ExactRatio::integer(prefix))
                    .unwrap();
                assert_eq!(
                    owner_at(&before, start + sample),
                    local,
                    "{label} owner before"
                );
                assert_eq!(
                    owner_at(&after, start + sample),
                    local,
                    "{label} owner after"
                );
            }
            for pieces in [&[73, 55, 128][..], &[17, 127, 112][..]] {
                assert_pcm_eq(
                    &authored(&after, &mut provider, start, pieces),
                    &expected,
                    &format!("{label}, delta {delta}, cold pieces {pieces:?}"),
                );
            }
            let first_muted = usize::try_from(103 - resume).unwrap();
            let first_unmuted = usize::try_from(119 - resume).unwrap();
            let sound = source_oracle(&provider, 7000..7256, ExactRatio::integer(7000), 256);
            let sound_only = |at: usize| {
                sound[at].map(|v| (f64::from(v) * 10_f64.powf(-9000.0 / 20_000.0)) as f32)
            };
            assert_eq!(expected[first_muted], sound_only(first_muted));
            assert_eq!(expected[first_unmuted - 1], sound_only(first_unmuted - 1));
            assert_ne!(expected[first_muted - 1], sound_only(first_muted - 1));
            assert_ne!(expected[first_unmuted], sound_only(first_unmuted));
            let restored = transaction.inverse.apply(&after).unwrap();
            assert_eq!(restored, before);
            assert_pcm_eq(
                &authored(&restored, &mut provider, start, &[101, 155]),
                &baseline,
                &format!("{label}, delta {delta}, restored"),
            );
        }
    }
}

#[test]
fn slip_activates_dormant_measured_audio_without_replacing_its_captured_lattice() {
    let (original, mut provider) = linked_fixture(Fixture::Dormant);
    let captured = capture(&original);
    let rebased = rebased(&captured);
    for (label, before, target) in [
        ("unbound", original, "a"),
        ("captured-empty-support", captured, "a"),
        ("rebased-empty-support", rebased, "physical-window"),
    ] {
        provider.calls = 0;
        assert_eq!(
            pcm(&before, &mut provider, 0..3203, 193, false),
            vec![[0.0; 2]; 3203]
        );
        assert_eq!(provider.calls, 0, "{label} must not read dormant audio");
        let (after, transaction) = slipped(&before, target, 2);
        let mut expected = vec![[0.0; 2]; 3203];
        // B(1)+7=1609; B(2)=3203. The first Original sample is 2/5,
        // not a new zero-phase lattice inferred from the formerly empty support.
        // W ends at Original 1601.6-7=1594.6, so its half-open filter
        // support contains integer samples 0..1595, not the full retained span.
        expected[1609..].copy_from_slice(&source_oracle(&provider, 0..1595, ratio(2, 5), 1594));
        for chunk in [193, 239] {
            assert_pcm_eq(
                &pcm(&after, &mut provider, 0..3203, chunk, false),
                &expected,
                &format!("{label}, chunk {chunk}"),
            );
        }
        assert!(provider.calls > 0);
        let restored = transaction.inverse.apply(&after).unwrap();
        assert_eq!(restored, before);
        provider.calls = 0;
        assert_eq!(
            pcm(&restored, &mut provider, 0..3203, 251, false),
            vec![[0.0; 2]; 3203]
        );
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn slip_keeps_symbolic_resume_and_chronological_reanchors_on_the_same_sample_clock() {
    let (old, mut provider) = linked_fixture(Fixture::Chronological);
    let layout_at = |lead| {
        let mut wire = serde_json::to_value(&old).unwrap();
        wire["nodes"]["lead"] = serde_json::to_value(hold(lead)).unwrap();
        wire["nodes"]["root"] =
            serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("a")])).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let first = layout_at(1);
    let second = layout_at(2);
    let mut wire = serde_json::to_value(layout_at(4)).unwrap();
    wire["nodes"]["suffix"] = serde_json::to_value(partition("a", 2, 6)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("suffix")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bindings = AudioBindingState::new(
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
                        anchor: Default::default(),
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
    let before = with_bindings(&current, &bindings);
    // The expression contributes B(1)-B(0)=1602, then each chronological
    // reanchor contributes 1601, plus 64 resumed samples: owner sample 4868.
    // Subtract independent offset 7 once to obtain Original sample 4861.
    let baseline = source_oracle(&provider, 0..8197, ExactRatio::integer(4861), 256);
    assert_eq!(
        pcm(&before, &mut provider, 6406..6662, 193, false),
        baseline
    );
    assert_eq!(owner_at(&before, 6406), ratio(4868 * 5, 8008));
    for delta in [-1, 1] {
        let (after, transaction) = slipped(&before, "suffix", delta);
        let phase = ratio(4861 * 5 + i128::from(delta) * 8008, 5);
        let expected = source_oracle(&provider, 0..8197, phase, 256);
        assert_ne!(expected, baseline);
        assert_eq!(owner_at(&after, 6406), ratio(4868 * 5, 8008));
        for chunk in [193, 239] {
            assert_eq!(
                pcm(&after, &mut provider, 6406..6662, chunk, false),
                expected
            );
        }
        let restored = transaction.inverse.apply(&after).unwrap();
        assert_eq!(restored, before);
        assert_eq!(
            pcm(&restored, &mut provider, 6406..6662, 251, false),
            baseline
        );
    }
}
