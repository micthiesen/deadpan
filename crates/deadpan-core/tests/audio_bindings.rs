use std::collections::BTreeMap;

use deadpan_core::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(a: i128, b: i128) -> ExactRatio {
    ExactRatio::new(a, b).unwrap()
}
fn play(allocation: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(allocation),
        ordinal,
    }
}
fn timing(ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: revision("timings"),
        ordinal,
    }
}
fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "hold",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn repeat(child: &str, allocation: &str, count: u32) -> BeatNode {
    BeatNode {
        label: "repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision(allocation), count).unwrap(),
            gap: None,
        },
    }
}
fn retime(
    child: &str,
    duration: i64,
    start: i64,
    end: i64,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> BeatNode {
    BeatNode {
        label: "retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(duration),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose,
        },
    }
}
fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("bindings").unwrap(),
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
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("root", children.iter().map(|name| id(name)).collect()),
    );
    let mut value = serde_json::to_value(empty).unwrap();
    value["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}
fn instance(repeats: &[(&str, &str, u32)]) -> InstancePath {
    InstancePath {
        node: id("a"),
        repeats: repeats
            .iter()
            .map(|(name, allocation, ordinal)| RepeatInstance {
                node: id(name),
                iteration: play(allocation, *ordinal),
            })
            .collect(),
    }
}
fn plain(ordinal: u32) -> AudioPlacementTemplate {
    AudioPlacementTemplate {
        reference: AudioReferenceClock {
            timing: timing(ordinal),
            root: AudioClockRoot::ProjectRootRoundEven,
            physical: id("a"),
        },
        arguments: vec![],
        births: vec![],
    }
}
fn repeated() -> AudioPlacementTemplate {
    AudioPlacementTemplate {
        arguments: vec![AudioRepeatArgument {
            reference_repeat: id("inner"),
            value: AudioRepeatValue::Live {
                repeat: id("inner"),
            },
        }],
        births: vec![AudioBirthClause {
            repeat: id("inner"),
            survivors: AudioBirthSurvivors::CapturedRepeat {
                repeat: id("inner"),
            },
            definition_root: id("a"),
        }],
        ..plain(0)
    }
}
fn state(layout: &ProjectDocument, template: AudioPlacementTemplate) -> AudioBindingState {
    AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(layout).unwrap(),
        }],
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: template,
                resume: None,
            },
        )]),
    )
    .unwrap()
}

#[test]
fn outer_wrap_and_inner_birth_choose_their_captured_lexical_roots() {
    let before = document(
        &["prefix", "inner"],
        [
            ("prefix", hold(1)),
            ("inner", repeat("a", "inner_plays", 2)),
            ("a", hold(2)),
        ],
    );
    let mut template = repeated();
    template.births.insert(
        0,
        AudioBirthClause {
            repeat: id("outer"),
            survivors: AudioBirthSurvivors::Run {
                allocation: revision("outer_plays"),
                first: 0,
                count: 1,
            },
            definition_root: id("inner"),
        },
    );
    let state = state(&before, template);
    let live = document(
        &["prefix", "outer"],
        [
            ("prefix", hold(1)),
            ("outer", repeat("inner", "outer_plays", 2)),
            ("inner", repeat("a", "inner_plays", 2)),
            ("a", hold(2)),
        ],
    );
    state.validate_for(&live).unwrap();
    let resolve = |outer, inner_allocation, inner| {
        state
            .resolve(
                &id("a"),
                &instance(&[
                    ("outer", "outer_plays", outer),
                    ("inner", inner_allocation, inner),
                ]),
                1000,
            )
            .unwrap()
    };
    assert_eq!(resolve(0, "inner_plays", 0).lattice.origin, ratio(1, 1));
    assert_eq!(resolve(0, "inner_plays", 1).lattice.origin, ratio(3, 1));
    let first = resolve(1, "inner_plays", 0).lattice;
    let second = resolve(1, "inner_plays", 1).lattice;
    assert_eq!(first.birth, Some(0));
    assert_eq!(
        first.clock,
        AudioClockRoot::DefinitionPointCeil { root: id("inner") }
    );
    assert_eq!(first.grid_rule, AudioBindingGridRule::PointCeil);
    assert_eq!(first.sample_boundary(ExactRatio::ZERO).unwrap(), 0);
    assert_eq!(second.sample_boundary(ExactRatio::ZERO).unwrap(), 3204);
    let inner_birth = resolve(1, "new_inner", 0).lattice;
    assert_eq!(inner_birth.birth, Some(1));
    assert_eq!(
        inner_birth.clock,
        AudioClockRoot::DefinitionPointCeil { root: id("a") }
    );
    assert_eq!(inner_birth.sample_boundary(ExactRatio::ZERO).unwrap(), 0);
    assert_eq!(inner_birth.instance.repeats, vec![]);
}

