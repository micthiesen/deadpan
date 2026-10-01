use super::*;

fn translated(state: &AudioBindingState, prefix: ExactRatio) -> AudioBindingState {
    AudioBindingState::new(
        state
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        state
            .bindings()
            .iter()
            .map(|(id, binding)| (id.clone(), binding.rebase_local(prefix).unwrap()))
            .collect(),
    )
    .unwrap()
}
fn symbolic() -> OwnedAudioBinding {
    OwnedAudioBinding {
        lattice: repeated(),
        resume: Some(AudioResume {
            local_boundary: ExactRatio::ONE,
            phase: AudioLocalPhase {
                constant: ratio(3, 7),
                terms: vec![AudioPhaseTerm {
                    placement: repeated(),
                    from_local: ExactRatio::ZERO,
                    to_local: ExactRatio::ONE,
                }],
            },
        }),
        reanchors: vec![AudioReanchorStep {
            placement: repeated(),
            window: Some(ExactFrameRange::new(ratio(1, 1), ratio(3, 1)).unwrap()),
        }],
    }
}

#[test]
fn signed_local_rebase_composes_and_inverts_every_current_coordinate() {
    let original = symbolic();
    for (a, b) in [
        (ratio(1, 1), ratio(2, 1)),
        (ratio(-7, 3), ratio(5, 7)),
        (ratio(1, 7), ratio(-1, 7)),
    ] {
        let moved = original.rebase_local(a).unwrap();
        assert_eq!(
            moved.lattice.reference_local_offset,
            ExactRatio::ZERO.checked_sub(a).unwrap()
        );
        let resume = moved.resume.as_ref().unwrap();
        assert_eq!(
            resume.local_boundary,
            ExactRatio::ONE.checked_add(a).unwrap()
        );
        assert_eq!(resume.phase.constant, ratio(3, 7));
        assert_eq!(resume.phase.terms[0].from_local, a);
        assert_eq!(
            resume.phase.terms[0].to_local,
            ExactRatio::ONE.checked_add(a).unwrap()
        );
        assert_eq!(
            resume.phase.terms[0].placement.reference_local_offset,
            moved.lattice.reference_local_offset
        );
        assert_eq!(
            moved.reanchors[0].placement.reference_local_offset,
            moved.lattice.reference_local_offset
        );
        assert_eq!(moved.reanchors[0].window, original.reanchors[0].window);
        assert_eq!(
            moved.rebase_local(b).unwrap(),
            original.rebase_local(a.checked_add(b).unwrap()).unwrap()
        );
        assert_eq!(
            moved
                .rebase_local(ExactRatio::ZERO.checked_sub(a).unwrap())
                .unwrap(),
            original
        );
        let encoded = serde_json::to_string(&moved).unwrap();
        assert_eq!(
            serde_json::from_str::<OwnedAudioBinding>(&encoded).unwrap(),
            moved
        );
    }
    assert!(
        !serde_json::to_string(&original)
            .unwrap()
            .contains("reference_local_offset")
    );
}

#[test]
fn local_rebase_overflow_keeps_input_intact_in_every_coordinate_family() {
    for field in ["lattice", "resume", "from", "to", "term", "step"] {
        let mut original = symbolic();
        match field {
            "lattice" => original.lattice.reference_local_offset = ratio(i128::MIN, 1),
            "resume" => original.resume.as_mut().unwrap().local_boundary = ratio(i128::MAX, 1),
            "from" => {
                original.resume.as_mut().unwrap().phase.terms[0].from_local = ratio(i128::MAX, 1)
            }
            "to" => original.resume.as_mut().unwrap().phase.terms[0].to_local = ratio(i128::MAX, 1),
            "term" => {
                original.resume.as_mut().unwrap().phase.terms[0]
                    .placement
                    .reference_local_offset = ratio(i128::MIN, 1)
            }
            "step" => original.reanchors[0].placement.reference_local_offset = ratio(i128::MIN, 1),
            _ => unreachable!(),
        }
        let before = original.clone();
        assert!(original.rebase_local(ExactRatio::ONE).is_err(), "{field}");
        assert_eq!(original, before);
    }
}

