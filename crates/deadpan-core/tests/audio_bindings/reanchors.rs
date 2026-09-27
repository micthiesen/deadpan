use super::*;

fn window(start: i64, end: i64) -> Option<ExactFrameRange> {
    Some(ExactFrameRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap())
}

fn with_steps(
    layouts: &[&ProjectDocument],
    lattice: AudioPlacementTemplate,
    resume: Option<AudioResume>,
    reanchors: Vec<AudioReanchorStep>,
) -> AudioBindingState {
    AudioBindingState::new(
        layouts
            .iter()
            .enumerate()
            .map(|(ordinal, document)| AudioTimingRecord {
                id: timing(u32::try_from(ordinal).unwrap()),
                layout: FrozenAudioLayout::capture(document).unwrap(),
            })
            .collect(),
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice,
                resume,
                reanchors,
            },
        )]),
    )
    .unwrap()
}

#[test]
fn clipped_first_play_and_full_later_plays_have_distinct_resume_entries() {
    let old = document(
        &["inner"],
        [("inner", repeat("a", "plays", 3)), ("a", hold(2))],
    );
    let state = with_steps(
        &[&old],
        repeated(),
        None,
        vec![AudioReanchorStep {
            placement: repeated(),
            window: window(1, 6),
        }],
    );
    state.validate_for(&old).unwrap();
    let resolve = |ordinal| {
        state
            .resolve(&id("a"), &instance(&[("inner", "plays", ordinal)]), 1000)
            .unwrap()
    };
    let first = resolve(0).resume.unwrap();
    assert_eq!(first.local_boundary, ExactRatio::ONE);
    // 1601.6 samples/frame: first boundary rounds to 1602, never 1601.6.
    assert_eq!(first.reference_local_delta, ratio(1602 * 5, 8008));
    for ordinal in [1, 2] {
        let later = resolve(ordinal).resume.unwrap();
        assert_eq!(later.local_boundary, ExactRatio::ZERO);
        assert_eq!(later.reference_local_delta, ExactRatio::ZERO);
    }
    assert_eq!(
        AudioBindingState::from_json(&state.to_json().unwrap()).unwrap(),
        state
    );
}

#[test]
fn hidden_retained_plays_leave_the_previous_map_intact() {
    let old = document(
        &["inner"],
        [("inner", repeat("a", "plays", 3)), ("a", hold(2))],
    );
    let prior = AudioResume {
        local_boundary: ExactRatio::ONE,
        phase: AudioLocalPhase {
            constant: ratio(11, 7),
            terms: vec![],
        },
    };
    let step = AudioReanchorStep {
        placement: repeated(),
        window: window(2, 6),
    };
    let state = with_steps(&[&old], repeated(), Some(prior), vec![step.clone()]);
    let hidden = state
        .resolve(&id("a"), &instance(&[("inner", "plays", 0)]), 1000)
        .unwrap();
    assert_eq!(
        hidden.resume.unwrap(),
        ResolvedAudioResume {
            local_boundary: ExactRatio::ONE,
            reference_local_delta: ratio(11, 7),
        }
    );
    let untouched = with_steps(&[&old], repeated(), None, vec![step]);
    assert!(
        untouched
            .resolve(&id("a"), &instance(&[("inner", "plays", 0)]), 1000)
            .unwrap()
            .resume
            .is_none()
    );
}

