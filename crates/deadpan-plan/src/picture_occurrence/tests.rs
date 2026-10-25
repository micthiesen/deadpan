use super::*;
use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(value),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn source(value: i64) -> BeatNode {
    let mut node = hold(value);
    node.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(value),
            video: SourceVideo::Stream {
                asset: AssetId::new("video").unwrap(),
                span: span(0, value),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
            edit_window: None,
        },
    };
    node
}
fn repeat(child: &str, count: u32, gap: i64, allocation: &str) -> BeatNode {
    let mut node = hold(1);
    node.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(revision(allocation), count).unwrap(),
        gap: (gap > 0).then(|| {
            let NodeKind::Hold { recipe } = hold(gap).kind else {
                unreachable!()
            };
            recipe
        }),
        escalation: None,
    };
    node
}
fn retime(child: &str, count: i64, start: i64, end: i64) -> BeatNode {
    let mut node = hold(count);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(count),
        mapping: range(start, end),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    node
}
fn span(start: i64, end: i64) -> SourceSpan {
    let base = SourceTimeBase::new(1, 30).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: base,
        },
        SourceTimestamp {
            ticks: end,
            time_base: base,
        },
    )
    .unwrap()
}
fn document(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("picture-owner-map").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", roots.iter().map(|name| id(name)).collect()),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            source_qualification: None,
            video: Some(span(0, 1000)),
            audio: None,
            still_image: false,
            frame_count: Some(frames(1000)),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn instance(node: &str, path: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: path
            .iter()
            .map(|(name, ordinal)| RepeatInstance {
                node: id(name),
                iteration: IterationId {
                    allocation: revision(name),
                    ordinal: *ordinal,
                },
            })
            .collect(),
    }
}
fn query(plan: &RenderPlan, node: &str, path: &[(&str, u32)], frame: i64) -> PictureOwnerMap {
    plan.picture_owner_map(
        &instance(node, path),
        ProjectFrame(frame),
        Default::default(),
    )
    .unwrap()
}
fn unavailable(result: Result<PictureOwnerMap, PlanError>) {
    assert!(
        matches!(result, Err(PlanError::InvalidPictureOccurrence(_))),
        "{result:?}"
    );
}

