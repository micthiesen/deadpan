use deadpan_core::*;
use serde_json::{Value, json};

fn node(id: &str) -> NodeId {
    NodeId::new(id).unwrap()
}

fn local(host: &str, position: ExactRatio) -> Anchor {
    Anchor::Local {
        node: node(host),
        position,
    }
}

fn fixture() -> Value {
    json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION,
        "project_id": "marks", "revision_id": "initial",
        "presentation_basis": {"width":16,"height":16,"frame_rate":{"numerator":30,"denominator":1},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "assets":{}, "marks":{}, "overrides":{},
        "nodes": {
            "root":{"label":"Sequence","kind":{"type":"sequence","children":["left","right"]}},
            "left":{"label":"Left","kind":{"type":"retime","purpose":"partition","child":"a","duration":5,"mapping":{"start":0,"end":5},"pitch":"preserve"}},
            "right":{"label":"Right","kind":{"type":"retime","purpose":"partition","child":"b","duration":5,"mapping":{"start":5,"end":10},"pitch":"preserve"}},
            "a":{"label":"Context A","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}},
            "b":{"label":"Context B","kind":{"type":"hold","recipe":{"duration":10,"video":{"type":"background"},"audio":{"type":"silence"}}}}
        }
    })
}

fn mark(coordinate: Anchor, fragments: Vec<MarkFragment>, bias: InsertionBias) -> Mark {
    Mark {
        owner: node("a"),
        label: "Cue".into(),
        boundary: BoundaryAnchor { coordinate, bias },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
        state: MarkState::Bound,
        fragments,
    }
}

fn fragment(owner: &str, coordinate: Anchor) -> MarkFragment {
    MarkFragment {
        owner: node(owner),
        coordinate,
        state: MarkState::Bound,
    }
}

fn document(mut wire: Value, mark: Mark) -> ProjectDocument {
    wire["marks"]["cue"] = serde_json::to_value(mark).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn query(
    document: &ProjectDocument,
    occurrence: Option<InstancePath>,
) -> Result<ResolvedBoundary, AnchorError> {
    let result = AnchorIndex::new(document)
        .unwrap()
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id: MarkId::new("cue").unwrap(),
                    occurrence,
                },
            },
        })?;
    let ResolvedSelectionKind::Point { point } = result.selection else {
        panic!("point")
    };
    Ok(point)
}

fn path(host: &str) -> InstancePath {
    InstancePath {
        node: node(host),
        repeats: vec![],
    }
}

#[test]
fn internal_partition_seams_choose_one_binding_by_bias_and_keep_external_edges() {
    for (position, bias, binding) in [
        (0, InsertionBias::Left, 0),
        (0, InsertionBias::Right, 0),
        (3, InsertionBias::Right, 0),
        (5, InsertionBias::Left, 0),
        (5, InsertionBias::Right, 1),
        (7, InsertionBias::Left, 1),
        (10, InsertionBias::Left, 1),
        (10, InsertionBias::Right, 1),
    ] {
        let document = document(
            fixture(),
            mark(
                local("a", ExactRatio::integer(position)),
                vec![fragment("b", local("b", ExactRatio::integer(position)))],
                bias,
            ),
        );
        let result = query(&document, None).unwrap();
        assert_eq!(result.exact_frame, ExactRatio::integer(position));
        assert_eq!(result.mark.unwrap().bindings, [binding]);
        // Explicit scope must not cause the rejected half to become visible.
        let hidden = if binding == 0 { "b" } else { "a" };
        assert_eq!(
            query(&document, Some(path(hidden))).unwrap_err().code,
            AnchorErrorCode::OutsideMapping
        );
    }
}

