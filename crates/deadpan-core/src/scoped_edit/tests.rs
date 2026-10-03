use super::*;
use crate::*;
use std::collections::BTreeMap;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn mark(name: &str) -> MarkId {
    MarkId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> HoldRecipe {
    HoldRecipe {
        duration: frames(value),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn repeated(name: &str, child: &str, plays: u32) -> BeatNode {
    let mut beat = BeatNode::sequence(name, vec![]);
    beat.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(revision(name), plays).unwrap(),
        gap: Some(hold(1)),
    };
    beat
}
fn fixture() -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("scoped").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    document.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("root", vec![node("outer")]),
        ),
        (node("outer"), repeated("outer", "body", 3)),
        (
            node("body"),
            BeatNode::sequence("body", vec![node("lead"), node("inner"), node("tail")]),
        ),
        (node("lead"), BeatNode::hold("lead", hold(2))),
        (node("inner"), repeated("inner", "inside", 2)),
        (
            node("inside"),
            BeatNode::sequence("inside", vec![node("a"), node("b")]),
        ),
        (node("a"), BeatNode::hold("a", hold(4))),
        (node("b"), BeatNode::hold("b", hold(3))),
        (node("tail"), BeatNode::hold("tail", hold(2))),
    ]);
    document.validate().unwrap();
    document
}
fn play(name: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(name),
        ordinal,
    }
}
fn target(outer: Option<u32>, inner: Option<u32>) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: node("a"),
        repeats: [("outer", outer), ("inner", inner)]
            .into_iter()
            .map(|(name, selected)| RepeatEditStep {
                repeat: node(name),
                branch: selected.map_or(RepeatEditBranch::Default, |ordinal| {
                    RepeatEditBranch::Play {
                        iteration: play(name, ordinal),
                    }
                }),
            })
            .collect(),
    }
}
fn instance(outer: u32, inner: u32) -> InstancePath {
    InstancePath {
        node: node("a"),
        repeats: vec![
            RepeatInstance {
                node: node("outer"),
                iteration: play("outer", outer),
            },
            RepeatInstance {
                node: node("inner"),
                iteration: play("inner", inner),
            },
        ],
    }
}
fn rename(label: &str) -> ScopedNodeEdit {
    ScopedNodeEdit::Rename {
        label: label.into(),
    }
}
fn request(
    document: &ProjectDocument,
    target: ScopedNodeTarget,
    edit: ScopedNodeEdit,
) -> CommandRequest {
    let requirements = document.scoped_edit_requirements(&target, &edit).unwrap();
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(&format!("{}-next", document.revision_id())),
        command: Command::EditScoped {
            target,
            edit,
            identities: OccurrenceIdentities {
                nodes: (0..requirements.nodes)
                    .map(|i| node(&format!("{}-n{i}", document.revision_id())))
                    .collect(),
                marks: (0..requirements.marks)
                    .map(|i| mark(&format!("{}-m{i}", document.revision_id())))
                    .collect(),
            },
        },
    }
}
fn prepare(document: &ProjectDocument, request: &CommandRequest) -> PreparedScopedEdit {
    let encoded = serde_json::to_string(request).unwrap();
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&encoded).unwrap(),
        *request
    );
    let prepared = prepare_scoped_edit(document, request).unwrap();
    assert_eq!(
        prepared.transaction,
        crate::apply(document, request).unwrap()
    );
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        *document
    );
    assert_eq!(
        ProjectDocument::from_json(&prepared.document.to_json().unwrap()).unwrap(),
        prepared.document
    );
    prepared.target.validate(&prepared.document).unwrap();
    prepared
}

