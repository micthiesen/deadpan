//! Reversed and effect-tail Holds render one canonical block through the
//! shared stage reader, whatever the read pieces.

use super::*;

fn effect_hold(frames: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Effect pause",
        HoldRecipe {
            picture_context: None,
            duration: duration(frames),
            video: HoldVideo::Background,
            audio,
        },
    )
}

#[test]
fn reversed_hold_plays_its_source_backwards_then_silence() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(
        rate,
        &["speech", "reverse"],
        [
            ("speech", source(rate, 299, 512..811)),
            (
                "reverse",
                effect_hold(
                    400,
                    HoldAudio::Reverse {
                        source: audio(512, 811),
                    },
                ),
            ),
        ],
    );
    let forward: Vec<_> = (512..811).map(fixture_sample).collect();
    let expected: Vec<_> = forward
        .iter()
        .copied()
        .chain(forward.iter().rev().copied())
        .chain([[0.0; 2]; 101])
        .collect();
    for pieces in [&[256][..], &[17, 251, 3]] {
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new();
        assert_pcm_close(&read_all(&mut renderer, &mut provider, pieces), &expected);
        // One canonical block for the whole Hold, read through its cache.
        assert_eq!(renderer.cached_stage_count(), 1);
    }
}

/// Speech over Edit [0, 4800) at 48 kHz frames, then a tail pause.
fn tail_plan(effect: TailEffect, treatments: AudioTreatments) -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut speech = source(rate, 4_800, 0..4_800);
    speech.audio_treatments = treatments;
    plan(
        rate,
        &["speech", "tail"],
        [
            ("speech", speech),
            (
                "tail",
                effect_hold(
                    20_000,
                    HoldAudio::Tail {
                        maximum: duration(16_000),
                        effect,
                    },
                ),
            ),
        ],
    )
}

fn tail_of(planned: Arc<RenderPlan>) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::new(planned);
    let mut provider = FixtureProvider::new();
    let all = read_all(&mut renderer, &mut provider, &[256, 97]);
    all[4_800..].to_vec()
}

fn trim(millidecibels: i32, muted: bool) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::new(millidecibels).unwrap(),
            muted,
            Vec::new(),
            Vec::new(),
        )
        .unwrap(),
    )
}

#[test]
fn tail_hold_rings_the_processed_sound_heard_before_it_then_exact_silence() {
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let planned = tail_plan(effect, AudioTreatments::default());
        // The input is the authored bus over the two seconds (here all 4,800
        // samples) before the pause, as heard: edges and gain applied.
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new();
        let heard = renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(0),
                4_800,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples;
        let expected = deadpan_audio::render_tail(
            deadpan_audio::TailRecipe::new(effect, 16_000, 20_000).unwrap(),
            &heard,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(expected[..16_000].iter().any(|frame| *frame != [0.0; 2]));
        let actual = tail_of(planned);
        assert_pcm_close(&actual, &expected);
        assert!(actual[16_000..].iter().all(|frame| *frame == [0.0; 2]));
    }
}

#[test]
fn a_tail_follows_later_edits_to_the_sound_before_it() {
    let plain = tail_of(tail_plan(TailEffect::Delay, AudioTreatments::default()));
    // A -6 dB trim on the speech scales the (linear) echo by the same factor.
    let quieter = tail_of(tail_plan(TailEffect::Delay, trim(-6_000, false)));
    let factor = 10_f32.powf(-6.0 / 20.0);
    assert!(plain.iter().any(|frame| frame[0].abs() > 0.01));
    for (q, p) in quieter.iter().zip(&plain) {
        assert!((q[0] - p[0] * factor).abs() < 1e-5, "{q:?} {p:?}");
    }
    // Muting the speech silences the tail completely.
    let muted = tail_of(tail_plan(TailEffect::Delay, trim(0, true)));
    assert!(muted.iter().all(|frame| *frame == [0.0; 2]));
    // A pause at the very start has nothing before it and stays silent.
    let rate = FrameRate::new(48_000, 1).unwrap();
    let first = plan(
        rate,
        &["tail", "speech"],
        [
            (
                "tail",
                effect_hold(
                    1_000,
                    HoldAudio::Tail {
                        maximum: duration(1_000),
                        effect: TailEffect::Reverb,
                    },
                ),
            ),
            ("speech", source(rate, 4_800, 0..4_800)),
        ],
    );
    let mut renderer = StageAudio::new(first);
    let mut provider = FixtureProvider::new();
    let opening = read_block(&mut renderer, &mut provider, 0, 256).samples;
    assert!(opening.iter().all(|frame| *frame == [0.0; 2]));
}

