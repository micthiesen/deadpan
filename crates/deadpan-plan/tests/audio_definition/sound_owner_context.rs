use super::*;

fn context<'a>(
    plan: &'a RenderPlan,
    node: &str,
    at: i64,
) -> deadpan_plan::AudioSoundOwnerContext<'a> {
    let query = plan
        .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap();
    let owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id(node))
        .unwrap();
    plan.audio_sound_owner_context(owner, Default::default())
        .unwrap()
}

#[test]
fn sound_owner_context_restores_hidden_current_sound_and_hold_history() {
    let nodes = [
        ("a", source(6)),
        ("pause", hold(4)),
        ("b", source(6)),
        (
            "group",
            BeatNode::sequence("Group", vec![id("a"), id("pause"), id("b")]),
        ),
    ];
    let whole = RenderPlan::compile(&catalog(document(
        FrameRate::new(48000, 1).unwrap(),
        &["group"],
        nodes.clone(),
        BTreeMap::new(),
    )))
    .unwrap();
    let cropped = RenderPlan::compile(&catalog(document(
        FrameRate::new(48000, 1).unwrap(),
        &["crop"],
        nodes.into_iter().chain([(
            "crop",
            retime(
                "group",
                8,
                4,
                12,
                PitchPolicy::FollowSpeed,
                RetimePurpose::Partition,
            ),
        )]),
        BTreeMap::new(),
    )))
    .unwrap();
    let input = context(&cropped, "group", 0);
    assert_eq!(input.samples(), AudioSample(-4)..AudioSample(12));
    let batch = input
        .source_voice_occurrences(
            &id("group"),
            recipe(&cropped, q(0, 1), 16),
            input.samples(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 1);
    let hidden = &batch.voices()[0];
    assert_eq!(hidden.samples(), input.samples());
    let reference = whole
        .source_voice_occurrence(
            occurrence("group", &[]),
            recipe(&whole, q(0, 1), 16),
            Default::default(),
        )
        .unwrap();
    for at in -4..12 {
        let query = hidden
            .processing(AudioSample(at)..AudioSample(at + 1), Default::default())
            .unwrap();
        assert_eq!(
            query.spans[0].source_point(AudioSample(at)).unwrap().ticks,
            q(at as i128 + 14, 1)
        );
    }
    let actual = hidden
        .gate_fades(
            AudioEdgePolicy::Automatic,
            AudioEdgePolicy::Automatic,
            input.samples(),
            Default::default(),
        )
        .unwrap();
    let expected = reference
        .gate_fades(
            AudioEdgePolicy::Automatic,
            AudioEdgePolicy::Automatic,
            reference.samples(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(actual.spans.len(), expected.spans.len());
    for (actual, mut expected) in actual.spans.into_iter().zip(expected.spans) {
        expected.samples.start.0 -= 4;
        expected.samples.end.0 -= 4;
        assert_eq!(actual, expected);
    }
    let holds = hidden
        .hold_policy(input.samples(), Default::default())
        .unwrap();
    assert_eq!(holds.rules.len(), 1);
    assert_eq!(holds.rules[0].samples, AudioSample(2)..AudioSample(6));
    let live = cropped
        .source_voice_occurrence(
            occurrence("group", &[]),
            recipe(&cropped, q(0, 1), 16),
            Default::default(),
        )
        .unwrap();
    assert_eq!(live.samples(), AudioSample(0)..AudioSample(8));
    assert!(
        live.processing(AudioSample(-1)..AudioSample(0), Default::default())
            .is_err()
    );
}

#[test]
fn sound_owner_context_keeps_enclosing_preserve_and_scopes_repeat_input() {
    let count = 1_000_000_000_u32;
    let ordinal = 900_000_000_u32;
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48000, 1).unwrap(),
        &["crop"],
        [
            ("child", source(8)),
            ("repeat", repeat("child", count)),
            (
                "stage",
                retime(
                    "repeat",
                    4 * i64::from(count),
                    0,
                    8 * i64::from(count),
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "crop",
                retime(
                    "stage",
                    4,
                    4 * i64::from(ordinal),
                    4 * i64::from(ordinal) + 4,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let input = context(&plan, "child", 0);
    assert_eq!(input.owner(), &occurrence("child", &[("repeat", ordinal)]));
    let batch = input
        .source_voice_occurrences(
            &id("child"),
            recipe(&plan, q(0, 1), 4),
            AudioSample(0)..AudioSample(1),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 1);
    assert!(
        batch.construction_work() < 1000,
        "{}",
        batch.construction_work()
    );
    let voice = &batch.voices()[0];
    assert_eq!(voice.instance(), input.owner());
    assert_eq!(voice.samples(), input.samples());
    assert!(voice.samples().start.0 < 0 && voice.samples().end.0 > 4);
    let projection = projected(voice);
    assert_eq!(projection.stage().descriptor().instance.node, id("stage"));
    assert_eq!(
        projection.input_sample_count().unwrap(),
        SignalSample(8 * i64::from(count))
    );
}

#[test]
fn sound_owner_context_keeps_fractional_current_phase_on_the_absolute_grid() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(30000, 1001).unwrap(),
        &["crop"],
        [
            ("owner", source(3)),
            (
                "crop",
                retime(
                    "owner",
                    2,
                    1,
                    3,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let input = context(&plan, "owner", 0);
    assert_eq!(input.samples(), AudioSample(-1602)..AudioSample(3203));
    let batch = input
        .source_voice_occurrences(
            &id("owner"),
            recipe(&plan, q(0, 1), 4804),
            input.samples(),
            Default::default(),
        )
        .unwrap();
    let voice = &batch.voices()[0];
    let sample = voice
        .processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    assert_eq!(
        sample.spans[0].source_point(AudioSample(0)).unwrap().ticks,
        q(8058, 5)
    );
    let gates = voice
        .gate_fades(
            AudioEdgePolicy::Hard,
            AudioEdgePolicy::Hard,
            voice.samples(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(gates.spans.len(), 2);
    assert_eq!(
        gates.spans[0].samples,
        AudioSample(-1602)..AudioSample(3202)
    );
    assert_eq!(gates.spans[0].length, 4804);
    assert_eq!(gates.spans[1].samples, AudioSample(3202)..AudioSample(3203));
    assert_eq!(gates.spans[1].length, 0);
    assert!(input.construction_work() > 0 && batch.construction_work() > 0);
}

#[test]
fn sound_owner_context_rejects_foreign_owner_and_bounds_work_and_support() {
    let doc = catalog(document(
        FrameRate::new(48000, 1).unwrap(),
        &["a", "b"],
        [("a", source(8)), ("b", source(8))],
        BTreeMap::new(),
    ));
    let plan = RenderPlan::compile(&doc).unwrap();
    let other = RenderPlan::compile(&doc).unwrap();
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = query.spans()[0].owners().last().unwrap();
    assert!(
        other
            .audio_sound_owner_context(owner, Default::default())
            .is_err()
    );
    assert!(matches!(
        plan.audio_sound_owner_context(
            owner,
            AudioQueryLimits {
                maximum_work: 1,
                ..Default::default()
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    let input = context(&plan, "a", 0);
    assert!(
        input
            .source_voice_occurrences(
                &id("a"),
                recipe(&plan, q(0, 1), 4),
                AudioSample(-1)..AudioSample(1),
                Default::default()
            )
            .is_err()
    );
    let outside = input
        .source_voice_occurrences(
            &id("b"),
            recipe(&plan, q(0, 1), 4),
            input.samples(),
            Default::default(),
        )
        .unwrap();
    assert!(outside.voices().is_empty());
    assert!(
        input
            .source_voice_occurrences(
                &id("b"),
                recipe(&plan, q(7, 1), 4),
                input.samples(),
                Default::default()
            )
            .is_err()
    );
}
