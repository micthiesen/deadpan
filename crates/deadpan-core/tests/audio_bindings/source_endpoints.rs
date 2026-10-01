use super::*;

fn picture() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 30,
            time_base,
        },
    )
    .unwrap()
}

fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> ProjectDocument {
    let mut wire = serde_json::to_value(super::document(&[], [])).unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("root", children.iter().map(|name| id(name)).collect()),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"]["picture"] = serde_json::to_value(AssetRecord {
        label: "Picture without audio".into(),
        content_hash: "a".repeat(64),
        video: Some(picture()),
        audio: None,
        still_image: false,
        frame_count: Some(frames(30)),
        source_qualification: None,
    })
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn source(duration: i64) -> BeatNode {
    let mut node = hold(duration);
    node.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(duration),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: AssetId::new("picture").unwrap(),
                span: picture(),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    node
}
fn crop(start: i64, end: i64) -> BeatNode {
    retime(
        "a",
        end - start,
        start,
        end,
        PitchPolicy::FollowSpeed,
        RetimePurpose::Partition,
    )
}
fn old(lead: i64, start: i64, end: i64) -> ProjectDocument {
    document(
        &["lead", "crop"],
        [
            ("lead", hold(lead)),
            ("crop", crop(start, end)),
            ("a", source(6)),
        ],
    )
}
fn make_state(layouts: &[&ProjectDocument], binding: OwnedAudioBinding) -> AudioBindingState {
    AudioBindingState::new(
        layouts
            .iter()
            .enumerate()
            .map(|(n, doc)| AudioTimingRecord {
                id: timing(u32::try_from(n).unwrap()),
                layout: FrozenAudioLayout::capture(doc).unwrap(),
            })
            .collect(),
        BTreeMap::from([(id("a"), binding)]),
    )
    .unwrap()
}
fn endpoint(placement: AudioPlacementTemplate, side: AudioSourceEndpoint) -> AudioReanchorStep {
    AudioReanchorStep::for_source_endpoint(placement, side)
}
fn bound(doc: &ProjectDocument, state: &AudioBindingState) -> ProjectDocument {
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn closed_source_end_keeps_the_endpoint_not_its_last_sample_and_rebases_once() {
    let old = old(5, 0, 1);
    let binding = OwnedAudioBinding {
        lattice: plain(0),
        resume: None,
        reanchors: vec![endpoint(plain(0), AudioSourceEndpoint::End)],
    };
    let before = make_state(&[&old], binding.clone());
    let resolved = before.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    assert_eq!(
        resolved.resume.as_ref().unwrap().local_boundary,
        ExactRatio::ONE
    );
    // B(6)-B(5)=9610-8008=1602; never use B(6)-1.
    assert_eq!(
        resolved.resume.unwrap().reference_local_delta,
        ratio(1602 * 5, 8008)
    );
    for prefix in [ratio(2, 1), ratio(7, 3), ratio(-1, 3)] {
        let translated = binding.rebase_local(prefix).unwrap();
        let after = make_state(&[&old], translated.clone());
        let got = after.resolve(&id("a"), &instance(&[]), 1000).unwrap();
        assert_eq!(
            got.resume.as_ref().unwrap().local_boundary,
            ExactRatio::ONE.checked_add(prefix).unwrap()
        );
        assert_eq!(
            got.resume.unwrap().reference_local_delta,
            ratio(1602 * 5, 8008)
        );
        assert_eq!(after.timings(), before.timings());
        assert_eq!(
            translated
                .rebase_local(ExactRatio::ZERO.checked_sub(prefix).unwrap())
                .unwrap(),
            binding
        );
    }
    assert_eq!(
        AudioBindingState::from_json(&before.to_json().unwrap()).unwrap(),
        before
    );
}

#[test]
fn closed_start_accepts_negative_historical_projection_and_empty_audio_support() {
    let old = old(1, 2, 3); // Physical zero projects to frame -1.
    let before = make_state(
        &[&old],
        OwnedAudioBinding {
            lattice: plain(0),
            resume: None,
            reanchors: vec![endpoint(plain(0), AudioSourceEndpoint::Start)],
        },
    );
    before.validate_for(&old).unwrap(); // Intentionally absent audio is still a Source clock.
    let got = before.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    assert_eq!(
        got.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        -1602
    );
    assert_eq!(got.resume.as_ref().unwrap().local_boundary, ratio(2, 1));
    assert_eq!(
        got.resume.unwrap().reference_local_delta,
        ratio(3204 * 5, 8008)
    );
}

#[test]
fn source_endpoint_follows_previous_symbolic_phase_and_chronological_steps() {
    let base = document(&["a"], [("a", source(6))]);
    let first = old(1, 0, 6);
    let second = old(2, 0, 6);
    let current = old(4, 3, 4);
    let binding = OwnedAudioBinding {
        lattice: plain(0),
        resume: Some(AudioResume {
            local_boundary: ExactRatio::ONE,
            phase: AudioLocalPhase {
                constant: ratio(64 * 5, 8008),
                terms: vec![AudioPhaseTerm {
                    placement: plain(0),
                    from_local: ExactRatio::ZERO,
                    to_local: ExactRatio::ONE,
                }],
            },
        }),
        reanchors: vec![
            AudioReanchorStep::for_allocation(
                plain(1),
                Some(ExactFrameRange::new(ratio(3, 1), ratio(7, 1)).unwrap()),
            ),
            AudioReanchorStep::for_allocation(
                plain(2),
                Some(ExactFrameRange::new(ratio(5, 1), ratio(8, 1)).unwrap()),
            ),
            endpoint(plain(3), AudioSourceEndpoint::End),
        ],
    };
    let state = make_state(&[&base, &first, &second, &current], binding);
    let got = state.resolve(&id("a"), &instance(&[]), 1000).unwrap();
    assert_eq!(got.resume.as_ref().unwrap().local_boundary, ratio(4, 1));
    // 64 + 1602 (term) + 1602 + 1602 + 1602 (closed end).
    assert_eq!(
        got.resume.unwrap().reference_local_delta,
        ratio(6472 * 5, 8008)
    );
    assert!(
        state
            .resolve(&id("a"), &instance(&[]), got.work - 1)
            .is_err()
    );
}

#[test]
fn endpoint_uses_each_birth_scope_and_keeps_intrinsic_crops() {
    let before = document(
        &["lead", "inner"],
        [
            ("lead", hold(1)),
            ("inner", repeat("crop", "plays", 2)),
            ("crop", crop(1, 3)),
            ("a", source(6)),
        ],
    );
    let mut template = repeated();
    template.births[0].definition_root = id("crop");
    let state = make_state(
        &[&before],
        OwnedAudioBinding {
            lattice: template.clone(),
            resume: None,
            reanchors: vec![endpoint(template.clone(), AudioSourceEndpoint::End)],
        },
    );
    let survivor = state
        .resolve(&id("a"), &instance(&[("inner", "plays", 0)]), 1000)
        .unwrap();
    let born = state
        .resolve(&id("a"), &instance(&[("inner", "born", 0)]), 1000)
        .unwrap();
    assert_eq!(survivor.resume.unwrap().local_boundary, ratio(3, 1));
    assert_eq!(
        survivor.lattice.grid_rule,
        AudioBindingGridRule::RootRoundEven
    );
    assert_eq!(born.resume.unwrap().local_boundary, ratio(3, 1));
    assert_eq!(born.lattice.grid_rule, AudioBindingGridRule::PointCeil);
    assert_eq!(born.lattice.sample_boundary(ratio(3, 1)).unwrap(), 3204); // ceil(3203.2)
    assert_eq!(
        born.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        -1601
    );
    // An innermost new definition excludes the formerly enclosing crop.
    template.births.push(AudioBirthClause {
        repeat: id("new-inner"),
        survivors: AudioBirthSurvivors::Run {
            allocation: revision("wrap"),
            first: 0,
            count: 1,
        },
        definition_root: id("a"),
    });
    let narrower = make_state(
        &[&before],
        OwnedAudioBinding {
            lattice: template.clone(),
            resume: None,
            reanchors: vec![endpoint(template, AudioSourceEndpoint::End)],
        },
    );
    let got = narrower
        .resolve(
            &id("a"),
            &instance(&[("inner", "plays", 0), ("new-inner", "wrap", 1)]),
            1000,
        )
        .unwrap();
    assert_eq!(got.resume.unwrap().local_boundary, ratio(6, 1));
    assert_eq!(
        got.lattice.clock,
        AudioClockRoot::DefinitionPointCeil { root: id("a") }
    );
}

#[test]
fn source_endpoint_rejects_wrong_kinds_window_and_hidden_old_allocation() {
    let old = old(1, 0, 1);
    let make = |step| OwnedAudioBinding {
        lattice: plain(0),
        resume: None,
        reanchors: vec![step],
    };
    let mut illegal = endpoint(plain(0), AudioSourceEndpoint::End);
    illegal.window = Some(ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::ONE).unwrap());
    let construct = |doc: &ProjectDocument, binding| {
        AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing(0),
                layout: FrozenAudioLayout::capture(doc).unwrap(),
            }],
            BTreeMap::from([(id("a"), binding)]),
        )
    };
    assert!(construct(&old, make(illegal)).is_err());
    let mut gap_recipe = endpoint(plain(0), AudioSourceEndpoint::Start);
    gap_recipe.placement.reference.recipe = AudioRecipeKind::RepeatGap;
    assert!(construct(&old, make(gap_recipe)).is_err());
    let mut gap_argument = endpoint(plain(0), AudioSourceEndpoint::Start);
    gap_argument.placement.gap_after = Some(AudioRepeatValue::Captured {
        iteration: play("gap", 0),
    });
    assert!(construct(&old, make(gap_argument)).is_err());
    let hold_doc = document(&["a"], [("a", hold(6))]);
    assert!(
        construct(
            &hold_doc,
            make(endpoint(plain(0), AudioSourceEndpoint::Start))
        )
        .is_err()
    );
    let state = construct(&old, make(endpoint(plain(0), AudioSourceEndpoint::End))).unwrap();
    assert!(state.validate_for(&hold_doc).is_err());
    let hidden = document(
        &["outer"],
        [
            ("a", source(6)),
            ("lead", hold(2)),
            (
                "group",
                BeatNode::sequence("group", vec![id("lead"), id("a")]),
            ),
            (
                "outer",
                retime(
                    "group",
                    1,
                    0,
                    1,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
    );
    let hidden_state =
        construct(&hidden, make(endpoint(plain(0), AudioSourceEndpoint::End))).unwrap();
    let error = hidden_state
        .resolve(&id("a"), &instance(&[]), 1000)
        .unwrap_err();
    assert!(error.to_string().contains("positive captured allocation"));
    // The historical AllocationEntry skip is deliberately unchanged.
    let ordinary = construct(
        &hidden,
        make(AudioReanchorStep::for_allocation(plain(0), None)),
    )
    .unwrap();
    assert!(
        ordinary
            .resolve(&id("a"), &instance(&[]), 1000)
            .unwrap()
            .resume
            .is_none()
    );
}

#[test]
fn endpoint_terms_stay_bounded_and_current_wire_is_closed() {
    let old = old(1, 0, 1);
    let step = endpoint(plain(0), AudioSourceEndpoint::End);
    let good = make_state(
        &[&old],
        OwnedAudioBinding {
            lattice: plain(0),
            resume: None,
            reanchors: vec![step.clone(); MAX_AUDIO_BINDING_TERMS],
        },
    );
    let mut wire = serde_json::to_value(&good).unwrap();
    wire["bindings"]["a"]["reanchors"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(&step).unwrap());
    assert!(AudioBindingState::from_json(&wire.to_string()).is_err());
    for value in [
        serde_json::json!(null),
        serde_json::json!({"type":"allocation_entry","point":1}),
        serde_json::json!({"type":"source_endpoint","endpoint":"middle"}),
        serde_json::json!({"type":"source_endpoint","endpoint":"end","point":1}),
    ] {
        let mut wire = serde_json::to_value(&step).unwrap();
        wire["anchor"] = value;
        assert!(serde_json::from_value::<AudioReanchorStep>(wire).is_err());
    }
    let ordinary = AudioReanchorStep::for_allocation(plain(0), None);
    assert!(
        serde_json::to_value(&ordinary)
            .unwrap()
            .get("anchor")
            .is_none()
    );
    let encoded = serde_json::to_string(&step).unwrap();
    assert!(
        serde_json::from_str::<AudioReanchorStep>(&encoded.replace(
            "\"endpoint\":\"end\"",
            "\"endpoint\":\"end\",\"endpo\\u0069nt\":\"start\""
        ))
        .is_err()
    );
}

#[test]
fn split_copies_symbolic_endpoint_and_inverse_restores_exact_state() {
    let old = old(1, 0, 4);
    let state = make_state(
        &[&old],
        OwnedAudioBinding {
            lattice: plain(0),
            resume: None,
            reanchors: vec![endpoint(plain(0), AudioSourceEndpoint::End)],
        },
    );
    let before = bound(&old, &state);
    let tx = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("split-endpoint"),
            command: Command::Split {
                node: id("crop"),
                at: frames(2),
                identities: SplitIdentities {
                    nodes: (0..10).map(|i| id(&format!("copy-{i}"))).collect(),
                },
            },
        },
    )
    .unwrap();
    let after = tx.forward.apply(&before).unwrap();
    let copies: Vec<_> = after
        .audio_bindings()
        .bindings()
        .values()
        .filter(|binding| !binding.reanchors.is_empty())
        .collect();
    assert_eq!(copies.len(), 2);
    assert!(
        copies
            .iter()
            .all(|binding| binding.reanchors == state.bindings()[&id("a")].reanchors)
    );
    assert_eq!(after.audio_bindings().timings(), state.timings());
    assert_eq!(tx.inverse.apply(&after).unwrap(), before);
}