#[test]
fn a_tail_does_not_feed_the_next_tail() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let tail = |frames, ring| {
        effect_hold(
            frames,
            HoldAudio::Tail {
                maximum: duration(ring),
                effect: TailEffect::Delay,
            },
        )
    };
    let planned = plan(
        rate,
        &["speech", "first", "second"],
        [
            ("speech", source(rate, 4_800, 0..4_800)),
            ("first", tail(16_000, 16_000)),
            ("second", tail(4_000, 4_000)),
        ],
    );
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    let mut heard = renderer
        .prepare_authored_bus(
            &mut provider,
            AudioSample(0),
            4_800,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    // The first tail rings (audibly) in the second one's window, but counts
    // as silent there: tails never chain or feed back.
    heard.resize(20_800, [0.0; 2]);
    let expected = deadpan_audio::render_tail(
        deadpan_audio::TailRecipe::new(TailEffect::Delay, 4_000, 4_000).unwrap(),
        &heard,
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut renderer = StageAudio::new(planned);
    let mut provider = FixtureProvider::new();
    let all = read_all(&mut renderer, &mut provider, &[256]);
    assert!(all[4_800..20_800].iter().any(|frame| frame[0].abs() > 0.01));
    assert_pcm_close(&all[20_800..], &expected);
}

#[test]
fn a_read_that_starts_inside_a_tail_hears_the_same_samples() {
    let planned = tail_plan(TailEffect::Reverb, AudioTreatments::default());
    let mut whole = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    let all = read_all(&mut whole, &mut provider, &[256]);
    let mut seek = StageAudio::new(planned);
    let mut provider = FixtureProvider::new();
    let middle = read_block(&mut seek, &mut provider, 9_000, 200).samples;
    assert_eq!(middle, all[9_000..9_200]);
}

#[test]
fn tone_hold_renders_the_canonical_sine_without_reading_media() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let level = GainDb::new(-6_000).unwrap();
    let planned = plan(
        rate,
        &["bleep"],
        [(
            "bleep",
            effect_hold(
                3_000,
                HoldAudio::Tone {
                    frequency_hz: 1_000,
                    level,
                },
            ),
        )],
    );
    let expected =
        deadpan_audio::tone(1_000, level.millidecibels(), 3_000, &AtomicBool::new(false)).unwrap();
    let mut renderer = StageAudio::new(planned);
    let mut provider = FixtureProvider::new();
    let actual = read_all(&mut renderer, &mut provider, &[256, 31]);
    assert_pcm_close(&actual, &expected);
    assert_eq!(provider.calls, 0, "a tone reads no source");
}

/// The authored bus (with every owner's gain) over a plan's tail region.
fn authored_tail(planned: Arc<RenderPlan>) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::new(planned);
    let mut provider = FixtureProvider::new();
    let mut tail = Vec::new();
    let mut at = 4_800;
    while at < 24_800 {
        let count = (24_800 - at).min(256);
        tail.extend(
            renderer
                .prepare_authored_bus(
                    &mut provider,
                    AudioSample(at),
                    count as u32,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
        at += count;
    }
    tail
}

/// Speech then a delay tail, both inside a group carrying `group`.
fn grouped_tail(group: AudioTreatments) -> Vec<[f32; 2]> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut wrapper = BeatNode::sequence("Group", vec![id("speech"), id("tail")]);
    wrapper.audio_treatments = group;
    authored_tail(plan(
        rate,
        &["group"],
        [
            ("speech", source(rate, 4_800, 0..4_800)),
            (
                "tail",
                effect_hold(
                    20_000,
                    HoldAudio::Tail {
                        maximum: duration(16_000),
                        effect: TailEffect::Delay,
                    },
                ),
            ),
            ("group", wrapper),
        ],
    ))
}

#[test]
fn gain_shared_by_the_tail_and_the_sound_before_it_applies_once() {
    let plain = grouped_tail(AudioTreatments::default());
    assert!(plain.iter().any(|frame| frame[0].abs() > 0.01));
    // A -6 dB group trim reaches the tail once (on its output), not twice.
    let quieter = grouped_tail(trim(-6_000, false));
    let factor = 10_f32.powf(-6.0 / 20.0);
    for (q, p) in quieter.iter().zip(&plain) {
        assert!((q[0] - p[0] * factor).abs() < 1e-5, "{q:?} {p:?}");
    }
    // A shared envelope is evaluated at the tail's own time only: -6 dB under
    // the speech and 0 dB under the tail leaves the tail unchanged, where
    // applying it at input time would have halved it.
    let step = |start: i64, end: i64, db: i32| {
        GainEnvelope::new(
            GainClock::OwnerOutput,
            GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap(),
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
    };
    let envelope = AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::new(0).unwrap(),
            false,
            vec![step(0, 4_800, -6_000)],
            Vec::new(),
        )
        .unwrap(),
    );
    let enveloped = grouped_tail(envelope);
    for (e, p) in enveloped.iter().zip(&plain) {
        assert!((e[0] - p[0]).abs() < 1e-5, "{e:?} {p:?}");
    }
    // Muting the group mutes the tail (once, on its output).
    let muted = grouped_tail(trim(0, true));
    assert!(muted.iter().all(|frame| *frame == [0.0; 2]));
}