#[test]
fn clear_override_exposes_a_definition_birth_even_for_a_surviving_play_id() {
    let before = document(
        &["inner"],
        [("inner", repeat("a", "inner_plays", 2)), ("a", hold(2))],
    );
    let mut wire = serde_json::to_value(before).unwrap();
    wire["nodes"]["override"] = serde_json::to_value(hold(3)).unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("inner"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: play("inner_plays", 0),
            root: id("override"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    let captured = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let state = state(&captured, repeated());
    let hidden = state
        .resolve(&id("a"), &instance(&[("inner", "inner_plays", 0)]), 1000)
        .unwrap();
    assert_eq!(hidden.lattice.birth, Some(0));
    assert_eq!(hidden.lattice.origin, ExactRatio::ZERO);
    let survivor = state
        .resolve(&id("a"), &instance(&[("inner", "inner_plays", 1)]), 1000)
        .unwrap();
    assert_eq!(survivor.lattice.birth, None);
    assert_eq!(survivor.lattice.origin, ratio(3, 1));
}

#[test]
fn captured_arguments_survive_extraction_and_explicit_definition_exclusion_is_required() {
    let before = document(
        &["inner"],
        [("inner", repeat("a", "inner_plays", 2)), ("a", hold(2))],
    );
    let mut captured = repeated();
    captured.arguments[0].value = AudioRepeatValue::Captured {
        iteration: play("inner_plays", 1),
    };
    captured.births.clear();
    let closed = state(&before, captured.clone());
    let extracted = document(&["a"], [("a", hold(2))]);
    closed.validate_for(&extracted).unwrap();
    assert_eq!(
        closed
            .resolve(&id("a"), &instance(&[]), 1000)
            .unwrap()
            .lattice
            .origin,
        ratio(2, 1)
    );
    captured.arguments[0].value = AudioRepeatValue::Captured {
        iteration: play("inner_plays", 9),
    };
    assert!(
        captured
            .validate(&FrozenAudioLayout::capture(&before).unwrap())
            .is_err()
    );
    let open = state(&before, repeated());
    assert!(open.resolve(&id("a"), &instance(&[]), 1000).is_err());
    let target = instance(&[]);
    let exclusions = [id("inner")];
    let root = id("a");
    let resolved = open
        .resolve_in(
            &root,
            AudioBindingEnvironment::Definition {
                root: &root,
                instance: &target,
                outside_repeats: &exclusions,
            },
            1000,
        )
        .unwrap();
    assert_eq!(resolved.lattice.grid_rule, AudioBindingGridRule::PointCeil);
    assert_eq!(resolved.lattice.birth, Some(0));
    assert!(
        open.resolve_in(
            &root,
            AudioBindingEnvironment::Definition {
                root: &root,
                instance: &target,
                outside_repeats: &[]
            },
            1000
        )
        .is_err()
    );
}

#[test]
fn selected_point_clock_and_meaningful_edit_support_are_not_round_even_partitions() {
    let before = document(
        &["stage"],
        [
            (
                "stage",
                retime("a", 6, 1, 5, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
            ("a", hold(8)),
        ],
    );
    let mut template = plain(0);
    template.reference.root = AudioClockRoot::PreserveInputPointCeil { stage: id("stage") };
    let bound = state(&before, template)
        .resolve(&id("a"), &instance(&[]), 1000)
        .unwrap();
    assert_eq!(bound.lattice.grid_origin, ExactRatio::ONE);
    assert_eq!(bound.lattice.local_support, ratio(1, 1)..ratio(5, 1));
    assert_eq!(
        bound.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        -1601
    );
    assert_eq!(bound.lattice.sample_boundary(ExactRatio::ONE).unwrap(), 0);
    for (purpose, expected) in [
        (RetimePurpose::Edit, ratio(2, 1)..ratio(6, 1)),
        (RetimePurpose::Partition, ratio(0, 1)..ratio(8, 1)),
    ] {
        let cropped = document(
            &["crop"],
            [
                (
                    "crop",
                    retime("a", 4, 2, 6, PitchPolicy::FollowSpeed, purpose),
                ),
                ("a", hold(8)),
            ],
        );
        let resolved = state(&cropped, plain(0))
            .resolve(&id("a"), &instance(&[]), 1000)
            .unwrap();
        assert_eq!(resolved.lattice.local_support, expected);
        assert_eq!(resolved.lattice.origin, ratio(-2, 1));
    }
}

#[test]
fn symbolic_phase_retains_each_play_rounding_and_composes_another_clock() {
    let before = document(
        &["inner"],
        [("inner", repeat("a", "inner_plays", 2)), ("a", hold(1))],
    );
    let template = repeated();
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(&before).unwrap(),
        }],
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: template.clone(),
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ONE,
                    phase: AudioLocalPhase {
                        constant: ExactRatio::ZERO,
                        terms: vec![AudioPhaseTerm {
                            placement: template,
                            from_local: ExactRatio::ZERO,
                            to_local: ExactRatio::ONE,
                        }],
                    },
                }),
            },
        )]),
    )
    .unwrap();
    for (ordinal, count) in [(0, 1602), (1, 1601)] {
        let resolved = state
            .resolve(
                &id("a"),
                &instance(&[("inner", "inner_plays", ordinal)]),
                1000,
            )
            .unwrap();
        assert_eq!(
            resolved.resume.unwrap().reference_local_delta,
            ratio(count * 5, 8008)
        );
    }
    let first = document(&["a"], [("a", hold(4))]);
    let second = document(&["prefix", "a"], [("prefix", hold(1)), ("a", hold(4))]);
    let state = AudioBindingState::new(
        vec![
            AudioTimingRecord {
                id: timing(0),
                layout: FrozenAudioLayout::capture(&first).unwrap(),
            },
            AudioTimingRecord {
                id: timing(1),
                layout: FrozenAudioLayout::capture(&second).unwrap(),
            },
        ],
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: plain(0),
                resume: Some(AudioResume {
                    local_boundary: ratio(2, 1),
                    phase: AudioLocalPhase {
                        constant: ExactRatio::ZERO,
                        terms: vec![
                            AudioPhaseTerm {
                                placement: plain(0),
                                from_local: ExactRatio::ZERO,
                                to_local: ExactRatio::ONE,
                            },
                            AudioPhaseTerm {
                                placement: plain(1),
                                from_local: ExactRatio::ONE,
                                to_local: ratio(2, 1),
                            },
                        ],
                    },
                }),
            },
        )]),
    )
    .unwrap();
    let resolved = state.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    assert_eq!(
        resolved
            .resume
            .unwrap()
            .reference_local_delta
            .checked_div(resolved.lattice.local_frames_per_sample().unwrap())
            .unwrap(),
        ratio(3204, 1)
    );
    assert!(state.resolve(&id("a"), &instance(&[]), 1).is_err());
}

