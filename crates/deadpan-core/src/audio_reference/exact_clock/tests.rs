use super::*;
use crate::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn instance(name: &str) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: vec![],
    }
}
fn play(n: u32) -> IterationId {
    IterationId {
        allocation: revision("plays"),
        ordinal: n,
    }
}
fn occurrence(name: &str, n: u32) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: play(n),
        }],
    }
}

fn add_retime(doc: &mut ProjectDocument, name: &str, child: &str, extent: ExactRatio) {
    let mut retime = BeatNode::sequence("Retime", vec![]);
    retime.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(1),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    doc.nodes.insert(id(name), retime);
    doc.nodes.insert(
        id(child),
        BeatNode::hold(
            "Hold",
            HoldRecipe {
                duration: frames(2),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        ),
    );
    doc.derived_clocks.insert(
        id(name),
        DerivedRetimeClock {
            extent,
            child_start: ExactRatio::ZERO,
            child_end: ratio(2, 1),
            preserve_input: None,
        },
    );
}

fn document() -> ProjectDocument {
    let mut doc = ProjectDocument::new(
        ProjectId::new("frozen-clocks").unwrap(),
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
    doc.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("first"), id("last")]),
    );
    add_retime(&mut doc, "first", "left", ratio(1, 2));
    add_retime(&mut doc, "last", "right", ratio(5, 2));
    doc
}

#[test]
fn exact_capture_roundtrip_preserves_recipe_counts_and_effective_projection() {
    let doc = document();
    let frozen = FrozenAudioLayout::capture(&doc).unwrap();
    assert_eq!(frozen, FrozenAudioLayout::capture_structural(&doc).unwrap());
    let wire = frozen.to_json().unwrap();
    let reopened = FrozenAudioLayout::from_json(&wire).unwrap();
    assert_eq!(reopened, frozen);
    assert_eq!(reopened.duration(), frames(3));
    assert_eq!(reopened.nodes()[&id("root")].duration, frames(2));
    assert_eq!(reopened.node_extent(&id("last")).unwrap(), ratio(5, 2));
    let projection = reopened
        .project(&instance("last"), ratio(1, 4), None, 10)
        .unwrap();
    assert_eq!(projection.origin, ratio(1, 2));
    assert_eq!(projection.point, ratio(3, 4));
    assert_eq!(projection.local_duration, frames(1));
    assert_eq!(projection.local_extent, ratio(5, 2));
    let source = reopened
        .project(&instance("right"), ratio(1, 1), None, 10)
        .unwrap();
    assert_eq!(source.frames_per_local_frame, ratio(5, 4));
    assert_eq!(source.point, ratio(7, 4));
    assert!(
        reopened
            .project(&instance("right"), ratio(1, 1), None, 1)
            .is_err()
    );
}

#[test]
fn exact_crops_retain_unclamped_origin_and_fractional_support() {
    let mut doc = document();
    let clock = doc.derived_clocks.get_mut(&id("first")).unwrap();
    clock.child_start = ratio(1, 2);
    clock.child_end = ratio(3, 2);
    let frozen = FrozenAudioLayout::capture(&doc).unwrap();
    let (projection, support) = frozen
        .project_scoped_with_allocation(&id("root"), &instance("left"), None, 10)
        .unwrap();
    assert_eq!(projection.origin, ratio(-1, 4));
    assert_eq!(projection.frames_per_local_frame, ratio(1, 2));
    assert_eq!(support.unwrap(), ratio(1, 2)..ratio(3, 2));
    let (projection, support) = frozen
        .project_scoped_with_support(&id("root"), &instance("first"), None, 10)
        .unwrap();
    assert_eq!(projection.local_extent, ratio(1, 2));
    assert_eq!(support, ExactRatio::ZERO..ratio(1, 2));
}

