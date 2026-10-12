use super::*;

fn qualified(document: ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn attached(document: ProjectDocument, owner: &str) -> ProjectDocument {
    let recipe = voice_recipe(document.presentation_basis().frame_rate, 6144);
    let event = BeatSound {
        label: "Full history".into(),
        source: recipe.source,
        mapping: recipe.mapping,
        offset: recipe.offset,
        gain_millidecibels: -3000,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(qualified(document)).unwrap();
    wire["beat_sounds"] = serde_json::json!({owner: {"first": event.clone(), "second": event}});
    // Owner treatments belong to the shared graph after this operand.
    wire["nodes"][owner]["audio_treatments"] =
        serde_json::to_value(AudioTreatments::from_clip_gain(
            ClipGain::new(GainDb::new(0).unwrap(), true, vec![], vec![]).unwrap(),
        ))
        .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn context<'a>(plan: &'a RenderPlan, name: &str) -> deadpan_plan::AudioSoundOwnerContext<'a> {
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id(name))
        .unwrap();
    plan.audio_sound_owner_context(owner, Default::default())
        .unwrap()
}

fn compare_history(case: u8) {
    let rate = FrameRate::new(48000, 1).unwrap();
    let (owner, root, offset, visible, nodes) = if case == 1 {
        (
            "owner",
            "outer",
            501,
            2048,
            vec![
                ("owner", source(rate, 6144, 0..6144)),
                (
                    "inner",
                    retime("owner", 4096, 0..6144, PitchPolicy::Preserve),
                ),
                (
                    "outer",
                    retime("inner", 3072, 0..4096, PitchPolicy::Preserve),
                ),
            ],
        )
    } else if case == 2 {
        (
            "owner",
            "outer",
            129,
            512,
            vec![
                ("owner", source(rate, 6144, 0..6144)),
                (
                    "inside",
                    retime("owner", 2048, 4096..6144, PitchPolicy::FollowSpeed),
                ),
                (
                    "outer",
                    retime("inside", 1024, 0..2048, PitchPolicy::Preserve),
                ),
            ],
        )
    } else {
        (
            "group",
            "group",
            1001,
            4000,
            vec![
                ("a", source(rate, 2048, 0..2048)),
                ("pause", hold(256, HoldAudio::Silence)),
                ("b", source(rate, 3840, 2048..5888)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("a"), id("pause"), id("b")]),
                ),
            ],
        )
    };
    let whole = compile(
        &attached(
            document(rate, &[root], nodes.clone(), BTreeMap::new()),
            owner,
        ),
        false,
    );
    let cropped = compile(
        &attached(
            document(
                rate,
                &["crop"],
                nodes.into_iter().chain([(
                    "crop",
                    retime(
                        root,
                        visible,
                        offset..offset + visible,
                        PitchPolicy::FollowSpeed,
                    ),
                )]),
                BTreeMap::new(),
            ),
            owner,
        ),
        false,
    );
    let input = context(&cropped, owner);
    let batch = input
        .source_voice_occurrences(
            &id(owner),
            voice_recipe(rate, 6144),
            input.samples(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 1);
    let voice = &batch.voices()[0];
    let reference = whole
        .source_voice_occurrence(
            InstancePath {
                node: id(owner),
                repeats: vec![],
            },
            voice_recipe(rate, 6144),
            Default::default(),
        )
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&cropped));
    let mut oracle = StageAudio::new(Arc::clone(&whole));
    let mut provider = FixtureProvider::new();
    for at in [
        -offset,
        -128,
        0,
        1024.min(input.samples().end.0 - 128),
        input.samples().end.0 - 128,
    ] {
        let actual = renderer
            .read_source_voice_occurrence(
                &mut provider,
                voice,
                AudioSample(at),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        let expected = oracle
            .read_source_voice_occurrence(
                &mut provider,
                &reference,
                AudioSample(at + offset),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples, expected.samples,
            "hidden history at {at}, case={case}"
        );
        assert_eq!(
            actual.suppressed,
            expected
                .suppressed
                .into_iter()
                .map(|r| AudioSample(r.start.0 - offset)..AudioSample(r.end.0 - offset))
                .collect::<Vec<_>>()
        );
        let partial = renderer
            .read_source_voice_occurrences(
                &mut provider,
                &batch,
                AudioSample(at + 31),
                47,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(partial.samples, actual.samples[31..78]);
        let cold = StageAudio::new(Arc::clone(&cropped))
            .read_source_voice_occurrence(
                &mut provider,
                voice,
                AudioSample(at),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(cold.samples, actual.samples);
        let saved = renderer
            .read_beat_sound_owner_input(
                &mut provider,
                &input,
                AudioSample(at),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        let gain = 10.0_f64.powf(-3000.0 / 20_000.0);
        let expected: Vec<_> = actual
            .samples
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let local = at + index as i64 + offset;
                let edge = if case == 0 && local < 2048 {
                    ((2 * (2047 - local) + 1) as f32 / 192.0).min(1.0)
                } else if case == 0 && local >= 2304 {
                    ((2 * (local - 2304) + 1) as f32 / 192.0).min(1.0)
                } else {
                    1.0
                };
                frame.map(|sample| {
                    let value = f64::from(sample * edge) * gain;
                    value + value
                })
            })
            .collect();
        assert_eq!(
            saved.samples, expected,
            "saved full input case={case}, at={at}"
        );
        assert_eq!(saved.suppressed, actual.suppressed);
        let partial = StageAudio::new(Arc::clone(&cropped))
            .read_beat_sound_owner_input(
                &mut provider,
                &input,
                AudioSample(at + 31),
                47,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(partial.samples, saved.samples[31..78]);
    }
    provider.unavailable = true;
    assert!(
        renderer
            .read_beat_sound_owner_input(
                &mut provider,
                &input,
                AudioSample(0),
                128,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
    assert!(
        renderer
            .read_source_voice_occurrence(
                &mut provider,
                voice,
                AudioSample(0),
                128,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}

#[test]
fn sound_owner_context_reads_cold_hidden_pcm_and_current_hold_gates() {
    compare_history(0);
}

#[test]
fn sound_owner_context_reads_full_nested_preserve_before_current_delivery() {
    compare_history(1);
}

#[test]
fn sound_owner_context_retains_selection_inside_the_enclosing_processor() {
    compare_history(2);
}
