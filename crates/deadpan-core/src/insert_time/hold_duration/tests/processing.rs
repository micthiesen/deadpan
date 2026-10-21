//! Hold edits preserve physical processing-input clocks, not old DSP output.
use super::*;

fn rate(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    let mut node = BeatNode::sequence("Stage", vec![]);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(length),
        mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        pitch,
        purpose: RetimePurpose::Edit,
    };
    node
}

fn processing(pitch: PitchPolicy, crop: i64, affine: bool) -> ProjectDocument {
    let mut document = fixture(0, false);
    document.nodes.insert(id("before"), source(3));
    document.nodes.insert(id("after"), source(4));
    let child = if affine {
        document.nodes.insert(
            id("affine"),
            rate("body", 6, 0, 8, PitchPolicy::FollowSpeed),
        );
        "affine"
    } else {
        "body"
    };
    document.nodes.insert(
        id("processing"),
        rate(child, 4, crop, if affine { 6 } else { 8 }, pitch),
    );
    document.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("processing"), id("tail")]),
    );
    document.validate().unwrap();
    document
}

fn instance(owner: &str, repeats: &[(&str, u32)]) -> InstancePath {
    InstancePath {
        node: id(owner),
        repeats: repeats
            .iter()
            .map(|(owner, ordinal)| RepeatInstance {
                node: id(owner),
                iteration: IterationId {
                    allocation: rev(owner),
                    ordinal: *ordinal,
                },
            })
            .collect(),
    }
}

fn captured(document: &ProjectDocument, allocation: &str) -> AudioBindingState {
    capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: rev(allocation),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn assert_old_lattice(old: &AudioBindingState, after: &ProjectDocument, path: &InstancePath) {
    let old = binding(old, path, &None);
    let actual = binding(after.audio_bindings(), path, &None);
    assert_eq!(actual.lattice, old.lattice);
}

#[test]
fn ordinary_hold_duration_processing_inputs_keep_exact_point_clocks_with_crop_and_history() {
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: 3 }] {
        for crop in [0, 1] {
            for partial in [false, true] {
                let mut before = processing(pitch, crop, false);
                let old = captured(&before, "old-input");
                let speech = instance("after", &[]);
                let lattice = binding(&old, &speech, &None).lattice;
                assert_eq!(
                    lattice.clock,
                    AudioClockRoot::PreserveInputPointCeil {
                        stage: id("processing")
                    }
                );
                assert_eq!(lattice.grid_origin, ExactRatio::integer(crop));
                assert_eq!(lattice.origin, ExactRatio::integer(5));
                assert_eq!(
                    lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                    if crop == 0 { 8008 } else { 6407 }
                );
                if partial {
                    before.audio_bindings = old.clone();
                    before.audio_bindings.bindings.remove(&id("after"));
                    before
                        .audio_bindings
                        .bindings
                        .get_mut(&id("processing"))
                        .unwrap()
                        .resume = Some(AudioResume {
                        local_boundary: ExactRatio::new(5, 2).unwrap(),
                        phase: AudioLocalPhase {
                            constant: ExactRatio::new(13, 7).unwrap(),
                            terms: vec![],
                        },
                    });
                    before.validate().unwrap();
                }
                for length in [1, 3] {
                    let after = resized(&before, "hold", length);
                    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
                    assert_eq!(
                        after.nodes()[&id("processing")],
                        before.nodes()[&id("processing")]
                    );
                    assert_old_lattice(&old, &after, &speech);
                    for owner in ["lead", "processing", "tail"] {
                        let binding = &after.audio_bindings.bindings()[&id(owner)];
                        assert!(binding.reanchors.is_empty(), "unchanged {owner}");
                        if partial {
                            assert_eq!(binding, &before.audio_bindings.bindings()[&id(owner)]);
                        }
                    }
                    // A second edit must capture the current pre-edit placement,
                    // while preserving the original lattice and resume history.
                    let mut next = request(&after, "hold", 2);
                    next.new_revision = rev("second-resize");
                    let edit = apply(&after, &next).unwrap();
                    let restored_length = edit.forward.apply(&after).unwrap();
                    assert_eq!(edit.inverse.apply(&restored_length).unwrap(), after);
                    assert_old_lattice(&old, &restored_length, &speech);
                    assert_eq!(
                        restored_length.duration().unwrap(),
                        before.duration().unwrap()
                    );
                }
                assert_eq!(
                    resized(&before, "hold", 2).audio_bindings,
                    before.audio_bindings
                );
                let mut invalid = before.clone();
                let NodeKind::Retime { mapping, .. } =
                    &mut invalid.nodes.get_mut(&id("processing")).unwrap().kind
                else {
                    panic!()
                };
                *mapping = FrameRange::new(ProjectFrame(crop), ProjectFrame(9)).unwrap();
                invalid.validate().unwrap();
                assert!(apply(&invalid, &request(&invalid, "hold", 1)).is_err());
            }
        }
    }
}

