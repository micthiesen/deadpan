//! Qualified decoded PCM and exact definition clocks feed the same reduction.
use super::*;
use deadpan_audio::{
    DefinitionWaveform, WaveformCompletion, WaveformControl, WaveformError, WaveformLimits,
    WaveformMemory, WaveformStopReason,
};

fn read_all(
    plan: &Arc<RenderPlan>,
    selector: AudioDefinitionSelector,
    provider: &mut impl AudioSourceProvider,
) -> Vec<[f32; 2]> {
    let definition = plan.audio_definition(selector).unwrap();
    let total = usize::try_from(definition.signal().sample_count().unwrap().0).unwrap();
    let mut audio = StageAudio::new(Arc::clone(plan));
    let mut result = Vec::new();
    for count in [17, 251, 1, 113].into_iter().cycle() {
        if result.len() == total {
            break;
        }
        let count = count.min(u32::try_from(total - result.len()).unwrap());
        result.extend(
            audio
                .read_definition(
                    provider,
                    &definition,
                    SignalSample(i64::try_from(result.len()).unwrap()),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    result
}

fn assert_extrema(waveform: &DefinitionWaveform, expected: &[[f32; 2]]) {
    for level in 0..waveform.level_count() {
        for (index, peak) in waveform.level(level).unwrap().iter().enumerate() {
            let range = waveform.bin_samples(level, index).unwrap();
            let slice = &expected
                [usize::try_from(range.start.0).unwrap()..usize::try_from(range.end.0).unwrap()];
            for channel in 0..2 {
                let minimum = slice
                    .iter()
                    .map(|sample| sample[channel])
                    .min_by(f32::total_cmp)
                    .unwrap();
                let maximum = slice
                    .iter()
                    .map(|sample| sample[channel])
                    .max_by(f32::total_cmp)
                    .unwrap();
                assert_eq!(peak.minimum()[channel].to_bits(), minimum.to_bits());
                assert_eq!(peak.maximum()[channel].to_bits(), maximum.to_bits());
            }
        }
    }
}

#[test]
fn waveform_matches_canonical_nested_preserve_tape_room_tone_and_suppression() {
    let (document, independent_outer, _) = nested_fixture();
    let memory = WaveformMemory::default();
    let mut provider = FixtureProvider::new();
    for frozen in [false, true] {
        let plan = compile(&document, frozen);
        for owner in [
            "root", "outer", "inner", "crop", "room", "a", "silent", "absent",
        ] {
            let selector = node(owner);
            let expected = read_all(&plan, selector.clone(), &mut provider);
            if owner == "outer" {
                assert_eq!(expected, independent_outer);
            }
            let definition = plan.audio_definition(selector.clone()).unwrap();
            let mut audio = StageAudio::new(Arc::clone(&plan));
            let result = audio
                .measure_definition(
                    &mut provider,
                    &definition,
                    WaveformControl {
                        limits: WaveformLimits::default(),
                        cancelled: &AtomicBool::new(false),
                        memory: &memory,
                    },
                    |_| {},
                )
                .unwrap();
            assert_eq!(result.completion, WaveformCompletion::Complete);
            assert_eq!(
                result.examined_samples,
                u64::try_from(expected.len()).unwrap()
            );
            assert_eq!(
                result.waveform.measured_end(),
                definition.signal().sample_count().unwrap()
            );
            assert_eq!(result.waveform.descriptor().definition, selector);
            assert_eq!(
                result.waveform.descriptor().owner_duration,
                definition.duration()
            );
            assert_eq!(
                result.waveform.descriptor().revision_id,
                *document.revision_id()
            );
            assert_extrema(&result.waveform, &expected);
        }
    }
    assert!(provider.context_calls > 0);
    assert_eq!(memory.resident_bytes(), 0);
}

#[test]
fn waveform_repeat_gap_and_selection_exhaustion_use_intrinsic_definition_clocks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = document(
        rate,
        &["repeat"],
        [
            ("source", source(rate, 513, 512..769)),
            (
                "repeat",
                BeatNode {
                    framing: None,
                    label: "Three plays with silence".into(),
                    audio_treatments: Default::default(),
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("source"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3)
                            .unwrap(),
                        gap: Some(HoldRecipe {
                            picture_context: None,
                            duration: frames(257),
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        }),
                        escalation: None,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&document, false);
    let mut provider = FixtureProvider::new();
    for selector in [
        node("repeat"),
        AudioDefinitionSelector::RepeatDefault {
            repeat: id("repeat"),
        },
        AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        },
    ] {
        let definition = plan.audio_definition(selector.clone()).unwrap();
        let expected = read_all(&plan, selector, &mut provider);
        let result = StageAudio::new(Arc::clone(&plan))
            .measure_definition(
                &mut provider,
                &definition,
                WaveformControl {
                    limits: WaveformLimits::default(),
                    cancelled: &AtomicBool::new(false),
                    memory: &WaveformMemory::default(),
                },
                |_| {},
            )
            .unwrap();
        assert_eq!(result.completion, WaveformCompletion::Complete);
        assert_extrema(&result.waveform, &expected);
        if result.waveform.descriptor().root == id("source") {
            assert_eq!(&expected[257..], &[[0.0; 2]; 256]);
        }
    }
}

#[test]
fn waveform_ntsc_preserve_uses_point_ceil_and_an_exact_terminal_owner_end() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = document(
        rate,
        &["prefix", "stage"],
        [
            ("prefix", hold(1, HoldAudio::Silence)),
            ("source", source(rate, 5, 0..8197)),
            ("stage", retime("source", 2, 1..4, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&document, false);
    let input = sample_reference(
        1602..6407,
        ratio(8008, 5),
        ExactRatio::ONE,
        4805,
        fixture_sample,
    );
    let expected = stretch(&input, 3204, 3, 2);
    let definition = plan.audio_definition(node("stage")).unwrap();
    let result = StageAudio::new(Arc::clone(&plan))
        .measure_definition(
            &mut FixtureProvider::new(),
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &AtomicBool::new(false),
                memory: &WaveformMemory::default(),
            },
            |_| {},
        )
        .unwrap();
    assert_extrema(&result.waveform, &expected);
    let last = result.waveform.level(0).unwrap().len() - 1;
    assert_eq!(
        result.waveform.bin_owner_frames(0, last).unwrap().end,
        ExactRatio::integer(2)
    );
    assert!(
        result
            .waveform
            .descriptor()
            .grid
            .at(SignalSample(3204))
            .unwrap()
            .compare_integer(2)
            .is_gt()
    );
    assert_eq!(
        result.waveform.bin_owner_frames(0, 0).unwrap().end,
        ratio(160, 1001)
    );
}

#[test]
fn waveform_one_request_cannot_renew_stage_work_at_block_boundaries() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = document(
        rate,
        &["first", "second"],
        [
            (
                "first",
                hold(
                    512,
                    HoldAudio::RoomTone {
                        source: audio(512..640),
                    },
                ),
            ),
            (
                "second",
                hold(
                    512,
                    HoldAudio::RoomTone {
                        source: audio(1024..1152),
                    },
                ),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&document, false);
    let definition = plan.audio_definition(node("root")).unwrap();
    let mut provider = FixtureProvider::new();
    let limits = StageLimits {
        maximum_prepared_stages: 1,
        ..StageLimits::default()
    };
    let mut separate = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
    for start in [0, 512] {
        separate
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(start),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
    }
    let mut shared = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
    let result = shared
        .measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &AtomicBool::new(false),
                memory: &WaveformMemory::default(),
            },
            |_| {},
        )
        .unwrap();
    assert!(
        matches!(&result.completion, WaveformCompletion::Partial(WaveformStopReason::Preparation(reason)) if reason.contains("prepared stages per read"))
    );
    assert_eq!(result.examined_samples, 512);
    assert_eq!(result.waveform.measured_end(), SignalSample(512));
    assert_extrema(
        &result.waveform,
        &read_all(&plan, node("root"), &mut provider),
    );
}

#[test]
fn waveform_output_cap_and_interruption_preserve_only_completed_prefix_bins() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = document(
        rate,
        &["source"],
        [("source", source(rate, 769, 0..769))],
        BTreeMap::new(),
    );
    let plan = compile(&document, false);
    let definition = plan.audio_definition(node("source")).unwrap();
    let memory = WaveformMemory::default();
    let mut provider = FixtureProvider::new();
    let result = StageAudio::new(Arc::clone(&plan))
        .measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::new(600, 4096, TIMEOUT).unwrap(),
                cancelled: &AtomicBool::new(false),
                memory: &memory,
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(
        result.completion,
        WaveformCompletion::Partial(WaveformStopReason::OutputLimit)
    );
    assert_eq!(result.examined_samples, 600);
    assert_eq!(result.waveform.measured_end(), SignalSample(512));
    let cancelled = AtomicBool::new(false);
    let mut updates = Vec::new();
    let interrupted = StageAudio::new(Arc::clone(&plan))
        .measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &cancelled,
                memory: &memory,
            },
            |data| {
                updates.push(data);
                cancelled.store(true, Ordering::Relaxed);
            },
        )
        .unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        interrupted.completion,
        WaveformCompletion::Partial(WaveformStopReason::Cancelled)
    );
    assert_eq!(interrupted.waveform.measured_end(), SignalSample(256));
    assert_eq!(updates[0].measured_end(), SignalSample(256));
    assert!(updates[0].level(1).unwrap().is_empty());
}