fn repeated(count: u32) -> ProjectDocument {
    let mut doc = document();
    add_retime(&mut doc, "gap", "gap-leaf", ratio(3, 2));
    add_retime(&mut doc, "tail", "tail-leaf", ratio(1, 2));
    doc.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("repeat"), id("tail")]),
    );
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("first"),
        iterations: IterationOrder::new(revision("plays"), count).unwrap(),
        gap: Some(HoldRecipe {
            duration: frames(1),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        }),
        escalation: None,
    };
    doc.nodes.insert(id("repeat"), repeat);
    doc.overrides.insert(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(1),
            root: id("last"),
        }])
        .unwrap(),
    );
    doc.gap_overrides.insert(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play(0),
            root: id("gap"),
        }])
        .unwrap(),
    );
    doc
}

#[test]
fn billion_play_capture_projects_sparse_fractional_branches_without_expansion() {
    let frozen = FrozenAudioLayout::capture(&repeated(1_000_000_000)).unwrap();
    let frozen = FrozenAudioLayout::from_json(&frozen.to_json().unwrap()).unwrap();
    assert_eq!(frozen.duration(), frames(1_500_000_002));
    let last = frozen
        .project(
            &occurrence("first", 999_999_999),
            ExactRatio::ZERO,
            None,
            30,
        )
        .unwrap();
    assert_eq!(last.origin, ratio(1_500_000_001, 1));
    assert!(last.work < 20);
    assert_eq!(
        frozen
            .project(&occurrence("last", 1), ExactRatio::ZERO, None, 30)
            .unwrap()
            .origin,
        ratio(2, 1)
    );
    assert_eq!(
        frozen
            .project(&occurrence("gap", 0), ExactRatio::ZERO, None, 30)
            .unwrap()
            .origin,
        ratio(1, 2)
    );
    let gap = frozen
        .project(&instance("repeat"), ExactRatio::ZERO, Some(&play(1)), 30)
        .unwrap();
    assert_eq!(gap.origin, ratio(9, 2));
    assert_eq!(gap.local_extent, ExactRatio::ONE);
    assert!(
        frozen
            .project(&occurrence("first", 1), ExactRatio::ZERO, None, 30)
            .is_err()
    );
    assert!(
        frozen
            .project(&instance("repeat"), ExactRatio::ZERO, Some(&play(0)), 30)
            .is_err()
    );
}

#[test]
fn exact_slices_keep_complete_geometry_and_validate_requested_aliases() {
    let doc = repeated(2);
    let frozen = FrozenAudioLayout::capture(&doc).unwrap();
    assert!(
        FrozenAudioLayout::capture_scoped(&doc, &BTreeSet::from([id("first")]))
            .unwrap()
            .is_none()
    );
    assert_eq!(frozen.sliced([&id("first")]).unwrap(), frozen);
    assert!(frozen.sliced([&id("missing")]).is_err());
}

#[test]
fn malformed_exact_tables_fail_admission_without_rounding() {
    let original = serde_json::to_value(FrozenAudioLayout::capture(&document()).unwrap()).unwrap();
    for (field, value) in [
        ("extent", ratio(0, 1)),
        ("extent", ratio(-1, 1)),
        ("child_start", ratio(-1, 2)),
        ("child_end", ratio(5, 2)),
        ("child_end", ExactRatio::ZERO),
    ] {
        let mut wire = original.clone();
        wire["derived_clocks"]["first"][field] = serde_json::to_value(value).unwrap();
        assert!(
            FrozenAudioLayout::from_json(&wire.to_string()).is_err(),
            "{field}: {value:?}"
        );
    }
    for owner in ["root", "left", "missing"] {
        let mut wire = original.clone();
        wire["derived_clocks"][owner] = wire["derived_clocks"]["first"].clone();
        assert!(
            FrozenAudioLayout::from_json(&wire.to_string()).is_err(),
            "{owner}"
        );
    }
    let mut fractional_root = original.clone();
    fractional_root["derived_clocks"]["first"]["extent"] =
        serde_json::to_value(ratio(1, 3)).unwrap();
    assert!(FrozenAudioLayout::from_json(&fractional_root.to_string()).is_err());
    let mut partition = original.clone();
    partition["nodes"]["first"]["kind"]["mapping"]["end"] = serde_json::json!(1);
    partition["nodes"]["first"]["kind"]["purpose"] = serde_json::json!("partition");
    assert!(
        FrozenAudioLayout::from_json(&partition.to_string())
            .unwrap_err()
            .message
            .contains("unity timing")
    );
    let encoded = original.to_string();
    let clock = original["derived_clocks"]["first"].to_string();
    let duplicate = encoded.replacen(
        "\"derived_clocks\":{",
        &format!("\"derived_clocks\":{{\"first\":{clock},"),
        1,
    );
    assert!(FrozenAudioLayout::from_json(&duplicate).is_err());
    let unknown = encoded.replacen("\"child_start\":", "\"unknown\":0,\"child_start\":", 1);
    assert!(FrozenAudioLayout::from_json(&unknown).is_err());
}

