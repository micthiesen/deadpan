use super::*;
use crate::{PlayOverride, ProjectFrame, RepeatInstance, RevisionId};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn node(duration: i64, kind: FrozenAudioKind) -> FrozenAudioNode {
    FrozenAudioNode {
        duration: frames(duration),
        edges: Default::default(),
        editorial_edges: Default::default(),
        kind,
    }
}

fn hold(duration: i64) -> FrozenAudioNode {
    node(
        duration,
        FrozenAudioKind::Hold {
            audio: ReferenceAudibility::Silence,
        },
    )
}

fn sequence(duration: i64, children: &[&str]) -> FrozenAudioNode {
    node(
        duration,
        FrozenAudioKind::Sequence {
            children: children.iter().map(|name| id(name)).collect(),
        },
    )
}

fn path(name: &str) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: Vec::new(),
    }
}

fn opaque_layout(pitch: PitchPolicy, exact: bool) -> FrozenAudioLayout {
    let derived_clocks = if exact {
        BTreeMap::from([(
            id("provider"),
            FrozenRetimeClock {
                extent: ExactRatio::integer(6),
                child_start: ExactRatio::new(1, 3).unwrap(),
                child_end: ExactRatio::new(29, 3).unwrap(),
            },
        )])
    } else {
        BTreeMap::new()
    };
    FrozenAudioLayout::admit(LayoutWire {
        root: id("root"),
        rate: FrameRate::new(30_000, 1001).unwrap(),
        nodes: BTreeMap::from([
            (id("root"), sequence(11, &["lead", "outer", "tail"])),
            (id("lead"), hold(3)),
            (id("outer"), sequence(6, &["provider"])),
            (
                id("provider"),
                node(
                    6,
                    FrozenAudioKind::Retime {
                        child: id("middle"),
                        mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(10)).unwrap(),
                        pitch,
                        purpose: RetimePurpose::Edit,
                    },
                ),
            ),
            (id("middle"), sequence(12, &["prefix", "owner", "suffix"])),
            (id("prefix"), hold(2)),
            (id("owner"), hold(8)),
            (id("suffix"), hold(2)),
            (id("tail"), hold(2)),
        ]),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
        audio_lineage: BTreeMap::new(),
        derived_clocks,
    })
    .unwrap()
}

fn repeated_layout() -> (FrozenAudioLayout, IterationOrder) {
    let iterations = IterationOrder::from_segments(
        (0..16).map(|index| (RevisionId::new(format!("plays-{index}")).unwrap(), 0, 2)),
    )
    .unwrap();
    let overrides = PlayOverrides::try_from(vec![PlayOverride {
        iteration: iterations.at(3).unwrap(),
        root: id("alternate"),
    }])
    .unwrap();
    let gap_overrides = PlayOverrides::try_from(vec![PlayOverride {
        iteration: iterations.at(9).unwrap(),
        root: id("gap-alternate"),
    }])
    .unwrap();
    let durations = BTreeMap::from([
        (id("follow"), frames(5)),
        (id("alternate"), frames(3)),
        (id("gap-alternate"), frames(2)),
    ]);
    let repeat = RepeatLayout::compile_with_gap_overrides(
        &iterations,
        &id("follow"),
        Some(&overrides),
        frames(1),
        Some(&gap_overrides),
        &durations,
    )
    .unwrap();
    let repeat_frames = repeat.duration().frames();
    let layout = FrozenAudioLayout::admit(LayoutWire {
        root: id("root"),
        rate: FrameRate::new(30_000, 1001).unwrap(),
        nodes: BTreeMap::from([
            (
                id("root"),
                sequence(repeat_frames + 5, &["lead", "repeat", "tail"]),
            ),
            (id("lead"), hold(2)),
            (
                id("repeat"),
                node(
                    repeat_frames,
                    FrozenAudioKind::Repeat {
                        child: id("follow"),
                        iterations: iterations.clone(),
                        gap_duration: frames(1),
                        gap_audio: ReferenceAudibility::Silence,
                    },
                ),
            ),
            (
                id("follow"),
                node(
                    5,
                    FrozenAudioKind::Retime {
                        child: id("owner"),
                        mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(8)).unwrap(),
                        pitch: PitchPolicy::FollowSpeed,
                        purpose: RetimePurpose::Edit,
                    },
                ),
            ),
            (id("owner"), hold(8)),
            (id("alternate"), hold(3)),
            (id("gap-alternate"), hold(2)),
            (id("tail"), hold(3)),
        ]),
        overrides: BTreeMap::from([(id("repeat"), overrides)]),
        gap_overrides: BTreeMap::from([(id("repeat"), gap_overrides)]),
        audio_lineage: BTreeMap::new(),
        derived_clocks: BTreeMap::new(),
    })
    .unwrap();
    (layout, iterations)
}

fn repeated_path(name: &str, iterations: &IterationOrder, play: u32) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iterations.at(play).unwrap(),
        }],
    }
}