#[test]
fn ordinary_hold_duration_processing_existing_input_resume_uses_current_entry() {
    let mut before = processing(PitchPolicy::Preserve, 6, false);
    before.audio_bindings = captured(&before, "clipped-input");
    let speech = instance("after", &[]);
    let old = binding(&before.audio_bindings, &speech, &None);
    assert_eq!(old.lattice.local_support.start, ExactRatio::ONE);
    before
        .audio_bindings
        .bindings
        .get_mut(&id("after"))
        .unwrap()
        .resume = Some(AudioResume {
        local_boundary: ExactRatio::new(3, 2).unwrap(),
        phase: AudioLocalPhase {
            constant: ExactRatio::new(7, 11).unwrap(),
            terms: vec![],
        },
    });
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    assert_eq!(
        after.audio_bindings.bindings()[&id("after")].lattice,
        before.audio_bindings.bindings()[&id("after")].lattice
    );
    assert_eq!(
        after.audio_bindings.bindings()[&id("after")].resume,
        before.audio_bindings.bindings()[&id("after")].resume
    );
    let step = after.audio_bindings.bindings()[&id("after")]
        .reanchors
        .last()
        .unwrap();
    assert_eq!(step.placement.reference.timing.allocation, rev("resize"));
    assert_eq!(
        step.placement.reference.root,
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("processing")
        }
    );
    assert!(step.window.is_none());
    let resumed = binding(&after.audio_bindings, &speech, &None)
        .resume
        .unwrap();
    assert_eq!(resumed.local_boundary, ExactRatio::ONE);
    // The old cropped entry is local1; append against that actual allocation,
    // not local0 or the original resume's local3/2.
    let old_binding = binding(&before.audio_bindings, &speech, &None);
    let from = old_binding
        .lattice
        .sample_boundary(ExactRatio::new(3, 2).unwrap())
        .unwrap();
    let to = old_binding
        .lattice
        .sample_boundary(ExactRatio::ONE)
        .unwrap();
    let expected = ExactRatio::new(7, 11)
        .unwrap()
        .checked_add(
            ExactRatio::integer(to - from)
                .checked_mul(old_binding.lattice.local_frames_per_sample().unwrap())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(resumed.reference_local_delta, expected);
}

#[test]
fn ordinary_hold_duration_processing_dormant_default_never_grows_clock_history() {
    let mut before = processing(PitchPolicy::Preserve, 0, false);
    before.nodes.insert(id("inner"), repeat("inner", "body", 2));
    before.nodes.insert(id("owned-a"), source(6));
    before.nodes.insert(id("owned-b"), source(6));
    before.overrides.insert(
        id("inner"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: IterationId {
                    allocation: rev("inner"),
                    ordinal: 0,
                },
                root: id("owned-a"),
            },
            PlayOverride {
                iteration: IterationId {
                    allocation: rev("inner"),
                    ordinal: 1,
                },
                root: id("owned-b"),
            },
        ])
        .unwrap(),
    );
    before.nodes.insert(
        id("processing"),
        rate("inner", 4, 0, 13, PitchPolicy::Preserve),
    );
    before.validate().unwrap();
    before.audio_bindings = captured(&before, "dormant-input");
    let clocks = before.audio_bindings.clone();
    let mut current = before;
    for index in 0..MAX_AUDIO_BINDING_TERMS + 2 {
        let mut change = request(&current, "hold", if index % 2 == 0 { 1 } else { 3 });
        change.new_revision = rev(&format!("dormant-input-{index}"));
        let edit = apply(&current, &change).unwrap();
        let after = edit.forward.apply(&current).unwrap();
        assert_eq!(edit.inverse.apply(&after).unwrap(), current);
        assert_eq!(after.audio_bindings, clocks);
        current = after;
    }
}

