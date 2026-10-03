use std::collections::BTreeMap;

use deadpan_core::*;

use super::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn repeat(child: &str, count: u32, gap: i64) -> BeatNode {
    let mut node = hold(1);
    node.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision("plays"), count).unwrap(),
        gap: (gap > 0).then(|| HoldRecipe {
            duration: FrameDuration::new(gap).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        }),
    };
    node
}
fn retime(child: &str, duration: i64, range: Range<i64>, pitch: PitchPolicy) -> BeatNode {
    let mut node = hold(duration);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: FrameDuration::new(duration).unwrap(),
        mapping: FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap(),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    node
}
fn document(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    document_at(FrameRate::new(48_000, 1).unwrap(), children, nodes)
}
fn document_at(
    rate: FrameRate,
    children: &[&str],
    nodes: Vec<(&str, BeatNode)>,
) -> ProjectDocument {
    let base = ProjectDocument::new(
        ProjectId::new("owner-occurrences").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|child| id(child)).collect()),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn plan(doc: ProjectDocument) -> RenderPlan {
    RenderPlan::compile(&doc).unwrap()
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision("plays"),
        ordinal,
    }
}
fn bind_clock(current: &ProjectDocument, old: &ProjectDocument, owner: &str) -> ProjectDocument {
    let timing = AudioTimingId {
        allocation: revision("retained-clock"),
        ordinal: 0,
    };
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(old).unwrap(),
        }],
        BTreeMap::from([(
            id(owner),
            OwnedAudioBinding {
                reanchors: vec![],
                lattice: AudioPlacementTemplate {
                    reference_local_offset: ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing,
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: id(owner),
                    },
                    arguments: vec![],
                    births: vec![],
                },
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ZERO,
                    phase: AudioLocalPhase {
                        constant: q(1, 3),
                        terms: vec![],
                    },
                }),
            },
        )]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn nested_repeats_return_concrete_stable_paths_and_exact_maps() {
    let plan = plan(document(
        &["outer"],
        vec![
            ("owner", hold(1)),
            ("inner", repeat("owner", 3, 1)),
            ("outer", repeat("inner", 2, 0)),
        ],
    ));
    let query = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(4)..AudioSample(6),
            Default::default(),
        )
        .unwrap();
    let paths: Vec<_> = query
        .occurrences()
        .iter()
        .map(|occurrence| occurrence.instance().clone())
        .collect();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].repeats[0].iteration, play(0));
    assert_eq!(paths[0].repeats[1].iteration, play(2));
    assert_eq!(paths[1].repeats[0].iteration, play(1));
    assert_eq!(paths[1].repeats[1].iteration, play(0));
    assert_eq!(query.occurrences()[0].visible_root_frames, q(4, 1)..q(5, 1));
    assert_eq!(query.occurrences()[1].map.root_frame_at_local_zero, q(5, 1));
}

#[test]
fn huge_repeat_query_visits_only_plays_overlapping_the_small_window() {
    let plan = plan(document(
        &["repeat"],
        vec![
            ("owner", hold(1)),
            ("repeat", repeat("owner", 1_000_000, 1)),
        ],
    ));
    let query = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(1_999_990)..AudioSample(1_999_994),
            AudioQueryLimits {
                maximum_spans: 4,
                maximum_work: 64,
            },
        )
        .unwrap();
    assert_eq!(query.occurrences().len(), 2);
    assert!(query.work() < 64);
    assert_eq!(
        query.occurrences()[0].instance().repeats[0].iteration,
        play(999_995)
    );
}

#[test]
fn retime_maps_exactly_and_preserve_exposes_full_stage_influence() {
    let followed = plan(document(
        &["prefix", "crop"],
        vec![
            ("prefix", hold(2)),
            ("owner", hold(8)),
            ("crop", retime("owner", 2, 2..6, PitchPolicy::FollowSpeed)),
        ],
    ));
    let query = followed
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(2)..AudioSample(4),
            Default::default(),
        )
        .unwrap();
    let occurrence = &query.occurrences()[0];
    assert_eq!(occurrence.map.root_frame_at_local_zero, q(1, 1));
    assert_eq!(occurrence.map.root_frames_per_local_frame, q(1, 2));
    assert_eq!(occurrence.visible_root_frames, q(2, 1)..q(4, 1));

    let preserved = plan(document(
        &["prefix", "stage"],
        vec![
            ("prefix", hold(2)),
            ("owner", hold(8)),
            ("stage", retime("owner", 2, 2..6, PitchPolicy::Preserve)),
        ],
    ));
    let query = preserved
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(2)..AudioSample(4),
            Default::default(),
        )
        .unwrap();
    let occurrence = &query.occurrences()[0];
    assert_eq!(occurrence.influence_root_frames, q(2, 1)..q(4, 1));
    assert_eq!(
        occurrence.influence_root_samples,
        AudioSample(2)..AudioSample(4)
    );
}

