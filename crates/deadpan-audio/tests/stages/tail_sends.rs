//! Sends select actual decoded contributions. The oracle feeds independently
//! rendered input into the fixed effect, without traversing the send reader.
use super::*;

fn sends(effect: TailEffect, db: i32) -> TailSends {
    TailSends::default().with_level(effect, Some(GainDb::new(db).unwrap()))
}

fn document(effect: TailEffect) -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    document_with_asset(
        rate,
        &["speech", "tail"],
        [
            ("speech", source(rate, 4_800, 0..4_800)),
            (
                "tail",
                effect_hold(
                    20_000,
                    HoldAudio::Tail {
                        maximum: duration(16_000),
                        effect,
                        input: TailInput::Sends,
                    },
                ),
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    )
}

fn changed(document: &ProjectDocument, node: &str, recipe: AudioTreatments) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"][node]["audio_treatments"] = serde_json::to_value(recipe).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn planned(document: &ProjectDocument) -> Arc<RenderPlan> {
    Arc::new(RenderPlan::compile(document).unwrap())
}

fn heard(document: &ProjectDocument) -> Vec<[f32; 2]> {
    StageAudio::new(planned(document))
        .prepare_authored_bus(
            &mut FixtureProvider::new(),
            AudioSample(0),
            4_800,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

fn expected(effect: TailEffect, input: &[[f32; 2]]) -> Vec<[f32; 2]> {
    deadpan_audio::render_tail(
        deadpan_audio::TailRecipe::new(effect, 16_000, 20_000).unwrap(),
        input,
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn sound(send: TailSends) -> SoundEvent {
    SoundEvent {
        owner: id("root"),
        label: "Selected send".into(),
        source: audio(0, 4_800),
        mapping: SourceAudioMapping::natural_rate(
            audio(0, 4_800).span,
            FrameRate::new(48_000, 1).unwrap(),
        )
        .unwrap(),
        offset: AudioSample(0),
        gain_millidecibels: -3_000,
        tail_sends: send,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn with_sound(document: &ProjectDocument, event: SoundEvent, attached: bool) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("b".repeat(64));
    wire["assets"]["media"]["still_image"] = serde_json::json!(false);
    if attached {
        let mut event = serde_json::to_value(event).unwrap();
        event.as_object_mut().unwrap().remove("owner");
        wire["beat_sounds"] = serde_json::json!({"speech": {"effect": event}});
    } else {
        wire["sounds"] = serde_json::json!({"effect": event});
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn sends_are_off_by_default_and_do_not_change_the_dry_mix() {
    for effect in [TailEffect::Reverb, TailEffect::Delay] {
        let base = document(effect);
        assert!(
            tail_of(planned(&base))
                .iter()
                .all(|sample| *sample == [0.0; 2])
        );
        let selected = changed(
            &base,
            "speech",
            AudioTreatments::default().with_tail_sends(sends(effect, -6_000)),
        );
        assert_eq!(heard(&base), heard(&selected));
        let input: Vec<_> = heard(&base)
            .into_iter()
            .map(|sample| sample.map(|value| (f64::from(value) * 10_f64.powf(-6.0 / 20.0)) as f32))
            .collect();
        assert_pcm_close(&tail_of(planned(&selected)), &expected(effect, &input));
        let wrong_effect = if effect == TailEffect::Reverb {
            TailEffect::Delay
        } else {
            TailEffect::Reverb
        };
        let wrong = changed(
            &base,
            "speech",
            AudioTreatments::default().with_tail_sends(sends(wrong_effect, 0)),
        );
        assert!(
            tail_of(planned(&wrong))
                .iter()
                .all(|sample| *sample == [0.0; 2])
        );
    }
}

#[test]
fn selected_root_and_attached_sound_sends_exclude_the_original() {
    for attached in [false, true] {
        let base = document(TailEffect::Delay);
        let selected = with_sound(&base, sound(sends(TailEffect::Delay, -6_000)), attached);
        // The event's hard edges and -3 dB gain precede its -6 dB send.
        let input: Vec<_> = (0..4_800)
            .map(|at| {
                fixture_sample(at).map(|value| (f64::from(value) * 10_f64.powf(-9.0 / 20.0)) as f32)
            })
            .collect();
        let actual = tail_of(planned(&selected));
        assert_pcm_close(&actual, &expected(TailEffect::Delay, &input));
        assert!(actual.iter().flatten().any(|value| value.abs() > 0.01));
        // A louder Original cannot leak into a sound-only send.
        let louder = changed(&selected, "speech", trim(12_000, false));
        assert_eq!(tail_of(planned(&louder)), actual);
    }
}

#[test]
fn beat_sends_tap_the_processed_group_mix_and_preserve_shared_drive() {
    let effect = TailEffect::Reverb;
    let base = with_sound(&document(effect), sound(TailSends::default()), true);
    let recipe = trim(-3_000, false)
        .with_saturation(Some(Saturation::new(GainDb::new(12_000).unwrap()).unwrap()))
        .unwrap()
        .with_balance(Some(StereoBalance::new(25).unwrap()))
        .unwrap();
    let dry = changed(&base, "speech", recipe.clone());
    let selected = changed(&base, "speech", recipe.with_tail_sends(sends(effect, 0)));
    assert_eq!(heard(&dry), heard(&selected));
    assert_pcm_close(
        &tail_of(planned(&selected)),
        &expected(effect, &heard(&dry)),
    );
}

#[test]
fn shared_ancestor_gain_applies_once_on_the_tail_output() {
    let base = changed(
        &document(TailEffect::Delay),
        "speech",
        AudioTreatments::default().with_tail_sends(sends(TailEffect::Delay, 0)),
    );
    let plain = authored_tail(planned(&base));
    let quiet = changed(&base, "root", trim(-6_000, false));
    let quieter = authored_tail(planned(&quiet));
    assert!(plain.iter().flatten().any(|value| value.abs() > 0.01));
    for (p, q) in plain.iter().zip(quieter) {
        for channel in 0..2 {
            assert!((q[channel] - p[channel] * 10_f32.powf(-6.0 / 20.0)).abs() < 1e-5);
        }
    }
}

#[test]
fn sends_are_identical_on_cold_seek_and_irregular_reads() {
    let document = with_sound(
        &document(TailEffect::Reverb),
        sound(sends(TailEffect::Reverb, 0)),
        false,
    );
    let plan = planned(&document);
    let all = read_all(
        &mut StageAudio::new(Arc::clone(&plan)),
        &mut FixtureProvider::new(),
        &[256],
    );
    assert_eq!(
        read_all(
            &mut StageAudio::new(Arc::clone(&plan)),
            &mut FixtureProvider::new(),
            &[17, 251, 3]
        ),
        all
    );
    assert_eq!(
        read_block(
            &mut StageAudio::new(plan),
            &mut FixtureProvider::new(),
            9_000,
            200
        )
        .samples,
        all[9_000..9_200]
    );
}

#[test]
fn selected_sends_keep_eq_and_shared_saturation_in_authored_order() {
    let effect = TailEffect::Reverb;
    let base = with_sound(&document(effect), sound(TailSends::default()), true);
    let eq = EqualizerRecipe::new(
        [
            GainDb::new(-3000).unwrap(),
            GainDb::new(6000).unwrap(),
            GainDb::UNITY,
        ],
        Some(EqCutoffHz::new(80).unwrap()),
        Some(EqCutoffHz::new(12000).unwrap()),
    )
    .unwrap();
    for order in [
        vec![
            AudioTreatmentStage::Equalizer,
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::ClipGain,
        ],
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::Equalizer,
        ],
    ] {
        let recipe = AudioTreatments::with_equalizer_stages(
            order,
            Some(ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap()),
            Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()),
            None,
            Some(eq),
        )
        .unwrap();
        let dry = changed(&base, "speech", recipe.clone());
        let selected = changed(&base, "speech", recipe.with_tail_sends(sends(effect, 0)));
        assert_pcm_close(
            &tail_of(planned(&selected)),
            &expected(effect, &heard(&dry)),
        );
        let full = tail_of(planned(&selected));
        let cold = read_block(
            &mut StageAudio::new(planned(&selected)),
            &mut FixtureProvider::new(),
            9000,
            127,
        )
        .samples;
        assert_eq!(cold, full[4200..4327]);
    }
}

#[test]
fn a_shared_root_send_does_not_double_its_output_gain() {
    let base = changed(
        &document(TailEffect::Reverb),
        "root",
        AudioTreatments::default().with_tail_sends(sends(TailEffect::Reverb, 0)),
    );
    let plain = authored_tail(planned(&base));
    let quiet = changed(
        &base,
        "root",
        trim(-6000, false).with_tail_sends(sends(TailEffect::Reverb, 0)),
    );
    let quieter = authored_tail(planned(&quiet));
    for (p, q) in plain.iter().zip(quieter) {
        for channel in 0..2 {
            assert!((q[channel] - p[channel] * 10_f32.powf(-6.0 / 20.0)).abs() < 1e-5);
        }
    }
}

#[test]
fn sends_respect_silent_hold_permissions_in_the_input_window() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut quiet = source(rate, 2400, 0..2400);
    quiet.audio_treatments = trim(0, true);
    let base = document_with_asset(
        rate,
        &["speech", "silence", "tail"],
        [
            ("speech", quiet),
            ("silence", hold(2400)),
            (
                "tail",
                effect_hold(
                    20_000,
                    HoldAudio::Tail {
                        maximum: duration(16000),
                        effect: TailEffect::Delay,
                        input: TailInput::Sends,
                    },
                ),
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let selected = with_sound(&base, sound(sends(TailEffect::Delay, 0)), false);
    let input = heard(&selected);
    assert!(
        input[..2300]
            .iter()
            .flatten()
            .any(|value| value.abs() > 0.01)
    );
    assert!(input[2400..].iter().all(|sample| *sample == [0.0; 2]));
    assert_pcm_close(
        &tail_of(planned(&selected)),
        &expected(TailEffect::Delay, &input),
    );
}

#[test]
fn tail_returns_never_feed_another_selected_send() {
    let base = changed(
        &document(TailEffect::Delay),
        "speech",
        AudioTreatments::default().with_tail_sends(sends(TailEffect::Delay, 0)),
    );
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["speech", "tail", "second"]);
    let second = effect_hold(
        20_000,
        HoldAudio::Tail {
            maximum: duration(16000),
            effect: TailEffect::Delay,
            input: TailInput::Sends,
        },
    );
    wire["nodes"]["second"] = serde_json::to_value(second).unwrap();
    wire["nodes"]["tail"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::default().with_tail_sends(sends(TailEffect::Delay, 24000)),
    )
    .unwrap();
    let both = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut input = heard(&base);
    input.resize(24800, [0.0; 2]);
    let expected = expected(TailEffect::Delay, &input);
    let all = read_all(
        &mut StageAudio::new(planned(&both)),
        &mut FixtureProvider::new(),
        &[256],
    );
    assert_pcm_close(&all[24800..], &expected);
}

#[test]
fn cancelled_send_preparation_can_retry_without_partial_tail_state() {
    let selected = with_sound(
        &document(TailEffect::Reverb),
        sound(sends(TailEffect::Reverb, 0)),
        false,
    );
    let plan = planned(&selected);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    provider.cancel_on_call = true;
    let cancelled = AtomicBool::new(false);
    assert!(
        renderer
            .read(&mut provider, AudioSample(9000), 200, TIMEOUT, &cancelled)
            .is_err()
    );
    assert!(cancelled.load(Ordering::Relaxed));
    cancelled.store(false, Ordering::Relaxed);
    provider.cancel_on_call = false;
    let recovered = renderer
        .read(&mut provider, AudioSample(9000), 200, TIMEOUT, &cancelled)
        .unwrap()
        .samples;
    assert_eq!(
        recovered,
        read_block(
            &mut StageAudio::new(plan),
            &mut FixtureProvider::new(),
            9000,
            200
        )
        .samples
    );
}

#[test]
fn selected_sends_keep_repeat_gap_and_split_tail_phase() {
    let base = repeated_tail_document();
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"]["speech"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::default().with_tail_sends(sends(TailEffect::Reverb, 0)),
    )
    .unwrap();
    wire["nodes"]["pause"]["kind"]["recipe"]["audio"]["input"] = serde_json::json!("sends");
    wire["nodes"]["repeat"]["kind"]["gap"]["audio"]["input"] = serde_json::json!("sends");
    let selected = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected = read_document(&base);
    assert_pcm_close(&read_document(&selected), &expected);
    let split = split_command(&selected, &id("pause"), 1000, "split-send-tail");
    assert_pcm_close(&read_document(&split), &expected);
    let mut wire = serde_json::to_value(&split).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            &split,
            AudioTimingId {
                allocation: RevisionId::new("send-bound").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_pcm_close(&read_document(&bound), &expected);
}

#[test]
fn send_preparation_respects_residency_before_media_reads() {
    let selected = with_sound(
        &document(TailEffect::Delay),
        sound(sends(TailEffect::Delay, 0)),
        false,
    );
    let mut renderer = StageAudio::with_limits(
        planned(&selected),
        StageLimits {
            maximum_resident_frames: 1024,
            ..StageLimits::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(9000),
            200,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.calls, 0);
}
