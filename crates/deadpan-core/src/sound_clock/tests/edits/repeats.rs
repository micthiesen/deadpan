use super::*;

fn wrap(document: &ProjectDocument, name: &str) -> ProjectDocument {
    edit(
        document,
        name,
        Command::RepeatSelection {
            parent: node("top"),
            selection: SliceCaptureSelection::Child {
                node: node("owner"),
            },
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: node("repeat"),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing(name),
        },
    )
}

#[test]
fn partially_repeating_a_sound_owner_keeps_fragments_and_exact_clock_births() {
    let mut before = edit(&fixture(), "prefix", insert("prefix", 0, 1));
    // With no unbound physical recipe, Repeat's structural capture omits its
    // base timing. Its suffix capture must not claim the sound journal's ID.
    before.audio_bindings = capture_unbound_audio_bindings(&before, timing("bind-all")).unwrap();
    let selection = SliceCaptureSelection::Range { range: range(3, 5) };
    let plan = before
        .repeat_selection(&node("top"), &selection, 2)
        .unwrap();
    let after = edit(
        &before,
        "partial-repeat",
        Command::RepeatSelection {
            parent: node("top"),
            selection,
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: node("repeat"),
                group: Some(node("group")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|i| node(&format!("part-{i}")))
                        .collect(),
                },
            },
            timing: timing("partial-repeat"),
        },
    );
    assert_eq!(after.beat_sounds.len(), 3);
    assert_eq!(after.duration().unwrap().frames(), 12);
    assert_eq!(
        after.audio_bindings.timings[&timing("partial-repeat")],
        FrozenAudioLayout::capture(&before).unwrap()
    );
    assert!(
        after
            .beat_sounds
            .values()
            .all(|events| events == &before.beat_sounds[&node("owner")])
    );
    let middle = crate::occurrence_edit::subtree_order(&after, &node("repeat"))
        .unwrap()
        .into_iter()
        .find(|node| after.beat_sounds.contains_key(node))
        .unwrap();
    assert_eq!(
        owner_journal(&after, &middle),
        vec![timing("prefix"), timing("partial-repeat")]
    );
    for index in 0..2 {
        assert_eq!(
            applicability(&after, &occurrence(&after, middle.as_str(), index)),
            vec![true, true]
        );
    }
    let grown = plays(&after, "grown-partial", 3);
    assert_eq!(
        applicability(&grown, &occurrence(&grown, middle.as_str(), 2)),
        vec![false; 3]
    );
    let shrunk = plays(&grown, "shrunk-partial", 2);
    let regrown = plays(&shrunk, "regrown-partial", 3);
    assert_ne!(
        occurrence(&grown, middle.as_str(), 2),
        occurrence(&regrown, middle.as_str(), 2)
    );
    assert!(
        applicability(&regrown, &occurrence(&regrown, middle.as_str(), 2))
            .into_iter()
            .all(|present| !present)
    );
}

fn plays(document: &ProjectDocument, name: &str, count: u32) -> ProjectDocument {
    edit(
        document,
        name,
        Command::SetRepeatPlays {
            node: node("repeat"),
            plays: count,
            timing: timing(name),
        },
    )
}

#[test]
fn split_owners_with_preexisting_external_repeat_ancestry_keep_sound_recipes() {
    let before = wrap(&fixture(), "outer");
    let after = repeat_range(&before, 2, 4);
    assert_eq!(
        after.duration().unwrap().frames(),
        before.duration().unwrap().frames() + 2
    );
    assert!(after.beat_sounds.len() > 1);
    assert!(
        after
            .beat_sounds
            .values()
            .all(|events| events == &before.beat_sounds[&node("owner")])
    );
}

fn repeat_range(before: &ProjectDocument, start: i64, end: i64) -> ProjectDocument {
    let selection = SliceCaptureSelection::Range {
        range: range(start, end),
    };
    let plan = before
        .repeat_selection(&node("top"), &selection, 2)
        .unwrap();
    edit(
        before,
        "nested",
        Command::RepeatSelection {
            parent: node("top"),
            selection,
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: node("nested-repeat"),
                group: Some(node("nested-group")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|i| node(&format!("nested-split-{i}")))
                        .collect(),
                },
            },
            timing: timing("nested"),
        },
    )
}