#[test]
fn born_play_drops_outer_cut_but_keeps_intrinsic_partition_entry() {
    let old = document(
        &["inner"],
        [
            ("inner", repeat("partition", "plays", 2)),
            (
                "partition",
                retime(
                    "a",
                    2,
                    1,
                    3,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
            ("a", hold(4)),
        ],
    );
    let mut template = repeated();
    template.births[0].definition_root = id("partition");
    let state = with_steps(
        &[&old],
        template.clone(),
        None,
        vec![AudioReanchorStep {
            placement: template,
            window: window(1, 4),
        }],
    );
    let resolve = |allocation, ordinal| {
        state
            .resolve(&id("a"), &instance(&[("inner", allocation, ordinal)]), 1000)
            .unwrap()
    };
    assert_eq!(
        resolve("plays", 0).resume.unwrap().local_boundary,
        ExactRatio::integer(2)
    );
    assert_eq!(
        resolve("plays", 1).resume.unwrap().local_boundary,
        ExactRatio::ONE
    );
    let born = resolve("new_plays", 0);
    assert_eq!(
        born.lattice.clock,
        AudioClockRoot::DefinitionPointCeil {
            root: id("partition")
        }
    );
    let born = born.resume.unwrap();
    assert_eq!(born.local_boundary, ExactRatio::ONE);
    // The intrinsic Partition's source origin is -1. PointCeil allocates
    // [-1601,0) before its visible entry, unlike root ties-to-even [-1602,0).
    assert_eq!(born.reference_local_delta, ratio(1601 * 5, 8008));
}

#[test]
fn outer_birth_at_the_same_retained_root_keeps_its_intrinsic_window() {
    let old = document(&["a"], [("a", hold(4))]);
    let mut template = plain(0);
    template.births.push(AudioBirthClause {
        repeat: id("outer"),
        survivors: AudioBirthSurvivors::Run {
            allocation: revision("wrap"),
            first: 0,
            count: 1,
        },
        definition_root: id("root"),
    });
    let state = with_steps(
        &[&old],
        template.clone(),
        None,
        vec![AudioReanchorStep {
            placement: template,
            window: window(2, 4),
        }],
    );
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["root"] = serde_json::json!("top");
    wire["nodes"]["top"] =
        serde_json::to_value(BeatNode::sequence("Project", vec![id("outer")])).unwrap();
    wire["nodes"]["outer"] = serde_json::to_value(repeat("root", "wrap", 2)).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    state.validate_for(&current).unwrap();
    let born = state
        .resolve(&id("a"), &instance(&[("outer", "wrap", 1)]), 1000)
        .unwrap();
    assert_eq!(born.lattice.birth, Some(0));
    let resumed = born.resume.unwrap();
    assert_eq!(resumed.local_boundary, ExactRatio::integer(2));
    assert_eq!(resumed.reference_local_delta, ratio(3204 * 5, 8008));
}

#[test]
fn preserve_input_selection_constrains_the_step_on_its_point_grid() {
    let old = document(
        &["preserve"],
        [
            (
                "preserve",
                retime("a", 4, 1, 3, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
            ("a", hold(4)),
        ],
    );
    let mut template = plain(0);
    template.reference.root = AudioClockRoot::PreserveInputPointCeil {
        stage: id("preserve"),
    };
    let state = with_steps(
        &[&old],
        template.clone(),
        None,
        vec![AudioReanchorStep {
            placement: template,
            window: window(2, 4),
        }],
    );
    state.validate_for(&old).unwrap();
    let resumed = state
        .resolve(&id("a"), &instance(&[]), 1000)
        .unwrap()
        .resume
        .unwrap();
    assert_eq!(resumed.local_boundary, ExactRatio::integer(2));
    assert_eq!(resumed.reference_local_delta, ratio(1602 * 5, 8008));
}

#[test]
fn chronological_steps_extend_an_existing_phase_on_each_captured_clock() {
    let original = document(&["a"], [("a", hold(4))]);
    let moved = document(&["prefix", "a"], [("prefix", hold(1)), ("a", hold(4))]);
    let moved_again = document(&["prefix", "a"], [("prefix", hold(2)), ("a", hold(4))]);
    let state = with_steps(
        &[&original, &moved, &moved_again],
        plain(0),
        Some(AudioResume {
            local_boundary: ExactRatio::ONE,
            phase: AudioLocalPhase {
                constant: ratio(1602 * 5, 8008),
                terms: vec![],
            },
        }),
        vec![
            AudioReanchorStep {
                placement: plain(1),
                window: window(3, 5),
            },
            AudioReanchorStep {
                placement: plain(2),
                window: window(5, 6),
            },
        ],
    );
    let result = state
        .resolve(&id("a"), &instance(&[]), 1000)
        .unwrap()
        .resume
        .unwrap();
    assert_eq!(result.local_boundary, ExactRatio::integer(3));
    // Each new clock contributes 1602; reconstructing phase from local frame
    // three would yield 4804.8 or an unrelated rounded 4805 instead of 4806.
    assert_eq!(result.reference_local_delta, ratio(4806 * 5, 8008));
    assert_eq!(
        state.allocation_ids(),
        std::collections::BTreeSet::from([&revision("timings")])
    );
}

#[test]
fn reanchor_queries_stay_bounded_for_billion_play_and_birth_occurrences() {
    let old = document(
        &["inner"],
        [
            ("inner", repeat("a", "plays", 1_000_000_000)),
            ("a", hold(2)),
        ],
    );
    let state = with_steps(
        &[&old],
        repeated(),
        None,
        vec![AudioReanchorStep {
            placement: repeated(),
            window: window(1, 2_000_000_000),
        }],
    );
    let result = state
        .resolve(
            &id("a"),
            &instance(&[("inner", "plays", 999_999_999)]),
            1000,
        )
        .unwrap();
    assert!(result.work < 100);
    assert_eq!(result.resume.unwrap().local_boundary, ExactRatio::ZERO);
    assert!(
        state
            .resolve(
                &id("a"),
                &instance(&[("inner", "plays", 999_999_999)]),
                result.work - 1
            )
            .is_err()
    );
    assert_eq!(
        state
            .resolve(&id("a"), &instance(&[("inner", "born", 0)]), 1000)
            .unwrap()
            .resume
            .unwrap()
            .local_boundary,
        ExactRatio::ZERO
    );
}

#[test]
fn aggregate_terms_steps_and_invalid_windows_fail_typed_and_wire_admission() {
    let old = document(&["a"], [("a", hold(4))]);
    let step = AudioReanchorStep {
        placement: plain(0),
        window: window(1, 4),
    };
    let valid = with_steps(
        &[&old],
        plain(0),
        Some(AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant: ExactRatio::ZERO,
                terms: vec![
                    AudioPhaseTerm {
                        placement: plain(0),
                        from_local: ExactRatio::ZERO,
                        to_local: ExactRatio::ZERO
                    };
                    MAX_AUDIO_BINDING_TERMS - 1
                ],
            },
        }),
        vec![step.clone()],
    );
    let mut too_many = valid.bindings().clone();
    too_many.get_mut(&id("a")).unwrap().reanchors.push(step);
    assert!(
        AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing(0),
                layout: FrozenAudioLayout::capture(&old).unwrap()
            }],
            too_many
        )
        .is_err()
    );
    let mut wire = serde_json::to_value(valid).unwrap();
    wire["bindings"]["a"]["reanchors"]
        .as_array_mut()
        .unwrap()
        .push(
            serde_json::to_value(AudioReanchorStep {
                placement: plain(0),
                window: None,
            })
            .unwrap(),
        );
    assert!(AudioBindingState::from_json(&wire.to_string()).is_err());
    let valid = with_steps(
        &[&old],
        plain(0),
        None,
        vec![AudioReanchorStep {
            placement: plain(0),
            window: None,
        }],
    );
    for (start, end) in [(1, 1), (2, 1)] {
        let mut bindings = valid.bindings().clone();
        bindings.get_mut(&id("a")).unwrap().reanchors[0].window = Some(ExactFrameRange {
            start: ExactRatio::integer(start),
            end: ExactRatio::integer(end),
        });
        assert!(
            AudioBindingState::new(
                vec![AudioTimingRecord {
                    id: timing(0),
                    layout: FrozenAudioLayout::capture(&old).unwrap()
                }],
                bindings.clone()
            )
            .is_err()
        );
        let mut wire = serde_json::to_value(&valid).unwrap();
        wire["bindings"] = serde_json::to_value(bindings).unwrap();
        assert!(AudioBindingState::from_json(&wire.to_string()).is_err());
    }
}
