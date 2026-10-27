use super::*;
use deadpan_plan::{AudioQueryLimits, AudioSoundOwnerContext};
use std::ops::Range;

fn oracle(
    plan: &RenderPlan,
    owner: &str,
    recipe: AudioSourceVoiceRecipe,
    samples: Range<AudioSample>,
    context: Option<&AudioSoundOwnerContext<'_>>,
) -> (usize, usize) {
    let full = match context {
        Some(context) => context.source_voice_occurrences(
            &id(owner),
            recipe.clone(),
            samples.clone(),
            Default::default(),
        ),
        None => plan.source_voice_occurrences(
            &id(owner),
            recipe.clone(),
            samples.clone(),
            Default::default(),
        ),
    }
    .unwrap();
    let found = match context {
        Some(context) => context.source_voice_occurrence_supports(
            &id(owner),
            recipe,
            samples,
            Default::default(),
        ),
        None => {
            plan.source_voice_occurrence_supports(&id(owner), recipe, samples, Default::default())
        }
    }
    .unwrap();
    let expected: Vec<_> = full
        .voices()
        .iter()
        .map(|voice| (voice.instance().clone(), voice.samples()))
        .collect();
    let actual: Vec<_> = found
        .supports()
        .iter()
        .map(|support| (support.instance().clone(), support.samples()))
        .collect();
    assert_eq!(actual, expected);
    (found.construction_work(), full.construction_work())
}

fn context_at<'p>(plan: &'p RenderPlan, owner: &str, at: i64) -> AudioSoundOwnerContext<'p> {
    let query = plan
        .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap();
    let clock = query
        .spans()
        .iter()
        .flat_map(|span| span.owners())
        .find(|clock| clock.instance().node == id(owner))
        .unwrap();
    plan.audio_sound_owner_context(clock, Default::default())
        .unwrap()
}

#[test]
fn support_discovery_matches_root_and_scoped_queries_without_construction_graphs() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["prefix", "owner", "tail"],
        [
            ("prefix", source(3)),
            ("owner", source(12)),
            ("tail", source(5)),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let context = context_at(&plan, "owner", 4);
    for samples in [0..0, 0..3, 2..4, 3..15, 14..16, 15..20] {
        oracle(
            &plan,
            "owner",
            recipe(&plan, q(1, 1), 4),
            AudioSample(samples.start)..AudioSample(samples.end),
            None,
        );
    }
    let (light, full) = oracle(
        &plan,
        "owner",
        recipe(&plan, q(1, 1), 4),
        context.samples(),
        Some(&context),
    );
    assert!(light < full, "support={light}, full={full}");
    let outside = context
        .source_voice_occurrence_supports(
            &id("tail"),
            recipe(&plan, q(0, 1), 4),
            context.samples(),
            Default::default(),
        )
        .unwrap();
    assert!(outside.supports().is_empty());
}

#[test]
fn support_discovery_preserve_hidden_history_uses_full_validated_fallback() {
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["crop"],
        [
            ("owner", source(1)),
            ("tail", source(7)),
            (
                "input",
                BeatNode::sequence("Input", vec![id("owner"), id("tail")]),
            ),
            (
                "stage",
                retime("input", 4, 0, 8, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
            (
                "crop",
                retime(
                    "stage",
                    2,
                    2,
                    4,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
        BTreeMap::new(),
    )))
    .unwrap();
    let (light, full) = oracle(
        &plan,
        "owner",
        recipe(&plan, q(0, 1), 1),
        AudioSample(0)..AudioSample(2),
        None,
    );
    assert_eq!(
        light, full,
        "fallback reuses its resolved path and exact work budget"
    );
    let found = plan
        .source_voice_occurrence_supports(
            &id("owner"),
            recipe(&plan, q(0, 1), 1),
            AudioSample(1)..AudioSample(2),
            Default::default(),
        )
        .unwrap();
    assert_eq!(found.supports().len(), 1);
    let context = context_at(&plan, "stage", 0);
    oracle(
        &plan,
        "owner",
        recipe(&plan, q(0, 1), 1),
        context.samples(),
        Some(&context),
    );
    // An event owned on the processor enters after it; only ancestors process it.
    oracle(
        &plan,
        "stage",
        recipe(&plan, q(0, 1), 1),
        AudioSample(0)..AudioSample(2),
        None,
    );
}

#[test]
fn support_discovery_matches_stable_repeat_overrides_and_compact_lookup() {
    let overrides = BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("other"),
        }])
        .unwrap(),
    )]);
    let plan = RenderPlan::compile(&catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        [
            ("owner", source(8)),
            ("other", source(12)),
            ("repeat", repeat("owner", 1_000_000)),
        ],
        overrides,
    )))
    .unwrap();
    for (owner, samples) in [
        ("owner", 0..24),
        ("other", 0..24),
        ("owner", 7_999_996..8_000_004),
    ] {
        oracle(
            &plan,
            owner,
            recipe(&plan, q(1, 1), 4),
            AudioSample(samples.start)..AudioSample(samples.end),
            None,
        );
    }
    let context = context_at(&plan, "owner", 7_999_999);
    let (light, full) = oracle(
        &plan,
        "owner",
        recipe(&plan, q(1, 1), 4),
        context.samples(),
        Some(&context),
    );
    assert!(light < full, "support={light}, full={full}");
}

