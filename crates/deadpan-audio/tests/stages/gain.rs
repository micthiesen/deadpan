//! Real decoded WAV oracles for the canonical authored bus. The expected gain
//! is applied to complete edge-faded PCM, never to a Preserve engine's input.
use super::*;
use deadpan_audio::{LimitedAudio, LimitedTile, LimiterContext};
use std::time::Instant;

const GAIN_TIMEOUT: Duration = Duration::from_secs(60);

fn treatment(
    trim: i32,
    mute: bool,
    envelopes: Vec<GainEnvelope>,
    ranges: Vec<GainRange>,
) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(trim).unwrap(), mute, envelopes, ranges).unwrap(),
    )
}

fn range(start: i64, end: i64) -> GainRange {
    GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap()
}

fn constant_envelope(start: i64, end: i64, db: i32) -> GainEnvelope {
    GainEnvelope::new(
        GainClock::OwnerOutput,
        range(start, end),
        GainDb::new(db).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(end),
                GainDb::new(db).unwrap(),
                GainCurve::Step,
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

fn changed(document: &ProjectDocument, recipes: &[(&str, AudioTreatments)]) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    for (owner, recipe) in recipes {
        wire["nodes"][owner]["audio_treatments"] = serde_json::to_value(recipe).unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn authored(
    renderer: &mut StageAudio,
    provider: &mut FixtureProvider,
    start: i64,
    count: u32,
) -> deadpan_audio::EdgeFadedBlock {
    renderer
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            count,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn authored_all(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    pieces: &[u32],
) -> Vec<[f32; 2]> {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = renderer.plan().audio_duration().unwrap().0 as usize;
    let mut samples = Vec::new();
    for count in pieces.iter().copied().cycle() {
        if samples.len() == total {
            break;
        }
        let count = count.min((total - samples.len()) as u32);
        samples.extend(authored(&mut renderer, provider, samples.len() as i64, count).samples);
    }
    samples
}

fn scaled(sample: [f32; 2], millidecibels: f64) -> [f32; 2] {
    let amplitude = 10.0_f64.powf(millidecibels / 20_000.0);
    sample.map(|value| (f64::from(value) * amplitude) as f32)
}

fn source_document(frames: i64) -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    document_with_asset(
        rate,
        &["source"],
        [("source", source(rate, frames, 0..frames))],
        BTreeMap::new(),
        audio(0, 8197).span,
    )
}

#[test]
fn overlapping_db_trim_and_exact_mute_follow_edges_without_changing_inspection() {
    let base = source_document(256);
    let treated = changed(
        &base,
        &[
            (
                "source",
                treatment(
                    6000,
                    false,
                    vec![
                        constant_envelope(10, 40, 3000),
                        constant_envelope(20, 50, -6000),
                    ],
                    vec![range(70, 80)],
                ),
            ),
            ("root", treatment(-3000, false, vec![], vec![])),
        ],
    );
    let mut provider = FixtureProvider::new();
    let neutral = authored_all(&base, &mut provider, &[256]);
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&treated).unwrap()));
    let bus = authored(&mut renderer, &mut provider, 0, 256);
    let expected: Vec<_> = neutral
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            if (70..80).contains(&at) {
                [0.0; 2]
            } else {
                let db = 3000 + if (10..40).contains(&at) { 3000 } else { 0 }
                    - if (20..50).contains(&at) { 6000 } else { 0 };
                scaled(*sample, f64::from(db))
            }
        })
        .collect();
    assert_eq!(bus.samples, expected);
    assert!(
        bus.suppressed.is_empty(),
        "gain mute grants no silence policy"
    );
    let inspection = renderer
        .prepare_edge_faded(
            &mut provider,
            AudioSample(0),
            256,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(inspection.samples, neutral);
    assert_eq!(
        authored_all(&treated, &mut provider, &[1, 7, 53, 2]),
        expected
    );
    assert_eq!(bus.stage, "authored_bus_pcm_before_mastering");
}