#[test]
fn each_default_play_combination_isolates_only_its_declared_scope_and_maps_presentations() {
    let document = fixture();
    for (outer, inner, expected_nodes) in [
        (None, None, 0),
        (None, Some(1), 3),
        (Some(1), None, 7),
        (Some(1), Some(1), 10),
    ] {
        let selected = target(outer, inner);
        let edit = rename("changed");
        assert_eq!(
            document.scoped_edit_requirements(&selected, &edit).unwrap(),
            ScopedEditRequirements {
                nodes: expected_nodes,
                marks: 0,
                unchanged: false
            }
        );
        let prepared = prepare(&document, &request(&document, selected.clone(), edit));
        assert_eq!(
            prepared.document.nodes().len(),
            document.nodes().len() + expected_nodes
        );
        assert_eq!(
            prepared.document.duration().unwrap(),
            document.duration().unwrap()
        );
        for a in 0..3 {
            for b in 0..2 {
                let original = instance(a, b);
                let matches = outer.is_none_or(|v| v == a) && inner.is_none_or(|v| v == b);
                assert_eq!(
                    selected.matches_instance(&document, &original).unwrap(),
                    matches
                );
                let mapped = prepared.map_instance(&document, &original).unwrap();
                assert_eq!(
                    prepared.document.nodes()[&mapped.node].label,
                    if matches { "changed" } else { "a" }
                );
                assert_eq!(
                    prepared
                        .target
                        .matches_instance(&prepared.document, &mapped)
                        .unwrap(),
                    matches
                );
                let before = AnchorIndex::new(&document)
                    .unwrap()
                    .resolve_target(&AnchorTarget {
                        boundary: BoundaryAnchor {
                            coordinate: Anchor::Occurrence {
                                instance: original,
                                position: ExactRatio::new(3, 2).unwrap(),
                            },
                            bias: InsertionBias::Right,
                        },
                        occurrence: None,
                    })
                    .unwrap();
                let after = AnchorIndex::new(&prepared.document)
                    .unwrap()
                    .resolve_target(&AnchorTarget {
                        boundary: BoundaryAnchor {
                            coordinate: Anchor::Occurrence {
                                instance: mapped,
                                position: ExactRatio::new(3, 2).unwrap(),
                            },
                            bias: InsertionBias::Right,
                        },
                        occurrence: None,
                    })
                    .unwrap();
                assert_eq!(before.exact_frame, after.exact_frame);
            }
        }
        let second = request(&prepared.document, prepared.target.clone(), rename("again"));
        let Command::EditScoped { identities, .. } = &second.command else {
            panic!()
        };
        assert_eq!(*identities, OccurrenceIdentities::default());
        let next = prepare(&prepared.document, &second);
        assert_eq!(next.document.nodes().len(), prepared.document.nodes().len());
        assert_eq!(next.document.overrides(), prepared.document.overrides());
    }
}