fn repeated_tail_document() -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let tail = |frames| HoldAudio::Tail {
        maximum: duration(frames),
        effect: TailEffect::Reverb,
    };
    let repeat = BeatNode {
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        label: "Two plays".into(),
        kind: NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
            gap: Some(HoldRecipe {
                picture_context: None,
                duration: duration(1_200),
                video: HoldVideo::Background,
                audio: tail(1_200),
            }),
            escalation: None,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    document_with_asset(
        rate,
        &["repeat", "after"],
        [
            ("speech", source(rate, 2_400, 0..2_400)),
            ("pause", effect_hold(2_400, tail(2_400))),
            (
                "group",
                BeatNode::sequence("Played", vec![id("speech"), id("pause")]),
            ),
            ("repeat", repeat),
            ("after", source(rate, 3_000, 5_000..8_000)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    )
}

fn read_document(document: &ProjectDocument) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let mut provider = FixtureProvider::new();
    provider.revisions.insert(document.revision_id().clone());
    faded_all(&mut renderer, &mut provider, &[256, 61])
}

#[test]
fn tails_in_repeats_splits_and_bound_clocks_render_and_the_original_after_them_reads() {
    // Plays [0, 4800) and [6000, 10800) with a 1200-sample gap tail between,
    // then the Original after them at 10800.
    let document = repeated_tail_document();
    let whole = read_document(&document);
    assert_eq!(whole.len(), 13_800);
    for ring in [2_400..4_800, 4_800..6_000, 8_400..10_800] {
        assert!(
            whole[ring.clone()]
                .iter()
                .any(|frame| frame[0].abs() > 1e-4),
            "{ring:?}"
        );
    }
    let after: Vec<_> = (5_000..8_000).map(fixture_sample).collect();
    // Away from the 96-sample seam fades the Original plays unchanged.
    assert_pcm_close(&whole[10_900..13_700], &after[100..2_900]);
    // Splitting the pause and the following Original keeps every sample:
    // a fragment keeps its occurrence's start, so its ring continues.
    let split = split_command(&document, &id("pause"), 1_000, "split-pause");
    let split = split_command(&split, &id("after"), 1_000, "split-after");
    assert_pcm_close(&read_document(&split), &whole);
    // Retained (bound) clocks read the same PCM, tails included.
    let bound = {
        let mut wire = serde_json::to_value(&split).unwrap();
        wire["audio_bindings"] = serde_json::to_value(
            capture_unbound_audio_bindings(
                &split,
                AudioTimingId {
                    allocation: RevisionId::new("bound").unwrap(),
                    ordinal: 0,
                },
            )
            .unwrap(),
        )
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    assert_pcm_close(&read_document(&bound), &whole);
    // Point-grid definition reads never fail: the played group's tail has no
    // edit position there and is silent, and the Original after it reads.
    let planned = Arc::new(RenderPlan::compile(&bound).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    provider.revisions.insert(bound.revision_id().clone());
    let group = planned
        .audio_definition(deadpan_plan::AudioDefinitionSelector::Node { node: id("group") })
        .unwrap();
    let played = renderer
        .read_definition(
            &mut provider,
            &group,
            SignalSample(2_400),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(played.samples.iter().all(|frame| *frame == [0.0; 2]));
    let speech = renderer
        .read_definition(
            &mut provider,
            &group,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(speech.samples.iter().any(|frame| *frame != [0.0; 2]));
}

#[test]
fn the_source_stage_explains_that_a_tail_is_read_from_the_processed_stages() {
    let planned = tail_plan(TailEffect::Reverb, AudioTreatments::default());
    let mut provider = FixtureProvider::new();
    let error = SequenceAudio::new(planned)
        .read_sources(
            &mut provider,
            AudioSample(4_800),
            16,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(matches!(error, SequenceAudioError::Unsupported { .. }));
    let message = error.to_string();
    assert!(
        message.contains("hanging tail") && message.contains("--limited"),
        "{message}"
    );
}