#[test]
fn every_template_clock_stays_inside_its_nearest_preserve_input() {
    let before = document(
        &["outer"],
        [
            (
                "outer",
                retime(
                    "inner",
                    12,
                    0,
                    6,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "inner",
                retime("a", 6, 1, 5, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
            ("a", hold(8)),
        ],
    );
    let layout = FrozenAudioLayout::capture(&before).unwrap();
    for root in [
        AudioClockRoot::ProjectRootRoundEven,
        AudioClockRoot::PreserveInputPointCeil { stage: id("outer") },
        AudioClockRoot::DefinitionPointCeil { root: id("inner") },
    ] {
        let mut template = plain(0);
        template.reference.root = root;
        assert!(
            template
                .validate(&layout)
                .unwrap_err()
                .message
                .contains("opaque Preserve")
        );
    }
    let mut template = plain(0);
    template.reference.root = AudioClockRoot::PreserveInputPointCeil { stage: id("inner") };
    template.validate(&layout).unwrap();
    let mut bad_term = template.clone();
    bad_term.reference.root = AudioClockRoot::ProjectRootRoundEven;
    assert!(
        AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing(0),
                layout: layout.clone()
            }],
            BTreeMap::from([(
                id("a"),
                OwnedAudioBinding {
                    lattice: template,
                    resume: Some(AudioResume {
                        local_boundary: ExactRatio::ONE,
                        phase: AudioLocalPhase {
                            constant: ExactRatio::ZERO,
                            terms: vec![AudioPhaseTerm {
                                placement: bad_term,
                                from_local: ExactRatio::ZERO,
                                to_local: ExactRatio::ONE,
                            }],
                        },
                    }),
                }
            )]),
        )
        .unwrap_err()
        .message
        .contains("opaque Preserve")
    );
    // A Preserve output remains a physical operand in its parent's input clock.
    let mut output = plain(0);
    output.reference.physical = id("inner");
    output.reference.root = AudioClockRoot::PreserveInputPointCeil { stage: id("outer") };
    output.validate(&layout).unwrap();
    output.births.push(AudioBirthClause {
        repeat: id("new_wrap"),
        definition_root: id("inner"),
        survivors: AudioBirthSurvivors::Run {
            allocation: revision("wrap"),
            first: 0,
            count: 1,
        },
    });
    output.validate(&layout).unwrap();
}