#[test]
fn correspondence_and_placement_compare_effective_clocks() {
    let old = FrozenAudioLayout::capture(&document()).unwrap();
    let mut doc = document();
    doc.derived_clocks.get_mut(&id("first")).unwrap().extent = ratio(3, 2);
    doc.derived_clocks.get_mut(&id("last")).unwrap().extent = ratio(3, 2);
    let new = FrozenAudioLayout::capture(&doc).unwrap();
    assert_eq!(new.duration(), old.duration());
    assert_ne!(old, new);
    assert!(
        old.sound_clock_correspondence(&new, &id("first"), &id("first"), 50)
            .is_err()
    );
    assert!(
        !old.sound_placement_unchanged(&new, &id("last"), 50)
            .unwrap()
            .0
    );
    old.sound_clock_correspondence(&old, &id("first"), &id("first"), 50)
        .unwrap();
}

#[test]
fn bindings_accept_affine_exact_tables_and_refuse_processing_stages() {
    let state = |frozen| AudioBindingState {
        timings: BTreeMap::from([(
            AudioTimingId {
                allocation: revision("capture"),
                ordinal: 0,
            },
            frozen,
        )]),
        ..AudioBindingState::default()
    };
    let follow = FrozenAudioLayout::capture(&document()).unwrap();
    // FollowSpeed geometry is affine; only the empty binding map is invalid.
    let message = state(follow.clone()).validate().unwrap_err().message;
    assert!(
        message.contains("unreferenced audio timing record"),
        "{message}"
    );
    let mut wire = serde_json::to_value(&follow).unwrap();
    wire["nodes"]["first"]["kind"]["pitch"] = serde_json::json!("preserve");
    let preserve = FrozenAudioLayout::from_json(&wire.to_string()).unwrap();
    let message = state(preserve).validate().unwrap_err().message;
    assert!(message.contains("owned output route"), "{message}");
}

#[test]
fn scope_scale_matches_exact_repeat_branches_without_occurrence_expansion() {
    for count in [4, 1_000_000_000] {
        let frozen = FrozenAudioLayout::capture(&repeated(count)).unwrap();
        // The derived extent is 1/2 over two child frames, independently of
        // the integral recipe's one-frame allocation.
        let (default, work) = frozen.scale_in_scope(&id("root"), &id("left"), 4).unwrap();
        assert_eq!(default, ratio(1, 4));
        assert_eq!(work, 4); // leaf, Retime, Repeat, explicit scope root.
        let first = frozen
            .project(&occurrence("left", 0), ExactRatio::ZERO, None, 30)
            .unwrap();
        let last = frozen
            .project(&occurrence("left", count - 1), ExactRatio::ZERO, None, 30)
            .unwrap();
        assert_eq!(first.frames_per_local_frame, default);
        assert_eq!(last.frames_per_local_frame, default);
        assert_ne!(first.origin, last.origin);

        for (leaf, play_index, expected) in
            [("right", 1, ratio(5, 4)), ("gap-leaf", 0, ratio(3, 4))]
        {
            let (scale, work) = frozen.scale_in_scope(&id("root"), &id(leaf), 4).unwrap();
            assert_eq!((scale, work), (expected, 4));
            assert_eq!(
                frozen
                    .project(&occurrence(leaf, play_index), ExactRatio::ZERO, None, 30)
                    .unwrap()
                    .frames_per_local_frame,
                scale
            );
        }

        // Synthetic gaps retain their existing projection path. The helper
        // accepts only physical node aliases and returns the Repeat's scale.
        let synthetic = frozen
            .project_scoped(
                &id("root"),
                &instance("repeat"),
                ExactRatio::ZERO,
                Some(&play(2)),
                30,
            )
            .unwrap();
        assert_eq!(synthetic.frames_per_local_frame, ExactRatio::ONE);
        assert_eq!(synthetic.gap_after, Some(play(2)));
        assert_eq!(
            frozen
                .scale_in_scope(&id("root"), &id("repeat"), 2)
                .unwrap(),
            (ExactRatio::ONE, 2)
        );
    }
}