fn assert_joined(
    layout: &FrozenAudioLayout,
    instance: &InstancePath,
    context: Option<&InstancePath>,
    fallback: bool,
) {
    let old = layout
        .sound_occurrence_placement(instance, context, MAX_DOCUMENT_NODES)
        .unwrap();
    let separate = context.map(|context| {
        layout
            .project(context, ExactRatio::ZERO, None, MAX_DOCUMENT_NODES)
            .unwrap()
    });
    let (joined, clock) = layout
        .sound_occurrence_placement_and_context(instance, context, MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(joined.owner_origin, old.owner_origin);
    assert_eq!(joined.allocation, old.allocation);
    assert_eq!(joined.delivery_start, old.delivery_start);
    assert_eq!(
        clock,
        separate
            .as_ref()
            .map(FrozenSoundContextClock::from_projection)
    );
    let expected_work = match &separate {
        None => old.work,
        Some(projection) if fallback => old.work + projection.work + 1,
        Some(_) => old.work + 1,
    };
    assert_eq!(joined.work, expected_work);
    if let Some(projection) = &separate
        && !fallback
        && projection.work > 1
    {
        assert!(joined.work < old.work + projection.work);
    }
    assert_eq!(
        layout
            .sound_occurrence_placement_and_context(instance, context, joined.work)
            .unwrap(),
        (joined.clone(), clock),
    );
    assert!(
        layout
            .sound_occurrence_placement_and_context(instance, context, joined.work - 1)
            .is_err()
    );
    assert_eq!(
        layout
            .sound_occurrence_placement(instance, context, old.work)
            .unwrap(),
        old
    );
    assert!(
        layout
            .sound_occurrence_placement(instance, context, old.work - 1)
            .is_err()
    );
}

#[test]
fn joined_context_clock_reuses_owner_provider_and_higher_projections() {
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: 7 }] {
        let layout = opaque_layout(pitch, false);
        for context in ["owner", "provider", "outer", "root"] {
            assert_joined(&layout, &path("owner"), Some(&path(context)), false);
        }
        assert_joined(&layout, &path("owner"), None, false);
        assert_joined(&layout, &path("root"), Some(&path("root")), false);
    }
}

#[test]
fn joined_context_clock_below_opaque_provider_projects_once_after_placement() {
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: -7 }] {
        let layout = opaque_layout(pitch, false);
        assert_joined(&layout, &path("owner"), Some(&path("middle")), true);
    }
}

#[test]
fn old_sound_placement_api_retains_original_path_work() {
    let layout = opaque_layout(PitchPolicy::Preserve, false);
    // Four ancestor edges; owner project costs five, provider project three,
    // outer project two and root project one. The existing placement also
    // charges its chain, context membership and constructed projection paths.
    assert_eq!(
        layout
            .sound_occurrence_placement(&path("owner"), None, MAX_DOCUMENT_NODES)
            .unwrap()
            .work,
        18
    );
    for context in ["owner", "middle", "provider", "outer", "root"] {
        assert_eq!(
            layout
                .sound_occurrence_placement(
                    &path("owner"),
                    Some(&path(context)),
                    MAX_DOCUMENT_NODES
                )
                .unwrap()
                .work,
            23
        );
    }
    assert_eq!(
        layout
            .sound_occurrence_placement(&path("root"), Some(&path("root")), MAX_DOCUMENT_NODES)
            .unwrap()
            .work,
        2
    );
}

#[test]
fn joined_context_clock_preserves_fractional_and_stable_repeat_geometry() {
    let exact = opaque_layout(PitchPolicy::Preserve, true);
    assert_joined(&exact, &path("owner"), Some(&path("owner")), false);
    assert_joined(&exact, &path("owner"), Some(&path("middle")), true);
    let (_, clock) = exact
        .sound_occurrence_placement_and_context(
            &path("owner"),
            Some(&path("owner")),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(
        clock.unwrap().frames_per_local_frame,
        ExactRatio::new(9, 14).unwrap()
    );

    let (layout, iterations) = repeated_layout();
    let owner = repeated_path("owner", &iterations, 10);
    for context in [
        owner.clone(),
        repeated_path("follow", &iterations, 10),
        path("repeat"),
        path("root"),
    ] {
        assert_joined(&layout, &owner, Some(&context), false);
    }
    let (_, clock) = layout
        .sound_occurrence_placement_and_context(&owner, Some(&owner), MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(
        clock.unwrap().frames_per_local_frame,
        ExactRatio::new(5, 7).unwrap()
    );
    let gap = repeated_path("gap-alternate", &iterations, 9);
    assert_joined(&layout, &gap, Some(&gap), false);
}

#[test]
fn joined_context_clock_refuses_foreign_repeat_branches_and_invalid_budgets() {
    let (layout, iterations) = repeated_layout();
    let owner = repeated_path("owner", &iterations, 10);
    for context in [
        repeated_path("follow", &iterations, 11),
        repeated_path("alternate", &iterations, 3),
        path("missing"),
    ] {
        assert!(
            layout
                .sound_occurrence_placement(&owner, Some(&context), MAX_DOCUMENT_NODES)
                .is_err()
        );
        assert!(
            layout
                .sound_occurrence_placement_and_context(&owner, Some(&context), MAX_DOCUMENT_NODES)
                .is_err()
        );
    }
    let mut wrong_repeat = owner.clone();
    wrong_repeat.repeats[0].node = id("outer-repeat");
    for instance in [
        path("owner"),
        repeated_path("owner", &iterations, 3),
        wrong_repeat,
    ] {
        assert!(
            layout
                .sound_occurrence_placement(&instance, None, MAX_DOCUMENT_NODES)
                .is_err()
        );
        assert!(
            layout
                .sound_occurrence_placement_and_context(&instance, None, MAX_DOCUMENT_NODES)
                .is_err()
        );
    }
    for maximum in [0, MAX_DOCUMENT_NODES + 1] {
        assert!(
            layout
                .sound_occurrence_placement(&owner, Some(&owner), maximum)
                .is_err()
        );
        assert!(
            layout
                .sound_occurrence_placement_and_context(&owner, Some(&owner), maximum)
                .is_err()
        );
    }
}