fn add_owned_mark(
    document: &mut ProjectDocument,
    name: &str,
    coordinate: Anchor,
    state: MarkState,
) {
    document.marks.insert(
        mark(name),
        Mark {
            owner: node("a"),
            label: name.into(),
            boundary: BoundaryAnchor {
                coordinate,
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state,
            fragments: vec![],
        },
    );
}

#[test]
fn nested_mark_counts_are_exact_and_concrete_marks_follow_wildcard_outer_defaults() {
    let mut document = fixture();
    add_owned_mark(
        &mut document,
        "local",
        Anchor::Local {
            node: node("a"),
            position: ExactRatio::ONE,
        },
        MarkState::Bound,
    );
    document
        .marks
        .get_mut(&mark("local"))
        .unwrap()
        .fragments
        .push(MarkFragment {
            owner: node("b"),
            coordinate: Anchor::Local {
                node: node("b"),
                position: ExactRatio::ONE,
            },
            state: MarkState::Bound,
        });
    add_owned_mark(
        &mut document,
        "unresolved",
        Anchor::Local {
            node: node("gone"),
            position: ExactRatio::ONE,
        },
        MarkState::Unresolved {
            reason: MarkLossReason::HostMissing,
        },
    );
    for a in 0..3 {
        for b in 0..2 {
            add_owned_mark(
                &mut document,
                &format!("concrete-{a}-{b}"),
                Anchor::Occurrence {
                    instance: instance(a, b),
                    position: ExactRatio::ONE,
                },
                MarkState::Bound,
            );
        }
    }
    document.validate().unwrap();
    for (outer, expected_marks) in [(None, 2), (Some(1), 4)] {
        let selected = target(outer, Some(1));
        let edit = rename("marked");
        assert_eq!(
            document
                .scoped_edit_requirements(&selected, &edit)
                .unwrap()
                .marks,
            expected_marks
        );
        let prepared = prepare(&document, &request(&document, selected, edit));
        assert_eq!(
            prepared.document.marks().len(),
            document.marks().len() + expected_marks
        );
        assert_eq!(
            prepared.document.marks()[&mark("local")],
            document.marks()[&mark("local")]
        );
        for a in 0..3 {
            for b in 0..2 {
                let name = mark(&format!("concrete-{a}-{b}"));
                let mapped = prepared.map_instance(&document, &instance(a, b)).unwrap();
                let stored = &prepared.document.marks()[&name];
                assert_eq!(stored.owner, mapped.node);
                assert_eq!(
                    stored.boundary.coordinate,
                    Anchor::Occurrence {
                        instance: mapped,
                        position: ExactRatio::ONE,
                    }
                );
            }
        }
        for copied in prepared
            .document
            .marks()
            .values()
            .filter(|mark| mark.label == "unresolved")
        {
            assert!(matches!(copied.state, MarkState::Unresolved { .. }));
            assert_eq!(
                copied.boundary.coordinate,
                Anchor::Local {
                    node: node("gone"),
                    position: ExactRatio::ONE
                }
            );
        }
    }
}

#[test]
fn default_edits_preserve_existing_overrides_and_dormant_defaults_have_no_fake_instance() {
    let mut document = fixture();
    let mut overrides = Vec::new();
    for ordinal in 0..2 {
        let id = node(&format!("alternate-{ordinal}"));
        document
            .nodes
            .insert(id.clone(), BeatNode::hold("own play", hold(7)));
        overrides.push(PlayOverride {
            iteration: play("inner", ordinal),
            root: id,
        });
    }
    document
        .overrides
        .insert(node("inner"), PlayOverrides::try_from(overrides).unwrap());
    document.validate().unwrap();
    let selected = target(None, None);
    selected.validate(&document).unwrap();
    assert!(
        selected
            .matches_instance(&document, &instance(0, 0))
            .is_err()
    );
    let prepared = prepare(
        &document,
        &request(&document, selected, rename("dormant template")),
    );
    assert_eq!(prepared.document.overrides(), document.overrides());
    assert_eq!(prepared.document.nodes().len(), document.nodes().len());
    for ordinal in 0..2 {
        assert_eq!(
            prepared.document.nodes()[&node(&format!("alternate-{ordinal}"))],
            document.nodes()[&node(&format!("alternate-{ordinal}"))]
        );
    }
    // The nested Default is still meaningful inside a concrete outer play.
    let nested = prepare(
        &document,
        &request(
            &document,
            target(Some(1), None),
            rename("one outer template"),
        ),
    );
    assert_ne!(nested.target.node, node("a"));
    let inner = &nested.target.repeats[1].repeat;
    assert_eq!(nested.document.overrides()[inner].len(), 2);
    assert_eq!(nested.document.nodes()[&node("a")].label, "a");
}

#[test]
fn explicit_final_gap_is_editable_but_is_not_a_default_or_a_project_occurrence() {
    let mut document = fixture();
    document
        .nodes
        .insert(node("owned-gap"), BeatNode::hold("final gap", hold(2)));
    document.gap_overrides.insert(
        node("inner"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play("inner", 1),
            root: node("owned-gap"),
        }])
        .unwrap(),
    );
    document.validate().unwrap();
    let mut selected = target(None, Some(1));
    selected.node = node("owned-gap");
    assert_eq!(
        document
            .scoped_edit_requirements(&selected, &rename("new gap"))
            .unwrap()
            .nodes,
        0
    );
    let mut concrete = instance(0, 1);
    concrete.node = node("owned-gap");
    assert!(selected.matches_instance(&document, &concrete).unwrap());
    let prepared = prepare(
        &document,
        &request(&document, selected.clone(), rename("new gap")),
    );
    assert_eq!(
        prepared.document.duration().unwrap(),
        document.duration().unwrap()
    );
    assert_eq!(
        prepared.map_instance(&document, &concrete).unwrap(),
        concrete
    );
    assert!(
        AnchorIndex::new(&document)
            .unwrap()
            .resolve_target(&AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: concrete,
                        position: ExactRatio::ZERO,
                    },
                    bias: InsertionBias::Right
                },
                occurrence: None,
            })
            .is_err()
    );
    selected.repeats[1].branch = RepeatEditBranch::Default;
    assert!(selected.validate(&document).is_err());

    let mut nested = target(Some(1), Some(1));
    nested.node = node("owned-gap");
    assert_eq!(
        document
            .scoped_edit_requirements(&nested, &rename("one final gap"))
            .unwrap()
            .nodes,
        8
    );
    let nested = prepare(
        &document,
        &request(&document, nested, rename("one final gap")),
    );
    assert_ne!(nested.target.node, node("owned-gap"));
    assert_eq!(
        nested.document.nodes()[&node("owned-gap")].label,
        "final gap"
    );
    assert_eq!(
        nested.document.duration().unwrap(),
        document.duration().unwrap()
    );
}

