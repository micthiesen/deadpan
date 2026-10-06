use super::*;

fn sound() -> SoundId {
    SoundId::new("effect").unwrap()
}

fn attached(document: &ProjectDocument, owner: &str) -> ProjectDocument {
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
        SourceTimestamp {
            ticks: 1000,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
    )
    .unwrap();
    let asset = AssetId::new("effect").unwrap();
    let registered = if document.assets().contains_key(&asset) {
        document.clone()
    } else {
        edit(
            document,
            &request(
                document,
                "registered",
                Command::AddAsset {
                    id: asset.clone(),
                    asset: AssetRecord {
                        label: "Effect".into(),
                        content_hash: "a".repeat(64),
                        audio: Some(span),
                        video: None,
                        frame_count: None,
                        still_image: false,
                        source_qualification: Some(
                            SourceQualificationId::new("b".repeat(64)).unwrap(),
                        ),
                    },
                },
            ),
        )
    };
    edit(
        &registered,
        &request(
            &registered,
            &format!("attached-{owner}"),
            Command::SetBeatSound {
                owner: id(owner),
                id: sound(),
                event: BeatSound {
                    label: "Effect".into(),
                    source: SourceAudio { asset, span },
                    mapping: SourceAudioMapping::natural_rate(
                        span,
                        registered.presentation_basis().frame_rate,
                    )
                    .unwrap(),
                    offset: AudioSample(37),
                    gain_millidecibels: -1500,
                    start_edge: AudioEdgePolicy::Hard,
                    end_edge: AudioEdgePolicy::Automatic,
                    overflow: SoundOverflowPolicy::Reject,
                },
            },
        ),
    )
}

fn fixture() -> ProjectDocument {
    attached(
        &tree(
            &["lead", "group", "tail"],
            vec![
                ("lead", hold(1)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("owner"), id("other")]),
                ),
                ("owner", hold(4)),
                ("other", hold(4)),
                ("tail", hold(2)),
            ],
        ),
        "owner",
    )
}

fn shifted(document: &ProjectDocument, name: &str) -> ProjectDocument {
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    edit(
        document,
        &request(
            document,
            name,
            Command::InsertTime {
                at: ProjectFrame(0),
                hold: recipe,
                id: id(name),
                identities: SplitIdentities::default(),
                timing: timing(name),
            },
        ),
    )
}

fn capture(
    document: &ProjectDocument,
    parent: &str,
    owner: &NodeId,
    name: &str,
) -> CapturedEditSlice {
    CapturedEditSlice::capture_selection(
        document,
        &id(parent),
        &SliceCaptureSelection::Child {
            node: owner.clone(),
        },
        timing(name),
    )
    .unwrap()
}

fn copied_owner(document: &ProjectDocument, prefix: &str) -> NodeId {
    document
        .beat_sounds()
        .keys()
        .find(|owner| owner.as_str().starts_with(prefix))
        .unwrap()
        .clone()
}

#[test]
fn copied_sound_clocks_narrow_ordinary_scope_and_survive_deleted_original_and_recopy() {
    let before = shifted(&shifted(&fixture(), "one"), "two");
    let source_journal = &before.audio_bindings().sound_clocks()[&id("owner")][&sound()];
    assert_eq!(source_journal.scope(), &id("group"));
    let slice = capture(&before, "group", &id("owner"), "capture");
    let wire = slice.to_json().unwrap();
    assert_eq!(CapturedEditSlice::from_json(&wire).unwrap(), slice);
    slice.validate_capture(&before).unwrap();
    let removed = edit(
        &before,
        &request(
            &before,
            "removed",
            Command::DeleteRipple {
                node: id("group"),
                timing: timing("removed"),
            },
        ),
    );
    assert!(removed.beat_sounds().is_empty());
    let once = edit(&removed, &paste(&removed, &slice, "first", 0));
    let first = copied_owner(&once, "first");
    let journal = &once.audio_bindings().sound_clocks()[&first][&sound()];
    assert_eq!(journal.scope(), &first);
    assert_eq!(journal.clocks().len(), 3);
    assert_eq!(
        once.beat_sounds()[&first][&sound()],
        before.beat_sounds()[&id("owner")][&sound()]
    );
    for reference in journal.clocks() {
        assert_eq!(reference.scope(), reference.owner());
        assert_ne!(reference.owner(), &first);
        assert!(!once.nodes().contains_key(reference.owner()));
    }
    let twice = edit(&once, &paste(&once, &slice, "second", 1));
    let second = copied_owner(&twice, "second");
    let second_journal = &twice.audio_bindings().sound_clocks()[&second][&sound()];
    assert_ne!(first, second);
    let first_ids: BTreeSet<_> = journal.clocks().iter().map(|r| r.timing()).collect();
    assert!(
        second_journal
            .clocks()
            .iter()
            .all(|r| !first_ids.contains(r.timing()))
    );
    // Copy the wrapper enclosing an already narrower sound scope. Retaining the
    // smaller scope keeps earlier alias maps valid without inventing old wrappers.
    let recopy = capture(&twice, "root", &id("first-node-0"), "recopy");
    let third = edit(&twice, &paste(&twice, &recopy, "third", 0));
    let third_owner = copied_owner(&third, "third");
    assert_eq!(
        third.audio_bindings().sound_clocks()[&third_owner][&sound()]
            .clocks()
            .len(),
        4
    );
    assert_eq!(slice.to_json().unwrap(), wire);
}

