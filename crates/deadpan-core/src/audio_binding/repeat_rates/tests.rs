use super::*;
use crate::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn play(ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision("plays"),
        ordinal,
    }
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(value),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn fixture(count: u32) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("repeat-rates").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let input = i64::from(count) * 3 - 1;
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: id("body"),
        iterations: IterationOrder::new(revision("plays"), count).unwrap(),
        gap: match hold(1).kind {
            NodeKind::Hold { recipe } => Some(recipe),
            _ => unreachable!(),
        },
        escalation: None,
    };
    let mut stage = BeatNode::sequence("Stage", vec![]);
    stage.kind = NodeKind::Retime {
        child: id("repeat"),
        duration: frames(input * 2),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    document.nodes = BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("stage")])),
        (id("stage"), stage),
        (id("repeat"), repeated),
        (id("body"), hold(2)),
    ])
    .into();
    document.validate().unwrap();
    document
}
fn order(document: &mut ProjectDocument, segments: Vec<(RevisionId, u32, u32)>) {
    let NodeKind::Repeat { iterations, .. } =
        &mut document.nodes.get_mut(&id("repeat")).unwrap().kind
    else {
        unreachable!()
    };
    *iterations = IterationOrder::from_segments(segments).unwrap();
}
fn captured(document: &ProjectDocument) -> AudioBindingState {
    capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap()
}
fn classes(
    document: &ProjectDocument,
    owner: &str,
    usage: RepeatOwnerUse,
    state: &AudioBindingState,
) -> (Vec<ResolvedAudioPlacement>, usize) {
    let binding = if matches!(usage, RepeatOwnerUse::DefaultGap) {
        &state.gap_bindings()[&id(owner)]
    } else {
        &state.bindings()[&id(owner)]
    };
    classes_with_current(
        document,
        &id("repeat"),
        &id(owner),
        &usage,
        &binding.lattice,
        &state.timings()[&binding.lattice.reference.timing],
        10_000,
    )
    .unwrap()
}

#[test]
fn million_plays_resolve_one_class_with_bounded_work_and_historical_units() {
    let document = fixture(1_000_000);
    let state = captured(&document);
    let (resolved, work) = classes(&document, "body", RepeatOwnerUse::DefaultBody, &state);
    assert_eq!(resolved.len(), 1);
    assert!(work < 256, "compact work: {work}");
    assert_eq!(resolved[0].frames_per_local_frame, ExactRatio::integer(2));
    assert_eq!(
        resolved[0].frames_per_sample,
        ExactRatio::new(5, 8008).unwrap()
    );
    assert_eq!(resolved[0].birth, None);
    let (gaps, gap_work) = classes(&document, "repeat", RepeatOwnerUse::DefaultGap, &state);
    assert_eq!(gaps.len(), 1);
    assert!(gap_work < 256, "compact gap work: {gap_work}");
    let binding = &state.bindings()[&id("body")].lattice;
    assert!(
        classes_with_current(
            &document,
            &id("repeat"),
            &id("body"),
            &RepeatOwnerUse::DefaultBody,
            binding,
            &state.timings()[&binding.reference.timing],
            work / 2
        )
        .is_err()
    );
}