#[test]
fn scope_scale_includes_explicit_retime_root_and_bounds_scope_work() {
    let frozen = FrozenAudioLayout::capture(&repeated(4)).unwrap();
    let (inside_definition, work) = frozen.scale_in_scope(&id("first"), &id("left"), 2).unwrap();
    assert_eq!((inside_definition, work), (ratio(1, 4), 2));
    assert_eq!(
        frozen
            .project_scoped(&id("first"), &instance("left"), ExactRatio::ZERO, None, 2)
            .unwrap()
            .frames_per_local_frame,
        inside_definition
    );
    for name in ["root", "first", "left"] {
        assert_eq!(
            frozen.scale_in_scope(&id(name), &id(name), 1).unwrap(),
            (ExactRatio::ONE, 1)
        );
    }
    for (scope, target, budget) in [
        ("missing", "left", 100),
        ("root", "missing", 100),
        ("last", "left", 100),
        ("root", "left", 3),
        ("root", "left", 0),
        ("root", "left", MAX_DOCUMENT_NODES + 1),
    ] {
        assert!(
            frozen
                .scale_in_scope(&id(scope), &id(target), budget)
                .is_err(),
            "scope {scope}, target {target}, budget {budget}"
        );
    }
}

#[test]
fn scope_scale_refuses_recipe_and_effective_pitch_processing() {
    // A live document refuses opaque exact clocks without their owned input
    // route. Historical timing tables omit that route, so exercise their own
    // admitted wire form directly, as retained bindings do.
    let baseline = serde_json::to_value(FrozenAudioLayout::capture(&document()).unwrap()).unwrap();
    for (pitch, recipe_unity, effective_unity) in [
        (PitchPolicy::Preserve, false, false),
        (PitchPolicy::Preserve, true, false),
        (PitchPolicy::Preserve, false, true),
        (PitchPolicy::shifted(3).unwrap(), true, true),
    ] {
        let mut wire = baseline.clone();
        wire["nodes"]["first"]["kind"]["pitch"] = serde_json::to_value(pitch).unwrap();
        if recipe_unity {
            wire["nodes"]["first"]["kind"]["mapping"]["end"] = serde_json::json!(1);
        }
        if effective_unity {
            wire["derived_clocks"]["first"]["extent"] = serde_json::to_value(ratio(2, 1)).unwrap();
            // Keep the root's effective duration integral.
            wire["derived_clocks"]["last"]["extent"] = serde_json::to_value(ratio(2, 1)).unwrap();
        }
        let frozen = FrozenAudioLayout::from_json(&wire.to_string()).unwrap();
        assert!(
            frozen
                .scale_in_scope(&id("root"), &id("left"), 100)
                .is_err()
        );
    }

    let mut unity = document();
    let NodeKind::Retime { pitch, mapping, .. } =
        &mut unity.nodes.get_mut(&id("first")).unwrap().kind
    else {
        panic!("fixture first must be a Retime");
    };
    *pitch = PitchPolicy::Preserve;
    *mapping = FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap();
    let first = unity.derived_clocks.get_mut(&id("first")).unwrap();
    first.child_end = ExactRatio::ONE;
    first.extent = ExactRatio::ONE;
    unity.derived_clocks.get_mut(&id("last")).unwrap().extent = ratio(2, 1);
    assert_eq!(
        FrozenAudioLayout::capture(&unity)
            .unwrap()
            .scale_in_scope(&id("root"), &id("left"), 3)
            .unwrap(),
        (ExactRatio::ONE, 3)
    );
}