#[test]
fn captured_slice_renames_endpoint_aliases_on_two_independent_pastes() {
    let historical = old(1, 0, 4);
    let state = make_state(
        &[&historical],
        OwnedAudioBinding {
            lattice: plain(0),
            resume: None,
            reanchors: vec![endpoint(plain(0), AudioSourceEndpoint::End)],
        },
    );
    let before = bound(&old(2, 0, 4), &state);
    let slice = CapturedEditSlice::capture_selection(
        &before,
        &id("root"),
        &SliceCaptureSelection::Child { node: id("crop") },
        AudioTimingId {
            allocation: revision("slice-capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let slice = CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap();
    let required = slice.identity_requirements().unwrap();
    let mut destination = document(&[], []);
    for (index, name) in ["first", "second"].into_iter().enumerate() {
        let tx = apply(
            &destination,
            &CommandRequest {
                project_id: destination.project_id().clone(),
                expected_revision: destination.revision_id().clone(),
                new_revision: revision(name),
                command: Command::SpliceSlice {
                    parent: id("root"),
                    index,
                    slice: slice.clone(),
                    timing: AudioTimingId {
                        allocation: revision(name),
                        ordinal: 0,
                    },
                    identities: SlicePasteIdentities {
                        authored: OccurrenceIdentities {
                            nodes: (0..required.nodes)
                                .map(|n| id(&format!("{name}-{n}")))
                                .collect(),
                            marks: (0..required.marks)
                                .map(|n| MarkId::new(format!("{name}-{n}")).unwrap())
                                .collect(),
                        },
                        aliases: (0..required.aliases)
                            .map(|n| id(&format!("{name}-alias-{n}")))
                            .collect(),
                    },
                },
            },
        )
        .unwrap();
        let next = tx.forward.apply(&destination).unwrap();
        assert_eq!(tx.inverse.apply(&next).unwrap(), destination);
        destination = next;
    }
    let endpoints: Vec<_> = destination
        .audio_bindings()
        .bindings()
        .iter()
        .filter(|(_, binding)| {
            binding
                .reanchors
                .iter()
                .any(|step| !step.anchor.is_allocation_entry())
        })
        .collect();
    assert_eq!(endpoints.len(), 2);
    let aliases: std::collections::BTreeSet<_> = endpoints
        .iter()
        .map(|(_, binding)| binding.reanchors[0].placement.reference.physical.clone())
        .collect();
    assert_eq!(aliases.len(), 2);
    assert!(!aliases.contains(&id("a")));
    for (owner, binding) in endpoints {
        assert_eq!(binding.reanchors.len(), 2);
        assert_eq!(
            binding.reanchors[0].anchor,
            AudioReanchorAnchor::SourceEndpoint {
                endpoint: AudioSourceEndpoint::End
            }
        );
        assert!(binding.reanchors[1].anchor.is_allocation_entry());
        let got = destination
            .audio_bindings()
            .resolve(
                owner,
                &InstancePath {
                    node: owner.clone(),
                    repeats: vec![],
                },
                1000,
            )
            .unwrap();
        let resume = got.resume.unwrap();
        assert_eq!(resume.local_boundary, ExactRatio::ZERO);
        // Historical End adds B(5)-B(1)=6406 samples. Slice capture then
        // subtracts B(6)-B(2)=6407, retaining exactly one sample of phase.
        assert_eq!(resume.reference_local_delta, ratio(-5, 8008));

        let mut without_endpoint = binding.clone();
        without_endpoint.reanchors.remove(0);
        let mut control_bindings = destination.audio_bindings().bindings().clone();
        control_bindings.insert(owner.clone(), without_endpoint);
        let control = AudioBindingState::new(
            destination
                .audio_bindings()
                .timings()
                .iter()
                .map(|(id, layout)| AudioTimingRecord {
                    id: id.clone(),
                    layout: layout.clone(),
                })
                .collect(),
            control_bindings,
        )
        .unwrap()
        .resolve(
            owner,
            &InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            1000,
        )
        .unwrap()
        .resume
        .unwrap();
        assert_eq!(control.local_boundary, ExactRatio::ZERO);
        assert_eq!(control.reference_local_delta, ExactRatio::ZERO);
    }
}

#[test]
fn source_endpoint_resolves_billion_play_and_new_birth_without_expansion() {
    let old = document(
        &["inner"],
        [
            ("inner", repeat("a", "plays", 1_000_000_000)),
            ("a", source(2)),
        ],
    );
    let state = make_state(
        &[&old],
        OwnedAudioBinding {
            lattice: repeated(),
            resume: None,
            reanchors: vec![endpoint(repeated(), AudioSourceEndpoint::End)],
        },
    );
    for path in [
        instance(&[("inner", "plays", 999_999_999)]),
        instance(&[("inner", "born", 0)]),
    ] {
        let resolved = state.resolve(&id("a"), &path, 1000).unwrap();
        assert!(resolved.work < 100);
        assert_eq!(resolved.resume.unwrap().local_boundary, ratio(2, 1));
        assert!(state.resolve(&id("a"), &path, resolved.work - 1).is_err());
    }
}
