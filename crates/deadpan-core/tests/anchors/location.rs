use super::*;

fn location_request(
    document: &ProjectDocument,
    position: ExactRatio,
    bias: InsertionBias,
) -> BoundaryLocationRequest {
    BoundaryLocationRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        position,
        bias,
    }
}

fn locate(
    document: &ProjectDocument,
    position: ExactRatio,
    bias: InsertionBias,
) -> BoundaryLocation {
    let index = AnchorIndex::new(document).unwrap();
    let result = index
        .locate_boundary(
            &location_request(document, position, bias),
            BoundaryQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(result.project_id, *document.project_id());
    assert_eq!(result.revision_id, *document.revision_id());
    assert_eq!(result.position, position);
    assert_eq!(result.bias, bias);
    // Every emitted owner is an actual occurrence in this revision, including
    // cropped domains and owned gap branches. Inverse resolution must retain
    // the exact project boundary, not merely the same rounded output frame.
    for scope in &result.scopes {
        scope.instance.validate(document).unwrap();
        let anchor = AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: scope.instance.clone(),
                    position: scope.position,
                },
                bias,
            },
            occurrence: None,
        };
        assert_eq!(index.resolve_target(&anchor).unwrap().exact_frame, position);
    }
    result
}

fn leaf(result: &BoundaryLocation) -> (&str, ExactRatio) {
    assert_eq!(result.terminal, BoundaryTerminal::Node);
    let scope = result.scopes.last().unwrap();
    (scope.instance.node.as_str(), scope.position)
}