#[test]
fn historical_override_and_new_allocation_each_split_birth_classes() {
    let mut old = fixture(5);
    old.nodes.insert(id("old-owned"), hold(4));
    old.overrides
        .entry(id("repeat"))
        .or_default()
        .insert(play(1), id("old-owned"));
    let NodeKind::Retime {
        duration, mapping, ..
    } = &mut old.nodes.get_mut(&id("stage")).unwrap().kind
    else {
        unreachable!()
    };
    *duration = frames(32);
    *mapping = FrameRange::new(ProjectFrame(0), ProjectFrame(16)).unwrap();
    old.validate().unwrap();
    let state = captured(&old);
    let mut current = old.clone();
    current.overrides.clear();
    current.nodes.remove(&id("old-owned"));
    order(
        &mut current,
        vec![(revision("plays"), 0, 5), (revision("new-plays"), 0, 2)],
    );
    current.validate().unwrap();
    let (classes, _) = classes(&current, "body", RepeatOwnerUse::DefaultBody, &state);
    assert_eq!(
        classes.iter().map(|class| class.birth).collect::<Vec<_>>(),
        vec![None, Some(0), None, Some(0)]
    );
    assert_eq!(
        classes
            .iter()
            .map(|class| class.frames_per_local_frame)
            .collect::<Vec<_>>(),
        vec![
            ExactRatio::integer(2),
            ExactRatio::integer(1),
            ExactRatio::integer(2),
            ExactRatio::integer(1)
        ]
    );
    assert_eq!(
        classes[1].clock,
        AudioClockRoot::DefinitionPointCeil { root: id("body") }
    );
    assert_eq!(classes[3].grid_rule, AudioBindingGridRule::PointCeil);
}

#[test]
fn gap_classes_split_old_overrides_and_old_final_after_current_reordering() {
    let mut old = fixture(5);
    old.nodes.insert(id("old-gap"), hold(1));
    old.gap_overrides
        .entry(id("repeat"))
        .or_default()
        .insert(play(1), id("old-gap"));
    old.validate().unwrap();
    let state = captured(&old);
    let mut current = old.clone();
    current.gap_overrides.clear();
    current.nodes.remove(&id("old-gap"));
    // Ordinal zero is current-final. Historical-final four is now audible.
    order(
        &mut current,
        vec![(revision("plays"), 1, 4), (revision("plays"), 0, 1)],
    );
    current.validate().unwrap();
    let (classes, _) = classes(&current, "repeat", RepeatOwnerUse::DefaultGap, &state);
    assert_eq!(classes.len(), 3);
    assert_eq!(
        classes
            .iter()
            .map(|class| class.gap_birth)
            .collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert_eq!(classes[1].gap_after, Some(play(2)));
    assert_eq!(
        classes[0].clock,
        AudioClockRoot::GapDefinitionPointCeil {
            repeat: id("repeat")
        }
    );
    assert_eq!(classes[2].frames_per_local_frame, ExactRatio::integer(1));
    assert_eq!(classes[1].frames_per_local_frame, ExactRatio::integer(2));
}

#[test]
fn current_owned_branches_exclude_defaults_and_final_gap_is_dormant() {
    let mut document = fixture(3);
    for ordinal in 0..3 {
        let owner = id(&format!("owned-{ordinal}"));
        document.nodes.insert(owner.clone(), hold(2));
        document
            .overrides
            .entry(id("repeat"))
            .or_default()
            .insert(play(ordinal), owner);
    }
    document.nodes.insert(id("owned-gap"), hold(2));
    document.nodes.insert(id("final-gap"), hold(2));
    document
        .gap_overrides
        .entry(id("repeat"))
        .or_default()
        .insert(play(0), id("owned-gap"));
    document
        .gap_overrides
        .entry(id("repeat"))
        .or_default()
        .insert(play(2), id("final-gap"));
    // A positive owned gap renders even with no positive default-gap recipe.
    let NodeKind::Repeat { gap, .. } = &mut document.nodes.get_mut(&id("repeat")).unwrap().kind
    else {
        unreachable!()
    };
    *gap = None;
    // The two-frame owned gap supplies the same eight-frame input extent.
    document.validate().unwrap();
    let state = captured(&document);
    assert!(
        classes(&document, "body", RepeatOwnerUse::DefaultBody, &state)
            .0
            .is_empty()
    );
    let (owned, _) = classes(
        &document,
        "owned-1",
        RepeatOwnerUse::OwnedBody {
            iteration: play(1),
            root: id("owned-1"),
        },
        &state,
    );
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].instance.repeats[0].iteration, play(1));
    assert_eq!(
        classes(
            &document,
            "owned-gap",
            RepeatOwnerUse::OwnedGap {
                iteration: play(0),
                root: id("owned-gap")
            },
            &state
        )
        .0
        .len(),
        1
    );
    assert!(
        classes(
            &document,
            "final-gap",
            RepeatOwnerUse::OwnedGap {
                iteration: play(2),
                root: id("final-gap")
            },
            &state
        )
        .0
        .is_empty()
    );
    let binding = &state.bindings()[&id("owned-1")].lattice;
    assert!(
        classes_with_current(
            &document,
            &id("repeat"),
            &id("owned-1"),
            &RepeatOwnerUse::OwnedBody {
                iteration: play(0),
                root: id("owned-1")
            },
            binding,
            &state.timings()[&binding.reference.timing],
            10_000
        )
        .is_err()
    );
}