#[test]
fn ordinary_hold_duration_processing_above_affine_keeps_rational_input_clock() {
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: 3 }] {
        let before = processing(pitch, 0, true);
        let old = captured(&before, "nested-input");
        let speech = instance("after", &[]);
        let old_speech = binding(&old, &speech, &None);
        assert_eq!(old_speech.lattice.origin, ExactRatio::new(15, 4).unwrap());
        assert_eq!(
            old_speech.lattice.frames_per_local_frame,
            ExactRatio::new(3, 4).unwrap()
        );
        assert_eq!(
            old_speech
                .lattice
                .sample_boundary(ExactRatio::ZERO)
                .unwrap(),
            6006
        );
        for length in [1, 3] {
            let after = resized(&before, "hold", length);
            assert_old_lattice(&old, &after, &speech);
            assert_eq!(after.nodes()[&id("affine")], before.nodes()[&id("affine")]);
            let live = captured(
                &{
                    let mut raw = after.clone();
                    raw.audio_bindings = AudioBindingState::default();
                    raw
                },
                "live-input",
            );
            let current = binding(&live, &speech, &None).lattice;
            assert_eq!(
                current.origin,
                ExactRatio::new(i128::from(3 * (3 + length)), 4).unwrap()
            );
            if length == 3 {
                assert_eq!(current.sample_boundary(ExactRatio::ZERO).unwrap(), 7208);
            }
        }
    }
}

#[test]
fn ordinary_hold_duration_processing_repeats_and_opaque_sibling_outputs_stay_on_own_clocks() {
    let mut before = fixture(2, true);
    before.nodes.insert(id("opaque-input"), source(2));
    before.nodes.insert(
        id("opaque"),
        rate("opaque-input", 2, 0, 2, PitchPolicy::Shift { semitones: 3 }),
    );
    let NodeKind::Sequence { children } = &mut before.nodes.get_mut(&id("body")).unwrap().kind
    else {
        panic!()
    };
    children.push(id("opaque"));
    before.nodes.insert(
        id("processing"),
        rate("inner", 8, 0, 12, PitchPolicy::Preserve),
    );
    let NodeKind::Repeat { child, .. } = &mut before.nodes.get_mut(&id("outer")).unwrap().kind
    else {
        panic!()
    };
    *child = id("processing");
    before.validate().unwrap();
    before.audio_bindings = captured(&before, "repeated-input");
    before.validate().unwrap();
    for length in [1, 3] {
        let after = resized(&before, "hold", length);
        assert_eq!(after.duration().unwrap(), before.duration().unwrap());
        for owner in ["lead", "tail", "processing", "opaque-input"] {
            assert_eq!(
                after.audio_bindings.bindings()[&id(owner)],
                before.audio_bindings.bindings()[&id(owner)]
            );
        }
        assert_eq!(
            after.audio_bindings.gap_bindings()[&id("outer")],
            before.audio_bindings.gap_bindings()[&id("outer")]
        );
        for owner in ["after", "owned-after", "opaque"] {
            let clock = &after.audio_bindings.bindings()[&id(owner)];
            assert_eq!(
                clock.lattice.reference.root,
                AudioClockRoot::PreserveInputPointCeil {
                    stage: id("processing")
                }
            );
            assert!(
                clock.reanchors.len()
                    > before.audio_bindings.bindings()[&id(owner)].reanchors.len()
            );
            assert!(clock.reanchors.iter().all(|step| step.window.is_none()));
        }
        for outer in 0..2 {
            for inner in 0..3 {
                assert_old_lattice(
                    &before.audio_bindings,
                    &after,
                    &instance(
                        if inner == 1 { "owned-after" } else { "after" },
                        &[("outer", outer), ("inner", inner)],
                    ),
                );
            }
        }
    }
    for owner in ["inner", "outer"] {
        let NodeKind::Repeat { iterations, .. } =
            &mut before.nodes.get_mut(&id(owner)).unwrap().kind
        else {
            panic!()
        };
        *iterations = IterationOrder::new(rev(owner), 1_000_000).unwrap();
    }
    before.audio_bindings = AudioBindingState::default();
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    assert_eq!(after.nodes().len(), before.nodes().len());
    assert!(after.audio_bindings.to_json().unwrap().len() < 50_000);
}