#[test]
fn preserve_voice_survives_a_later_crop_that_removes_its_geometric_interval() {
    let plan = plan(document(
        &["outer"],
        vec![
            ("owner", hold(1)),
            ("tail", hold(7)),
            (
                "input",
                BeatNode::sequence("Input", vec![id("owner"), id("tail")]),
            ),
            ("stage", retime("input", 4, 0..8, PitchPolicy::Preserve)),
            ("outer", retime("stage", 2, 2..4, PitchPolicy::FollowSpeed)),
        ],
    ));
    let query = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(0)..AudioSample(2),
            Default::default(),
        )
        .unwrap();
    assert_eq!(query.occurrences().len(), 1);
    let occurrence = &query.occurrences()[0];
    assert!(!positive(&occurrence.visible_root_frames).unwrap());
    assert_eq!(occurrence.influence_root_frames, q(0, 1)..q(2, 1));
    assert_eq!(
        occurrence.influence_root_samples,
        AudioSample(0)..AudioSample(2)
    );
}

#[test]
fn occurrence_query_uses_current_structure_not_retained_original_bindings() {
    let old = document(&["owner"], vec![("owner", hold(3))]);
    let current = document(
        &["prefix", "owner"],
        vec![("prefix", hold(1)), ("owner", hold(3))],
    );
    let current = bind_clock(&current, &old, "owner");
    let plan = plan(current);
    let occurrence = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(2)..AudioSample(3),
            Default::default(),
        )
        .unwrap();
    assert_eq!(occurrence.occurrences().len(), 1);
    assert_eq!(
        occurrence.occurrences()[0].visible_root_frames,
        q(1, 1)..q(4, 1)
    );
    assert_eq!(
        occurrence.occurrences()[0].map.root_frame_at_local_zero,
        q(1, 1)
    );
}

#[test]
fn visible_sample_boundaries_use_canonical_round_even_grid() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = plan(document_at(
        rate,
        &["prefix", "owner"],
        vec![("prefix", hold(1)), ("owner", hold(1))],
    ));
    let expected = rate.audio_boundary(ProjectFrame(1)).unwrap()
        ..rate.audio_boundary(ProjectFrame(2)).unwrap();
    let query = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(1600)..AudioSample(3203),
            Default::default(),
        )
        .unwrap();
    assert_eq!(query.occurrences().len(), 1);
    assert_eq!(query.occurrences()[0].visible_root_samples, expected);
}