#[test]
fn ordinary_crop_endpoints_are_unchanged_and_equal_results_retain_all_bindings() {
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        let mut wire = fixture();
        wire["nodes"]["left"]["kind"]["purpose"] = json!("edit");
        wire["nodes"]["right"]["kind"]["purpose"] = json!("edit");
        let document = document(
            wire,
            mark(
                local("a", ExactRatio::integer(5)),
                vec![fragment("b", local("b", ExactRatio::integer(5)))],
                bias,
            ),
        );
        let result = query(&document, None).unwrap();
        assert_eq!(result.exact_frame, ExactRatio::integer(5));
        let metadata = result.mark.unwrap();
        assert_eq!(metadata.id, MarkId::new("cue").unwrap());
        assert_eq!(metadata.bindings, [0, 1]);
    }
}

#[test]
fn distinct_exact_boundaries_are_ambiguous_even_when_they_round_to_one_frame() {
    let document = document(
        fixture(),
        mark(
            local("root", ExactRatio::new(21, 10).unwrap()),
            vec![fragment(
                "b",
                local("root", ExactRatio::new(22, 10).unwrap()),
            )],
            InsertionBias::Right,
        ),
    );
    let index = AnchorIndex::new(&document).unwrap();
    for binding in document.marks()[&MarkId::new("cue").unwrap()].bindings() {
        let result = index
            .resolve_target(&AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: binding.coordinate,
                    bias: InsertionBias::Right,
                },
                occurrence: None,
            })
            .unwrap();
        assert_eq!(result.frame, ProjectFrame(2));
        assert!(result.mark.is_none());
    }
    assert_eq!(
        query(&document, None).unwrap_err().code,
        AnchorErrorCode::MarkAmbiguous
    );
}

#[test]
fn unresolved_primary_does_not_hide_bound_fragment_or_implicitly_reattach() {
    let mut cue = mark(
        local("a", ExactRatio::integer(3)),
        vec![fragment("b", local("b", ExactRatio::integer(7)))],
        InsertionBias::Right,
    );
    cue.state = MarkState::Unresolved {
        reason: MarkLossReason::OwnerMissing,
    };
    let document = document(fixture(), cue.clone());
    assert_eq!(query(&document, None).unwrap().mark.unwrap().bindings, [1]);
    assert_eq!(document.marks()[&MarkId::new("cue").unwrap()], cue);
    cue.fragments[0].state = MarkState::Unresolved {
        reason: MarkLossReason::OutsideMapping,
    };
    let document = self::document(fixture(), cue);
    assert_eq!(
        query(&document, None).unwrap_err().code,
        AnchorErrorCode::MarkUnresolved
    );
}

#[test]
fn owner_lifetime_is_independent_of_coordinate_visibility() {
    let document = document(
        fixture(),
        mark(
            local("b", ExactRatio::integer(7)),
            vec![],
            InsertionBias::Right,
        ),
    );
    // The coordinate's owner a is hidden at local 7. Ownership is authored node
    // lifetime, not a demand that the owner also have a visible play there.
    assert_eq!(
        query(&document, None).unwrap().exact_frame,
        ExactRatio::integer(7)
    );
}

#[test]
fn fully_scoped_occurrence_binding_uses_bias_without_changing_stored_lifetime() {
    let document = document(
        fixture(),
        mark(
            Anchor::Occurrence {
                instance: path("b"),
                position: ExactRatio::integer(5),
            },
            vec![],
            InsertionBias::Left,
        ),
    );
    assert_eq!(
        document.marks()[&MarkId::new("cue").unwrap()].state,
        MarkState::Bound
    );
    assert_eq!(
        query(&document, None).unwrap_err().code,
        AnchorErrorCode::OutsideMapping
    );
}