#[test]
fn state_round_trips_structured_ids_and_rejects_invalid_or_unbounded_intent() {
    let before = document(&["a"], [("a", hold(4))]);
    let state = state(&before, plain(0));
    state.validate_for(&before).unwrap();
    let json = state.to_json().unwrap();
    assert_eq!(AudioBindingState::from_json(&json).unwrap(), state);
    let mut wire = serde_json::to_value(&state).unwrap();
    assert!(wire["timings"].is_array());
    let record = wire["timings"][0].clone();
    wire["timings"].as_array_mut().unwrap().push(record);
    assert!(AudioBindingState::from_json(&wire.to_string()).is_err());
    let mut wire = serde_json::to_value(&state).unwrap();
    wire["bindings"]["a"]["lattice"]["reference"]["unrecognized"] = true.into();
    assert!(AudioBindingState::from_json(&wire.to_string()).is_err());
    let mut bad = plain(0);
    bad.reference.physical = id("missing");
    assert!(
        bad.validate(&FrozenAudioLayout::capture(&before).unwrap())
            .is_err()
    );
    let mut bad = repeated();
    bad.births[0].definition_root = id("inner");
    let repeated_doc = document(
        &["inner"],
        [("inner", repeat("a", "inner_plays", 2)), ("a", hold(2))],
    );
    assert!(
        bad.validate(&FrozenAudioLayout::capture(&repeated_doc).unwrap())
            .is_err()
    );
    let mut bad = plain(0);
    bad.births.push(AudioBirthClause {
        repeat: id("new"),
        definition_root: id("a"),
        survivors: AudioBirthSurvivors::Run {
            allocation: revision("birth"),
            first: u32::MAX,
            count: 2,
        },
    });
    assert!(
        bad.validate(&FrozenAudioLayout::capture(&before).unwrap())
            .is_err()
    );
    let mut wire = serde_json::to_value(&state).unwrap();
    wire["bindings"]["a"]["lattice"]["arguments"] = serde_json::to_value(vec![
        AudioRepeatArgument {
            reference_repeat: id("inner"),
            value: AudioRepeatValue::Live {
                repeat: id("inner")
            }
        };
        MAX_DOCUMENT_DEPTH
            + 1
    ])
    .unwrap();
    assert!(
        AudioBindingState::from_json(&wire.to_string())
            .unwrap_err()
            .message
            .contains("sequence limit")
    );
    let mut wire = serde_json::to_value(&before).unwrap();
    let mut different = before.presentation_basis().clone();
    different.frame_rate = FrameRate::new(24, 1).unwrap();
    wire["presentation_basis"] = serde_json::to_value(different).unwrap();
    let different = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(state.validate_for(&different).is_err());
    let placement = state
        .resolve(&id("a"), &instance(&[]), 1000)
        .unwrap()
        .lattice;
    assert!(placement.sample_boundary(ratio(i128::MAX, 1)).is_err());
    assert!(state.allocation_ids().contains(&revision("timings")));
}