fn add_sound(document: &mut ProjectDocument) -> SourceAudio {
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
    )
    .unwrap();
    let asset = AssetId::new("sound").unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let source = SourceAudio { asset, span };
    document.sounds.insert(
        SoundId::new("sound").unwrap(),
        SoundEvent {
            owner: node("root"),
            label: "sound".into(),
            source: source.clone(),
            mapping: SourceAudioMapping::natural_rate(
                span,
                document.presentation_basis().frame_rate,
            )
            .unwrap(),
            offset: AudioSample(137),
            gain_millidecibels: 0,
            start_edge: AudioEdgePolicy::Automatic,
            end_edge: AudioEdgePolicy::Automatic,
            overflow: SoundOverflowPolicy::Reject,
        },
    );
    source
}

#[test]
fn permissions_follow_each_matching_concrete_play_and_hold_audio_retires_only_changed_issuers() {
    let mut document = fixture();
    let source = add_sound(&mut document);
    let sound = SoundId::new("sound").unwrap();
    document.sound_allowances.insert(
        sound.clone(),
        SoundHoldAllowances::try_from(
            (0..3)
                .flat_map(|a| {
                    (0..2).map(move |b| SoundHoldIssuer::Node {
                        instance: instance(a, b),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    document.validate().unwrap();
    for outer in [None, Some(1)] {
        let prepared = prepare(
            &document,
            &request(&document, target(outer, Some(1)), rename("permitted")),
        );
        for a in 0..3 {
            for b in 0..2 {
                let mapped = prepared.map_instance(&document, &instance(a, b)).unwrap();
                assert!(
                    prepared.document.sound_allowances()[&sound]
                        .contains(&SoundHoldIssuer::Node { instance: mapped })
                );
            }
        }
        assert_eq!(prepared.document.sounds(), document.sounds());
        assert_eq!(prepared.document.sound_routes(), document.sound_routes());
        let changed = prepare(
            &prepared.document,
            &request(
                &prepared.document,
                prepared.target.clone(),
                ScopedNodeEdit::SetHoldAudio {
                    audio: HoldAudio::RoomTone {
                        source: source.clone(),
                    },
                },
            ),
        );
        assert_eq!(
            changed.document.sound_allowances()[&sound].len(),
            if outer.is_none() { 3 } else { 5 }
        );
        for issuer in changed.document.sound_allowances()[&sound].iter() {
            assert_ne!(issuer.instance().node, changed.target.node);
        }
        assert_eq!(changed.document.sounds(), document.sounds());
    }
}

#[test]
fn retained_clocks_and_fractional_retime_positions_survive_nested_gain_isolation() {
    let mut document = fixture();
    let duration = document.duration().unwrap();
    let mut retime = BeatNode::sequence("rate", vec![]);
    retime.kind = NodeKind::Retime {
        child: node("outer"),
        duration: frames(17),
        mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(duration.frames() - 1)).unwrap(),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    document.nodes.insert(node("rate"), retime);
    document
        .nodes
        .insert(node("root"), BeatNode::sequence("root", vec![node("rate")]));
    document.audio_bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("captured"),
            ordinal: 0,
        },
    )
    .unwrap();
    document.validate().unwrap();
    let treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    let prepared = prepare(
        &document,
        &request(
            &document,
            target(Some(1), Some(1)),
            ScopedNodeEdit::SetAudioTreatments {
                treatments: treatments.clone(),
            },
        ),
    );
    assert_eq!(
        prepared.document.audio_bindings().timings,
        document.audio_bindings().timings
    );
    assert_eq!(
        prepared.document.nodes()[&prepared.target.node].audio_treatments,
        treatments
    );
    let binding = &prepared.document.audio_bindings().bindings[&prepared.target.node];
    let original_binding = &document.audio_bindings().bindings[&node("a")];
    assert_eq!(
        binding.lattice.reference,
        original_binding.lattice.reference
    );
    assert_eq!(binding.resume, original_binding.resume);
    assert_eq!(binding.reanchors, original_binding.reanchors);
    assert!(binding.lattice.arguments.iter().any(|argument| {
        matches!(&argument.value, AudioRepeatValue::Live { repeat }
            if repeat == &prepared.target.repeats[1].repeat)
    }));
    assert!(!binding.lattice.arguments.iter().any(|argument| {
        matches!(&argument.value, AudioRepeatValue::Live { repeat } if repeat == &node("inner"))
    }));
    for a in 0..3 {
        for b in 0..2 {
            let mapped = prepared.map_instance(&document, &instance(a, b)).unwrap();
            let boundary = |instance| AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance,
                        position: ExactRatio::new(5, 2).unwrap(),
                    },
                    bias: InsertionBias::Right,
                },
                occurrence: None,
            };
            let before = AnchorIndex::new(&document)
                .unwrap()
                .resolve_target(&boundary(instance(a, b)))
                .unwrap();
            let after = AnchorIndex::new(&prepared.document)
                .unwrap()
                .resolve_target(&boundary(mapped))
                .unwrap();
            assert_eq!(before.exact_frame, after.exact_frame);
        }
    }
}

#[test]
fn exact_noop_never_isolates_or_authors_a_revision() {
    let document = fixture();
    let selected = target(Some(1), Some(1));
    let requirements = document
        .scoped_edit_requirements(&selected, &rename("a"))
        .unwrap();
    assert_eq!(
        requirements,
        ScopedEditRequirements {
            nodes: 0,
            marks: 0,
            unchanged: true
        }
    );
    let request = request(&document, selected, rename("a"));
    let error = prepare_scoped_edit(&document, &request).unwrap_err();
    assert!(error.message.contains("does not change"));
    assert!(document.overrides().is_empty());
}

#[test]
fn scoped_framing_and_edges_preserve_other_values_and_reject_incompatible_payloads() {
    let document = fixture();
    let framing = Framing::creep(
        FramingPose::identity(),
        FramingPose::new(
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::integer(2),
        )
        .unwrap(),
        FramingCurve::Smoothstep,
    )
    .unwrap();
    let prepared = prepare(
        &document,
        &request(
            &document,
            target(Some(1), Some(1)),
            ScopedNodeEdit::SetFraming {
                framing: Some(framing.clone()),
            },
        ),
    );
    assert_eq!(
        prepared.document.nodes()[&prepared.target.node].framing,
        Some(framing)
    );
    assert_eq!(
        prepared.document.nodes()[&node("a")],
        document.nodes()[&node("a")]
    );
    let next = prepare(
        &prepared.document,
        &request(
            &prepared.document,
            prepared.target.clone(),
            ScopedNodeEdit::SetAudioEdge {
                edge: AudioBoundaryKind::NodeEnd,
                policy: AudioEdgePolicy::Hard,
            },
        ),
    );
    assert_eq!(
        next.document.nodes()[&next.target.node].framing,
        prepared.document.nodes()[&prepared.target.node].framing
    );
    assert_eq!(
        next.document.nodes()[&next.target.node]
            .audio_edges
            .node_end,
        AudioEdgePolicy::Hard
    );
    assert_eq!(
        next.document.nodes()[&node("a")].audio_edges.node_end,
        AudioEdgePolicy::Automatic
    );
    assert!(
        document
            .scoped_edit_requirements(&target(None, None), &rename("bad\0label"))
            .is_err()
    );
    assert!(
        document
            .scoped_edit_requirements(
                &target(None, None),
                &ScopedNodeEdit::SetAudioEdge {
                    edge: AudioBoundaryKind::SourcePlacementStart,
                    policy: AudioEdgePolicy::Hard,
                }
            )
            .is_err()
    );
    let mut sequence = target(None, None);
    sequence.node = node("inside");
    assert!(
        document
            .scoped_edit_requirements(
                &sequence,
                &ScopedNodeEdit::SetHoldAudio {
                    audio: HoldAudio::Silence
                }
            )
            .is_err()
    );
}

#[test]
fn late_value_validation_failure_leaves_isolation_and_owned_marks_unpublished() {
    let mut document = fixture();
    document
        .nodes
        .insert(node("physical"), BeatNode::hold("physical", hold(4)));
    document.nodes.get_mut(&node("a")).unwrap().kind = NodeKind::Retime {
        child: node("physical"),
        duration: frames(4),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Partition,
    };
    add_owned_mark(
        &mut document,
        "local",
        Anchor::Local {
            node: node("a"),
            position: ExactRatio::ONE,
        },
        MarkState::Bound,
    );
    document.validate().unwrap();
    let request = request(
        &document,
        target(Some(1), Some(1)),
        ScopedNodeEdit::SetAudioEdge {
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    );
    let before = document.to_json().unwrap();
    let error = prepare_scoped_edit(&document, &request).unwrap_err();
    assert!(error.message.contains("partition"), "{error}");
    assert_eq!(document.to_json().unwrap(), before);
    assert!(document.overrides().is_empty());
}

#[test]
fn compound_scoped_steps_have_one_inverse_and_reject_identity_reuse_or_late_failure() {
    let document = fixture();
    let first = request(&document, target(Some(1), Some(1)), rename("first"));
    let prepared = prepare(&document, &first);
    let second = request(
        &prepared.document,
        prepared.target.clone(),
        rename("second"),
    );
    let leaf = |request: &CommandRequest| ResolvedStep::Edit {
        edit: LeafEdit::new(request.new_revision.clone(), request.command.clone()).unwrap(),
    };
    let compound = |second: &CommandRequest| CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("compound"),
        command: Command::Compound {
            transaction: ResolvedTransaction::new(
                0,
                BTreeMap::new(),
                vec![leaf(&first), leaf(second)],
            )
            .unwrap(),
        },
    };
    let transaction = crate::apply(&document, &compound(&second)).unwrap();
    let after = transaction.forward.apply(&document).unwrap();
    let mut expected = prepare(&prepared.document, &second).document;
    expected.revision_id = revision("compound");
    assert_eq!(after, expected);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), document);
    let mut bad = second.clone();
    let Command::EditScoped {
        target: bad_target, ..
    } = &mut bad.command
    else {
        panic!()
    };
    bad_target.node = node("missing");
    assert!(crate::apply(&document, &compound(&bad)).is_err());
    let mut reused = request(
        &prepared.document,
        target(Some(0), Some(0)),
        rename("reused"),
    );
    reused.new_revision = revision("reuse");
    let Command::EditScoped {
        identities: original,
        ..
    } = &first.command
    else {
        panic!()
    };
    let Command::EditScoped { identities, .. } = &mut reused.command else {
        panic!()
    };
    identities.nodes[0] = original.nodes[0].clone();
    assert_eq!(
        crate::apply(&document, &compound(&reused))
            .unwrap_err()
            .code,
        EditErrorCode::IdentityConflict
    );
}