#[test]
fn explicit_unity_and_opposing_owner_gains_are_bit_identical() {
    let base = source_document(256);
    let mut provider = FixtureProvider::new();
    let expected = authored_all(&base, &mut provider, &[256]);
    for recipes in [
        vec![("source", treatment(0, false, vec![], vec![]))],
        vec![
            ("source", treatment(24000, false, vec![], vec![])),
            ("root", treatment(-24000, false, vec![], vec![])),
        ],
    ] {
        let actual = authored_all(&changed(&base, &recipes), &mut provider, &[53, 1, 119]);
        for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
}

#[test]
fn preserve_gain_uses_nominal_output_map_after_complete_processing_and_partitions() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let base = document_with_asset(
        rate,
        &["stretch"],
        [
            ("source", source(rate, 4096, 0..4096)),
            (
                "stretch",
                retime("source", 6144, 0..4096, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        range(0, 4096),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(4096),
                GainDb::new(6000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let treated = changed(
        &base,
        &[("source", treatment(0, false, vec![envelope], vec![]))],
    );
    let mut provider = FixtureProvider::new();
    let neutral = authored_all(&base, &mut provider, &[6144]);
    // The declared Q32 interpolation rounds nominal progress before the dB
    // interpolation. Derive it from the output sample index independently.
    let expected: Vec<_> = neutral
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            let progress = ratio(at as i128 * i128::from(GAIN_NUMERIC_SCALE), 6144)
                .round_even()
                .unwrap();
            scaled(
                *sample,
                -6000.0 + progress as f64 * 12000.0 / GAIN_NUMERIC_SCALE as f64,
            )
        })
        .collect();
    assert_eq!(authored_all(&treated, &mut provider, &[6144]), expected);
    let divided = split_command(&treated, &id("stretch"), 2049, "gain-partition");
    assert_eq!(
        authored_all(&divided, &mut provider, &[1, 197, 13, 256]),
        expected
    );
    let plan = Arc::new(RenderPlan::compile(&divided).unwrap());
    for (start, count) in [(4097, 201), (0, 17), (2039, 29), (6001, 143)] {
        let mut fresh = StageAudio::new(Arc::clone(&plan));
        assert_eq!(
            authored(&mut fresh, &mut provider, start, count).samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn repeat_child_restarts_and_repeat_gain_spans_plays_and_roomtone_gaps_once() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeated = BeatNode {
        label: "Repeated voice".into(),
        framing: None,
        audio_edges: Default::default(),
        audio_treatments: Default::default(),
        kind: NodeKind::Repeat {
            child: id("source"),
            iterations: IterationOrder::new(RevisionId::new("gain-plays").unwrap(), 2).unwrap(),
            gap: Some(HoldRecipe {
                duration: duration(128),
                video: HoldVideo::Background,
                audio: HoldAudio::RoomTone {
                    source: audio(512, 1024),
                },
                picture_context: None,
            }),
        },
    };
    let base = document_with_asset(
        rate,
        &["repeat"],
        [("source", source(rate, 256, 0..256)), ("repeat", repeated)],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let treated = changed(
        &base,
        &[
            (
                "source",
                treatment(0, false, vec![constant_envelope(0, 64, 6000)], vec![]),
            ),
            (
                "repeat",
                treatment(
                    -3000,
                    false,
                    vec![constant_envelope(200, 440, 3000)],
                    vec![],
                ),
            ),
        ],
    );
    let mut provider = FixtureProvider::new();
    let baseline = authored_all(&base, &mut provider, &[640]);
    let expected: Vec<_> = baseline
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            let child_gain = if at < 64 || (384..448).contains(&at) {
                6000
            } else {
                0
            };
            let repeat_gain = -3000 + if (200..440).contains(&at) { 3000 } else { 0 };
            scaled(*sample, f64::from(child_gain + repeat_gain))
        })
        .collect();
    assert_eq!(authored_all(&treated, &mut provider, &[640]), expected);
    assert_eq!(
        authored_all(&treated, &mut provider, &[7, 129, 11]),
        expected
    );
}

fn with_sound(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    let mut asset = document.assets()[&AssetId::new("media").unwrap()].clone();
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    asset.still_image = false;
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    let source = audio(0, 256);
    let event = SoundEvent {
        owner: id("root"),
        label: "Independent sound".into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            document.presentation_basis().frame_rate,
        )
        .unwrap(),
        source,
        offset: AudioSample(0),
        gain_millidecibels: 3000,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    wire["sounds"] =
        serde_json::to_value(BTreeMap::from([(SoundId::new("effect").unwrap(), event)])).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn source_mute_leaves_independent_sound_and_root_gain_composes_with_event_gain_once() {
    let base = source_document(256);
    let sounded = with_sound(&base);
    let treated = changed(
        &sounded,
        &[
            ("source", treatment(24000, true, vec![], vec![])),
            ("root", treatment(-6000, false, vec![], vec![])),
        ],
    );
    let mut provider = FixtureProvider::new();
    let actual = authored_all(&treated, &mut provider, &[256]);
    let expected: Vec<_> = (0..256)
        .map(|at| scaled(fixture_sample(at), -3000.0))
        .collect();
    assert_eq!(actual, expected);
    let root_muted = changed(&sounded, &[("root", treatment(0, true, vec![], vec![]))]);
    assert_eq!(
        authored_all(&root_muted, &mut provider, &[37, 219]),
        vec![[0.0; 2]; 256]
    );
    assert!(provider.calls > 0);
}

#[test]
fn limiter_halos_use_authored_gain_for_whole_and_cold_shuffled_reads() {
    let base = source_document(4096);
    let treated = changed(
        &base,
        &[(
            "source",
            treatment(
                12000,
                false,
                vec![constant_envelope(1000, 2500, -6000)],
                vec![range(2000, 2010)],
            ),
        )],
    );
    let mut provider = FixtureProvider::new();
    let bus = authored_all(&treated, &mut provider, &[4096]);
    let expected = LimitedTile::prepare(
        LimiterContext {
            project_samples: AudioSample(0)..AudioSample(4096),
            start: AudioSample(0),
            samples: bus,
        },
        AudioSample(0)..AudioSample(4096),
        Instant::now() + GAIN_TIMEOUT,
        &AtomicBool::new(false),
    )
    .unwrap();
    let plan = Arc::new(RenderPlan::compile(&treated).unwrap());
    let mut warm = LimitedAudio::new(Arc::clone(&plan));
    for (start, count) in [(0, 4096), (1997, 17), (3001, 9), (997, 19), (0, 1)] {
        for cold in [false, true] {
            let mut fresh = LimitedAudio::new(Arc::clone(&plan));
            let renderer = if cold { &mut fresh } else { &mut warm };
            let block = renderer
                .read(
                    &mut provider,
                    AudioSample(start),
                    count,
                    GAIN_TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.samples,
                expected.samples[start as usize..start as usize + count as usize]
            );
            assert_eq!(
                block.gain,
                expected.gain[start as usize..start as usize + count as usize]
            );
        }
    }
}

#[test]
fn fully_muted_cached_bus_still_readmits_and_rejects_revoked_original() {
    struct Revocable {
        provider: FixtureProvider,
        revoked: bool,
    }
    impl AudioSourceProvider for Revocable {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            if self.revoked {
                return Err(PreparationError::SourceUnavailable(
                    "revoked gain fixture".into(),
                ));
            }
            self.provider.source(project, revision, asset, cancelled)
        }
    }
    let document = changed(
        &source_document(4096),
        &[("source", treatment(-96000, true, vec![], vec![]))],
    );
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let mut renderer = LimitedAudio::new(plan);
    let mut provider = Revocable {
        provider: FixtureProvider::new(),
        revoked: false,
    };
    let first = renderer
        .read(
            &mut provider,
            AudioSample(10),
            100,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(first.samples, vec![[0.0; 2]; 100]);
    assert!(first.suppressed.is_empty());
    let calls = provider.provider.calls;
    renderer
        .read(
            &mut provider,
            AudioSample(10),
            100,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(provider.provider.calls > calls);
    provider.revoked = true;
    assert!(
        matches!(renderer.read(&mut provider, AudioSample(10), 100, GAIN_TIMEOUT, &AtomicBool::new(false)), Err(deadpan_audio::LimitedAudioError::Stage(StageAudioError::Preparation(PreparationError::SourceUnavailable(message)))) if message == "revoked gain fixture")
    );
}

#[test]
fn nonfinite_authored_output_rejects_after_admission_without_poisoning_inspection_or_budget() {
    let base = source_document(256);
    let boost = treatment(
        24000,
        false,
        (0..MAX_GAIN_ENVELOPES)
            .map(|_| constant_envelope(0, 256, 24000))
            .collect(),
        vec![],
    );
    let document = changed(&base, &[("source", boost.clone()), ("root", boost)]);
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let mut renderer = StageAudio::with_limits(
        plan,
        StageLimits {
            maximum_resident_frames: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    let before = renderer
        .prepare_edge_faded(
            &mut provider,
            AudioSample(0),
            256,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    for _ in 0..2 {
        let calls = provider.calls;
        assert!(matches!(
            renderer.prepare_authored_bus(
                &mut provider,
                AudioSample(0),
                256,
                GAIN_TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Preparation(
                PreparationError::InvalidSamples
            ))
        ));
        assert!(
            provider.calls > calls,
            "gain failure does not bypass admission"
        );
    }
    let after = renderer
        .prepare_edge_faded(
            &mut provider,
            AudioSample(0),
            256,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn gain_conversion_reuses_original_with_exact_three_frame_residency() {
    let document = changed(
        &source_document(256),
        &[("source", treatment(3000, false, vec![], vec![]))],
    );
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let mut provider = FixtureProvider::new();
    let expected = authored_all(&document, &mut provider, &[256]);
    for maximum_resident_frames in [767, 768] {
        let mut renderer = StageAudio::with_limits(
            Arc::clone(&plan),
            StageLimits {
                maximum_resident_frames,
                ..Default::default()
            },
        )
        .unwrap();
        let calls = provider.calls;
        let result = renderer.prepare_authored_bus(
            &mut provider,
            AudioSample(0),
            256,
            GAIN_TIMEOUT,
            &AtomicBool::new(false),
        );
        if maximum_resident_frames == 767 {
            assert!(matches!(
                result,
                Err(StageAudioError::Limit("resident PCM or stage cache"))
            ));
            assert_eq!(
                provider.calls, calls,
                "insufficient buffer space rejects before source work"
            );
        } else {
            assert_eq!(result.unwrap().samples, expected);
            assert!(provider.calls > calls);
            assert_eq!(
                authored(&mut renderer, &mut provider, 0, 256).samples,
                expected
            );
        }
    }
}

#[test]
fn exhausted_bound_original_keeps_independent_sound_and_current_root_gain() {
    let base = source_document(256);
    let mut wire = serde_json::to_value(&base).unwrap();
    let mut leaf = base.nodes()[&id("source")].clone();
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ExactRatio::integer(256),
        selection: ExactFrameRange::new(ExactRatio::integer(100), ExactRatio::integer(150))
            .unwrap(),
    };
    wire["nodes"]["source"] = serde_json::to_value(leaf).unwrap();
    let selected = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut wire = serde_json::to_value(&selected).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            &selected,
            AudioTimingId {
                allocation: RevisionId::new("gain-binding").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = FixtureProvider::new();
    let neutral = authored_all(&bound, &mut provider, &[256]);
    let treated = changed(
        &with_sound(&bound),
        &[
            ("source", treatment(3000, false, vec![], vec![])),
            ("root", treatment(-3000, false, vec![], vec![])),
        ],
    );
    let expected: Vec<_> = neutral
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            let sound = fixture_sample(at as i64);
            [0, 1].map(|channel| (f64::from(sample[channel]) + f64::from(sound[channel])) as f32)
        })
        .collect();
    assert_eq!(authored_all(&treated, &mut provider, &[256]), expected);
    assert_eq!(
        authored_all(&treated, &mut provider, &[99, 3, 47, 1, 106]),
        expected
    );
}

#[test]
fn odd_sample_insert_keeps_source_envelope_origin_and_current_sequence_keys() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let base = document_with_asset(
        rate,
        &["source"],
        [("source", source(rate, 2, 0..3203))],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let source_gain = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ratio(3, 2)).unwrap(),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ratio(3, 2),
                GainDb::new(6000).unwrap(),
                GainCurve::Smoothstep,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let treated = changed(
        &base,
        &[("source", treatment(3000, false, vec![source_gain], vec![]))],
    );
    let revision = RevisionId::new("gain-odd-insert").unwrap();
    let tx = apply(
        &treated,
        &CommandRequest {
            project_id: treated.project_id().clone(),
            expected_revision: treated.revision_id().clone(),
            new_revision: revision.clone(),
            command: Command::InsertTime {
                at: ProjectFrame(1),
                hold: HoldRecipe {
                    duration: duration(1),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
                id: id("gain-inserted-pause"),
                identities: SplitIdentities {
                    nodes: (0..8)
                        .map(|index| id(&format!("gain-odd-fragment-{index}")))
                        .collect(),
                },
                timing: AudioTimingId {
                    allocation: revision,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    let inserted = tx.forward.apply(&treated).unwrap();
    let current = changed(
        &inserted,
        &[(
            "root",
            treatment(0, false, vec![constant_envelope(2, 3, 6000)], vec![]),
        )],
    );
    let mut provider = FixtureProvider::new();
    let old = authored_all(&treated, &mut provider, &[3203]);
    let expected: Vec<_> = (0..4805)
        .map(|at| {
            if at < 1602 {
                old[at]
            } else if at < 3203 {
                [0.0; 2]
            } else {
                scaled(
                    old.get(at - 3203 + 1602).copied().unwrap_or([0.0; 2]),
                    if at == 3203 { 0.0 } else { 6000.0 },
                )
            }
        })
        .collect();
    let actual = authored_all(&current, &mut provider, &[4805]);
    // This reference has one extra f32 rounding between source and root gain.
    // The current Sequence key at frame 2 begins at sample 3204, whereas the
    // resumed allocation begins at round-even sample 3203.
    for at in 0..1602 {
        assert_eq!(actual[at], expected[at]);
    }
    assert!(actual[1602..3203].iter().all(|sample| *sample == [0.0; 2]));
    for at in 3203..4805 {
        for channel in 0..2 {
            assert!(
                (actual[at][channel] - expected[at][channel]).abs() <= f32::EPSILON * 8.0,
                "resumed gain at {at}"
            );
        }
    }
    assert_eq!(
        authored_all(&current, &mut provider, &[97, 256, 1, 131]),
        actual
    );
}