#[test]
fn live_validation_bounds_resume_anchors_without_narrowing_exact_phase() {
    let before = document(&["a"], [("a", hold(4))]);
    for (boundary, admitted) in [(-1, false), (0, true), (4, true), (5, false)] {
        let binding = OwnedAudioBinding {
            lattice: plain(0),
            resume: Some(AudioResume {
                local_boundary: ratio(boundary, 1),
                phase: AudioLocalPhase {
                    constant: ratio(i128::MIN + 1, 1),
                    terms: vec![],
                },
            }),
        };
        let state = AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing(0),
                layout: FrozenAudioLayout::capture(&before).unwrap(),
            }],
            BTreeMap::from([(id("a"), binding)]),
        )
        .unwrap();
        assert_eq!(state.validate_for(&before).is_ok(), admitted);
        if admitted {
            assert_eq!(
                state
                    .resolve(&id("a"), &instance(&[]), 1000)
                    .unwrap()
                    .resume
                    .unwrap()
                    .reference_local_delta,
                ratio(i128::MIN + 1, 1)
            );
        }
    }
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(&before).unwrap(),
        }],
        BTreeMap::from([(
            id("root"),
            OwnedAudioBinding {
                lattice: plain(0),
                resume: None,
            },
        )]),
    )
    .unwrap();
    assert!(
        state
            .validate_for(&before)
            .unwrap_err()
            .message
            .contains("physical recipe")
    );
}

#[test]
fn programmatic_bindings_obey_the_same_individual_wire_cap() {
    let before = document(&["a"], [("a", hold(4))]);
    let mut placement = plain(0);
    for index in 0..100 {
        placement.births.push(AudioBirthClause {
            repeat: id(&format!("guard_{index}")),
            definition_root: id("a"),
            survivors: AudioBirthSurvivors::Run {
                allocation: revision("wrap"),
                first: 0,
                count: 1,
            },
        });
    }
    let binding = OwnedAudioBinding {
        lattice: plain(0),
        resume: Some(AudioResume {
            local_boundary: ExactRatio::ONE,
            phase: AudioLocalPhase {
                constant: ExactRatio::ZERO,
                terms: vec![
                    AudioPhaseTerm {
                        placement,
                        from_local: ExactRatio::ZERO,
                        to_local: ExactRatio::ONE,
                    };
                    MAX_AUDIO_BINDING_TERMS
                ],
            },
        }),
    };
    let error = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(&before).unwrap(),
        }],
        BTreeMap::from([(id("a"), binding)]),
    )
    .unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
    assert!(
        error
            .message
            .contains("individual audio binding byte limit")
    );
}

