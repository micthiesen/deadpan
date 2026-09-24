use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn recipe(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn source(frames: i64) -> BeatNode {
    BeatNode {
        label: "Original".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Stream {
                    asset: AssetId::new("media").unwrap(),
                    span: source_span(),
                },
                audio: None,
                link: LinkRelation::Independent,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        },
        audio_edges: Default::default(),
    }
}
fn source_span() -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
        SourceTimestamp {
            ticks: 100,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
    )
    .unwrap()
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Project", children.iter().map(|name| id(name)).collect()),
    );
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION, "project_id":"pause", "revision_id":"initial",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "assets":{"media": AssetRecord {
            label: "Media".into(), content_hash: "a".repeat(64), video: Some(source_span()),
            audio: None, still_image: false, frame_count: None, source_qualification: None,
        }}, "marks":{}, "overrides":{}, "nodes":nodes,
    }).to_string()).unwrap()
}
fn pool(prefix: &str, count: usize) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..count)
            .map(|index| id(&format!("{prefix}-{index}")))
            .collect(),
    }
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}
fn insertion(document: &ProjectDocument, name: &str, at: i64, frames: i64) -> CommandRequest {
    request(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: recipe(frames),
            id: id(&format!("pause-{name}")),
            identities: pool(name, 3),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}
fn edit(document: &ProjectDocument, request: CommandRequest) -> ProjectDocument {
    let request_wire = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&request_wire).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let wire = serde_json::to_string(&transaction).unwrap();
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&wire).unwrap(),
        transaction
    );
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    result
}
fn children(document: &ProjectDocument) -> &[NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        panic!("Sequence")
    };
    children
}
fn owner(document: &ProjectDocument, root_child: &NodeId) -> NodeId {
    match &document.nodes()[root_child].kind {
        NodeKind::Retime {
            child,
            purpose: RetimePurpose::Partition,
            ..
        } => child.clone(),
        _ => root_child.clone(),
    }
}
fn resolved(document: &ProjectDocument, owner: &NodeId) -> ResolvedAudioBinding {
    document
        .audio_bindings()
        .resolve(
            owner,
            &InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            MAX_AUDIO_BINDING_ENTRIES,
        )
        .unwrap()
}
fn reference_at_anchor(document: &ProjectDocument, owner: &NodeId) -> ExactRatio {
    let resolved = resolved(document, owner);
    let start = resolved
        .lattice
        .sample_boundary(resolved.lattice.local_support.start)
        .unwrap();
    ExactRatio::integer(start)
        .checked_add(
            resolved
                .resume
                .unwrap()
                .reference_local_delta
                .checked_div(resolved.lattice.local_frames_per_sample().unwrap())
                .unwrap(),
        )
        .unwrap()
}
fn split(document: &ProjectDocument, name: &str, target: NodeId, at: i64) -> ProjectDocument {
    edit(
        document,
        request(
            document,
            name,
            Command::Split {
                node: target,
                at: duration(at),
                identities: pool(name, 3),
            },
        ),
    )
}

#[test]
fn start_interior_end_and_empty_project_insert_one_reversible_pause() {
    for at in [0, 1, 3] {
        let before = tree(&["original"], vec![("original", source(3))]);
        let request = insertion(&before, "pause", at, 2);
        let transaction = apply(&before, &request).unwrap();
        assert_eq!(transaction.duration_delta, 2);
        let after = edit(&before, request);
        assert_eq!(after.duration().unwrap(), duration(5));
        assert_eq!(
            after.nodes()[&id("original")],
            before.nodes()[&id("original")]
        );
        assert_eq!(
            after.nodes()[&id("pause-pause")],
            BeatNode::hold("Pause", recipe(2))
        );
        assert_eq!(children(&after).len(), if at == 1 { 3 } else { 2 });
        let index = if at == 0 { 0 } else { 1 };
        assert_eq!(children(&after)[index], id("pause-pause"));
    }
    let before = tree(&[], vec![]);
    let after = edit(&before, insertion(&before, "first", 0, 2));
    assert_eq!(children(&after), &[id("pause-first")]);
    assert!(after.audio_bindings().is_empty());
}