#[test]
fn split_repeat_aliases_preserve_historical_cohorts_and_later_births() {
    let shifted = edit(&fixture(), "prefix", insert("prefix", 0, 1));
    let wrapped = wrap(&shifted, "wrapped");
    let before = plays(&wrapped, "grown", 3);
    let after = repeat_range(&before, 11, 13);
    let owner = crate::occurrence_edit::subtree_order(&after, &node("nested-repeat"))
        .unwrap()
        .into_iter()
        .find(|id| after.beat_sounds.contains_key(id))
        .unwrap();
    let clock = &after.audio_bindings.sound_clocks[&owner][&sound("effect")];
    assert_eq!(clock.clocks().len(), 4);
    let ancestors = FrozenAudioLayout::capture(&after)
        .unwrap()
        .sound_placement_repeats(clock.scope(), MAX_DOCUMENT_NODES)
        .unwrap()
        .0;
    assert_eq!(ancestors.len(), 2);
    assert_eq!(ancestors[0], node("nested-repeat"));
    assert_ne!(ancestors[1], node("repeat"));
    let instance = |document: &ProjectDocument, outer: u32, inner: u32| InstancePath {
        node: owner.clone(),
        repeats: ancestors
            .iter()
            .zip([outer, inner])
            .map(|(id, index)| {
                let NodeKind::Repeat { iterations, .. } = &document.nodes[id].kind else {
                    panic!()
                };
                RepeatInstance {
                    node: id.clone(),
                    iteration: iterations.at(index).unwrap(),
                }
            })
            .collect(),
    };
    for outer in 0..2 {
        assert_eq!(
            applicability(&after, &instance(&after, outer, 0)),
            vec![true; 4]
        );
        assert_eq!(
            applicability(&after, &instance(&after, outer, 2)),
            vec![false, false, false, true]
        );
    }
    let resize = |document: &ProjectDocument, name: &str, plays| {
        edit(
            document,
            name,
            Command::SetRepeatPlays {
                node: node("nested-repeat"),
                plays,
                timing: timing(name),
            },
        )
    };
    let grown = resize(&after, "grow-nested", 3);
    assert_eq!(
        applicability(&grown, &instance(&grown, 2, 2)),
        vec![false; 5]
    );
    let shrunk = resize(&grown, "shrink-nested", 2);
    let regrown = resize(&shrunk, "regrow-nested", 3);
    assert_ne!(instance(&grown, 2, 2), instance(&regrown, 2, 2));
    assert!(
        applicability(&regrown, &instance(&regrown, 2, 2))
            .into_iter()
            .all(|present| !present)
    );
}

fn occurrence(document: &ProjectDocument, owner: &str, index: u32) -> InstancePath {
    let NodeKind::Repeat { iterations, .. } = &document.nodes[&node("repeat")].kind else {
        panic!()
    };
    InstancePath {
        node: node(owner),
        repeats: vec![RepeatInstance {
            node: node("repeat"),
            iteration: iterations.at(index).unwrap(),
        }],
    }
}

fn applicability(document: &ProjectDocument, instance: &InstancePath) -> Vec<bool> {
    let current = FrozenAudioLayout::capture(document).unwrap();
    let journal = &document.audio_bindings.sound_clocks[&instance.node][&sound("effect")];
    journal
        .clocks()
        .iter()
        .map(|reference| {
            let proof = document.audio_bindings.timings[reference.timing()]
                .sound_clock_correspondence_with_repeats(
                    &current,
                    reference.scope(),
                    journal.scope(),
                    reference.repeats(),
                    MAX_DOCUMENT_NODES,
                )
                .unwrap();
            proof
                .try_remap_instance_with_work(instance, MAX_DOCUMENT_NODES)
                .unwrap()
                .0
                .is_some()
        })
        .collect()
}

#[test]
fn repeat_wrap_count_gap_and_regrowth_keep_exact_stable_clock_births() {
    let before = fixture();
    let shifted = edit(&before, "shifted", insert("shifted", 0, 2));
    let wrapped = wrap(&shifted, "wrapped");
    assert_eq!(wrapped.beat_sounds, before.beat_sounds);
    assert_eq!(
        journal(&wrapped),
        vec![timing("shifted"), timing("wrapped")]
    );
    for index in 0..2 {
        assert_eq!(
            applicability(&wrapped, &occurrence(&wrapped, "owner", index)),
            vec![true, true]
        );
    }
    let grown = plays(&wrapped, "grown", 3);
    let third = occurrence(&grown, "owner", 2);
    assert_eq!(applicability(&grown, &third), vec![false, false, false]);
    let gapped = edit(
        &grown,
        "gapped",
        Command::SetRepeatGaps {
            node: node("repeat"),
            gap: Some(HoldRecipe {
                duration: frames(1),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            }),
            branches: vec![],
            timing: timing("gapped"),
        },
    );
    assert_eq!(
        applicability(&gapped, &third),
        vec![false, false, false, true]
    );
    assert_eq!(
        applicability(&gapped, &occurrence(&gapped, "owner", 0)),
        vec![true; 4]
    );
    let shrunk = plays(&gapped, "shrunk", 2);
    let regrown = plays(&shrunk, "regrown", 3);
    let fresh = occurrence(&regrown, "owner", 2);
    assert_ne!(fresh.repeats, third.repeats);
    assert!(
        applicability(&regrown, &fresh)
            .into_iter()
            .all(|present| !present)
    );
    // Sample offset and all authored sound parameters are independent of maps.
    assert_eq!(regrown.beat_sounds, before.beat_sounds);
}