#[test]
fn repeated_binding_cannot_be_inferred_away_by_an_unrepeated_match() {
    let mut wire = fixture();
    wire["nodes"]["left"]["kind"]["child"] = json!("repeat");
    let order = IterationOrder::new(RevisionId::new("allocation").unwrap(), 1).unwrap();
    wire["nodes"]["repeat"] = json!({"label":"Repeat","kind":{"type":"repeat","child":"a","iterations":order,"gap":null}});
    let document = document(
        wire,
        mark(
            local("a", ExactRatio::integer(3)),
            vec![fragment("b", local("b", ExactRatio::integer(7)))],
            InsertionBias::Right,
        ),
    );
    assert_eq!(
        query(&document, None).unwrap_err().code,
        AnchorErrorCode::OccurrenceRequired
    );
    assert_eq!(
        query(&document, Some(path("b")))
            .unwrap()
            .mark
            .unwrap()
            .bindings,
        [1]
    );
    let repeated = InstancePath {
        node: node("a"),
        repeats: vec![RepeatInstance {
            node: node("repeat"),
            iteration: order.at(0).unwrap(),
        }],
    };
    assert_eq!(
        query(&document, Some(repeated))
            .unwrap()
            .mark
            .unwrap()
            .bindings,
        [0]
    );
    assert_eq!(
        query(&document, Some(path("a"))).unwrap_err().code,
        AnchorErrorCode::OccurrenceInvalid
    );
}

#[test]
fn source_clock_bindings_share_explicit_source_scope_independently_of_ownership() {
    let mut wire = fixture();
    let clock = SourceTimeBase::new(1, 30).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: clock,
        },
        SourceTimestamp {
            ticks: 10,
            time_base: clock,
        },
    )
    .unwrap();
    let asset = AssetId::new("original").unwrap();
    wire["assets"]["original"] = serde_json::to_value(AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    for id in ["a", "b"] {
        wire["nodes"][id]["kind"] = serde_json::to_value(NodeKind::Source {
            source: SourceNode {
                duration: FrameDuration::new(10).unwrap(),
                video: SourceVideo::Stream {
                    asset: asset.clone(),
                    span,
                },
                audio: None,
                link: LinkRelation::Independent,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        })
        .unwrap();
    }
    let coordinate = Anchor::Source {
        asset,
        moment: SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks: 5,
                time_base: clock,
            },
        },
    };
    let document = document(
        wire,
        mark(
            coordinate.clone(),
            vec![fragment("b", coordinate)],
            InsertionBias::Right,
        ),
    );
    assert_eq!(
        query(&document, None).unwrap_err().code,
        AnchorErrorCode::OccurrenceRequired
    );
    let resolved = query(&document, Some(path("b"))).unwrap();
    assert_eq!(resolved.exact_frame, ExactRatio::integer(5));
    assert_eq!(resolved.mark.unwrap().bindings, [0, 1]);
    assert_eq!(
        query(&document, Some(path("a"))).unwrap_err().code,
        AnchorErrorCode::OutsideMapping
    );
    assert_eq!(
        query(&document, Some(path("right"))).unwrap_err().code,
        AnchorErrorCode::SourceUnavailable
    );
}

#[test]
fn hidden_partition_bindings_do_not_expand_a_billion_repeat_plays() {
    let mut wire = fixture();
    wire["nodes"]["left"]["kind"]["child"] = json!("repeat");
    let order = IterationOrder::new(RevisionId::new("allocation").unwrap(), 1_000_000_000).unwrap();
    wire["nodes"]["repeat"] = json!({"label":"Repeat","kind":{"type":"repeat","child":"a","iterations":order,"gap":null}});
    let document = document(
        wire,
        mark(
            local("a", ExactRatio::integer(3)),
            vec![fragment("b", local("b", ExactRatio::integer(7)))],
            InsertionBias::Right,
        ),
    );
    assert!(document.to_json().unwrap().len() < 8_000);
    for (ordinal, visible) in [(0, true), (999_999_999, false)] {
        let path = InstancePath {
            node: node("a"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration: order.at(ordinal).unwrap(),
            }],
        };
        let result = query(&document, Some(path));
        if visible {
            assert_eq!(result.unwrap().exact_frame, ExactRatio::integer(3));
        } else {
            assert_eq!(result.unwrap_err().code, AnchorErrorCode::OutsideMapping);
        }
    }
}
