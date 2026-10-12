//! Decoded real PCM through stored owner returns. Oracles compose independently
//! prepared dry samples and finite kernels, outside the recursive mixer.
use super::*;

fn base() -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut body = BeatNode::sequence("Repeated source", vec![]);
    body.kind = NodeKind::Repeat {
        child: id("source"),
        iterations: IterationOrder::new(RevisionId::new("wet-plays").unwrap(), 6).unwrap(),
        gap: None,
        escalation: None,
    };
    document_with_asset(
        rate,
        &["body"],
        [("body", body), ("source", source(rate, 4800, 0..4800))],
        BTreeMap::new(),
        audio(0, 8197).span,
    )
}

fn changed(document: &ProjectDocument, node: &str, recipe: AudioTreatments) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"][node]["audio_treatments"] = serde_json::to_value(recipe).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn send(effect: TailEffect, level: i32) -> AudioTreatments {
    AudioTreatments::default()
        .with_wet_sends(TailSends::default().with_level(effect, Some(GainDb::new(level).unwrap())))
}

fn reader(document: &ProjectDocument) -> StageAudio {
    StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()))
}

fn read(
    reader: &mut StageAudio,
    provider: &mut FixtureProvider,
    start: i64,
    frames: u32,
) -> Vec<[f32; 2]> {
    reader
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            frames,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

fn all(document: &ProjectDocument) -> Vec<[f32; 2]> {
    let mut provider = FixtureProvider::new();
    provider.revisions.insert(document.revision_id().clone());
    read(
        &mut reader(document),
        &mut provider,
        0,
        document.duration().unwrap().frames() as u32,
    )
}

pub(super) fn wet(effect: TailEffect, input: &[[f32; 2]]) -> Vec<[f64; 2]> {
    let cancel = AtomicBool::new(false);
    let mut processor = deadpan_audio::FiniteWetEffect::new(effect, &cancel).unwrap();
    let history = processor.descriptor().history_frames;
    let width = processor.descriptor().output_frames;
    let mut output = Vec::new();
    for start in (0..input.len()).step_by(width) {
        let mut left = vec![0.0; processor.input_frames()];
        let mut right = left.clone();
        for (offset, (left, right)) in left.iter_mut().zip(&mut right).enumerate() {
            let at = start as i64 - history as i64 + offset as i64;
            if at >= 0
                && let Some(frame) = input.get(at as usize)
            {
                *left = f64::from(frame[0]);
                *right = f64::from(frame[1]);
            }
        }
        let mut a = vec![0.0; width];
        let mut b = a.clone();
        processor
            .process_tile(&left, &right, &mut a, &mut b, &cancel)
            .unwrap();
        output.extend(
            a.into_iter()
                .zip(b)
                .map(|(a, b)| [a, b])
                .take(input.len() - start),
        );
    }
    output
}

#[test]
fn root_returns_match_dry_plus_finite_wet_and_cold_partitions() {
    let base = base();
    let dry = all(&base);
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let document = changed(&base, "root", send(effect, -6000));
        let plan = RenderPlan::compile(&document).unwrap();
        assert!(plan.has_audio_wet_sends());
        let returned = wet(effect, &dry);
        let expected: Vec<_> = dry
            .iter()
            .zip(returned)
            .map(|(dry, wet)| {
                std::array::from_fn(|ch| {
                    (f64::from(dry[ch]) + wet[ch] * 10f64.powf(-6.0 / 20.0)) as f32
                })
            })
            .collect();
        let actual = all(&document);
        assert_pcm_close(&actual, &expected);
        assert!(
            actual
                .iter()
                .zip(&dry)
                .any(|(a, b)| (a[0] - b[0]).abs() > 0.001)
        );
        for (at, count) in [(14399, 257), (0, 31), (8191, 8193), (28000, 800)] {
            let cold = read(
                &mut reader(&document),
                &mut FixtureProvider::new(),
                at,
                count,
            );
            assert_pcm_close(&cold, &actual[at as usize..at as usize + count as usize]);
        }
    }
}

#[test]
fn child_owner_returns_enter_before_parent_saturation() {
    let base = base();
    let dry = all(&base);
    let saturation = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let document = changed(
            &changed(&base, "body", send(effect, -3000)),
            "root",
            AudioTreatments::default()
                .with_saturation(Some(saturation))
                .unwrap(),
        );
        let returned = wet(effect, &dry);
        let expected: Vec<_> = dry
            .iter()
            .zip(&returned)
            .map(|(dry, wet)| {
                std::array::from_fn(|ch| {
                    saturation.shape(f64::from(dry[ch]) + wet[ch] * 10f64.powf(-3.0 / 20.0)) as f32
                })
            })
            .collect();
        let wrong: Vec<[f32; 2]> = dry
            .iter()
            .zip(&returned)
            .map(|(dry, wet)| {
                std::array::from_fn(|ch| {
                    (saturation.shape(f64::from(dry[ch])) + wet[ch] * 10f64.powf(-3.0 / 20.0))
                        as f32
                })
            })
            .collect();
        let actual = all(&document);
        assert_pcm_close(&actual, &expected);
        assert!(
            actual
                .iter()
                .zip(wrong)
                .any(|(a, b)| (a[0] - b[0]).abs() > 0.001)
        );
    }
}

