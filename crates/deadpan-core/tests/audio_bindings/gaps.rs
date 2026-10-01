use super::*;

fn gap(duration: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(duration),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn gap_repeat(child: &str, allocation: &str, count: u32) -> BeatNode {
    let mut node = repeat(child, allocation, count);
    let NodeKind::Repeat { gap: value, .. } = &mut node.kind else {
        unreachable!()
    };
    *value = Some(gap(2));
    node
}
fn fixture(count: u32) -> ProjectDocument {
    document(
        &["prefix", "r"],
        [
            ("prefix", hold(1)),
            ("r", gap_repeat("a", "plays", count)),
            ("a", hold(2)),
        ],
    )
}
fn install(document: &ProjectDocument, state: &AudioBindingState) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn edit(
    before: &ProjectDocument,
    name: &str,
    command: Command,
) -> (ProjectDocument, EditTransaction) {
    let transaction = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    (transaction.forward.apply(before).unwrap(), transaction)
}
fn resolve(
    state: &AudioBindingState,
    owner: &str,
    after: &IterationId,
    outer: Vec<RepeatInstance>,
) -> ResolvedAudioBinding {
    state
        .resolve_gap_in(
            &id(owner),
            AudioBindingEnvironment::GapOccurrence {
                instance: &InstancePath {
                    node: id(owner),
                    repeats: outer,
                },
                after,
            },
            1000,
        )
        .unwrap()
}
fn gap_state(
    document: &ProjectDocument,
    change: impl FnOnce(&mut OwnedAudioBinding),
) -> AudioBindingState {
    let captured = capture_unbound_audio_bindings(document, timing(0)).unwrap();
    let mut binding = captured.gap_bindings()[&id("r")].clone();
    change(&mut binding);
    AudioBindingState::new_with_gaps(
        captured
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        BTreeMap::new(),
        BTreeMap::from([(id("r"), binding)]),
    )
    .unwrap()
}

#[test]
fn surviving_preceding_plays_keep_gaps_and_former_final_or_new_plays_are_born() {
    let original = fixture(3);
    let captured = capture_unbound_audio_bindings(&original, timing(0)).unwrap();
    let bound = install(&original, &captured);
    let (grown, _) = edit(
        &bound,
        "grow",
        Command::SetRepeat {
            node: id("r"),
            plays: 5,
            gap: Some(gap(2)),
        },
    );
    assert_eq!(grown.audio_bindings(), &captured);
    for ordinal in 0..2 {
        let lattice = resolve(grown.audio_bindings(), "r", &play("plays", ordinal), vec![]).lattice;
        assert!(!lattice.gap_birth);
        assert_eq!(
            lattice.origin,
            ExactRatio::integer(3 + i64::from(ordinal) * 4)
        );
        assert_eq!(lattice.gap_after, Some(play("plays", ordinal)));
    }
    for identity in [play("plays", 2), play("grow", 0)] {
        let lattice = resolve(grown.audio_bindings(), "r", &identity, vec![]).lattice;
        assert!(lattice.gap_birth);
        assert_eq!(lattice.birth, None);
        assert_eq!(lattice.origin, ExactRatio::ZERO);
        assert_eq!(lattice.local_duration, frames(2));
        assert_eq!(lattice.frames_per_local_frame, ExactRatio::ONE);
        assert_eq!(lattice.grid_rule, AudioBindingGridRule::PointCeil);
        assert_eq!(lattice.gap_after, None);
    }
    let (reordered, _) = edit(
        &grown,
        "move",
        Command::MovePlays {
            node: id("r"),
            start: 1,
            end: 2,
            destination: 0,
        },
    );
    assert_eq!(
        resolve(reordered.audio_bindings(), "r", &play("plays", 1), vec![])
            .lattice
            .origin,
        ExactRatio::integer(7)
    );
    let (overridden, _) = edit(
        &reordered,
        "override",
        Command::SetPlayOverride {
            node: id("r"),
            iteration: play("plays", 1),
            subtree: Subtree {
                root: id("replacement"),
                nodes: BTreeMap::from([(id("replacement"), hold(4))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    assert!(
        !resolve(overridden.audio_bindings(), "r", &play("plays", 1), vec![])
            .lattice
            .gap_birth
    );
}

#[test]
fn one_play_capture_retains_a_configured_gap_before_growth_and_direct_definition() {
    let original = fixture(1);
    let captured = capture_unbound_audio_bindings(&original, timing(0)).unwrap();
    assert_eq!(captured.gap_bindings().len(), 1);
    let bound = install(&original, &captured);
    let (grown, _) = edit(
        &bound,
        "grow-one",
        Command::SetRepeat {
            node: id("r"),
            plays: 2,
            gap: Some(gap(2)),
        },
    );
    assert!(
        resolve(grown.audio_bindings(), "r", &play("plays", 0), vec![])
            .lattice
            .gap_birth
    );
    let root = id("r");
    let direct = captured
        .resolve_gap_in(
            &root,
            AudioBindingEnvironment::GapDefinition {
                root: AudioDefinitionScope::RepeatGap(&root),
                instance: &InstancePath {
                    node: root.clone(),
                    repeats: vec![],
                },
                outside_repeats: &[],
                after: None,
            },
            1000,
        )
        .unwrap();
    assert!(direct.lattice.gap_birth);
    assert_eq!(direct.lattice.origin, ExactRatio::ZERO);
    assert_eq!(direct.lattice.local_duration, frames(2));
}

#[test]
fn outer_birth_retains_surviving_inner_gap_and_own_birth_is_innermost() {
    let original = document(
        &["prefix", "outer"],
        [
            ("prefix", hold(1)),
            ("outer", repeat("r", "outer-plays", 2)),
            ("r", gap_repeat("a", "plays", 2)),
            ("a", hold(2)),
        ],
    );
    let state = capture_unbound_audio_bindings(&original, timing(0)).unwrap();
    let outer = vec![RepeatInstance {
        node: id("outer"),
        iteration: play("new-outer", 0),
    }];
    let surviving = resolve(&state, "r", &play("plays", 0), outer.clone()).lattice;
    assert_eq!(surviving.birth, Some(0));
    assert!(!surviving.gap_birth);
    assert_eq!(surviving.origin, ExactRatio::integer(2));
    assert_eq!(
        surviving.clock,
        AudioClockRoot::DefinitionPointCeil { root: id("r") }
    );
    let born = resolve(&state, "r", &play("plays", 1), outer).lattice;
    assert_eq!(born.birth, Some(0));
    assert!(born.gap_birth);
    assert_eq!(
        born.clock,
        AudioClockRoot::GapDefinitionPointCeil { repeat: id("r") }
    );
    assert_eq!(born.origin, ExactRatio::ZERO);
}

#[test]
fn repeat_output_window_does_not_survive_same_id_gap_definition_birth() {
    let old = fixture(2);
    let state = gap_state(&old, |binding| {
        binding.lattice.reference.root = AudioClockRoot::DefinitionPointCeil { root: id("r") };
        binding.reanchors.push(AudioReanchorStep {
            placement: binding.lattice.clone(),
            window: Some(ExactFrameRange::new(ExactRatio::ONE, ExactRatio::integer(4)).unwrap()),
        });
    });
    let born = resolve(&state, "r", &play("plays", 1), vec![]);
    assert_eq!(born.resume.unwrap().local_boundary, ExactRatio::ZERO);
    let existing = resolve(&state, "r", &play("plays", 0), vec![]);
    assert_eq!(existing.resume.unwrap().local_boundary, ExactRatio::ZERO);
}

#[test]
fn hidden_gap_has_no_reanchor_entry_and_later_visible_gap_starts_at_zero() {
    let old = fixture(3);
    let state = gap_state(&old, |binding| {
        binding.reanchors.push(AudioReanchorStep {
            placement: binding.lattice.clone(),
            window: Some(
                ExactFrameRange::new(ExactRatio::integer(5), ExactRatio::integer(9)).unwrap(),
            ),
        });
    });
    assert!(
        resolve(&state, "r", &play("plays", 0), vec![])
            .resume
            .is_none()
    );
    assert_eq!(
        resolve(&state, "r", &play("plays", 1), vec![])
            .resume
            .unwrap()
            .local_boundary,
        ExactRatio::ZERO
    );
}

#[test]
fn gap_removal_prunes_intent_and_readding_does_not_resurrect_it() {
    let original = fixture(2);
    let bound = install(
        &original,
        &capture_unbound_audio_bindings(&original, timing(0)).unwrap(),
    );
    let (longer, _) = edit(
        &bound,
        "longer",
        Command::SetRepeat {
            node: id("r"),
            plays: 2,
            gap: Some(gap(7)),
        },
    );
    assert_eq!(longer.audio_bindings(), bound.audio_bindings());
    let (removed, transaction) = edit(
        &longer,
        "remove-gap",
        Command::SetRepeat {
            node: id("r"),
            plays: 2,
            gap: None,
        },
    );
    assert!(removed.audio_bindings().gap_bindings().is_empty());
    assert!(transaction.changed_ids.contains(&id("r")));
    assert_eq!(transaction.inverse.apply(&removed).unwrap(), longer);
    let (readded, _) = edit(
        &removed,
        "readd-gap",
        Command::SetRepeat {
            node: id("r"),
            plays: 2,
            gap: Some(gap(2)),
        },
    );
    assert!(readded.audio_bindings().gap_bindings().is_empty());
    let recaptured = capture_unbound_audio_bindings(&readded, timing(1)).unwrap();
    assert_eq!(
        recaptured.gap_bindings()[&id("r")].lattice.reference.timing,
        timing(1)
    );
    assert!(recaptured.gap_bindings()[&id("r")].reanchors.is_empty());
}

#[test]
fn split_and_occurrence_isolation_remap_live_gap_owner_and_restore_exact_inverse() {
    let original = fixture(2);
    let bound = install(
        &original,
        &capture_unbound_audio_bindings(&original, timing(0)).unwrap(),
    );
    let (split, transaction) = edit(
        &bound,
        "split-gap",
        Command::Split {
            node: id("r"),
            at: frames(3),
            identities: SplitIdentities {
                nodes: ["left", "right", "copy-r", "copy-a"].map(id).to_vec(),
            },
        },
    );
    let copied = &split.audio_bindings().gap_bindings()[&id("copy-r")];
    assert_eq!(
        copied.lattice.gap_after,
        Some(AudioRepeatValue::Live {
            repeat: id("copy-r")
        })
    );
    assert_eq!(copied.lattice.reference.physical, id("r"));
    assert_eq!(transaction.inverse.apply(&split).unwrap(), bound);

    let original = document(
        &["outer"],
        [
            ("outer", repeat("r", "outer-plays", 2)),
            ("r", gap_repeat("a", "plays", 2)),
            ("a", hold(2)),
        ],
    );
    let bound = install(
        &original,
        &capture_unbound_audio_bindings(&original, timing(0)).unwrap(),
    );
    let (isolated, transaction) = edit(
        &bound,
        "isolate-gap",
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("r"),
                repeats: vec![RepeatInstance {
                    node: id("outer"),
                    iteration: play("outer-plays", 0),
                }],
            },
            edit: OccurrenceEdit::Rename {
                label: "selected gap owner".into(),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("copy-r"), id("copy-a")],
                marks: vec![],
            },
        },
    );
    let copied = &isolated.audio_bindings().gap_bindings()[&id("copy-r")];
    assert_eq!(
        copied.lattice.gap_after,
        Some(AudioRepeatValue::Live {
            repeat: id("copy-r")
        })
    );
    assert_eq!(copied.lattice.reference.physical, id("r"));
    assert_eq!(transaction.inverse.apply(&isolated).unwrap(), bound);
}

#[test]
fn gap_capture_and_resolution_are_bounded_for_a_billion_plays() {
    let original = fixture(1_000_000_000);
    let state = capture_unbound_audio_bindings(&original, timing(0)).unwrap();
    assert_eq!(state.gap_bindings().len(), 1);
    let result = resolve(&state, "r", &play("plays", 999_999_998), vec![]);
    assert!(result.work < 100);
    assert_eq!(result.lattice.origin, ExactRatio::integer(3_999_999_995));
    assert!(state.to_json().unwrap().len() < 10_000);
    assert_eq!(
        AudioBindingState::from_json(&state.to_json().unwrap()).unwrap(),
        state
    );
}

#[test]
fn gap_admission_rejects_cross_kind_arguments_and_malformed_maps() {
    let original = fixture(2);
    let state = gap_state(&original, |_| {});
    let wire = serde_json::to_value(&state).unwrap();
    for (path, bad) in [
        (
            "/gap_bindings/r/lattice/reference/recipe",
            serde_json::json!("node"),
        ),
        ("/gap_bindings/r/lattice/gap_after", serde_json::Value::Null),
        (
            "/gap_bindings/r/lattice/reference/physical",
            serde_json::json!("a"),
        ),
        ("/gap_bindings", serde_json::Value::Null),
    ] {
        let mut malformed = wire.clone();
        *malformed.pointer_mut(path).unwrap() = bad;
        assert!(
            AudioBindingState::from_json(&malformed.to_string()).is_err(),
            "{path}"
        );
    }
    let mut binding = state.gap_bindings()[&id("r")].clone();
    binding.lattice.gap_after = Some(AudioRepeatValue::Captured {
        iteration: play("plays", 1),
    });
    assert!(
        AudioBindingState::new_with_gaps(
            vec![AudioTimingRecord {
                id: timing(0),
                layout: FrozenAudioLayout::capture(&original).unwrap()
            }],
            BTreeMap::new(),
            BTreeMap::from([(id("r"), binding)])
        )
        .is_err()
    );
    assert!(
        state
            .resolve(
                &id("r"),
                &InstancePath {
                    node: id("r"),
                    repeats: vec![]
                },
                1000
            )
            .is_err()
    );
}

#[test]
fn insertion_captures_gap_prefixes_and_moves_seams_but_refuses_repeat_interiors() {
    let before = document(
        &["r", "suffix"],
        [
            ("r", gap_repeat("a", "plays", 2)),
            ("a", hold(2)),
            ("suffix", hold(4)),
        ],
    );
    let command = |at| Command::InsertTime {
        at: ProjectFrame(at),
        hold: gap(3),
        id: id("pause"),
        identities: SplitIdentities {
            nodes: ["left", "right", "copy"].map(id).to_vec(),
        },
        timing: AudioTimingId {
            allocation: revision("pause-edit"),
            ordinal: 0,
        },
    };
    let (after, transaction) = edit(&before, "pause-edit", command(8));
    assert_eq!(transaction.duration_delta, 3);
    assert_eq!(after.audio_bindings().gap_bindings().len(), 1);
    // Capture changed the gap's authored clock while its Repeat node stayed
    // byte-for-byte unchanged. Consumers must still invalidate that owner.
    assert!(!transaction.forward.nodes.contains_key(&id("r")));
    assert!(transaction.changed_ids.contains(&id("r")));
    let gap = resolve(after.audio_bindings(), "r", &play("plays", 0), vec![]).lattice;
    assert_eq!(gap.origin, ExactRatio::integer(2));
    assert!(!gap.gap_birth);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    let (seam, transaction) = edit(&before, "pause-edit", command(0));
    assert_eq!(seam.duration().unwrap().frames(), 13);
    assert_eq!(seam.nodes()[&id("r")], before.nodes()[&id("r")]);
    assert_eq!(
        seam.audio_bindings().gap_bindings()[&id("r")]
            .reanchors
            .len(),
        1
    );
    assert_eq!(transaction.inverse.apply(&seam).unwrap(), before);
    let error = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("pause-edit"),
            command: command(1),
        },
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(
        error
            .message
            .contains("use a Source or ordinary Hold boundary")
    );
}

#[test]
fn historical_child_override_does_not_turn_its_following_gap_into_a_birth() {
    let original = fixture(3);
    let (overridden, _) = edit(
        &original,
        "initial-override",
        Command::SetPlayOverride {
            node: id("r"),
            iteration: play("plays", 0),
            subtree: Subtree {
                root: id("replacement"),
                nodes: BTreeMap::from([(id("replacement"), hold(5))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let state = capture_unbound_audio_bindings(&overridden, timing(0)).unwrap();
    let first = resolve(&state, "r", &play("plays", 0), vec![]).lattice;
    assert!(!first.gap_birth);
    assert_eq!(first.origin, ExactRatio::integer(6));
    let second = resolve(&state, "r", &play("plays", 1), vec![]).lattice;
    assert!(!second.gap_birth);
    assert_eq!(second.origin, ExactRatio::integer(10));
}

#[test]
fn transparent_partition_hidden_gap_does_not_gain_a_clamped_reanchor() {
    let original = document(
        &["partition"],
        [
            (
                "partition",
                retime(
                    "r",
                    2,
                    0,
                    2,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
            ("r", gap_repeat("a", "plays", 2)),
            ("a", hold(2)),
        ],
    );
    let state = gap_state(&original, |binding| {
        binding.reanchors.push(AudioReanchorStep {
            placement: binding.lattice.clone(),
            window: None,
        });
    });
    let hidden = resolve(&state, "r", &play("plays", 0), vec![]);
    assert_eq!(
        hidden.lattice.local_support,
        ExactRatio::ZERO..ExactRatio::integer(2)
    );
    assert!(hidden.resume.is_none());
}

#[test]
fn gap_and_node_templates_share_the_aggregate_admission_budget() {
    let original = fixture(2);
    let captured = capture_unbound_audio_bindings(&original, timing(0)).unwrap();
    let mut gap_binding = captured.gap_bindings()[&id("r")].clone();
    gap_binding.reanchors = vec![
        AudioReanchorStep {
            placement: gap_binding.lattice.clone(),
            window: None
        };
        MAX_AUDIO_BINDING_TERMS
    ];
    let mut node_binding = captured.bindings()[&id("prefix")].clone();
    node_binding.reanchors = vec![
        AudioReanchorStep {
            placement: node_binding.lattice.clone(),
            window: None
        };
        MAX_AUDIO_BINDING_TERMS
    ];
    let node_map: BTreeMap<_, _> = (0..150)
        .map(|index| (id(&format!("node-{index}")), node_binding.clone()))
        .collect();
    let gap_map: BTreeMap<_, _> = (0..150)
        .map(|index| (id(&format!("gap-{index}")), gap_binding.clone()))
        .collect();
    let timings = vec![AudioTimingRecord {
        id: timing(0),
        layout: FrozenAudioLayout::capture(&original).unwrap(),
    }];
    let wire = serde_json::json!({"timings":timings, "bindings":node_map, "gap_bindings":gap_map});
    assert_eq!(
        AudioBindingState::from_json(&wire.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(
        AudioBindingState::new_with_gaps(timings, node_map, gap_map)
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn shortening_a_gap_preserves_its_existing_resume_coordinate() {
    let original = fixture(2);
    let state = gap_state(&original, |binding| {
        binding.resume = Some(AudioResume {
            local_boundary: ExactRatio::integer(2),
            phase: AudioLocalPhase {
                constant: ratio(3, 2),
                terms: vec![],
            },
        });
    });
    let bound = install(&original, &state);
    let (shortened, transaction) = edit(
        &bound,
        "short-gap",
        Command::SetRepeat {
            node: id("r"),
            plays: 2,
            gap: Some(gap(1)),
        },
    );
    assert_eq!(shortened.audio_bindings(), &state);
    let resolved = resolve(shortened.audio_bindings(), "r", &play("plays", 0), vec![])
        .resume
        .unwrap();
    assert_eq!(resolved.local_boundary, ExactRatio::integer(2));
    assert_eq!(resolved.reference_local_delta, ratio(3, 2));
    assert_eq!(transaction.inverse.apply(&shortened).unwrap(), bound);
}

#[test]
fn an_authored_gap_definition_has_no_preceding_play_or_outer_arguments() {
    let original = fixture(1);
    let state = gap_state(&original, |binding| {
        binding.lattice.reference.root = AudioClockRoot::GapDefinitionPointCeil { repeat: id("r") };
        binding.lattice.gap_after = None;
    });
    state.validate_for(&original).unwrap();
    let root = id("r");
    let resolved = state
        .resolve_gap_in(
            &root,
            AudioBindingEnvironment::GapDefinition {
                root: AudioDefinitionScope::RepeatGap(&root),
                instance: &InstancePath {
                    node: root.clone(),
                    repeats: vec![],
                },
                outside_repeats: &[],
                after: None,
            },
            100,
        )
        .unwrap();
    assert_eq!(resolved.lattice.gap_after, None);
    assert_eq!(resolved.lattice.origin, ExactRatio::ZERO);
    assert_eq!(resolved.lattice.local_duration, frames(2));
    assert_eq!(resolved.lattice.grid_rule, AudioBindingGridRule::PointCeil);
}

#[test]
fn a_gap_own_argument_cannot_be_confused_with_its_outer_repeat_path() {
    let original = fixture(2);
    let state = gap_state(&original, |_| {});
    let own = id("r");
    let after = play("plays", 0);
    let instance = InstancePath {
        node: own.clone(),
        repeats: vec![RepeatInstance {
            node: own.clone(),
            iteration: after.clone(),
        }],
    };
    assert!(
        state
            .resolve_gap_in(
                &own,
                AudioBindingEnvironment::GapOccurrence {
                    instance: &instance,
                    after: &after
                },
                1000
            )
            .is_err()
    );
    let instance = InstancePath {
        node: own.clone(),
        repeats: vec![],
    };
    assert!(
        state
            .resolve_gap_in(
                &own,
                AudioBindingEnvironment::GapDefinition {
                    root: AudioDefinitionScope::RepeatGap(&own),
                    instance: &instance,
                    outside_repeats: std::slice::from_ref(&own),
                    after: None
                },
                1000
            )
            .is_err()
    );
}

macro_rules! closed_gap_origin {
    ($($name:ident: $version:literal => $legacy:ident),+ $(,)?) => {$(
        #[test]
        fn $name() {
            let original = fixture(2);
            let state = gap_state(&original, |binding| {
                binding.resume = Some(AudioResume { local_boundary: ExactRatio::ZERO, phase: AudioLocalPhase {
                    constant: ExactRatio::ZERO, terms: vec![AudioPhaseTerm { placement: binding.lattice.clone(), from_local: ExactRatio::ZERO, to_local: ExactRatio::ONE }],
                } });
                binding.reanchors.push(AudioReanchorStep { placement: binding.lattice.clone(), window: None });
            });
            let document = install(&original, &state);
            let mut wire = serde_json::to_value(&document).unwrap();
            wire["schema_version"] = serde_json::json!($version);
            let old = $legacy::Document::from_json(&wire.to_string()).unwrap();
            assert!(old.matches(&document));
            for path in ["lattice", "resume/phase/terms/0/placement", "reanchors/0/placement"] {
                for value in [serde_json::json!(null), serde_json::to_value(ExactRatio::ZERO).unwrap(), serde_json::to_value(ExactRatio::ONE).unwrap()] {
                    let mut forged = wire.clone();
                    forged.pointer_mut(&format!("/audio_bindings/gap_bindings/r/{path}")).unwrap()["reference_local_offset"] = value;
                    assert!($legacy::Document::from_json(&forged.to_string()).is_err());
                }
            }
            let state = gap_state(&original, |binding| { *binding = binding.rebase_local(ExactRatio::ONE).unwrap(); });
            assert!(!old.matches(&install(&original, &state)));
        }
    )+};
}
closed_gap_origin! {
    origin_gap_v22:22=>legacy_v22, origin_gap_v23:23=>legacy_v23,
    origin_gap_v24:24=>legacy_v24, origin_gap_v25:25=>legacy_v25,
    origin_gap_v26:26=>legacy_v26, origin_gap_v27:27=>legacy_v27,
    origin_gap_v28:28=>legacy_v28, origin_gap_v29:29=>legacy_v29,
    origin_gap_v30:30=>legacy_v30, origin_gap_v31:31=>legacy_v31, origin_gap_v32:32=>legacy_v32,
}