#[test]
fn existing_split_seam_reanchors_without_manufacturing_another_split() {
    let before = tree(&["original"], vec![("original", source(2))]);
    let before = split(&before, "split", id("original"), 1);
    let old_children = children(&before).to_vec();
    let suffix = owner(&before, &old_children[1]);
    let after = edit(&before, insertion(&before, "insert", 1, 1));
    assert_eq!(after.nodes().len(), before.nodes().len() + 1);
    assert_eq!(
        children(&after),
        &[
            old_children[0].clone(),
            id("pause-insert"),
            old_children[1].clone()
        ]
    );
    assert_eq!(
        reference_at_anchor(&after, &suffix),
        ExactRatio::integer(1602)
    );
    let binding = &after.audio_bindings().bindings()[&suffix];
    let resume = binding.resume.as_ref().unwrap();
    assert_eq!(resume.local_boundary, ExactRatio::ONE);
    assert_eq!(resume.phase.terms.len(), 1);
    assert_eq!(resume.phase.terms[0].placement.reference.physical, suffix);
    assert_eq!(
        resolved(&after, &suffix).lattice.local_support,
        ExactRatio::ZERO..ExactRatio::integer(2)
    );
}

#[test]
fn first_pause_locks_an_automatic_empty_project_before_final_validation() {
    let before = ProjectDocument::new_automatic(
        ProjectId::new("automatic-pause").unwrap(),
        revision("initial"),
        id("root"),
    )
    .unwrap();
    let after = edit(&before, insertion(&before, "first", 0, 2));
    assert_eq!(after.duration().unwrap(), duration(2));
    assert_eq!(children(&after), &[id("pause-first")]);
    assert_eq!(after.presentation_basis(), before.presentation_basis());
    assert_eq!(after.basis_state().rate_origin, FrameRateOrigin::TimedEdit);
    assert!(
        !after
            .audio_bindings()
            .bindings()
            .contains_key(&id("pause-first"))
    );
    after.validate().unwrap();
}

#[test]
fn every_shifted_fragment_gets_its_own_old_entry_and_existing_lattice_survives() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let before = split(&before, "first-cut", id("original"), 1);
    let target = children(&before)[1].clone();
    let before = split(&before, "second-cut", target, 1);
    let old_owners: Vec<_> = children(&before)
        .iter()
        .map(|child| owner(&before, child))
        .collect();
    let after = edit(&before, insertion(&before, "first-pause", 1, 1));
    assert!(
        after.audio_bindings().bindings()[&old_owners[0]]
            .resume
            .is_none()
    );
    assert_eq!(
        reference_at_anchor(&after, &old_owners[1]),
        ExactRatio::integer(1602)
    );
    assert_eq!(
        reference_at_anchor(&after, &old_owners[2]),
        ExactRatio::integer(3203)
    );
    let again = edit(&after, insertion(&after, "second-pause", 1, 1));
    for old in &old_owners {
        assert_eq!(
            again.audio_bindings().bindings()[old].lattice,
            after.audio_bindings().bindings()[old].lattice
        );
    }
    assert_eq!(
        reference_at_anchor(&again, &old_owners[1]),
        ExactRatio::integer(1602)
    );
    assert_eq!(
        reference_at_anchor(&again, &old_owners[2]),
        ExactRatio::integer(3203)
    );
}

#[test]
fn repeated_interior_insertion_composes_current_phase_on_one_immutable_lattice() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let first = edit(&before, insertion(&before, "first", 1, 1));
    let first_suffix = owner(&first, &children(&first)[2]);
    let old_lattice = first.audio_bindings().bindings()[&first_suffix]
        .lattice
        .clone();
    let second = edit(&first, insertion(&first, "second", 3, 1));
    let second_suffix = owner(&second, &children(&second)[4]);
    let binding = &second.audio_bindings().bindings()[&second_suffix];
    assert_eq!(binding.lattice, old_lattice);
    let resume = binding.resume.as_ref().unwrap();
    assert_eq!(resume.local_boundary, ExactRatio::integer(2));
    assert_eq!(resume.phase.terms.len(), 2);
    assert_eq!(
        resume.phase.terms[1].placement.reference.physical,
        first_suffix
    );
    assert_eq!(
        resume.phase.terms[1].placement.reference.timing.allocation,
        revision("second")
    );
    // First entry1602 plus the CURRENT interval B(3)-B(2)=1602.
    // Recomputing Original B(2)=3203 would lose one sample.
    assert_eq!(
        reference_at_anchor(&second, &second_suffix),
        ExactRatio::integer(3204)
    );
}