#[test]
fn live_axis_alias_is_distinct_from_historical_repeat_alias() {
    let old = fixture(3);
    let state = captured(&old);
    let mut current = old.clone();
    let repeated = current.nodes.remove(&id("repeat")).unwrap();
    current.nodes.insert(id("live-repeat"), repeated);
    let NodeKind::Retime { child, .. } = &mut current.nodes.get_mut(&id("stage")).unwrap().kind
    else {
        unreachable!()
    };
    *child = id("live-repeat");
    current.validate().unwrap();
    let mut template = state.bindings()[&id("body")].lattice.clone();
    template.arguments[0].value = AudioRepeatValue::Live {
        repeat: id("live-repeat"),
    };
    template.births[0].repeat = id("live-repeat");
    let (classes, _) = classes_with_current(
        &current,
        &id("live-repeat"),
        &id("body"),
        &RepeatOwnerUse::DefaultBody,
        &template,
        &state.timings()[&template.reference.timing],
        10_000,
    )
    .unwrap();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].instance.repeats[0].node, id("repeat"));
    assert_eq!(classes[0].frames_per_local_frame, ExactRatio::integer(2));
}

#[test]
fn missing_live_axis_is_refused_instead_of_treated_as_a_birth() {
    let document = fixture(3);
    let state = captured(&document);
    let mut template = state.bindings()[&id("body")].lattice.clone();
    template.arguments[0].value = AudioRepeatValue::Live {
        repeat: id("other-repeat"),
    };
    template.births[0].repeat = id("other-repeat");
    assert!(
        classes_with_current(
            &document,
            &id("repeat"),
            &id("body"),
            &RepeatOwnerUse::DefaultBody,
            &template,
            &state.timings()[&template.reference.timing],
            10_000
        )
        .is_err()
    );
}

#[test]
fn historical_final_gap_identity_is_not_the_largest_ordinal() {
    let mut old = fixture(5);
    order(
        &mut old,
        vec![(revision("plays"), 1, 4), (revision("plays"), 0, 1)],
    );
    old.validate().unwrap();
    let state = captured(&old);
    let mut current = old.clone();
    order(&mut current, vec![(revision("plays"), 0, 5)]);
    current.validate().unwrap();
    let (classes, _) = classes(&current, "repeat", RepeatOwnerUse::DefaultGap, &state);
    assert_eq!(classes.len(), 2);
    assert!(classes[0].gap_birth);
    assert!(!classes[1].gap_birth);
    assert_eq!(classes[1].gap_after, Some(play(1)));
}