#[test]
fn isolation_checks_cumulative_growth_and_never_expands_repeat_count() {
    let mut document = fixture();
    if let NodeKind::Repeat { iterations, .. } =
        &mut document.nodes.get_mut(&node("outer")).unwrap().kind
    {
        *iterations = IterationOrder::new(revision("outer"), u32::MAX).unwrap();
    }
    document.validate().unwrap();
    let prepared = prepare(
        &document,
        &request(
            &document,
            target(Some(u32::MAX - 1), Some(1)),
            rename("last"),
        ),
    );
    assert_eq!(prepared.document.nodes().len(), document.nodes().len() + 10);
    assert_eq!(prepared.document.overrides()[&node("outer")].len(), 1);
    let mut crowded = fixture();
    let mut children = vec![node("outer")];
    while crowded.nodes.len() < MAX_DOCUMENT_NODES - 8 {
        let id = node(&format!("empty-{}", crowded.nodes.len()));
        children.push(id.clone());
        crowded.nodes.insert(id, BeatNode::sequence("", vec![]));
    }
    crowded
        .nodes
        .insert(node("root"), BeatNode::sequence("root", children));
    crowded.validate().unwrap();
    // The outer clone needs seven nodes and fits; its inner clone needs three
    // more. Admission must reject their sum before allocating either subtree.
    assert_eq!(
        crowded
            .scoped_edit_requirements(&target(Some(1), Some(1)), &rename("x"))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(crowded.nodes.len(), MAX_DOCUMENT_NODES - 8);
}

#[test]
fn invalid_paths_and_untrusted_pools_fail_before_any_isolation() {
    let document = fixture();
    let valid = target(Some(1), Some(1));
    let mut missing = valid.clone();
    missing.repeats.remove(0);
    let mut extra = valid.clone();
    extra.repeats.push(extra.repeats[0].clone());
    let mut wrong = valid.clone();
    wrong.repeats.swap(0, 1);
    let mut retired = valid.clone();
    retired.repeats[1].branch = RepeatEditBranch::Play {
        iteration: play("inner", 2),
    };
    let mut absent = valid.clone();
    absent.node = node("absent");
    for invalid_target in [missing, extra, wrong, retired, absent] {
        assert!(
            document
                .scoped_edit_requirements(&invalid_target, &rename("x"))
                .is_err()
        );
    }
    let before = document.to_json().unwrap();
    for mode in 0..6 {
        let mut request = request(&document, valid.clone(), rename("x"));
        let Command::EditScoped { identities, .. } = &mut request.command else {
            panic!()
        };
        match mode {
            0 => {
                identities.nodes.pop();
            }
            1 => identities.nodes.push(node("extra")),
            2 => identities.nodes[0] = node("root"),
            3 => identities.nodes[1] = identities.nodes[0].clone(),
            4 => identities.marks.push(mark("extra")),
            5 => request.expected_revision = revision("stale"),
            _ => unreachable!(),
        }
        assert!(prepare_scoped_edit(&document, &request).is_err());
        assert_eq!(document.to_json().unwrap(), before);
    }
    let mut retained = document.clone();
    retained.audio_lineage.insert(
        node("a"),
        AudioLineageId {
            allocation: revision("old"),
            origin: node("retired-name"),
        },
    );
    retained.validate().unwrap();
    let mut request = request(&retained, valid, rename("x"));
    let Command::EditScoped { identities, .. } = &mut request.command else {
        panic!()
    };
    identities.nodes[0] = node("retired-name");
    assert_eq!(
        prepare_scoped_edit(&retained, &request).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
}

#[test]
fn fresh_mark_pools_and_presentation_mapping_are_revision_bound() {
    let mut document = fixture();
    add_owned_mark(
        &mut document,
        "owned",
        Anchor::Local {
            node: node("a"),
            position: ExactRatio::ONE,
        },
        MarkState::Bound,
    );
    document.validate().unwrap();
    let command = request(&document, target(Some(1), Some(1)), rename("x"));
    let prepared = prepare(&document, &command);
    let mapped = prepared.map_instance(&document, &instance(1, 1)).unwrap();
    assert_eq!(mapped.node, prepared.target.node);
    assert_eq!(
        prepared
            .map_instance(&prepared.document, &mapped)
            .unwrap_err()
            .code,
        EditErrorCode::RevisionConflict
    );
    assert!(prepared.map_instance(&document, &instance(3, 1)).is_err());
    for duplicate in [false, true] {
        let mut rejected = command.clone();
        let Command::EditScoped { identities, .. } = &mut rejected.command else {
            panic!()
        };
        assert_eq!(identities.marks.len(), 2);
        identities.marks[1] = if duplicate {
            identities.marks[0].clone()
        } else {
            mark("owned")
        };
        assert_eq!(
            prepare_scoped_edit(&document, &rejected).unwrap_err().code,
            EditErrorCode::IdentityConflict
        );
    }
}

#[test]
fn scoped_wire_rejects_unknown_fields_temporal_edits_and_excessive_ancestry() {
    for value in [
        serde_json::json!({"type":"default", "iteration": {"allocation":"outer", "ordinal":0}}),
        serde_json::json!({"type":"play", "iteration": {"allocation":"outer", "ordinal":0}, "extra":true}),
    ] {
        assert!(serde_json::from_value::<RepeatEditBranch>(value).is_err());
    }
    assert!(
        serde_json::from_value::<ScopedNodeEdit>(
            serde_json::json!({"type":"set_hold_duration","duration":5})
        )
        .is_err()
    );
    let mut value = serde_json::to_value(target(None, None)).unwrap();
    value["repeats"] = serde_json::to_value(vec![
        target(None, None).repeats[0].clone();
        MAX_DOCUMENT_DEPTH + 1
    ])
    .unwrap();
    assert!(serde_json::from_value::<ScopedNodeTarget>(value).is_err());
    let document = fixture();
    let mut value =
        serde_json::to_value(request(&document, target(None, None), rename("x"))).unwrap();
    value["command"]["target"]["unqualified_cursor"] = serde_json::json!(12);
    assert!(serde_json::from_value::<CommandRequest>(value).is_err());
}