#[test]
fn ordinary_hold_interior_and_its_partition_can_be_inserted_into_again() {
    let before = tree(
        &["old"],
        vec![("old", BeatNode::hold("Old pause", recipe(4)))],
    );
    let first = edit(&before, insertion(&before, "first", 1, 1));
    let second = edit(&first, insertion(&first, "second", 3, 1));
    let suffix = owner(&second, &children(&second)[4]);
    assert_eq!(second.nodes()[&suffix], before.nodes()[&id("old")]);
    assert_eq!(
        reference_at_anchor(&second, &suffix),
        ExactRatio::integer(3204)
    );
    assert_eq!(second.duration().unwrap(), duration(6));
}

#[test]
fn fully_bound_input_keeps_a_phase_only_clock_until_terms_exist_and_prunes_unused_end_clock() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let state = capture_unbound_audio_bindings(
        &before,
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["audio_bindings"] = serde_json::to_value(&state).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let interior = edit(&before, insertion(&before, "middle", 1, 1));
    let suffix = owner(&interior, &children(&interior)[2]);
    assert_eq!(
        reference_at_anchor(&interior, &suffix),
        ExactRatio::integer(1602)
    );
    assert_eq!(interior.audio_bindings().timings().len(), 2);
    assert_eq!(
        interior.audio_bindings().bindings()[&suffix].lattice,
        state.bindings()[&id("original")].lattice
    );
    let end = edit(&before, insertion(&before, "end", 4, 1));
    assert_eq!(end.audio_bindings(), &state);
    let start = edit(&before, insertion(&before, "start", 0, 1));
    assert_eq!(start.audio_bindings().timings(), state.timings());
}

fn mark(position: i64, bias: InsertionBias, pinned: bool) -> Mark {
    Mark {
        owner: id("root"),
        label: "Cue".into(),
        boundary: BoundaryAnchor {
            coordinate: if pinned {
                Anchor::Sequence {
                    frame: ProjectFrame(position),
                }
            } else {
                Anchor::Local {
                    node: id("root"),
                    position: ExactRatio::integer(position),
                }
            },
            bias,
        },
        loss_policy: AnchorLossPolicy::DeleteOwned,
        state: MarkState::Bound,
        fragments: vec![],
    }
}
fn mark_position(document: &ProjectDocument, name: &str) -> ExactRatio {
    let selection = AnchorIndex::new(document)
        .unwrap()
        .resolve(&SelectionRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id: MarkId::new(name).unwrap(),
                    occurrence: None,
                },
            },
        })
        .unwrap();
    let ResolvedSelectionKind::Point { point } = selection.selection else {
        panic!("point")
    };
    point.exact_frame
}

#[test]
fn split_and_insertion_compose_mark_bias_and_keep_sequence_pins() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let mut wire = serde_json::to_value(before).unwrap();
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        ("left", mark(1, InsertionBias::Left, false)),
        ("right", mark(1, InsertionBias::Right, false)),
        ("content", mark(3, InsertionBias::Right, false)),
        ("pinned", mark(3, InsertionBias::Right, true)),
    ]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, insertion(&before, "insert", 1, 2));
    assert_eq!(mark_position(&after, "left"), ExactRatio::ONE);
    assert_eq!(mark_position(&after, "right"), ExactRatio::integer(3));
    assert_eq!(mark_position(&after, "content"), ExactRatio::integer(5));
    assert_eq!(mark_position(&after, "pinned"), ExactRatio::integer(3));
}

