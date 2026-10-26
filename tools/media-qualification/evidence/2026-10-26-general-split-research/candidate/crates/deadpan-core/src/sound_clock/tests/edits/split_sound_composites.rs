use super::*;

fn processed() -> AudioTreatments {
    AudioTreatments::default()
        .with_trim(GainDb::new(-1700).unwrap())
        .unwrap()
        .with_equalizer(Some(
            EqualizerRecipe::new(
                [3000, -1500, 6000].map(|gain| GainDb::new(gain).unwrap()),
                None,
                None,
            )
            .unwrap(),
        ))
        .unwrap()
        .with_saturation(Some(Saturation::new(GainDb::new(6000).unwrap()).unwrap()))
        .unwrap()
        .with_tail_sends(
            TailSends::default().with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
        )
}

fn composite(kind: &str, parent: &str) -> ProjectDocument {
    let mut before = fixture();
    let mut owner = BeatNode::sequence("Complete processed context", vec![node("a"), node("b")]);
    before.nodes.insert(node("a"), hold(2));
    if kind == "sequence" {
        before.nodes.insert(node("b"), hold(2));
    } else if kind == "repeat" {
        owner.kind = NodeKind::Repeat {
            child: node("a"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
            escalation: None,
        };
    } else {
        owner.kind = NodeKind::Retime {
            child: node("a"),
            duration: frames(4),
            mapping: range(0, 2),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        };
    }
    owner.audio_treatments = processed();
    before.nodes.insert(node("owner"), owner);
    let event = before.beat_sounds[&node("owner")].clone();
    before.beat_sounds.insert(node("a"), event);
    if parent != "sequence" {
        before.nodes.insert(
            node("top"),
            BeatNode {
                kind: if parent == "repeat" {
                    NodeKind::Repeat {
                        child: node("owner"),
                        iterations: IterationOrder::new(revision("outer"), 2).unwrap(),
                        gap: None,
                        escalation: None,
                    }
                } else {
                    NodeKind::Retime {
                        child: node("owner"),
                        duration: frames(6),
                        mapping: range(0, 4),
                        pitch: PitchPolicy::Preserve,
                        purpose: RetimePurpose::Edit,
                    }
                },
                audio_treatments: processed(),
                ..BeatNode::sequence("Genuine shared processor", vec![])
            },
        );
        before.nodes.remove(&node("sibling"));
    }
    before.validate().unwrap();
    before
}

fn split(before: &ProjectDocument, target: &str, name: &str, at: i64) -> ProjectDocument {
    edit(
        before,
        name,
        Command::Split {
            node: node(target),
            at: frames(at),
            identities: SplitIdentities {
                nodes: (0..20)
                    .map(|index| node(&format!("{name}-{index}")))
                    .collect(),
            },
        },
    )
}

#[test]
fn complete_composite_splits_keep_processors_descendant_events_and_captured_clocks() {
    for kind in ["sequence", "repeat", "retime"] {
        for parent in ["sequence", "repeat", "retime"] {
            for captured in [false, true] {
                let mut before = composite(kind, parent);
                if captured {
                    before = edit(&before, "prefix-one", insert("prefix-one", 0, 1));
                    before = edit(&before, "prefix-two", insert("prefix-two", 0, 1));
                }
                let after = split(&before, "owner", "cut", 2);
                assert_eq!(
                    after.duration().unwrap(),
                    before.duration().unwrap(),
                    "{kind}/{parent}"
                );
                assert_eq!(after.beat_sounds.len(), 4, "{kind}/{parent}");
                assert_eq!(after.nodes[&node("owner")], before.nodes[&node("owner")]);
                assert_eq!(after.nodes[&node("cut-2")].audio_treatments, processed());
                for (owner, events) in &after.beat_sounds {
                    assert_eq!(events, &before.beat_sounds[&node("owner")]);
                    let journal = &after.audio_bindings.sound_clocks[owner][&sound("effect")];
                    assert_eq!(journal.clocks().len(), if captured { 3 } else { 1 });
                }
                let refined = split(&after, "cut-0", "refine", 1);
                assert_eq!(refined.duration().unwrap(), before.duration().unwrap());
                assert_eq!(refined.beat_sounds.len(), 6);
            }
        }
    }
}

#[test]
fn root_owned_sound_stays_at_root_when_a_processed_child_is_split() {
    for parent in ["sequence", "repeat", "retime"] {
        let mut before = composite("sequence", parent);
        let event = before.beat_sounds[&node("owner")].clone();
        before.beat_sounds.insert(node("root"), event.clone());
        before
            .nodes
            .get_mut(&node("root"))
            .unwrap()
            .audio_treatments = processed();
        before.validate().unwrap();
        let after = split(&before, "owner", "cut", 2);
        assert_eq!(after.beat_sounds[&node("root")], event);
        assert!(
            !after
                .audio_bindings
                .sound_clocks
                .contains_key(&node("root"))
        );
        let bytes = before.to_json().unwrap();
        let error = apply(
            &before,
            &request(
                &before,
                "root-cut",
                Command::Split {
                    node: node("root"),
                    at: frames(1),
                    identities: SplitIdentities {
                        nodes: (0..40)
                            .map(|index| node(&format!("root-cut-{index}")))
                            .collect(),
                    },
                },
            ),
        )
        .unwrap_err();
        assert!(error.message.contains("root-owned"), "{error}");
        assert_eq!(before.to_json().unwrap(), bytes);
    }
}

#[test]
fn copying_a_whole_previously_split_scope_retains_its_renamed_complete_forest() {
    let before = composite("sequence", "sequence");
    let cut = split(&before, "a", "inner", 1);
    let after = split(&cut, "top", "outer", 4);
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.beat_sounds.len(), cut.beat_sounds.len() * 2);
}

