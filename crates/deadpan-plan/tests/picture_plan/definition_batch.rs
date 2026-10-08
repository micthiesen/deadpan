use super::definition::{nested, target};
use super::*;
use deadpan_plan::{LookupStats, ScopedHoldBoundaries};

fn ample() -> BoundaryQueryLimits {
    BoundaryQueryLimits {
        max_scopes: 1_000_000,
        max_comparisons: 1_000_000,
    }
}

fn work(results: &[ScopedHoldBoundaries]) -> BoundaryQueryLimits {
    BoundaryQueryLimits {
        max_scopes: results
            .iter()
            .map(|result| result.lookup.visited_nodes)
            .sum(),
        max_comparisons: results
            .iter()
            .map(|result| {
                result.lookup.sequence_comparisons + result.lookup.iteration_run_comparisons
            })
            .sum(),
    }
}

fn compare_oracle(plan: &RenderPlan, targets: &[ScopedNodeTarget]) -> Vec<ScopedHoldBoundaries> {
    let batch = plan.scoped_hold_boundaries_batch(targets, ample()).unwrap();
    for (selected, actual) in targets.iter().zip(&batch) {
        let mut independent = plan.scoped_hold_boundaries(selected, ample()).unwrap();
        // Ancestry work differs; every boundary sample and its actual picture
        // walk work must remain exactly equal to the independent query.
        independent.lookup = actual.lookup;
        assert_eq!(&independent, actual);
    }
    batch
}