#[test]
fn zero_bounds_stale_revision_and_identity_errors_leave_input_unchanged() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let saved = before.to_json().unwrap();
    for (at, frames, code) in [
        (0, 0, EditErrorCode::InvalidDuration),
        (-1, 1, EditErrorCode::InvalidCommand),
        (5, 1, EditErrorCode::InvalidCommand),
    ] {
        assert_eq!(
            apply(&before, &insertion(&before, "bad", at, frames))
                .unwrap_err()
                .code,
            code
        );
    }
    let mut bad = insertion(&before, "bad", 1, 1);
    bad.expected_revision = revision("stale");
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    let mut bad = insertion(&before, "bad", 1, 1);
    let Command::InsertTime { timing, .. } = &mut bad.command else {
        unreachable!()
    };
    timing.allocation = revision("foreign");
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    let Command::InsertTime {
        id: hold_id,
        identities,
        timing,
        ..
    } = &mut bad.command
    else {
        unreachable!()
    };
    timing.allocation = revision("bad");
    identities.nodes[0] = hold_id.clone();
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
    let mut bad = insertion(&before, "bad", 1, 1);
    let Command::InsertTime { identities, .. } = &mut bad.command else {
        unreachable!()
    };
    identities.nodes.clear();
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    assert_eq!(before.to_json().unwrap(), saved);
    edit(&before, insertion(&before, "valid", 1, 1));
}

#[test]
fn unsupported_shifted_structures_and_any_nonempty_gap_fail_atomically() {
    let nested = tree(
        &["nested", "last"],
        vec![
            ("nested", BeatNode::sequence("Nested", vec![id("inner")])),
            ("inner", source(2)),
            ("last", source(2)),
        ],
    );
    assert_eq!(
        apply(&nested, &insertion(&nested, "bad", 1, 1))
            .unwrap_err()
            .code,
        EditErrorCode::InvalidCommand
    );
    // A fully preceding gap-free nested scope is not shifted and is safe.
    edit(&nested, insertion(&nested, "suffix", 2, 1));
    let repeat = |gap| BeatNode {
        label: "Repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("inner"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap,
        },
    };
    let repeated = tree(
        &["repeat"],
        vec![("repeat", repeat(None)), ("inner", source(2))],
    );
    assert!(
        apply(&repeated, &insertion(&repeated, "bad", 0, 1))
            .unwrap_err()
            .message
            .contains("cannot yet shift")
    );
    let gapped = tree(
        &["repeat"],
        vec![("repeat", repeat(Some(recipe(1)))), ("inner", source(2))],
    );
    // Even an unaffected prefix gap is explicitly refused by full capture.
    assert!(
        apply(&gapped, &insertion(&gapped, "bad", 5, 1))
            .unwrap_err()
            .message
            .contains("gap binding ownership")
    );
}

#[test]
fn aggregate_identity_and_composed_phase_limits_fail_before_a_transaction() {
    let before = tree(&["original"], vec![("original", source(4))]);
    let mut bad = insertion(&before, "bad", 1, 1);
    let Command::InsertTime { identities, .. } = &mut bad.command else {
        unreachable!()
    };
    identities.nodes = vec![id("unused"); MAX_DOCUMENT_NODES + 1];
    assert_eq!(
        apply(&before, &bad).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
    let timing = AudioTimingId {
        allocation: revision("captured"),
        ordinal: 0,
    };
    let state = capture_unbound_audio_bindings(&before, timing).unwrap();
    let mut bindings = state.bindings().clone();
    let binding = bindings.get_mut(&id("original")).unwrap();
    binding.resume = Some(AudioResume {
        local_boundary: ExactRatio::ZERO,
        phase: AudioLocalPhase {
            constant: ExactRatio::ZERO,
            terms: vec![
                AudioPhaseTerm {
                    placement: binding.lattice.clone(),
                    from_local: ExactRatio::ZERO,
                    to_local: ExactRatio::ZERO,
                };
                MAX_AUDIO_BINDING_TERMS
            ],
        },
    });
    let state = AudioBindingState::new(
        state
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
    )
    .unwrap();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        apply(&before, &insertion(&before, "bad", 1, 1))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}

#[test]
fn duration_overflow_and_nonfallback_provider_are_rejected_before_capture() {
    let before = tree(&["original"], vec![("original", source(i64::MAX))]);
    assert_eq!(
        apply(&before, &insertion(&before, "overflow", 0, 1))
            .unwrap_err()
            .code,
        EditErrorCode::TimingOverflow
    );
    let before = tree(&["original"], vec![("original", source(4))]);
    let mut request = insertion(&before, "provider", 1, 1);
    let Command::InsertTime { hold, .. } = &mut request.command else {
        unreachable!()
    };
    hold.video = HoldVideo::Accepted {
        asset: AssetId::new("media").unwrap(),
        frames: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
    };
    let error = apply(&before, &request).unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains("Background or Freeze"));
}