fn attach_effect(document: &mut ProjectDocument, owner: &str) -> SoundId {
    let effect = SoundId::new("effect").unwrap();
    let source = SourceAudio {
        asset: audio().asset,
        span: span(3100, 3200),
    };
    document.beat_sounds.insert(
        id(owner),
        std::collections::BTreeMap::from([(
            effect.clone(),
            BeatSound {
                label: "Effect".into(),
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    document.presentation_basis().frame_rate,
                )
                .unwrap(),
                source,
                offset: AudioSample(37),
                treatments: AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        )]),
    );
    document.validate().unwrap();
    effect
}

#[test]
fn ordinary_hold_duration_processing_root_ripple_shares_full_sound_journal_layout() {
    let mut before = processing(PitchPolicy::Preserve, 1, false);
    // This is the existing root-ripple path: move an unchanged processing
    // output carrying an accepted attachment by growing the preceding Hold.
    // Its intrinsic input still needs the same complete old timing layout.
    let effect = attach_effect(&mut before, "processing");
    let old_layout = FrozenAudioLayout::capture(&before).unwrap();
    let after = resized(&before, "lead", 4);
    let timing = AudioTimingId {
        allocation: rev("resize"),
        ordinal: 0,
    };
    assert_eq!(after.audio_bindings.timings()[&timing], old_layout);
    assert_eq!(after.beat_sounds, before.beat_sounds);
    assert_eq!(
        after.audio_bindings.sound_clocks()[&id("processing")][&effect].clocks()[0].timing(),
        &timing
    );
    let output = binding(after.audio_bindings(), &instance("processing", &[]), &None);
    assert_eq!(output.lattice.origin, ExactRatio::integer(3));
    assert_eq!(output.lattice.clock, AudioClockRoot::ProjectRootRoundEven);
    let live_output = FrozenAudioLayout::capture(&after)
        .unwrap()
        .project(
            &instance("processing", &[]),
            ExactRatio::ZERO,
            None,
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(live_output.origin, ExactRatio::integer(4));
    assert_eq!(
        after.nodes()[&id("processing")],
        before.nodes()[&id("processing")]
    );
    assert_eq!(
        after.audio_bindings.bindings()[&id("after")]
            .lattice
            .reference
            .root,
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("processing")
        }
    );
    let input = binding(after.audio_bindings(), &instance("after", &[]), &None);
    assert_eq!(input.lattice.origin, ExactRatio::integer(5));
    assert_eq!(input.lattice.grid_origin, ExactRatio::ONE);
    assert_eq!(
        input.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        6407
    );
    assert!(
        after.audio_bindings.bindings()[&id("after")]
            .reanchors
            .is_empty()
    );
}

#[test]
fn ordinary_hold_duration_processing_changed_attachment_scope_refuses_atomically() {
    for historical in [false, true] {
        let mut before = processing(PitchPolicy::Preserve, 1, false);
        if historical {
            before.audio_bindings = captured(&before, "attached-input");
        }
        attach_effect(&mut before, "processing");
        let snapshot = before.clone();
        let json = before.to_json().unwrap();
        let error = apply(&before, &request(&before, "hold", 3)).unwrap_err();
        // The unpublished physical-input edit cannot transport an attachment
        // whose processing operand itself changed. Keep this existing sound
        // lifecycle boundary explicit rather than accepting a fresh phase.
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert_eq!(error.message, "sound clock processing subtree changed");
        assert_eq!(before, snapshot);
        assert_eq!(before.to_json().unwrap(), json);
        assert_eq!(before.audio_bindings, snapshot.audio_bindings);
        assert_eq!(before.beat_sounds, snapshot.beat_sounds);
    }
}