#[test]
fn waveform_later_source_failure_keeps_a_truthful_measured_prefix() {
    struct Failing {
        provider: FixtureProvider,
        remaining: usize,
    }
    impl AudioSourceProvider for Failing {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            if self.remaining == 0 {
                return Err(PreparationError::SourceUnavailable(
                    "revoked during measurement".into(),
                ));
            }
            self.remaining -= 1;
            self.provider.source(project, revision, asset, cancelled)
        }
    }
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = document(
        rate,
        &["source"],
        [("source", source(rate, 1024, 0..1024))],
        BTreeMap::new(),
    );
    let plan = compile(&document, false);
    let definition = plan.audio_definition(node("source")).unwrap();
    let result = StageAudio::new(Arc::clone(&plan))
        .measure_definition(
            &mut Failing {
                provider: FixtureProvider::new(),
                remaining: 2,
            },
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &AtomicBool::new(false),
                memory: &WaveformMemory::default(),
            },
            |_| {},
        )
        .unwrap();
    assert!(
        matches!(&result.completion, WaveformCompletion::Partial(WaveformStopReason::Preparation(reason)) if reason.contains("revoked"))
    );
    assert_eq!(result.examined_samples, 512);
    assert_eq!(result.waveform.measured_end(), SignalSample(512));
    assert_extrema(
        &result.waveform,
        &(0..512).map(fixture_sample).collect::<Vec<_>>(),
    );
}