#[test]
fn nested_forest_copies_remain_complete_inside_a_genuine_wider_processor() {
    for parent in ["sequence", "repeat", "retime"] {
        let before = composite("sequence", parent);
        let inner = split(&before, "a", "inner", 1);
        let outer = split(&inner, "owner", "outer", 2);
        assert_eq!(outer.duration().unwrap(), before.duration().unwrap());
        let oldest = &outer.audio_bindings.sound_clocks[&node("a")][&sound("effect")].clocks()[0];
        assert_eq!(oldest.introduced_splits().len(), 3, "{parent}");
        assert_eq!(
            oldest
                .introduced_splits()
                .iter()
                .filter(|split| split.historical_node == node("a"))
                .count(),
            2
        );
        let refined = split(&outer, "outer-0", "refine", 1);
        assert_eq!(refined.duration().unwrap(), before.duration().unwrap());

        let mut omitted = outer.clone();
        omitted
            .audio_bindings
            .sound_clocks
            .get_mut(&node("a"))
            .unwrap()
            .get_mut(&sound("effect"))
            .unwrap()
            .clocks[0]
            .introduced_splits
            .remove(1);
        assert!(
            omitted.validate().is_err(),
            "omitted copied forest: {parent}"
        );

        let mut crossed = outer.clone();
        let relations = &mut crossed
            .audio_bindings
            .sound_clocks
            .get_mut(&node("a"))
            .unwrap()
            .get_mut(&sound("effect"))
            .unwrap()
            .clocks[0]
            .introduced_splits;
        let other = relations[1].pieces[1].clone();
        relations[1].pieces[1] = relations[0].pieces[1].clone();
        relations[0].pieces[1] = other;
        assert!(
            crossed.validate().is_err(),
            "crossed complete contexts: {parent}"
        );
    }
}

#[test]
fn composite_split_rejects_reordered_overlapping_and_edited_complete_contexts() {
    let before = composite("sequence", "sequence");
    let after = split(&before, "owner", "cut", 2);
    let mut reordered = after.clone();
    let NodeKind::Sequence { children } = &mut reordered.nodes.get_mut(&node("top")).unwrap().kind
    else {
        unreachable!()
    };
    *children = vec![node("cut-0"), node("sibling"), node("cut-1")];
    assert!(reordered.validate().is_err());

    let mut overlap = after.clone();
    let NodeKind::Retime { mapping, .. } = &mut overlap.nodes.get_mut(&node("cut-1")).unwrap().kind
    else {
        unreachable!()
    };
    *mapping = range(0, 2);
    assert!(overlap.validate().is_err());

    let mut edited = after.clone();
    // A complete copied leaf must retain its exact policy, not just duration.
    let NodeKind::Hold { recipe } = &mut edited.nodes.get_mut(&node("cut-3")).unwrap().kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::Tone {
        frequency_hz: 440,
        level: GainDb::new(-18000).unwrap(),
    };
    assert!(edited.validate().is_err());

    let mut incomplete = serde_json::to_value(&after).unwrap();
    let clocks = incomplete["audio_bindings"]["sound_clocks"]
        .as_object_mut()
        .unwrap();
    for events in clocks.values_mut() {
        let references = events["effect"]["clocks"].as_array_mut().unwrap();
        for reference in references {
            reference["introduced_splits"][0]["pieces"]
                .as_array_mut()
                .unwrap()
                .pop();
        }
    }
    assert!(ProjectDocument::from_json(&incomplete.to_string()).is_err());
}

#[test]
fn a_recorded_split_cannot_authorize_an_unrelated_group_below_a_processor() {
    for parent in ["repeat", "retime"] {
        let before = composite("sequence", parent);
        let mut forged = split(&before, "a", "cut", 1);
        let (NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. }) =
            &mut forged.nodes.get_mut(&node("top")).unwrap().kind
        else {
            unreachable!()
        };
        *child = node("forged-group");
        forged.nodes.insert(
            node("forged-group"),
            BeatNode::sequence("Unpaired group", vec![node("owner")]),
        );
        for events in forged.audio_bindings.sound_clocks.values_mut() {
            for journal in events.values_mut() {
                for reference in &mut journal.clocks {
                    reference.introduced_groups.insert(node("forged-group"));
                }
            }
        }
        let wire = forged.to_json().unwrap();
        let error = forged.validate().unwrap_err();
        assert!(
            error
                .message
                .contains("sound clock processing subtree changed"),
            "{parent}: {error}"
        );
        assert!(ProjectDocument::from_json(&wire).is_err());
    }
}