#[test]
fn repeat_overrides_are_live_but_default_synthetic_gaps_are_not_owners() {
    let doc = document(
        &["repeat", "override", "gap-override"],
        vec![
            ("base", hold(1)),
            ("override", hold(2)),
            ("gap-override", hold(1)),
            ("repeat", repeat("base", 3, 1)),
        ],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["repeat"]);
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("override"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(0),
            root: id("gap-override"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let plan = plan(ProjectDocument::from_json(&wire.to_string()).unwrap());
    let owner = plan
        .audio_owner_occurrences(
            &id("override"),
            AudioSample(1)..AudioSample(3),
            Default::default(),
        )
        .unwrap();
    assert_eq!(owner.occurrences().len(), 1);
    assert_eq!(
        owner.occurrences()[0].instance().repeats[0].iteration,
        play(1)
    );
    let gap = plan
        .audio_owner_occurrences(
            &id("gap-override"),
            AudioSample(1)..AudioSample(3),
            Default::default(),
        )
        .unwrap();
    assert_eq!(gap.occurrences().len(), 1);
    assert_eq!(
        gap.occurrences()[0].instance().repeats[0].iteration,
        play(0)
    );
    assert!(
        plan.audio_owner_occurrences(
            &id("repeat"),
            AudioSample(0)..AudioSample(5),
            Default::default()
        )
        .unwrap()
        .occurrences()
        .iter()
        .all(|occurrence| occurrence.instance().node == id("repeat"))
    );
}

#[test]
fn sparse_override_in_preserve_skips_unrelated_million_play_runs() {
    let doc = document(
        &["stage", "override"],
        vec![
            ("base", hold(1)),
            ("override", hold(1)),
            ("repeat", repeat("base", 1_000_000, 0)),
            (
                "stage",
                retime("repeat", 500_000, 0..1_000_000, PitchPolicy::Preserve),
            ),
        ],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["stage"]);
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(900_000),
            root: id("override"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let plan = plan(ProjectDocument::from_json(&wire.to_string()).unwrap());
    let query = plan
        .audio_owner_occurrences(
            &id("override"),
            AudioSample(0)..AudioSample(1),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 64,
            },
        )
        .unwrap();
    assert_eq!(query.occurrences().len(), 1);
    assert_eq!(
        query.occurrences()[0].instance.repeats[0].iteration,
        play(900_000)
    );
    assert_eq!(
        query.occurrences()[0].influence_root_samples,
        AudioSample(0)..AudioSample(500_000)
    );
    assert!(query.work() < 64);
}

#[test]
fn an_interior_zero_sample_allocation_is_not_an_audible_occurrence() {
    let plan = plan(document(
        &["prefix", "stage"],
        vec![
            ("prefix", hold(1)),
            ("before", hold(1)),
            ("owner", hold(1)),
            ("after", hold(3)),
            (
                "contents",
                BeatNode::sequence("Contents", vec![id("before"), id("owner"), id("after")]),
            ),
            (
                "stage",
                retime("contents", 1, 0..5, PitchPolicy::FollowSpeed),
            ),
        ],
    ));
    // Owner maps to root [1.2,1.4), whose RoundEven allocation is [1,1).
    let query = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(0)..AudioSample(2),
            Default::default(),
        )
        .unwrap();
    assert!(query.occurrences().is_empty());
}

#[test]
fn a_zero_sample_preserve_output_does_not_expand_its_repeat_input() {
    let plan = plan(document(
        &["prefix", "outer"],
        vec![
            ("prefix", hold(1)),
            ("before", hold(1)),
            ("after", hold(3)),
            ("owner", hold(1)),
            ("repeat", repeat("owner", 1_000_000, 0)),
            (
                "preserve",
                retime("repeat", 1, 0..1_000_000, PitchPolicy::Preserve),
            ),
            (
                "contents",
                BeatNode::sequence("Contents", vec![id("before"), id("preserve"), id("after")]),
            ),
            (
                "outer",
                retime("contents", 1, 0..5, PitchPolicy::FollowSpeed),
            ),
        ],
    ));
    // The complete Preserve stage maps to [1.2,1.4), which allocates no root
    // samples. None of its million input occurrences need to be enumerated.
    let query = plan
        .audio_owner_occurrences(
            &id("owner"),
            AudioSample(0)..AudioSample(2),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 64,
            },
        )
        .unwrap();
    assert!(query.occurrences().is_empty());
    assert!(query.work() < 64);
}

#[test]
fn limits_and_invalid_ranges_fail_without_partial_results() {
    let plan = plan(document(
        &["repeat"],
        vec![("owner", hold(1)), ("repeat", repeat("owner", 8, 0))],
    ));
    assert!(matches!(
        plan.audio_owner_occurrences(
            &id("owner"),
            AudioSample(0)..AudioSample(8),
            AudioQueryLimits {
                maximum_spans: 3,
                maximum_work: 64,
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(
        plan.audio_owner_occurrences(
            &id("owner"),
            AudioSample(-1)..AudioSample(1),
            Default::default()
        )
        .is_err()
    );
    assert!(
        plan.audio_owner_occurrences(
            &id("owner"),
            AudioSample(2)..AudioSample(1),
            Default::default()
        )
        .is_err()
    );
    assert!(
        plan.audio_owner_occurrences(
            &id("absent"),
            AudioSample(0)..AudioSample(1),
            Default::default()
        )
        .is_err()
    );
}