#[test]
fn waveform_foreign_resource_cancel_deadline_and_cached_admission_failures_are_explicit() {
    let (document, _, _) = nested_fixture();
    let plan = compile(&document, false);
    let definition = plan.audio_definition(node("outer")).unwrap();
    let mut provider = FixtureProvider::new();
    let memory = WaveformMemory::default();
    let active = AtomicBool::new(false);
    let mut foreign = StageAudio::new(compile(&document, false));
    assert!(matches!(
        foreign.measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &active,
                memory: &memory
            },
            |_| {}
        ),
        Err(WaveformError::Stage(StageAudioError::ForeignDefinition))
    ));
    let mut audio = StageAudio::new(Arc::clone(&plan));
    assert!(matches!(
        audio.measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &active,
                memory: &WaveformMemory::new(1).unwrap()
            },
            |_| {}
        ),
        Err(WaveformError::MemoryLimit)
    ));
    assert_eq!(provider.calls, 0);
    for (limits, cancelled, reason) in [
        (
            WaveformLimits::default(),
            true,
            WaveformStopReason::Cancelled,
        ),
        (
            WaveformLimits::new(1024, 4096, Duration::from_nanos(1)).unwrap(),
            false,
            WaveformStopReason::Deadline,
        ),
    ] {
        let result = audio
            .measure_definition(
                &mut provider,
                &definition,
                WaveformControl {
                    limits,
                    cancelled: &AtomicBool::new(cancelled),
                    memory: &memory,
                },
                |_| {},
            )
            .unwrap();
        assert_eq!(result.completion, WaveformCompletion::Partial(reason));
        assert_eq!(result.waveform.measured_end(), SignalSample(0));
    }
    assert_eq!(provider.calls, 0);
    let complete = audio
        .measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &active,
                memory: &memory,
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(complete.completion, WaveformCompletion::Complete);
    let calls = provider.calls;
    provider.unavailable = true;
    let failed = audio
        .measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &active,
                memory: &memory,
            },
            |_| {},
        )
        .unwrap();
    assert!(matches!(
        failed.completion,
        WaveformCompletion::Partial(WaveformStopReason::Preparation(_))
    ));
    assert_eq!(failed.waveform.measured_end(), SignalSample(0));
    assert!(
        provider.calls > calls,
        "cached Preserve remains subject to source admission"
    );
}

#[test]
fn waveform_empty_definition_completes_without_source_access() {
    let document = document(FrameRate::new(24, 1).unwrap(), &[], [], BTreeMap::new());
    let plan = compile(&document, false);
    let definition = plan.audio_definition(node("root")).unwrap();
    let mut provider = FixtureProvider::new();
    let result = StageAudio::new(Arc::clone(&plan))
        .measure_definition(
            &mut provider,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &AtomicBool::new(false),
                memory: &WaveformMemory::default(),
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(result.completion, WaveformCompletion::Complete);
    assert_eq!(result.waveform.level_count(), 0);
    assert_eq!(result.examined_samples, 0);
    assert_eq!(provider.calls, 0);
}