fn wrap(
    document: &ProjectDocument,
    child: &str,
    id: &str,
    plays: u32,
    gap: i64,
    next: &str,
) -> ProjectDocument {
    edit(
        document,
        Command::WrapRepeat {
            node: node(child),
            id: node(id),
            plays,
            gap: (gap > 0).then(|| HoldRecipe {
                duration: duration(gap),
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
            anchor_policy: WrapAnchorPolicy::First,
        },
        next,
    )
}

#[test]
fn descent_retains_every_owner_and_fractional_clock_without_rounding() {
    let document = tree(
        &["prefix", "outer"],
        vec![
            ("prefix", hold(11)),
            ("outer", retime("inner", 1, 8, 13)),
            ("inner", retime("leaf", 2, 17, 9)),
            ("leaf", hold(20)),
        ],
    );
    let before = document.to_json().unwrap();
    let result = locate(
        &document,
        ExactRatio::new(103, 7).unwrap(),
        InsertionBias::Right,
    );
    let scopes: Vec<_> = result
        .scopes
        .iter()
        .map(|scope| {
            (
                scope.instance.node.as_str(),
                scope.position,
                scope.duration.frames(),
            )
        })
        .collect();
    assert_eq!(
        scopes,
        vec![
            ("root", ExactRatio::new(103, 7).unwrap(), 24),
            ("group", ExactRatio::new(103, 7).unwrap(), 24),
            ("outer", ExactRatio::new(26, 7).unwrap(), 13),
            ("inner", ExactRatio::integer(3), 9),
            ("leaf", ExactRatio::integer(7), 20),
        ]
    );
    assert_eq!(leaf(&result), ("leaf", ExactRatio::integer(7)));
    assert_eq!(document.to_json().unwrap(), before);
}

#[test]
fn sequence_bias_skips_empty_children_and_outward_edges_have_no_provider() {
    let document = tree(
        &["leading", "first", "empty", "last", "trailing"],
        vec![
            ("leading", BeatNode::sequence("Leading", vec![])),
            ("first", hold(2)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("last", hold(3)),
            ("trailing", BeatNode::sequence("Trailing", vec![])),
        ],
    );
    for (at, bias, expected, local) in [
        (0, InsertionBias::Right, "first", 0),
        (2, InsertionBias::Left, "first", 2),
        (2, InsertionBias::Right, "last", 0),
        (5, InsertionBias::Left, "last", 3),
    ] {
        let located = locate(&document, ExactRatio::integer(at), bias);
        assert_eq!(leaf(&located), (expected, ExactRatio::integer(local)));
        assert_eq!(
            located.scopes.last().unwrap().entry,
            BoundaryEntry::Sequence {
                index: if expected == "first" { 1 } else { 3 }
            }
        );
    }
    for (at, bias, terminal) in [
        (0, InsertionBias::Left, BoundaryTerminal::ProjectStart),
        (5, InsertionBias::Right, BoundaryTerminal::ProjectEnd),
    ] {
        let result = locate(&document, ExactRatio::integer(at), bias);
        assert_eq!(result.terminal, terminal);
        assert_eq!(result.scopes.len(), 1);
        assert_eq!(result.comparisons, 0);
    }
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        let empty = locate(&empty(), ExactRatio::ZERO, bias);
        assert_eq!(empty.scopes.len(), 1);
        assert_eq!(empty.comparisons, 0);
        assert_eq!(
            empty.terminal,
            if bias == InsertionBias::Left {
                BoundaryTerminal::ProjectStart
            } else {
                BoundaryTerminal::ProjectEnd
            }
        );
    }
}

#[test]
fn crop_edges_choose_only_visible_content_and_keep_gap_local_clock_separate() {
    let original = tree(&["leaf"], vec![("leaf", hold(3))]);
    let repeated = wrap(&original, "leaf", "repeat", 3, 2, "repeated");
    let mut json = serde_json::to_value(&repeated).unwrap();
    json["nodes"]["group"]["kind"]["children"] = serde_json::json!(["crop"]);
    json["nodes"]["crop"] = serde_json::to_value(retime("repeat", 3, 8, 5)).unwrap();
    let cropped = ProjectDocument::from_json(&json.to_string()).unwrap();
    // The visible domain begins in the first gap and ends at the second play.
    let gap = locate(&cropped, ExactRatio::ZERO, InsertionBias::Right);
    assert_eq!(gap.scopes.last().unwrap().instance.node, node("repeat"));
    assert_eq!(gap.scopes.last().unwrap().position, ExactRatio::integer(3));
    assert_eq!(
        gap.terminal,
        BoundaryTerminal::Gap {
            after: play(&cropped, "repeat", 0).iteration,
            position: ExactRatio::ZERO,
            duration: duration(2),
        }
    );
    let seam = locate(&cropped, ExactRatio::integer(2), InsertionBias::Left);
    assert!(matches!(seam.terminal, BoundaryTerminal::Gap {position, ..}
        if position == ExactRatio::integer(2)));
    let next = locate(&cropped, ExactRatio::integer(2), InsertionBias::Right);
    assert_eq!(leaf(&next), ("leaf", ExactRatio::ZERO));
    assert_eq!(
        next.scopes.last().unwrap().instance.repeats,
        vec![play(&cropped, "repeat", 1)]
    );
    assert_eq!(
        leaf(&locate(
            &cropped,
            ExactRatio::integer(5),
            InsertionBias::Left
        )),
        ("leaf", ExactRatio::integer(3))
    );
    assert_eq!(
        locate(&cropped, ExactRatio::ZERO, InsertionBias::Left).terminal,
        BoundaryTerminal::ProjectStart
    );
}

#[test]
fn nested_repeats_keep_outer_ownership_for_implicit_and_explicit_gap_branches() {
    let original = tree(&["leaf"], vec![("leaf", hold(3))]);
    let inner = wrap(&original, "leaf", "inner", 2, 1, "inner-wrap");
    let outer = wrap(&inner, "group", "outer", 3, 2, "outer-wrap");
    let result = locate(
        &outer,
        ExactRatio::new(25, 2).unwrap(),
        InsertionBias::Right,
    );
    assert_eq!(
        result
            .scopes
            .iter()
            .map(|scope| scope.instance.node.as_str())
            .collect::<Vec<_>>(),
        ["root", "outer", "group", "inner"]
    );
    let scope = result.scopes.last().unwrap();
    assert_eq!(scope.instance.repeats, [play(&outer, "outer", 1)]);
    assert_eq!(scope.position, ExactRatio::new(7, 2).unwrap());
    assert_eq!(
        result.terminal,
        BoundaryTerminal::Gap {
            after: play(&outer, "inner", 0).iteration,
            position: ExactRatio::new(1, 2).unwrap(),
            duration: duration(1),
        }
    );
    let first = play(&outer, "inner", 0).iteration;
    let owned = edit(
        &outer,
        Command::SetGapOverride {
            node: node("inner"),
            iteration: first,
            subtree: Subtree {
                root: node("gap"),
                nodes: BTreeMap::from([
                    (
                        node("gap"),
                        BeatNode::sequence("Gap group", vec![node("gap-leaf")]),
                    ),
                    (node("gap-leaf"), hold(2)),
                ]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "owned-gap",
    );
    let result = locate(
        &owned,
        ExactRatio::new(27, 2).unwrap(),
        InsertionBias::Right,
    );
    assert_eq!(leaf(&result), ("gap-leaf", ExactRatio::new(1, 2).unwrap()));
    assert_eq!(
        result.scopes.last().unwrap().instance.repeats,
        [play(&owned, "outer", 1), play(&owned, "inner", 0)]
    );
    assert_eq!(
        result.scopes[result.scopes.len() - 2].instance.node,
        node("gap")
    );
    assert_eq!(
        result.scopes[result.scopes.len() - 2].entry,
        BoundaryEntry::RepeatGap
    );
    assert_eq!(result.scopes[2].entry, BoundaryEntry::RepeatPlay);
}

#[test]
fn billion_play_lookup_keeps_stable_reordered_overrides_and_a_shared_work_budget() {
    let original = tree(&["leaf"], vec![("leaf", hold(3))]);
    let repeated = wrap(&original, "leaf", "repeat", 1_000_000_000, 2, "repeat");
    let selected = play(&repeated, "repeat", 500_000_000).iteration;
    let moved = edit(
        &repeated,
        Command::MovePlays {
            node: node("repeat"),
            start: 500_000_000,
            end: 500_000_001,
            destination: 0,
        },
        "move",
    );
    let overridden = edit(
        &moved,
        Command::SetPlayOverride {
            node: node("repeat"),
            iteration: selected.clone(),
            subtree: Subtree {
                root: node("custom"),
                nodes: BTreeMap::from([(node("custom"), hold(7))]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "override",
    );
    let result = locate(&overridden, ExactRatio::integer(4), InsertionBias::Right);
    assert_eq!(leaf(&result), ("custom", ExactRatio::integer(4)));
    assert_eq!(
        result.scopes.last().unwrap().instance.repeats[0].iteration,
        selected
    );
    let far = locate(
        &overridden,
        ExactRatio::integer(4_900_000_006),
        InsertionBias::Right,
    );
    assert_eq!(leaf(&far), ("leaf", ExactRatio::integer(2)));
    assert!(far.comparisons < 10);
    let index = AnchorIndex::new(&overridden).unwrap();
    let request = location_request(&overridden, far.position, far.bias);
    let exact = BoundaryQueryLimits {
        max_scopes: far.scopes.len(),
        max_comparisons: far.comparisons,
    };
    assert_eq!(index.locate_boundary(&request, exact).unwrap(), far);
    for limits in [
        BoundaryQueryLimits {
            max_scopes: exact.max_scopes - 1,
            ..exact
        },
        BoundaryQueryLimits {
            max_comparisons: exact.max_comparisons - 1,
            ..exact
        },
        BoundaryQueryLimits {
            max_scopes: 0,
            ..exact
        },
        BoundaryQueryLimits {
            max_comparisons: 0,
            ..exact
        },
    ] {
        assert_eq!(
            index.locate_boundary(&request, limits).unwrap_err().code,
            AnchorErrorCode::QueryLimit
        );
    }
}

#[test]
fn transparent_split_still_reports_full_retained_owner_clocks() {
    let original = tree(&["leaf"], vec![("leaf", hold(20))]);
    let split = edit(
        &original,
        Command::Split {
            node: node("leaf"),
            at: duration(7),
            identities: SplitIdentities {
                nodes: vec![node("left"), node("right"), node("copy")],
            },
        },
        "split",
    );
    let result = locate(&split, ExactRatio::integer(8), InsertionBias::Right);
    let last = result.scopes.last().unwrap();
    assert_eq!(last.duration, duration(20));
    assert_eq!(last.position, ExactRatio::integer(8));
    assert!(result.scopes.iter().any(|scope| matches!(
        split.nodes()[&scope.instance.node].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    )));
}

#[test]
fn stale_foreign_outside_and_overflow_queries_fail_without_mutation() {
    let document = tree(
        &["outer"],
        vec![("outer", retime("leaf", 0, 7, 13)), ("leaf", hold(7))],
    );
    let index = AnchorIndex::new(&document).unwrap();
    let request = location_request(&document, ExactRatio::ONE, InsertionBias::Right);
    let limits = BoundaryQueryLimits::default();
    let mut stale = request.clone();
    stale.expected_revision = revision("stale");
    let error = index.locate_boundary(&stale, limits).unwrap_err();
    assert_eq!(error.code, AnchorErrorCode::RevisionConflict);
    assert_eq!(error.current_revision, Some(document.revision_id().clone()));
    stale.project_id = ProjectId::new("different").unwrap();
    assert_eq!(
        index.locate_boundary(&stale, limits).unwrap_err().code,
        AnchorErrorCode::ProjectConflict
    );
    for position in [
        ExactRatio::new(-1, 2).unwrap(),
        ExactRatio::new(27, 2).unwrap(),
    ] {
        let outside = BoundaryLocationRequest {
            position,
            ..request.clone()
        };
        assert_eq!(
            index.locate_boundary(&outside, limits).unwrap_err().code,
            AnchorErrorCode::OutOfRange
        );
    }
    let overflow = BoundaryLocationRequest {
        position: ExactRatio::new(1, i128::MAX).unwrap(),
        ..request
    };
    assert_eq!(
        index.locate_boundary(&overflow, limits).unwrap_err().code,
        AnchorErrorCode::TimingOverflow
    );
}

#[test]
fn default_scope_budget_includes_the_root_at_maximum_document_depth() {
    let mut nodes = BTreeMap::new();
    for depth in 0..MAX_DOCUMENT_DEPTH {
        let name = if depth == 0 {
            "root".to_owned()
        } else {
            format!("owner-{depth}")
        };
        nodes.insert(
            node(&name),
            BeatNode::sequence("Owner", vec![node(&format!("owner-{}", depth + 1))]),
        );
    }
    nodes.insert(node(&format!("owner-{MAX_DOCUMENT_DEPTH}")), hold(1));
    let mut wire = serde_json::to_value(empty()).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let index = AnchorIndex::new(&document).unwrap();
    let request = location_request(
        &document,
        ExactRatio::new(1, 2).unwrap(),
        InsertionBias::Right,
    );
    let limits = BoundaryQueryLimits::default();
    let result = index.locate_boundary(&request, limits).unwrap();
    assert_eq!(result.scopes.len(), MAX_DOCUMENT_DEPTH + 1);
    assert_eq!(result.comparisons, MAX_DOCUMENT_DEPTH);
    assert_eq!(result.terminal, BoundaryTerminal::Node);
    assert_eq!(
        index
            .locate_boundary(
                &request,
                BoundaryQueryLimits {
                    max_scopes: MAX_DOCUMENT_DEPTH,
                    ..limits
                }
            )
            .unwrap_err()
            .code,
        AnchorErrorCode::QueryLimit
    );
}

proptest! {
    #[test]
    fn small_repeats_match_an_expanded_half_frame_boundary_reference(
        count in 1u32..20, frames in 1i64..30, gap in 0i64..10,
    ) {
        let original = tree(&["leaf"], vec![("leaf", hold(frames))]);
        let repeated = wrap(&original, "leaf", "repeat", count, gap, "repeat");
        // This deliberately enumerates only the small test reference, never
        // using RepeatLayout or rounding the production query coordinates.
        let mut expanded = Vec::new();
        for play_index in 0..count {
            for local in 0..frames*2 {
                expanded.push((play_index, false, local));
            }
            if play_index + 1 < count {
                for local in 0..gap*2 {
                    expanded.push((play_index, true, local));
                }
            }
        }
        let index = AnchorIndex::new(&repeated).unwrap();
        for (ordinal, (play_index, in_gap, local)) in expanded.iter().enumerate() {
            for (bias, edge) in [(InsertionBias::Right, 0), (InsertionBias::Left, 1)] {
                let position = ExactRatio::new(ordinal as i128 + edge, 2).unwrap();
                let request = location_request(&repeated, position, bias);
                let result = index.locate_boundary(&request, BoundaryQueryLimits::default()).unwrap();
                let expected = ExactRatio::new(i128::from(*local) + edge, 2).unwrap();
                let identity = play(&repeated, "repeat", *play_index).iteration;
                if *in_gap {
                    prop_assert_eq!(result.terminal, BoundaryTerminal::Gap {
                        after: identity, position: expected, duration: duration(gap),
                    });
                } else {
                    prop_assert_eq!(result.terminal, BoundaryTerminal::Node);
                    let last = result.scopes.last().unwrap();
                    prop_assert_eq!(last.position, expected);
                    prop_assert_eq!(&last.instance.repeats[0].iteration, &identity);
                }
            }
        }
    }
}