#[test]
fn symbolic_local_translation_preserves_each_repeated_sample_count() {
    let original = document(
        &["inner"],
        [("inner", repeat("a", "inner_plays", 2)), ("a", hold(1))],
    );
    let mut binding = symbolic();
    binding.reanchors.clear();
    binding.resume.as_mut().unwrap().phase.constant = ExactRatio::ZERO;
    let before = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing(0),
            layout: FrozenAudioLayout::capture(&original).unwrap(),
        }],
        BTreeMap::from([(id("a"), binding)]),
    )
    .unwrap();
    for prefix in [ExactRatio::ONE, ratio(5, 3), ratio(-1, 7)] {
        let after = translated(&before, prefix);
        assert_eq!(after.timings(), before.timings());
        for (ordinal, count) in [(0, 1602), (1, 1601)] {
            let path = instance(&[("inner", "inner_plays", ordinal)]);
            let old = before.resolve(&id("a"), &path, 1000).unwrap();
            let new = after.resolve(&id("a"), &path, 1000).unwrap();
            assert_eq!(
                new.resume.as_ref().unwrap().reference_local_delta,
                ratio(count * 5, 8008)
            );
            assert_eq!(
                new.resume.as_ref().unwrap().reference_local_delta,
                old.resume.as_ref().unwrap().reference_local_delta
            );
            assert_eq!(
                new.resume.unwrap().local_boundary,
                old.resume
                    .unwrap()
                    .local_boundary
                    .checked_add(prefix)
                    .unwrap()
            );
            assert_eq!(
                new.lattice.local_support.start,
                old.lattice.local_support.start.checked_add(prefix).unwrap()
            );
            assert_eq!(
                new.lattice.local_support.end,
                old.lattice.local_support.end.checked_add(prefix).unwrap()
            );
            for point in [ratio(-2, 1), ExactRatio::ZERO, ratio(5, 7), ratio(2, 1)] {
                assert_eq!(
                    new.lattice
                        .sample_boundary(point.checked_add(prefix).unwrap())
                        .unwrap(),
                    old.lattice.sample_boundary(point).unwrap()
                );
            }
        }
    }
}

#[test]
fn chronological_reanchors_convert_historical_entries_after_local_translation() {
    let original = document(&["a"], [("a", hold(4))]);
    let moved = document(&["prefix", "a"], [("prefix", hold(1)), ("a", hold(4))]);
    let moved_again = document(&["prefix", "a"], [("prefix", hold(2)), ("a", hold(4))]);
    let before = AudioBindingState::new(
        [&original, &moved, &moved_again]
            .iter()
            .enumerate()
            .map(|(ordinal, doc)| AudioTimingRecord {
                id: timing(u32::try_from(ordinal).unwrap()),
                layout: FrozenAudioLayout::capture(doc).unwrap(),
            })
            .collect(),
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: plain(0),
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ONE,
                    phase: AudioLocalPhase {
                        constant: ratio(1602 * 5, 8008),
                        terms: vec![],
                    },
                }),
                reanchors: vec![
                    AudioReanchorStep {
                        placement: plain(1),
                        window: Some(ExactFrameRange::new(ratio(3, 1), ratio(5, 1)).unwrap()),
                    },
                    AudioReanchorStep {
                        placement: plain(2),
                        window: Some(ExactFrameRange::new(ratio(5, 1), ratio(6, 1)).unwrap()),
                    },
                ],
            },
        )]),
    )
    .unwrap();
    let old = before.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    assert_eq!(
        old.resume.as_ref().unwrap().reference_local_delta,
        ratio(4806 * 5, 8008)
    );
    for prefix in [ExactRatio::ONE, ratio(7, 3), ratio(-2, 1)] {
        let after = translated(&before, prefix);
        let new = after.resolve(&id("a"), &instance(&[]), 1000).unwrap();
        assert_eq!(
            new.resume.as_ref().unwrap().reference_local_delta,
            old.resume.as_ref().unwrap().reference_local_delta
        );
        assert_eq!(
            new.resume.unwrap().local_boundary,
            ratio(3, 1).checked_add(prefix).unwrap()
        );
        assert_eq!(after.timings(), before.timings());
        assert_eq!(
            after.bindings()[&id("a")].reanchors[0].window,
            before.bindings()[&id("a")].reanchors[0].window
        );
    }
}

#[test]
fn translated_point_clock_keeps_captured_support() {
    let original = document(
        &["stage"],
        [
            ("a", hold(6)),
            (
                "stage",
                retime("a", 2, 2, 6, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
    );
    let mut template = plain(0);
    template.reference.root = AudioClockRoot::PreserveInputPointCeil { stage: id("stage") };
    let before = state(&original, template);
    let old = before.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    let after = translated(&before, ratio(3, 1));
    let new = after.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    assert_eq!(new.lattice.grid_rule, AudioBindingGridRule::PointCeil);
    assert_eq!(new.lattice.grid_origin, old.lattice.grid_origin);
    assert_eq!(new.lattice.local_support, ratio(5, 1)..ratio(9, 1));
    for point in [ratio(-4, 3), ratio(2, 1), ratio(17, 5), ratio(6, 1)] {
        assert_eq!(
            new.lattice
                .sample_boundary(point.checked_add(ratio(3, 1)).unwrap())
                .unwrap(),
            old.lattice.sample_boundary(point).unwrap()
        );
    }
}