#[test]
fn run_birth_boundaries_preserve_the_resolvers_captured_argument() {
    let mut old = fixture(5);
    old.nodes.insert(id("owned"), hold(2));
    old.overrides
        .entry(id("repeat"))
        .or_default()
        .insert(play(2), id("owned"));
    old.validate().unwrap();
    let state = captured(&old);
    let mut template = state.bindings()[&id("owned")].lattice.clone();
    assert!(matches!(
        template.arguments[0].value,
        AudioRepeatValue::Captured { .. }
    ));
    template.births.push(AudioBirthClause {
        repeat: id("repeat"),
        survivors: AudioBirthSurvivors::Run {
            allocation: revision("plays"),
            first: 1,
            count: 2,
        },
        definition_root: id("owned"),
    });
    let mut current = old.clone();
    current.overrides.clear();
    current.nodes.remove(&id("owned"));
    current.validate().unwrap();
    let (classes, _) = classes_with_current(
        &current,
        &id("repeat"),
        &id("body"),
        &RepeatOwnerUse::DefaultBody,
        &template,
        &state.timings()[&template.reference.timing],
        10_000,
    )
    .unwrap();
    assert_eq!(
        classes.iter().map(|class| class.birth).collect::<Vec<_>>(),
        vec![Some(0), None, Some(0)]
    );
    assert_eq!(classes[1].instance.repeats[0].iteration, play(2));
    assert_eq!(classes[1].frames_per_local_frame, ExactRatio::integer(2));
    assert_eq!(classes[0].frames_per_local_frame, ExactRatio::integer(1));
}

#[test]
fn overridden_default_gaps_and_zero_extent_owned_gaps_are_dormant() {
    let mut old = fixture(3);
    for ordinal in 0..2 {
        let owner = id(&format!("gap-{ordinal}"));
        old.nodes.insert(owner.clone(), hold(1));
        old.gap_overrides
            .entry(id("repeat"))
            .or_default()
            .insert(play(ordinal), owner);
    }
    old.validate().unwrap();
    let state = captured(&old);
    assert!(
        classes(&old, "repeat", RepeatOwnerUse::DefaultGap, &state)
            .0
            .is_empty()
    );
    let mut current = old.clone();
    current
        .nodes
        .insert(id("gap-0"), BeatNode::sequence("Dormant gap", vec![]));
    let NodeKind::Retime {
        mapping, duration, ..
    } = &mut current.nodes.get_mut(&id("stage")).unwrap().kind
    else {
        unreachable!()
    };
    *mapping = FrameRange::new(ProjectFrame(0), ProjectFrame(7)).unwrap();
    *duration = frames(14);
    current.validate().unwrap();
    assert!(
        classes(
            &current,
            "gap-0",
            RepeatOwnerUse::OwnedGap {
                iteration: play(0),
                root: id("gap-0")
            },
            &state
        )
        .0
        .is_empty()
    );
}

// Production passes its already-cached snapshot; each standalone test query
// constructs one here. Snapshot construction is outside the helper's budget.
fn classes_with_current(
    document: &ProjectDocument,
    repeat: &NodeId,
    owner: &NodeId,
    usage: &RepeatOwnerUse,
    template: &AudioPlacementTemplate,
    layout: &FrozenAudioLayout,
    maximum_work: usize,
) -> Result<(Vec<ResolvedAudioPlacement>, usize), DocumentError> {
    let current = FrozenAudioLayout::capture(document)?;
    repeated_lattice_classes(
        RepeatRateContext {
            document,
            layout: &current,
        },
        repeat,
        owner,
        usage,
        template,
        layout,
        maximum_work,
    )
}

#[test]
fn indexed_current_ancestry_work_does_not_scan_unrelated_owners() {
    let initial = fixture(3);
    let state = captured(&initial);
    let (_, initial_work) = classes(&initial, "body", RepeatOwnerUse::DefaultBody, &state);
    let mut current = initial.clone();
    let mut children = Vec::new();
    for index in 0..1024 {
        let owner = id(&format!("unrelated-{index}"));
        current.nodes.insert(owner.clone(), hold(1));
        children.push(owner);
    }
    children.push(id("stage"));
    current
        .nodes
        .insert(id("root"), BeatNode::sequence("Root", children));
    current.validate().unwrap();
    let (resolved, current_work) = classes(&current, "body", RepeatOwnerUse::DefaultBody, &state);
    assert_eq!(current_work, initial_work);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].frames_per_local_frame, ExactRatio::integer(2));
}