#[test]
fn aggregate_wire_admission_precedes_any_typed_layout_body() {
    let before = document(&["a"], [("a", hold(4))]);
    let mut layout = serde_json::to_value(FrozenAudioLayout::capture(&before).unwrap()).unwrap();
    let leaf = layout["nodes"]["a"].clone();
    let nodes = layout["nodes"].as_object_mut().unwrap();
    for index in 0..500 {
        nodes.insert(format!("leaf_{index}"), leaf.clone());
    }
    // This structurally scannable but semantically invalid leaf must never be
    // materialized: aggregate complexity takes precedence across all records.
    nodes.get_mut("a").unwrap()["duration"] = (-1).into();
    let records = MAX_AUDIO_BINDING_ENTRIES / nodes.len() + 1;
    let timings: Vec<_> = (0..records)
        .map(|ordinal| {
            serde_json::json!({
                "id": {"allocation": "old", "ordinal": ordinal},
                "layout": layout,
            })
        })
        .collect();
    let json = serde_json::json!({"timings": timings, "bindings": {}}).to_string();
    assert!(json.len() < MAX_DOCUMENT_JSON_BYTES);
    let error = AudioBindingState::from_json(&json).unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
    assert!(error.message.contains("audio binding work exhausted"));
}

#[test]
fn constructor_validation_charges_all_template_walks_together() {
    let before = document(&["a"], [("a", hold(4))]);
    let bindings = (0..34_000)
        .map(|index| {
            (
                id(&format!("owner_{index}")),
                OwnedAudioBinding {
                    lattice: plain(0),
                    resume: None,
                },
            )
        })
        .collect();
    let error = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(&before).unwrap(),
        }],
        bindings,
    )
    .unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
}

#[test]
fn near_limit_binding_round_trips_inside_a_pretty_project_document() {
    let before = document(&["a"], [("a", hold(4))]);
    let names: Vec<_> = (0..17)
        .map(|index| format!("guard_{index:02}_{}", "x".repeat(119)))
        .collect();
    let mut template = plain(0);
    for name in &names {
        template.births.push(AudioBirthClause {
            repeat: id(name),
            definition_root: id("a"),
            survivors: AudioBirthSurvivors::Run {
                allocation: revision("wrap"),
                first: 0,
                count: 1,
            },
        });
    }
    let term = AudioPhaseTerm {
        placement: template,
        from_local: ExactRatio::ZERO,
        to_local: ExactRatio::ONE,
    };
    let mut binding = OwnedAudioBinding {
        lattice: plain(0),
        resume: Some(AudioResume {
            local_boundary: ExactRatio::ONE,
            phase: AudioLocalPhase::default(),
        }),
    };
    let cap = 1024 * 1024;
    let base_bytes = serde_json::to_string(&binding).unwrap().len();
    let term_bytes = serde_json::to_string(&term).unwrap().len() + 1;
    let count = ((cap - 4096 - base_bytes) / term_bytes).min(MAX_AUDIO_BINDING_TERMS);
    binding.resume.as_mut().unwrap().phase.terms = vec![term; count];
    let compact_size = serde_json::to_string(&binding).unwrap().len();
    assert!(compact_size > cap * 9 / 10 && compact_size < cap);
    assert!(serde_json::to_string_pretty(&binding).unwrap().len() > cap);
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(&before).unwrap(),
        }],
        BTreeMap::from([(id("a"), binding)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(before).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("root", vec![id(&names[0])])).unwrap();
    for (index, name) in names.iter().enumerate() {
        let child = names.get(index + 1).map_or("a", String::as_str);
        wire["nodes"][name] = serde_json::to_value(repeat(child, "wrap", 1)).unwrap();
    }
    wire["audio_bindings"] = serde_json::to_value(&state).unwrap();
    let admitted = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let pretty = admitted.to_json().unwrap();
    let reopened = ProjectDocument::from_json(&pretty).unwrap();
    assert_eq!(reopened, admitted);
}