#[test]
fn repeated_source_range_maps_through_ancestor_retime_exactly() {
    let doc = document(
        &["retime"],
        vec![
            ("retime", retime("repeat", 96, 0, 240)),
            ("repeat", repeat("source", 2, 0, "repeat")),
            ("source", source(120)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let map = query(&plan, "source", &[("repeat", 1)], 60);
    assert_eq!(map.project_id, *doc.project_id());
    assert_eq!(map.revision_id, *doc.revision_id());
    assert_eq!(map.root_allocation, q(48, 1)..q(96, 1));
    assert_eq!(map.owner_to_root_offset, q(48, 1));
    assert_eq!(map.owner_to_root_scale, q(2, 5));
    assert_eq!(map.effective_duration, q(120, 1));
    assert_eq!(
        map.normalized_range(range(60, 84)).unwrap(),
        (q(1, 4), q(3, 4))
    );
    assert_eq!(
        map.normalized_range(range(48, 96)).unwrap(),
        (q(0, 1), q(1, 1))
    );
    assert!(map.normalized_range(range(47, 84)).is_err());
    assert!(map.normalized_range(range(60, 97)).is_err());
    let first = query(&plan, "source", &[("repeat", 0)], 12);
    assert_eq!(first.root_allocation, q(0, 1)..q(48, 1));
    unavailable(plan.picture_owner_map(
        &instance("source", &[("repeat", 0)]),
        ProjectFrame(60),
        Default::default(),
    ));
}

#[test]
fn every_owner_kind_uses_its_own_full_output_extent() {
    let doc = document(
        &["retime"],
        vec![
            ("retime", retime("repeat", 104, 0, 260)),
            ("repeat", repeat("group", 2, 0, "repeat")),
            (
                "group",
                BeatNode::sequence("Group", vec![id("source"), id("hold")]),
            ),
            ("source", source(120)),
            ("hold", hold(10)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    for (owner, path, at, start, end, extent, scale) in [
        ("root", vec![], 2, 0, 104, 104, q(1, 1)),
        ("retime", vec![], 2, 0, 104, 104, q(1, 1)),
        ("repeat", vec![], 2, 0, 104, 260, q(2, 5)),
        ("group", vec![("repeat", 0)], 2, 0, 52, 130, q(2, 5)),
        ("source", vec![("repeat", 0)], 2, 0, 48, 120, q(2, 5)),
        ("hold", vec![("repeat", 0)], 49, 48, 52, 10, q(2, 5)),
    ] {
        let map = query(&plan, owner, &path, at);
        assert_eq!(map.root_allocation, q(start, 1)..q(end, 1), "{owner}");
        assert_eq!(map.owner_to_root_offset, q(start, 1), "{owner}");
        assert_eq!(map.effective_duration, q(extent, 1), "{owner}");
        assert_eq!(map.owner_to_root_scale, scale, "{owner}");
    }
}

#[test]
fn nonzero_retime_start_clips_allocation_without_rebasing_owner_zero() {
    let doc = document(
        &["retime"],
        vec![
            ("retime", retime("repeat", 80, 20, 220)),
            ("repeat", repeat("source", 2, 0, "repeat")),
            ("source", source(120)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let first = query(&plan, "source", &[("repeat", 0)], 0);
    assert_eq!(first.owner_to_root_offset, q(-8, 1));
    assert_eq!(first.root_allocation, q(0, 1)..q(40, 1));
    assert_eq!(
        first.normalized_range(range(0, 40)).unwrap(),
        (q(1, 6), q(1, 1))
    );
    let second = query(&plan, "source", &[("repeat", 1)], 40);
    assert_eq!(second.owner_to_root_offset, q(40, 1));
    assert_eq!(second.root_allocation, q(40, 1)..q(80, 1));
    assert_eq!(
        second.normalized_range(range(40, 80)).unwrap(),
        (q(0, 1), q(5, 6))
    );
    assert!(second.normalized_range(range(39, 80)).is_err());
}

#[test]
fn derived_fractional_extent_rejects_partly_owned_integer_frame_boundaries() {
    let doc = document(
        &["lead", "owner"],
        vec![
            ("lead", retime("left", 1, 0, 2)),
            ("owner", retime("right", 1, 0, 2)),
            ("left", source(2)),
            ("right", source(2)),
        ],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([
        (
            id("lead"),
            DerivedRetimeClock {
                extent: q(1, 2),
                child_start: q(0, 1),
                child_end: q(2, 1),
                preserve_input: None,
            },
        ),
        (
            id("owner"),
            DerivedRetimeClock {
                extent: q(3, 2),
                child_start: q(0, 1),
                child_end: q(2, 1),
                preserve_input: None,
            },
        ),
    ]))
    .unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let map = query(&plan, "owner", &[], 0);
    assert_eq!(map.effective_duration, q(3, 2));
    assert_eq!(
        plan.nodes[plan.by_id[&id("owner")]].inspection.duration,
        frames(1)
    );
    assert_eq!(map.root_allocation, q(1, 2)..q(2, 1));
    assert!(map.normalized_range(range(0, 1)).is_err());
    assert_eq!(
        map.normalized_range(range(1, 2)).unwrap(),
        (q(1, 3), q(1, 1))
    );
    let child = query(&plan, "right", &[], 1);
    assert_eq!(child.owner_to_root_offset, q(1, 2));
    assert_eq!(child.owner_to_root_scale, q(3, 4));
    assert_eq!(child.effective_duration, q(2, 1));
    assert_eq!(
        child.normalized_range(range(1, 2)).unwrap(),
        (q(1, 3), q(1, 1))
    );
}

#[test]
fn nested_stable_play_paths_and_overrides_reject_stale_or_reordered_addresses() {
    let doc = document(
        &["outer"],
        vec![
            ("outer", repeat("group", 2, 2, "outer")),
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("inner"), id("tail")]),
            ),
            ("lead", hold(2)),
            ("inner", repeat("source", 3, 1, "inner")),
            ("tail", hold(1)),
            ("source", source(4)),
        ],
    );
    let before = RenderPlan::compile(&doc).unwrap();
    assert_eq!(
        query(&before, "source", &[("outer", 1), ("inner", 2)], 32).root_allocation,
        q(31, 1)..q(35, 1)
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["alternate"] = serde_json::to_value(hold(6)).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("inner"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation: revision("inner"),
                ordinal: 2,
            },
            root: id("alternate"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let exact = instance("alternate", &[("outer", 1), ("inner", 2)]);
    let map = plan
        .picture_owner_map(&exact, ProjectFrame(34), Default::default())
        .unwrap();
    assert_eq!(map.root_allocation, q(33, 1)..q(39, 1));
    assert_eq!(
        map.normalized_range(range(34, 38)).unwrap(),
        (q(1, 6), q(5, 6))
    );
    for path in [
        instance("source", &[("outer", 1), ("inner", 2)]),
        instance("alternate", &[("inner", 2), ("outer", 1)]),
        instance("alternate", &[("outer", 1)]),
        instance("alternate", &[("outer", 1), ("inner", 99)]),
        instance("alternate", &[("outer", 1), ("inner", 2), ("foreign", 0)]),
    ] {
        unavailable(plan.picture_owner_map(&path, ProjectFrame(34), Default::default()));
    }
}

#[test]
fn owned_gap_has_its_own_map_and_implicit_or_dormant_gaps_grant_no_body() {
    let doc = document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 3, 1, "repeat")),
            ("source", source(4)),
        ],
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["gap"] = serde_json::to_value(hold(2)).unwrap();
    wire["nodes"]["last-gap"] = serde_json::to_value(hold(3)).unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: IterationId {
                    allocation: revision("repeat"),
                    ordinal: 0,
                },
                root: id("gap"),
            },
            PlayOverride {
                iteration: IterationId {
                    allocation: revision("repeat"),
                    ordinal: 2,
                },
                root: id("last-gap"),
            },
        ])
        .unwrap(),
    )]))
    .unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let gap = query(&plan, "gap", &[("repeat", 0)], 4);
    assert_eq!(gap.root_allocation, q(4, 1)..q(6, 1));
    assert_eq!(
        gap.normalized_range(range(4, 6)).unwrap(),
        (q(0, 1), q(1, 1))
    );
    assert!(gap.normalized_range(range(3, 6)).is_err());
    unavailable(plan.picture_owner_map(
        &instance("source", &[("repeat", 0)]),
        ProjectFrame(4),
        Default::default(),
    ));
    unavailable(plan.picture_owner_map(
        &instance("source", &[("repeat", 1)]),
        ProjectFrame(10),
        Default::default(),
    ));
    unavailable(plan.picture_owner_map(
        &instance("last-gap", &[("repeat", 2)]),
        ProjectFrame(14),
        Default::default(),
    ));
}