fn scoped_document(plays: u32) -> ProjectDocument {
    let mut wire = serde_json::to_value(nested(plays)).unwrap();
    let mut cutaway = source(2, 4004, 6006);
    cutaway.cutaways.push(Cutaway {
        range: range(0, 1),
        asset: asset_id("video"),
        selection: ExactSourceSpan::from(span(9009, 10_010)),
        fit: CutawayFit::Hold,
        removed: false,
    });
    for (name, value) in [
        (
            "owned",
            BeatNode::sequence("Owned", vec![id("speed"), id("owned-pause"), id("cutaway")]),
        ),
        ("speed", retime("fast-source", 2, 2, 8)),
        ("fast-source", source(8, 0, 8008)),
        ("owned-pause", hold(3)),
        ("cutaway", cutaway),
        (
            "gap",
            BeatNode::sequence(
                "Dormant gap",
                vec![id("gap-left"), id("gap-pause"), id("gap-right")],
            ),
        ),
        ("gap-left", source(2, 0, 2002)),
        ("gap-pause", hold(2)),
        ("gap-right", source(2, 2002, 4004)),
        ("outer-owned", hold(1)),
    ] {
        wire["nodes"][name] = serde_json::to_value(value).unwrap();
    }
    wire["overrides"]["inner"] = serde_json::json!([{
        "iteration": {"allocation":"inner", "ordinal":1}, "root":"owned"
    }]);
    wire["gap_overrides"]["inner"] = serde_json::json!([{
        "iteration": {"allocation":"inner", "ordinal":2}, "root":"gap"
    }]);
    wire["overrides"]["outer"] = serde_json::json!([{
        "iteration": {"allocation":"outer", "ordinal":plays - 1}, "root":"outer-owned"
    }]);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn batch_matches_independent_queries_for_nested_owned_cutaway_retime_and_dormant_gap() {
    let doc = scoped_document(2);
    let plan = RenderPlan::compile(&doc).unwrap();
    let targets = plan.authored_hold_targets(ample()).unwrap();
    assert_eq!(
        targets,
        vec![
            target("gap-pause", &[("outer", None), ("inner", Some(2))]),
            target("outer-owned", &[("outer", Some(1))]),
            target("owned-pause", &[("outer", None), ("inner", Some(1))]),
            target("pause", &[("outer", None), ("inner", None)]),
        ]
    );
    for target in &targets {
        target.validate(&doc).unwrap();
    }
    let batch = compare_oracle(&plan, &targets);
    let owned = batch
        .iter()
        .find(|boundaries| boundaries.target.node == id("owned-pause"))
        .unwrap();
    assert_eq!(
        owned.left.as_ref().unwrap().instance.node,
        id("fast-source")
    );
    assert_eq!(
        ticks(&owned.left.as_ref().unwrap().picture),
        ExactRatio::new(13_013, 2).unwrap()
    );
    assert_eq!(owned.right.as_ref().unwrap().instance.node, id("cutaway"));
    assert_eq!(
        ticks(&owned.right.as_ref().unwrap().picture),
        ExactRatio::new(19_019, 2).unwrap()
    );
    let gap = &batch[0];
    assert_eq!(gap.definition, id("gap"));
    assert!(gap.left.is_some() && gap.right.is_some());
    assert!(batch[1].left.is_none() && batch[1].right.is_none());
}

#[test]
fn single_reads_shared_play_while_batch_requires_owned_ancestry() {
    let doc = scoped_document(3);
    let plan = RenderPlan::compile(&doc).unwrap();
    let canonical = plan.authored_hold_targets(ample()).unwrap();
    for selected in [
        target("pause", &[("outer", Some(0)), ("inner", None)]),
        target("pause", &[("outer", None), ("inner", Some(0))]),
        target("owned-pause", &[("outer", Some(0)), ("inner", Some(1))]),
        target("gap-pause", &[("outer", Some(0)), ("inner", Some(2))]),
    ] {
        selected.validate(&doc).unwrap();
        let mut effective = plan.scoped_hold_boundaries(&selected, ample()).unwrap();
        assert_eq!(effective.target, selected);
        let owned = canonical
            .iter()
            .find(|target| target.node == selected.node)
            .unwrap();
        let batch = plan
            .scoped_hold_boundaries_batch(std::slice::from_ref(owned), ample())
            .unwrap();
        // The effective Play sees the same definition pictures, but carries no
        // authority to edit its shared provider without occurrence isolation.
        effective.target = owned.clone();
        effective.lookup = batch[0].lookup;
        assert_eq!(effective, batch[0]);
        assert!(matches!(
            plan.scoped_hold_boundaries_batch(&[selected], ample()),
            Err(PlanError::InvalidScopedHold(_))
        ));
    }
}

#[test]
fn batch_budget_is_aggregate_and_output_order_including_duplicates_is_retained() {
    let plan = RenderPlan::compile(&scoped_document(2)).unwrap();
    let mut targets = plan.authored_hold_targets(ample()).unwrap();
    targets.reverse();
    targets.push(targets[0].clone());
    let expected = compare_oracle(&plan, &targets);
    let exact = work(&expected);
    assert_eq!(
        plan.scoped_hold_boundaries_batch(&targets, exact).unwrap(),
        expected
    );
    assert!(matches!(
        plan.scoped_hold_boundaries_batch(
            &targets,
            BoundaryQueryLimits {
                max_scopes: exact.max_scopes - 1,
                ..exact
            }
        ),
        Err(PlanError::PictureQueryLimit("node visits"))
    ));
    assert!(matches!(
        plan.scoped_hold_boundaries_batch(
            &targets,
            BoundaryQueryLimits {
                max_comparisons: exact.max_comparisons - 1,
                ..exact
            }
        ),
        Err(PlanError::PictureQueryLimit("comparisons"))
    ));
    assert_eq!(
        plan.scoped_hold_boundaries_batch(
            &[],
            BoundaryQueryLimits {
                max_scopes: 0,
                max_comparisons: 0
            }
        )
        .unwrap(),
        vec![]
    );
    assert!(matches!(
        plan.authored_hold_targets(BoundaryQueryLimits {
            max_scopes: 3,
            ..ample()
        }),
        Err(PlanError::PictureQueryLimit("node visits"))
    ));
    assert!(matches!(
        plan.authored_hold_targets(BoundaryQueryLimits {
            max_comparisons: 0,
            ..ample()
        }),
        Err(PlanError::PictureQueryLimit("comparisons"))
    ));
    targets.push(target("pause", &[]));
    assert!(matches!(
        plan.scoped_hold_boundaries_batch(&targets, ample()),
        Err(PlanError::InvalidScopedHold(_))
    ));
}

#[test]
fn batch_work_does_not_expand_repeat_occurrences() {
    let mut prior = None;
    for plays in [2, u32::MAX] {
        let plan = RenderPlan::compile(&scoped_document(plays)).unwrap();
        let targets = plan.authored_hold_targets(ample()).unwrap();
        assert_eq!(targets.len(), 4);
        let batch = compare_oracle(&plan, &targets);
        let actual: Vec<LookupStats> = batch.iter().map(|result| result.lookup).collect();
        if let Some(prior) = prior {
            assert_eq!(actual, prior);
        }
        prior = Some(actual);
    }
}

#[test]
fn flat_batch_above_8192_siblings_has_linear_visits_and_logarithmic_picture_searches() {
    let count = 9001_usize;
    let names: Vec<_> = (0..count)
        .map(|number| format!("hold-{number:05}"))
        .collect();
    let roots: Vec<_> = names.iter().map(String::as_str).collect();
    let doc = document(&roots, roots.iter().map(|&name| (name, hold(1))).collect());
    let plan = RenderPlan::compile(&doc).unwrap();
    let targets = plan
        .authored_hold_targets(BoundaryQueryLimits {
            max_scopes: count,
            max_comparisons: 0,
        })
        .unwrap();
    assert_eq!(targets.len(), count);
    let last = targets.last().unwrap();
    assert!(matches!(
        plan.scoped_hold_boundaries(last, BoundaryQueryLimits::default()),
        Err(PlanError::PictureQueryLimit("comparisons"))
    ));
    let late = plan
        .scoped_hold_boundaries_batch(std::slice::from_ref(last), BoundaryQueryLimits::default())
        .unwrap();
    assert_eq!(late[0].range.start, ExactRatio::integer(9000));
    assert_eq!(
        late[0].left.as_ref().unwrap().instance.node,
        id("hold-08999")
    );
    assert!(late[0].right.is_none());
    let batch = plan
        .scoped_hold_boundaries_batch(&targets, ample())
        .unwrap();
    let actual = work(&batch);
    assert_eq!(actual.max_scopes, 5 * count - 4);
    let binary_bound = usize::try_from(usize::BITS - count.leading_zeros()).unwrap();
    assert!(
        actual.max_comparisons <= 2 * count * binary_bound,
        "{actual:?}"
    );
    for index in [0, count / 2, count - 1] {
        let mut oracle = plan
            .scoped_hold_boundaries(&targets[index], ample())
            .unwrap();
        oracle.lookup = batch[index].lookup;
        assert_eq!(oracle, batch[index]);
    }
    assert!(matches!(
        plan.scoped_hold_boundaries_batch(
            &targets,
            BoundaryQueryLimits {
                max_scopes: actual.max_scopes - 1,
                ..actual
            }
        ),
        Err(PlanError::PictureQueryLimit("node visits"))
    ));
}

fn generated_document(fallback: HoldFallback, cutaway: bool) -> ProjectDocument {
    let doc = document(
        &["speed", "pause"],
        vec![
            ("speed", retime("generated", 2, 2, 8)),
            ("generated", hold(10)),
            ("pause", hold(2)),
        ],
    );
    let (artifact, assets) = generated_fixture("batch-", ['1', '2', '3']);
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["generated"]["kind"]["recipe"]["video"] =
        serde_json::to_value(HoldVideo::Generated {
            accepted: Box::new(AcceptedGeneration { artifact, fallback }),
        })
        .unwrap();
    if cutaway {
        wire["nodes"]["generated"]["cutaways"] = serde_json::to_value(vec![Cutaway {
            range: range(0, 10),
            asset: asset_id("video"),
            selection: ExactSourceSpan::from(span(1001, 2002)),
            fit: CutawayFit::Hold,
            removed: false,
        }])
        .unwrap();
    }
    for (asset, record) in assets {
        wire["assets"][asset.as_str()] = serde_json::to_value(record).unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn terminal_generated_fallback_uses_canonical_clock_and_rejects_changed_sample_evidence() {
    for fallback in [
        HoldFallback::Background,
        HoldFallback::Freeze {
            asset: asset_id("video"),
            timestamp: span(1001, 2002).start(),
        },
    ] {
        let doc = generated_document(fallback.clone(), false);
        let plan = RenderPlan::compile(&doc).unwrap();
        let batch = compare_oracle(&plan, &[target("pause", &[])]);
        let sample = batch[0].left.as_ref().unwrap();
        assert_eq!(sample.instance.node, id("generated"));
        assert_eq!(sample.local_position, ExactRatio::new(13, 2).unwrap());
        assert!(matches!(
            sample.picture,
            Picture::Accepted {
                frame: SourceFrameId(6),
                ..
            }
        ));
        let expected = match fallback {
            HoldFallback::Background => Picture::Background,
            HoldFallback::Freeze { asset, timestamp } => Picture::Freeze {
                asset,
                point: SourcePoint {
                    ticks: ExactRatio::integer(timestamp.ticks),
                    time_base: timestamp.time_base,
                },
            },
        };
        assert_eq!(
            plan.definition_hold_fallback_picture(sample).unwrap(),
            Some(expected.clone())
        );
        assert_eq!(
            plan.clone()
                .definition_hold_fallback_picture(sample)
                .unwrap(),
            Some(expected)
        );
        assert!(
            RenderPlan::compile(&doc)
                .unwrap()
                .definition_hold_fallback_picture(sample)
                .is_err(),
            "another compilation cannot reuse private provider evidence"
        );
        let mut tampered = sample.clone();
        tampered.picture = Picture::Background;
        assert!(plan.definition_hold_fallback_picture(&tampered).is_err());
        let mut tampered = sample.clone();
        tampered.instance.node = id("pause");
        assert!(plan.definition_hold_fallback_picture(&tampered).is_err());
        let mut tampered = sample.clone();
        tampered.local_position = ExactRatio::integer(1);
        assert!(plan.definition_hold_fallback_picture(&tampered).is_err());
        let mut tampered = sample.clone();
        tampered.revision_id = revision("foreign");
        assert!(plan.definition_hold_fallback_picture(&tampered).is_err());
        let wire = serde_json::to_value(sample).unwrap();
        assert!(wire.get("hold_provider").is_none());
    }
}

#[test]
fn generated_hold_cutaway_and_other_providers_do_not_expose_a_fallback() {
    for removed in [false, true] {
        let mut wire =
            serde_json::to_value(generated_document(HoldFallback::Background, true)).unwrap();
        wire["nodes"]["generated"]["cutaways"][0]["removed"] = serde_json::json!(removed);
        let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let plan = RenderPlan::compile(&doc).unwrap();
        let boundaries = compare_oracle(&plan, &[target("pause", &[])]);
        let sample = boundaries[0].left.as_ref().unwrap();
        assert_eq!(sample.instance.node, id("generated"));
        assert!(if removed {
            sample.picture == Picture::Background
        } else {
            matches!(sample.picture, Picture::Source { .. })
        });
        assert_eq!(plan.definition_hold_fallback_picture(sample).unwrap(), None);
        let ordinary = plan
            .definition_picture(&id("pause"), ExactRatio::new(1, 2).unwrap(), ample())
            .unwrap();
        assert_eq!(
            plan.definition_hold_fallback_picture(&ordinary).unwrap(),
            None
        );
    }
    let gap_doc = document(
        &["repeat"],
        vec![
            ("repeat", repeat("child", 2, 1, "repeat")),
            ("child", hold(1)),
        ],
    );
    let (artifact, assets) = generated_fixture("gap-", ['4', '5', '6']);
    let mut wire = serde_json::to_value(gap_doc).unwrap();
    wire["nodes"]["repeat"]["kind"]["gap"]["video"] = serde_json::to_value(HoldVideo::Generated {
        accepted: Box::new(AcceptedGeneration {
            artifact,
            fallback: HoldFallback::Background,
        }),
    })
    .unwrap();
    for (asset, record) in assets {
        wire["assets"][asset.as_str()] = serde_json::to_value(record).unwrap();
    }
    let gap_doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&gap_doc).unwrap();
    let gap = plan
        .definition_picture(&id("repeat"), ExactRatio::new(3, 2).unwrap(), ample())
        .unwrap();
    assert!(gap.gap_after.is_some());
    assert!(matches!(
        gap.picture,
        Picture::Accepted {
            generated: Some(_),
            ..
        }
    ));
    assert_eq!(plan.definition_hold_fallback_picture(&gap).unwrap(), None);
}