#[test]
fn silent_hold_suppresses_incoming_root_return_and_resumed_source_keeps_history() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let base = document_with_asset(
        rate,
        &["before", "pause", "after"],
        [
            ("before", source(rate, 4800, 0..4800)),
            ("pause", hold(1000)),
            ("after", source(rate, 4800, 0..4800)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let dry = all(&base);
    let document = changed(&base, "root", send(TailEffect::Reverb, 0));
    let returned = wet(TailEffect::Reverb, &dry);
    let expected: Vec<_> = dry
        .iter()
        .zip(returned)
        .enumerate()
        .map(|(at, (dry, wet))| {
            if (4800..5800).contains(&at) {
                [0.0; 2]
            } else {
                std::array::from_fn(|ch| (f64::from(dry[ch]) + wet[ch]) as f32)
            }
        })
        .collect();
    let actual = all(&document);
    assert_pcm_close(&actual, &expected);
    assert!(actual[4800..5800].iter().all(|frame| *frame == [0.0; 2]));
    let cold = read(
        &mut reader(&document),
        &mut FixtureProvider::new(),
        5800,
        257,
    );
    assert_pcm_close(&cold, &actual[5800..6057]);
}

#[test]
fn return_admission_and_cancellation_leave_reader_reusable() {
    let document = changed(&base(), "root", send(TailEffect::Delay, 0));
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let limits = StageLimits {
        maximum_resident_frames: 1024 * 1024,
        ..StageLimits::default()
    };
    let mut constrained = StageAudio::with_limits(Arc::clone(&plan), limits).unwrap();
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        constrained.prepare_authored_bus(
            &mut provider,
            AudioSample(15000),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    // A refusal cannot corrupt accounting or strand a prepared stage.
    assert!(matches!(
        constrained.prepare_authored_bus(
            &mut provider,
            AudioSample(15000),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    let mut normal = StageAudio::new(plan);
    assert!(
        normal
            .prepare_authored_bus(
                &mut provider,
                AudioSample(15000),
                256,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(read(&mut normal, &mut provider, 15000, 256).len(), 256);
}

#[test]
fn split_source_keeps_full_wet_history_and_repeat_occurrence_identity() {
    let document = changed(&base(), "source", send(TailEffect::Reverb, -3000));
    let expected = all(&document);
    let split = split_command(&document, &id("source"), 2400, "split-wet");
    assert_pcm_close(&all(&split), &expected);
    let mut provider = FixtureProvider::new();
    provider.revisions.insert(split.revision_id().clone());
    let cold = read(&mut reader(&split), &mut provider, 3 * 4800 + 2450, 512);
    assert_pcm_close(&cold, &expected[3 * 4800 + 2450..3 * 4800 + 2962]);
}

#[test]
fn allowed_sound_cannot_carry_forbidden_original_history_into_silent_hold() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let base = document_with_asset(
        rate,
        &["before", "pause", "after"],
        [
            ("before", source(rate, 2600, 0..2600)),
            ("pause", hold(800)),
            ("after", source(rate, 2600, 0..2600)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("b".repeat(64));
    wire["assets"]["media"]["still_image"] = serde_json::json!(false);
    wire["sounds"] = serde_json::json!({"sound":SoundEvent {
        owner:id("root"), label:"Permitted independent sound".into(), source:audio(0,6000),
        mapping:SourceAudioMapping::natural_rate(audio(0,6000).span,rate).unwrap(),
        offset:AudioSample(0), gain_millidecibels:-6000, tail_sends:Default::default(), wet_sends: Default::default(),
        start_edge:AudioEdgePolicy::Hard, end_edge:AudioEdgePolicy::Hard, overflow:SoundOverflowPolicy::Reject }});
    wire["sound_allowances"] = serde_json::json!({"sound":SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
        instance:InstancePath {node:id("pause"),repeats:vec![]}}]).unwrap()});
    let base = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let root = send(TailEffect::Reverb, 0)
        .with_saturation(Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()))
        .unwrap();
    let document = changed(&base, "root", root);
    let mute = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::UNITY, true, vec![], vec![]).unwrap(),
    );
    let without_original = changed(&changed(&document, "before", mute.clone()), "after", mute);
    let actual = all(&document);
    let permitted_only = all(&without_original);
    assert_pcm_close(&actual[2600..3400], &permitted_only[2600..3400]);
    assert!(actual[2600..3400].iter().flatten().any(|x| x.abs() > 0.001));
    assert!(
        actual[..2600]
            .iter()
            .zip(&permitted_only[..2600])
            .any(|(a, b)| (a[0] - b[0]).abs() > 0.01)
    );
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["sound_allowances"] = serde_json::json!({});
    let forbidden = all(&ProjectDocument::from_json(&wire.to_string()).unwrap());
    assert!(forbidden[2600..3400].iter().all(|x| *x == [0.0; 2]));
}

#[test]
fn nested_wet_returns_share_bounded_workspace_without_self_feedback() {
    let base = base();
    let child = changed(&base, "body", send(TailEffect::Reverb, -9000));
    let child_pcm = all(&child);
    let document = changed(&child, "root", send(TailEffect::Delay, -6000));
    let returned = wet(TailEffect::Delay, &child_pcm);
    let expected: Vec<_> = child_pcm
        .iter()
        .zip(returned)
        .map(|(dry, wet)| {
            std::array::from_fn(|ch| {
                (f64::from(dry[ch]) + wet[ch] * 10f64.powf(-6.0 / 20.0)) as f32
            })
        })
        .collect();
    assert_pcm_close(&all(&document), &expected);
    let cold = read(
        &mut reader(&document),
        &mut FixtureProvider::new(),
        14500,
        257,
    );
    assert_pcm_close(&cold, &expected[14500..14757]);
}

#[test]
fn wet_return_precedes_parent_equalizer_with_complete_prefix_context() {
    let base = base();
    let child = changed(&base, "body", send(TailEffect::Reverb, -6000));
    let dry = all(&child);
    let recipe = EqualizerRecipe::new(
        [
            GainDb::new(6000).unwrap(),
            GainDb::new(-3000).unwrap(),
            GainDb::UNITY,
        ],
        None,
        None,
    )
    .unwrap();
    let document = changed(
        &child,
        "root",
        AudioTreatments::default()
            .with_equalizer(Some(recipe))
            .unwrap(),
    );
    let mut eq = deadpan_audio::Equalizer::new(recipe, &AtomicBool::new(false)).unwrap();
    let mut expected = Vec::new();
    for start in (0..dry.len()).step_by(deadpan_dsp::FINITE_FILTER_OUTPUT_FRAMES) {
        let mut left = vec![0.0; deadpan_dsp::FINITE_FILTER_INPUT_FRAMES];
        let mut right = left.clone();
        for (at, (left, right)) in left.iter_mut().zip(&mut right).enumerate() {
            let index = start as i64 + at as i64 - deadpan_dsp::FINITE_FILTER_RADIUS as i64;
            if index >= 0
                && let Some(frame) = dry.get(index as usize)
            {
                *left = f64::from(frame[0]);
                *right = f64::from(frame[1]);
            }
        }
        let mut a = vec![0.0; deadpan_dsp::FINITE_FILTER_OUTPUT_FRAMES];
        let mut b = a.clone();
        eq.process_tile(&left, &right, &mut a, &mut b, &AtomicBool::new(false))
            .unwrap();
        expected.extend(
            a.into_iter()
                .zip(b)
                .map(|(a, b)| [a as f32, b as f32])
                .take(dry.len() - start),
        );
    }
    assert_pcm_close(&all(&document), &expected);
}

#[test]
fn explicit_tail_provider_excludes_unselected_incoming_parent_returns() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let tail = BeatNode::hold(
            "Selected tail policy",
            HoldRecipe {
                duration: duration(3000),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Tail {
                    maximum: duration(3000),
                    effect,
                    input: TailInput::Original,
                },
            },
        );
        let base = document_with_asset(
            rate,
            &["before", "tail"],
            [("before", source(rate, 4800, 0..4800)), ("tail", tail)],
            BTreeMap::new(),
            audio(0, 8197).span,
        );
        let dry = all(&base);
        let actual = all(&changed(&base, "root", send(TailEffect::Reverb, 0)));
        // The pause's explicit provider selects its effect and input. A parent's
        // unrelated incoming reverb cannot replace or add to that selection.
        assert_pcm_close(&actual[4800..], &dry[4800..]);
    }
}

#[test]
fn default_repeat_tail_gap_excludes_parent_returns_and_resumes_them_afterward() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut body = BeatNode::sequence("Tail gap", vec![]);
    body.kind = NodeKind::Repeat {
        child: id("source"),
        iterations: IterationOrder::new(RevisionId::new("wet-gap-plays").unwrap(), 2).unwrap(),
        gap: Some(HoldRecipe {
            duration: duration(3000),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Tail {
                maximum: duration(3000),
                effect: TailEffect::Delay,
                input: TailInput::Original,
            },
        }),
        escalation: None,
    };
    let base = document_with_asset(
        rate,
        &["body"],
        [("body", body), ("source", source(rate, 4800, 0..4800))],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let dry = all(&base);
    let actual = all(&changed(&base, "root", send(TailEffect::Reverb, 0)));
    assert_pcm_close(&actual[4800..7800], &dry[4800..7800]);
    assert!(
        actual[7800..]
            .iter()
            .zip(&dry[7800..])
            .any(|(a, b)| (a[0] - b[0]).abs() > 0.01)
    );
}