#[test]
fn first_sound_copy_retains_source_phase_and_can_move_after_paste() {
    let before = fixture();
    assert!(before.audio_bindings().sound_clocks().is_empty());
    let slice = capture(&before, "root", &id("group"), "capture");
    let copied = edit(&before, &paste(&before, &slice, "pasted", 3));
    let owner = copied_owner(&copied, "pasted");
    let journal = &copied.audio_bindings().sound_clocks()[&owner][&sound()];
    assert_eq!(journal.clocks().len(), 1);
    assert_ne!(journal.scope(), &owner);
    let original_layout = &copied.audio_bindings().timings()[journal.clocks()[0].timing()];
    let original = original_layout
        .project(
            &InstancePath {
                node: journal.clocks()[0].owner().clone(),
                repeats: vec![],
            },
            ExactRatio::ZERO,
            None,
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(original.origin, ExactRatio::ONE);
    let moved = shifted(&copied, "moved-copy");
    let journal = &moved.audio_bindings().sound_clocks()[&owner][&sound()];
    assert_eq!(journal.clocks().len(), 2);
    assert_eq!(journal.clocks()[1].timing(), &timing("moved-copy"));
    assert_eq!(journal.clocks()[1].owner(), &owner);
}

#[test]
fn copied_sound_clocks_reject_capture_collisions_forged_aliases_and_partial_owners() {
    let before = shifted(&fixture(), "shifted");
    let snapshot = before.to_json().unwrap();
    let error = CapturedEditSlice::capture_selection(
        &before,
        &id("group"),
        &SliceCaptureSelection::Child { node: id("owner") },
        timing("shifted"),
    )
    .unwrap_err();
    assert!(error.message.contains("capture timing identity"), "{error}");
    assert!(
        CapturedEditSlice::capture(&before, &id("group"), range(3, 5), timing("partial")).is_err()
    );
    let slice = capture(&before, "group", &id("owner"), "capture");
    let mut forged = serde_json::to_value(&slice).unwrap();
    forged["audio_bindings"]["sound_clocks"]["owner"]["effect"]["clocks"][0]["owner"] =
        json!("other");
    assert!(CapturedEditSlice::from_json(&forged.to_string()).is_err());
    for variant in 0..4 {
        let mut bad = paste(&before, &slice, "bad", 0);
        let Command::SpliceSlice {
            identities, timing, ..
        } = &mut bad.command
        else {
            unreachable!()
        };
        match variant {
            0 => {
                identities.aliases.pop();
            }
            1 => identities.aliases[0] = id("owner"),
            2 => identities.aliases[0] = identities.authored.nodes[0].clone(),
            _ => timing.ordinal = u32::MAX,
        }
        assert!(apply(&before, &bad).is_err(), "variant {variant}");
    }
    assert_eq!(before.to_json().unwrap(), snapshot);
}

#[test]
fn copied_sound_clocks_link_fresh_repeat_plays_to_every_historical_alias() {
    let mut repeated = hold(1);
    repeated.kind = NodeKind::Repeat {
        child: id("owner"),
        iterations: IterationOrder::new(revision("plays"), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    let initial = attached(
        &tree(
            &["lead", "repeat"],
            vec![("lead", hold(1)), ("repeat", repeated), ("owner", hold(4))],
        ),
        "owner",
    );
    let before = shifted(&initial, "shifted");
    let slice = capture(&before, "root", &id("repeat"), "capture");
    let pasted = edit(&before, &paste(&before, &slice, "copy", 0));
    let owner = copied_owner(&pasted, "copy");
    let journal = &pasted.audio_bindings().sound_clocks()[&owner][&sound()];
    let NodeKind::Repeat { iterations, .. } = &pasted.nodes()[journal.scope()].kind else {
        panic!()
    };
    assert_eq!(journal.clocks().len(), 2);
    for reference in journal.clocks() {
        let historical = &pasted.audio_bindings().timings()[reference.timing()];
        let FrozenAudioKind::Repeat {
            iterations: old, ..
        } = &historical.nodes()[reference.scope()].kind
        else {
            panic!()
        };
        assert_eq!(old, iterations);
        assert_ne!(reference.scope(), journal.scope());
    }
}

#[test]
fn copied_sound_clocks_survive_interior_insertion_and_range_replacement_timing_slots() {
    let before = shifted(&fixture(), "shifted");
    let slice = capture(&before, "root", &id("group"), "capture");
    for replacement in [false, true] {
        let name = if replacement { "replace" } else { "interior" };
        let Command::SpliceSlice {
            identities, timing, ..
        } = paste(&before, &slice, name, 0).command
        else {
            unreachable!()
        };
        let preflight = if replacement {
            before
                .slice_replacement(&id("root"), range(10, 11), &slice)
                .unwrap()
                .required_ids
        } else {
            before
                .slice_splice_interior(&id("root"), &id("tail"), duration(1), &slice)
                .unwrap()
                .required_ids
        };
        let split_identities = SplitIdentities {
            nodes: (0..preflight)
                .map(|n| id(&format!("{name}-split-{n}")))
                .collect(),
        };
        let command = if replacement {
            Command::ReplaceSlice {
                parent: id("root"),
                range: range(10, 11),
                slice: slice.clone(),
                identities,
                split_identities,
                timing,
            }
        } else {
            Command::SpliceSliceAt {
                parent: id("root"),
                target: id("tail"),
                at: duration(1),
                slice: slice.clone(),
                identities,
                split_identities,
                timing,
            }
        };
        let after = edit(&before, &request(&before, name, command));
        let owner = copied_owner(&after, name);
        let journal = &after.audio_bindings().sound_clocks()[&owner][&sound()];
        assert_eq!(journal.clocks().len(), 2);
        assert_eq!(
            after.beat_sounds()[&owner][&sound()],
            before.beat_sounds()[&id("owner")][&sound()]
        );
        assert!(
            journal
                .clocks()
                .iter()
                .all(|reference| reference.timing().allocation == revision(name))
        );
    }
}

#[test]
fn copied_shared_sound_clock_scope_is_checked_once_for_all_events() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    for index in 0..800 {
        let name = format!("extra-{index}");
        wire["nodes"][&name] = serde_json::to_value(hold(1)).unwrap();
        wire["nodes"]["group"]["kind"]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!(name));
    }
    let event = wire["beat_sounds"]["owner"]["effect"].clone();
    wire["beat_sounds"]["owner"] = serde_json::to_value(
        (0..MAX_DOCUMENT_SOUNDS)
            .map(|index| (format!("effect-{index}"), event.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let before = shifted(&initial, "shifted");
    let slice = capture(&before, "root", &id("group"), "capture");
    slice.validate_capture(&before).unwrap();
    let empty = tree(&[], Vec::new());
    let pasted = edit(&empty, &paste(&empty, &slice, "many", 0));
    let owner = copied_owner(&pasted, "many");
    assert_eq!(pasted.beat_sounds()[&owner].len(), MAX_DOCUMENT_SOUNDS);
    let journals = &pasted.audio_bindings().sound_clocks()[&owner];
    assert_eq!(journals.len(), MAX_DOCUMENT_SOUNDS);
    assert!(journals.values().all(|journal| journal.clocks().len() == 2));
}

#[test]
fn sound_clock_states_skip_the_structural_byte_bound_and_count_exactly() {
    let before = shifted(&shifted(&fixture(), "one"), "two");
    let state = before.audio_bindings();
    assert!(!state.sound_clocks().is_empty());
    let (bound, outcome) = binding_wire_check_for_tests(state);
    // Sound journals have no structural bound, so the state is counted
    // exactly, with the outcome `to_json` gives after validation.
    assert_eq!(bound, None);
    assert_eq!(outcome, state.to_json().map(|_| ()));
}