#[test]
fn first_play_isolation_moves_retained_sounds_without_losing_their_history() {
    let wrapped = wrap(&fixture(), "wrapped");
    let count = wrapped
        .first_play_attachment_nodes(&node("repeat"))
        .unwrap();
    assert_eq!(count, 1);
    let isolated = edit(
        &wrapped,
        "isolated",
        Command::KeepFirstPlayAttachments {
            node: node("repeat"),
            identities: OccurrenceIdentities {
                nodes: vec![node("first-owner")],
                marks: vec![],
            },
        },
    );
    assert!(!isolated.beat_sounds.contains_key(&node("owner")));
    assert_eq!(
        isolated.beat_sounds[&node("first-owner")],
        wrapped.beat_sounds[&node("owner")]
    );
    assert!(
        !isolated
            .audio_bindings
            .sound_clocks
            .contains_key(&node("owner"))
    );
    assert_eq!(
        applicability(&isolated, &occurrence(&isolated, "first-owner", 0)),
        vec![true]
    );
    let mut invalid = occurrence(&isolated, "first-owner", 1);
    let current = FrozenAudioLayout::capture(&isolated).unwrap();
    let journal = &isolated.audio_bindings.sound_clocks[&node("first-owner")][&sound("effect")];
    let reference = &journal.clocks()[0];
    let proof = isolated.audio_bindings.timings[reference.timing()]
        .sound_clock_correspondence_with_repeats(
            &current,
            reference.scope(),
            journal.scope(),
            reference.repeats(),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert!(
        proof
            .try_remap_instance_with_work(&invalid, MAX_DOCUMENT_NODES)
            .is_err()
    );
    invalid.repeats.clear();
    assert!(
        proof
            .try_remap_instance_with_work(&invalid, MAX_DOCUMENT_NODES)
            .is_err()
    );
}

#[test]
fn an_edit_after_a_repeated_sound_does_not_consume_another_clock() {
    let wrapped = wrap(&fixture(), "wrapped");
    let end = wrapped.duration().unwrap().frames();
    let appended = edit(&wrapped, "appended", insert("appended", end, 1));
    assert_eq!(journal(&appended), journal(&wrapped));
}

#[test]
fn a_sound_on_a_dormant_final_gap_is_born_only_when_that_gap_activates() {
    let mut before = wrap(&fixture(), "wrapped");
    let last = occurrence(&before, "owner", 1).repeats[0].iteration.clone();
    before.nodes.insert(node("dormant"), hold(4));
    before
        .gap_overrides
        .entry(node("repeat"))
        .or_default()
        .insert(last, node("dormant"));
    let events = before.beat_sounds.remove(&node("owner")).unwrap();
    before.audio_bindings.sound_clocks.clear();
    before.beat_sounds.insert(node("dormant"), events);
    before.validate().unwrap();
    let grown = plays(&before, "grown", 3);
    assert_eq!(
        applicability(&grown, &occurrence(&grown, "dormant", 1)),
        vec![false]
    );
    let moved = edit(&grown, "moved", insert("moved", 0, 1));
    assert_eq!(
        applicability(&moved, &occurrence(&moved, "dormant", 1)),
        vec![false, true]
    );
}

#[test]
fn repeat_owned_sound_refuses_a_changed_processing_extent_atomically() {
    let mut before = wrap(&fixture(), "wrapped");
    let events = before.beat_sounds.remove(&node("owner")).unwrap();
    before.audio_bindings.sound_clocks.clear();
    before.beat_sounds.insert(node("repeat"), events);
    before.validate().unwrap();
    let snapshot = before.to_json().unwrap();
    let error = apply(
        &before,
        &request(
            &before,
            "grown",
            Command::SetRepeatPlays {
                node: node("repeat"),
                plays: 3,
                timing: timing("grown"),
            },
        ),
    )
    .unwrap_err();
    assert!(
        error.message.contains("processing subtree changed"),
        "{error}"
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
}