#[test]
fn source_cutaway_changes_provider_without_changing_owner_clock() {
    let mut original = source(20);
    original.cutaways = vec![Cutaway {
        range: range(5, 10),
        asset: AssetId::new("video").unwrap(),
        selection: ExactSourceSpan::from(span(100, 110)),
        fit: CutawayFit::Hold,
        removed: false,
    }];
    let plan = RenderPlan::compile(&document(&["source"], vec![("source", original)])).unwrap();
    let map = query(&plan, "source", &[], 3);
    assert_eq!(map.root_allocation, q(0, 1)..q(20, 1));
    assert_eq!(
        map.normalized_range(range(2, 18)).unwrap(),
        (q(1, 10), q(9, 10))
    );
    let inside = query(&plan, "source", &[], 6);
    assert_eq!(inside, map);
    assert!(matches!(
        plan.picture(ProjectFrame(6)).unwrap().picture,
        crate::Picture::Source { selection, .. } if selection == ExactSourceSpan::from(span(100, 110))
    ));
}

#[test]
fn captured_hold_provider_geometry_never_becomes_live_clock_ancestry() {
    let mut frozen = hold(4);
    let NodeKind::Hold { recipe } = &mut frozen.kind else {
        unreachable!()
    };
    recipe.video = HoldVideo::Freeze {
        asset: AssetId::new("video").unwrap(),
        timestamp: SourceTimestamp {
            ticks: 42,
            time_base: span(0, 1000).start().time_base,
        },
    };
    recipe.picture_context = Some(
        CapturedFraming::new(vec![CapturedCanvas {
            width: 16,
            height: 16,
            fit: CapturedFit::Fit,
            layers: vec![Some(FramingPose::new(q(1, 4), q(1, 2), q(2, 1)).unwrap())],
        }])
        .unwrap(),
    );
    let plan = RenderPlan::compile(&document(
        &["source", "freeze"],
        vec![("source", source(10)), ("freeze", frozen)],
    ))
    .unwrap();
    let map = query(&plan, "freeze", &[], 11);
    assert_eq!(map.root_allocation, q(10, 1)..q(14, 1));
    assert_eq!(map.effective_duration, q(4, 1));
    unavailable(plan.picture_owner_map(
        &instance("source", &[]),
        ProjectFrame(11),
        Default::default(),
    ));
}

#[test]
fn compact_million_play_lookup_charges_bounded_canonical_and_ancestry_work() {
    let plan = RenderPlan::compile(&document(
        &["repeat"],
        vec![
            ("repeat", repeat("source", 1_000_000, 0, "repeat")),
            ("source", source(4)),
        ],
    ))
    .unwrap();
    let target = instance("source", &[("repeat", 999_999)]);
    let limits = BoundaryQueryLimits {
        max_scopes: 6,
        max_comparisons: 4,
    };
    let map = plan
        .picture_owner_map(&target, ProjectFrame(3_999_999), limits)
        .unwrap();
    assert_eq!(map.root_allocation, q(3_999_996, 1)..q(4_000_000, 1));
    assert_eq!(map.lookup.visited_nodes, 6);
    assert_eq!(map.lookup.sequence_comparisons, 2);
    assert_eq!(map.lookup.iteration_run_comparisons, 2);
    for limits in [
        BoundaryQueryLimits {
            max_scopes: 5,
            max_comparisons: 4,
        },
        BoundaryQueryLimits {
            max_scopes: 6,
            max_comparisons: 3,
        },
    ] {
        assert!(matches!(
            plan.picture_owner_map(&target, ProjectFrame(3_999_999), limits),
            Err(PlanError::PictureQueryLimit(_))
        ));
    }
    assert!(matches!(
        plan.picture_owner_map(&target, ProjectFrame(-1), Default::default()),
        Err(PlanError::FrameOutOfRange { .. })
    ));
    assert!(matches!(
        plan.picture_owner_map(&target, ProjectFrame(4_000_000), Default::default()),
        Err(PlanError::FrameOutOfRange { .. })
    ));
}