#[test]
fn support_discovery_preserves_fractional_44100_recipe_and_scoped_offsets() {
    let original = catalog(document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["crop"],
        [
            ("owner", source(16)),
            (
                "crop",
                retime(
                    "owner",
                    8,
                    2,
                    14,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
        ],
        BTreeMap::new(),
    ));
    let base = SourceTimeBase::new(1, 44_100).unwrap();
    let timestamp = |ticks| SourceTimestamp {
        ticks,
        time_base: base,
    };
    let mut wire = serde_json::to_value(original).unwrap();
    wire["assets"]["sound"]["audio"] =
        serde_json::to_value(SourceSpan::new(timestamp(0), timestamp(44_100)).unwrap()).unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let source = SourceAudio {
        asset: AssetId::new("sound").unwrap(),
        span: SourceSpan::new(timestamp(3100), timestamp(16000)).unwrap(),
    };
    let frames = SourceAudioMapping::natural_rate(
        source.span,
        plan.metadata().presentation_basis.frame_rate,
    )
    .unwrap()
    .duration_frames(FrameDuration::ZERO)
    .unwrap();
    let recipe = AudioSourceVoiceRecipe {
        source,
        mapping: SourceAudioMapping::Placement {
            start: q(1, 3),
            frames,
        },
        offset: AudioSample(37),
    };
    let context = context_at(&plan, "owner", 100);
    for samples in [0..1, 1..127, 1599..1603, 6400..6401] {
        oracle(
            &plan,
            "owner",
            recipe.clone(),
            AudioSample(samples.start)..AudioSample(samples.end),
            None,
        );
    }
    oracle(&plan, "owner", recipe, context.samples(), Some(&context));
}

#[test]
fn support_discovery_keeps_round_even_zero_sample_allocations() {
    for prefix in [0, 1] {
        let mut nodes = vec![
            ("owner", source(1)),
            ("tail", source(1)),
            (
                "input",
                BeatNode::sequence("Input", vec![id("owner"), id("tail")]),
            ),
            (
                "stage",
                retime(
                    "input",
                    1,
                    0,
                    2,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Edit,
                ),
            ),
        ];
        let children = if prefix == 0 {
            vec!["stage"]
        } else {
            nodes.push(("prefix", source(1)));
            vec!["prefix", "stage"]
        };
        let plan = RenderPlan::compile(&catalog(document(
            FrameRate::new(48_000, 1).unwrap(),
            &children,
            nodes,
            BTreeMap::new(),
        )))
        .unwrap();
        oracle(
            &plan,
            "owner",
            recipe(&plan, q(0, 1), 1),
            AudioSample(prefix)..AudioSample(prefix + 1),
            None,
        );
        let found = plan
            .source_voice_occurrence_supports(
                &id("owner"),
                recipe(&plan, q(0, 1), 1),
                AudioSample(prefix)..AudioSample(prefix + 1),
                Default::default(),
            )
            .unwrap();
        assert_eq!(found.supports().len(), prefix as usize);
    }
}

#[test]
fn support_discovery_rejects_invalid_empty_recipes_and_aggregate_limits() {
    let doc = catalog(document(
        FrameRate::new(48_000, 1).unwrap(),
        &["owner", "other"],
        [("owner", source(12)), ("other", source(12))],
        BTreeMap::new(),
    ));
    let plan = RenderPlan::compile(&doc).unwrap();
    let mut missing = recipe(&plan, q(0, 1), 4);
    missing.source.asset = AssetId::new("missing").unwrap();
    let mut nonnatural = recipe(&plan, q(0, 1), 4);
    nonnatural.mapping = SourceAudioMapping::FitBeat;
    let mut out_of_catalog = recipe(&plan, q(0, 1), 4);
    let base = SourceTimeBase::new(1, 48_000).unwrap();
    out_of_catalog.source.span = SourceSpan::new(
        SourceTimestamp {
            ticks: 48_010,
            time_base: base,
        },
        SourceTimestamp {
            ticks: 48_014,
            time_base: base,
        },
    )
    .unwrap();
    let context = context_at(&plan, "other", 13);
    for bad in [
        missing,
        nonnatural,
        out_of_catalog,
        recipe(&plan, q(11, 1), 4),
    ] {
        assert!(
            plan.source_voice_occurrence_supports(
                &id("owner"),
                bad.clone(),
                AudioSample(0)..AudioSample(0),
                Default::default()
            )
            .is_err()
        );
        assert!(
            context
                .source_voice_occurrence_supports(
                    &id("owner"),
                    bad,
                    context.samples(),
                    Default::default()
                )
                .is_err()
        );
    }
    for limits in [
        AudioQueryLimits {
            maximum_work: 1,
            ..Default::default()
        },
        AudioQueryLimits {
            maximum_spans: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            plan.source_voice_occurrence_supports(
                &id("owner"),
                recipe(&plan, q(0, 1), 4),
                AudioSample(0)..AudioSample(12),
                limits
            ),
            Err(PlanError::AudioQueryLimit(_))
        ));
    }
}
