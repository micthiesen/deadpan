use super::*;

mod grouping;
mod repeats;

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}
fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let transaction = apply(document, &request(document, name, command)).unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    result
}
fn insert(name: &str, at: i64, duration: i64) -> Command {
    Command::InsertTime {
        at: ProjectFrame(at),
        hold: HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
        id: node(name),
        identities: SplitIdentities { nodes: vec![] },
        timing: timing(name),
    }
}
fn journal(document: &ProjectDocument) -> Vec<AudioTimingId> {
    document.audio_bindings.sound_clocks[&node("owner")][&sound("effect")]
        .clocks()
        .iter()
        .map(|reference| reference.timing().clone())
        .collect()
}
fn owner_journal(document: &ProjectDocument, owner: &NodeId) -> Vec<AudioTimingId> {
    document.audio_bindings.sound_clocks[owner][&sound("effect")]
        .clocks()
        .iter()
        .map(|reference| reference.timing().clone())
        .collect()
}

#[test]
fn prefix_insertion_deletion_and_moves_keep_chronology_and_exact_inverse() {
    let before = fixture();
    let inserted = edit(&before, "inserted", insert("inserted", 0, 2));
    assert_eq!(journal(&inserted), vec![timing("inserted")]);
    assert_eq!(
        inserted.audio_bindings.timings[&timing("inserted")],
        FrozenAudioLayout::capture(&before).unwrap()
    );
    assert_eq!(inserted.beat_sounds, before.beat_sounds);
    let deleted = edit(
        &inserted,
        "deleted",
        Command::DeleteRipple {
            node: node("inserted"),
            timing: timing("deleted"),
        },
    );
    assert_eq!(
        journal(&deleted),
        vec![timing("inserted"), timing("deleted")]
    );
    assert_eq!(deleted.nodes, before.nodes);
    let moved = edit(
        &deleted,
        "moved",
        Command::MoveRange {
            source_revision: deleted.revision_id().clone(),
            source_parent: node("root"),
            range: range(0, 1),
            destination: MoveRangeDestination::Seam {
                parent: node("root"),
                index: 2,
            },
            identities: SplitIdentities { nodes: vec![] },
            timing: timing("moved"),
        },
    );
    assert_eq!(
        journal(&moved),
        vec![timing("inserted"), timing("deleted"), timing("moved")]
    );
    assert_eq!(
        moved.source_splice_boundary(&node("root"), 0).unwrap(),
        ProjectFrame(0)
    );
    let restored = edit(
        &moved,
        "moved-back",
        Command::MoveRange {
            source_revision: moved.revision_id().clone(),
            source_parent: node("root"),
            range: range(8, 9),
            destination: MoveRangeDestination::Seam {
                parent: node("root"),
                index: 0,
            },
            identities: SplitIdentities { nodes: vec![] },
            timing: timing("moved-back"),
        },
    );
    assert_eq!(
        journal(&restored),
        vec![
            timing("inserted"),
            timing("deleted"),
            timing("moved"),
            timing("moved-back")
        ]
    );
    assert_eq!(restored.nodes, before.nodes);
    assert_eq!(restored.beat_sounds, before.beat_sounds);
}

#[test]
fn edits_after_owner_need_no_journal_and_whole_deletion_retires_exact_owner() {
    let before = fixture();
    let appended = edit(&before, "appended", insert("appended", 9, 2));
    assert!(appended.audio_bindings.sound_clocks.is_empty());
    let shifted = edit(&before, "shifted", insert("shifted", 0, 2));
    let removed = edit(
        &shifted,
        "removed",
        Command::DeleteRipple {
            node: node("top"),
            timing: timing("removed"),
        },
    );
    assert!(removed.beat_sounds.is_empty());
    assert!(removed.audio_bindings.sound_clocks.is_empty());
    assert!(!removed.nodes.contains_key(&node("owner")));
    removed.validate().unwrap();
}

fn blank(duration: i64) -> SourceNode {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 1000,
            time_base,
        },
    )
    .unwrap();
    SourceNode {
        duration: frames(duration),
        edit_window: None,
        video: SourceVideo::Blank,
        video_mapping: SourceVideoMapping::FitBeat,
        audio: Some(SourceAudio {
            asset: AssetId::new("effect").unwrap(),
            span,
        }),
        audio_mapping: SourceAudioMapping::natural_rate(
            span,
            FrameRate::new(30_000, 1001).unwrap(),
        )
        .unwrap(),
        link: LinkRelation::Independent,
        audio_offset: AudioSample(0),
    }
}

#[test]
fn changed_processing_subtree_and_partial_owner_deletion_refuse_atomically() {
    let before = fixture();
    let snapshot = before.to_json().unwrap();
    let command = Command::SpliceSource {
        parent: node("top"),
        index: 2,
        source: blank(1),
        id: node("extra"),
        label: "Extra".into(),
        timing: timing("changed"),
    };
    let error = apply(&before, &request(&before, "changed", command)).unwrap_err();
    assert!(
        error.message.contains("processing subtree changed"),
        "{error}"
    );
    let selected = range(1, 3);
    let count = before
        .range_deletion(&node("top"), selected)
        .unwrap()
        .required_ids;
    let command = Command::DeleteRange {
        parent: node("top"),
        range: selected,
        identities: SplitIdentities {
            nodes: (0..count).map(|i| node(&format!("split-{i}"))).collect(),
        },
        timing: timing("partial"),
    };
    let error = apply(&before, &request(&before, "partial", command)).unwrap_err();
    assert!(
        error.message.contains("sound owner disappeared") || error.message.contains("sound clock"),
        "{error}"
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
}

#[test]
fn reordering_equal_shaped_siblings_cannot_reassign_an_unclocked_owner() {
    for cached_scope in [false, true] {
        let mut before = fixture();
        if cached_scope {
            // This unchanged owner is visited first and populates the shared
            // scope proof. The later owner still needs its own identity check.
            before.nodes.insert(node("alpha"), hold(4));
            let NodeKind::Sequence { children } =
                &mut before.nodes.get_mut(&node("top")).unwrap().kind
            else {
                unreachable!()
            };
            children.insert(0, node("alpha"));
            before
                .beat_sounds
                .insert(node("alpha"), before.beat_sounds[&node("owner")].clone());
        }
        before.validate().unwrap();
        let snapshot = before.to_json().unwrap();
        let error = apply(
            &before,
            &request(
                &before,
                "reordered",
                Command::MoveRange {
                    source_revision: before.revision_id().clone(),
                    source_parent: node("top"),
                    range: if cached_scope {
                        range(5, 9)
                    } else {
                        range(1, 5)
                    },
                    destination: MoveRangeDestination::Seam {
                        parent: node("top"),
                        index: if cached_scope { 3 } else { 2 },
                    },
                    identities: SplitIdentities { nodes: vec![] },
                    timing: timing("reordered"),
                },
            ),
        )
        .unwrap_err();
        assert!(
            error
                .message
                .contains("sound owner moved outside its captured scope"),
            "{error}"
        );
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
}

#[test]
fn metadata_and_rename_preserve_journals_recipe_replacement_retires_only_one() {
    let mut before = fixture();
    let event = before.beat_sounds[&node("owner")][&sound("effect")].clone();
    before
        .beat_sounds
        .get_mut(&node("owner"))
        .unwrap()
        .insert(sound("second"), event.clone());
    let shifted = edit(&before, "shifted", insert("shifted", 0, 2));
    let mut metadata = event.clone();
    metadata.label = "Renamed sound".into();
    metadata.gain_millidecibels = -6000;
    metadata.end_edge = AudioEdgePolicy::Hard;
    let changed = edit(
        &shifted,
        "metadata",
        Command::SetBeatSound {
            owner: node("owner"),
            id: sound("effect"),
            event: metadata.clone(),
        },
    );
    assert_eq!(changed.audio_bindings, shifted.audio_bindings);
    let renamed = edit(
        &changed,
        "rename-owner",
        Command::Rename {
            node: node("owner"),
            label: "Owner label".into(),
        },
    );
    assert_eq!(renamed.audio_bindings, shifted.audio_bindings);
    assert_eq!(renamed.beat_sounds, changed.beat_sounds);
    metadata.offset.0 += 1;
    let replaced = edit(
        &renamed,
        "recipe",
        Command::SetBeatSound {
            owner: node("owner"),
            id: sound("effect"),
            event: metadata,
        },
    );
    assert!(!replaced.audio_bindings.sound_clocks[&node("owner")].contains_key(&sound("effect")));
    assert_eq!(
        clock_ids(&replaced.audio_bindings.sound_clocks[&node("owner")][&sound("second")]),
        vec![timing("shifted")]
    );
    let deleted = edit(
        &replaced,
        "delete-event",
        Command::DeleteBeatSound {
            owner: node("owner"),
            id: sound("second"),
        },
    );
    assert!(deleted.audio_bindings.sound_clocks.is_empty());
    assert!(deleted.beat_sounds[&node("owner")].contains_key(&sound("effect")));
}

#[test]
fn reused_capture_identity_and_wrong_allocation_refuse() {
    let mut before = fixture();
    let retained = AudioTimingRecord {
        id: timing("collision"),
        layout: FrozenAudioLayout::capture(&before).unwrap(),
    };
    before.audio_bindings = state(vec![retained], clocks(vec![timing("collision")]));
    before.validate().unwrap();
    let error = apply(
        &before,
        &request(&before, "collision", insert("collision", 0, 1)),
    )
    .unwrap_err();
    assert!(error.message.contains("identity already exists"), "{error}");
    let error = apply(
        &before,
        &request(&before, "other", insert("wrong-allocation", 0, 1)),
    )
    .unwrap_err();
    assert!(error.message.contains("allocation must equal"), "{error}");
}

#[test]
fn late_compound_failure_does_not_publish_first_transport() {
    let before = fixture();
    let snapshot = before.to_json().unwrap();
    let transaction = ResolvedTransaction::new(
        0,
        BTreeMap::new(),
        vec![
            ResolvedStep::Edit {
                edit: LeafEdit::new(revision("first"), insert("first", 0, 2)).unwrap(),
            },
            ResolvedStep::Edit {
                edit: LeafEdit::new(
                    revision("second"),
                    Command::SpliceSource {
                        parent: node("top"),
                        index: 2,
                        source: blank(1),
                        id: node("inside"),
                        label: "Inside".into(),
                        timing: timing("second"),
                    },
                )
                .unwrap(),
            },
        ],
    )
    .unwrap();
    let error = apply(
        &before,
        &request(&before, "outer", Command::Compound { transaction }),
    )
    .unwrap_err();
    assert!(
        error.message.contains("processing subtree changed"),
        "{error}"
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
}

fn paste_command(slice: &CapturedEditSlice, name: &str, index: usize) -> Command {
    let required = slice.identity_requirements().unwrap();
    Command::SpliceSlice {
        parent: node("root"),
        index,
        slice: slice.clone(),
        timing: timing(name),
        identities: SlicePasteIdentities {
            authored: OccurrenceIdentities {
                nodes: (0..required.nodes)
                    .map(|i| node(&format!("{name}-node-{i}")))
                    .collect(),
                marks: (0..required.marks)
                    .map(|i| MarkId::new(format!("{name}-mark-{i}")).unwrap())
                    .collect(),
            },
            aliases: (0..required.aliases)
                .map(|i| node(&format!("{name}-alias-{i}")))
                .collect(),
        },
    }
}

#[test]
fn ordinary_whole_owner_paste_merges_imported_events_and_saved_clocks() {
    let before = fixture();
    let slice = CapturedEditSlice::capture_selection(
        &before,
        &node("root"),
        &SliceCaptureSelection::Child { node: node("top") },
        timing("copy"),
    )
    .unwrap();
    let shifted = edit(&before, "shifted", insert("shifted", 0, 2));
    let pasted = edit(&shifted, "pasted", paste_command(&slice, "pasted", 0));
    assert_eq!(pasted.beat_sounds.len(), 2);
    assert_eq!(journal(&pasted), vec![timing("shifted"), timing("pasted")]);
    let copied_owner = pasted
        .beat_sounds
        .keys()
        .find(|id| *id != &node("owner"))
        .unwrap();
    assert_eq!(
        pasted.beat_sounds[copied_owner],
        before.beat_sounds[&node("owner")]
    );
    assert_eq!(
        owner_journal(&pasted, copied_owner),
        vec![AudioTimingId {
            allocation: revision("pasted"),
            ordinal: 1,
        }]
    );
    CapturedEditSlice::capture_selection(
        &pasted,
        &node("root"),
        &SliceCaptureSelection::Child { node: node("top") },
        timing("copy-retained"),
    )
    .unwrap();
    // Retained clocks elsewhere must not prevent copying an unrelated beat.
    CapturedEditSlice::capture_selection(
        &pasted,
        &node("root"),
        &SliceCaptureSelection::Child { node: node("lead") },
        timing("copy-unrelated"),
    )
    .unwrap();
}

#[test]
fn direct_occurrence_clone_cannot_drop_retained_clocks() {
    let before = edit(&fixture(), "shifted", insert("shifted", 0, 2));
    let mut private = before.clone();
    let error = crate::occurrence_edit::clone_nodes(
        &mut private,
        &BTreeMap::from([(node("owner"), node("copy"))]),
        &revision("cloned"),
    )
    .unwrap_err();
    assert!(error.message.contains("complete processing scope"));
    assert_eq!(private, before);
}

#[test]
fn explicit_source_and_range_commands_transport_after_unsounded_endpoint_splits() {
    let mut before = fixture();
    before.nodes.insert(node("lead"), hold(3));
    before.validate().unwrap();
    let parent = node("root");
    let splits = |name: &str, count: usize| SplitIdentities {
        nodes: (0..count)
            .map(|i| node(&format!("{name}-split-{i}")))
            .collect(),
    };
    let interior = before
        .source_splice_interior(&parent, &node("lead"), frames(1))
        .unwrap();
    let replacement = before.source_replacement(&parent, range(1, 2)).unwrap();
    let deletion = before.range_deletion(&parent, range(1, 2)).unwrap();
    for (name, command) in [
        (
            "seam",
            Command::SpliceSource {
                parent: parent.clone(),
                index: 1,
                source: blank(2),
                id: node("seam-source"),
                label: "Audio".into(),
                timing: timing("seam"),
            },
        ),
        (
            "interior",
            Command::SpliceSourceAt {
                parent: parent.clone(),
                target: node("lead"),
                at: frames(1),
                source: blank(2),
                id: node("interior-source"),
                label: "Audio".into(),
                identities: splits("interior", interior.required_ids),
                timing: timing("interior"),
            },
        ),
        (
            "replace",
            Command::ReplaceSource {
                parent: parent.clone(),
                range: range(1, 2),
                source: blank(2),
                id: node("replace-source"),
                label: "Audio".into(),
                identities: splits("replace", replacement.required_ids),
                timing: timing("replace"),
            },
        ),
        (
            "children",
            Command::ReplaceSourceChildren {
                parent: parent.clone(),
                first: node("lead"),
                last: node("lead"),
                source: blank(2),
                id: node("children-source"),
                label: "Audio".into(),
                timing: timing("children"),
            },
        ),
        (
            "delete-range",
            Command::DeleteRange {
                parent: parent.clone(),
                range: range(1, 2),
                identities: splits("delete-range", deletion.required_ids),
                timing: timing("delete-range"),
            },
        ),
        (
            "delete-children",
            Command::DeleteChildren {
                parent: parent.clone(),
                first: node("lead"),
                last: node("lead"),
                timing: timing("delete-children"),
            },
        ),
    ] {
        let after = edit(&before, name, command);
        assert_eq!(journal(&after), vec![timing(name)], "{name}");
        assert_eq!(after.beat_sounds, before.beat_sounds, "{name}");
        assert_eq!(
            after.audio_bindings.timings[&timing(name)],
            FrozenAudioLayout::capture(&before).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn retained_clock_slice_wire_roundtrips_and_rejects_duplicate_history() {
    let before = fixture();
    let slice = CapturedEditSlice::capture_selection(
        &before,
        &node("root"),
        &SliceCaptureSelection::Child { node: node("top") },
        timing("copy"),
    )
    .unwrap();
    let encoded = serde_json::to_string(&slice).unwrap();
    assert_eq!(CapturedEditSlice::from_json(&encoded).unwrap(), slice);
    let mut wire = serde_json::to_value(&slice).unwrap();
    let clocks = wire["audio_bindings"]["sound_clocks"]["owner"]["effect"]["clocks"]
        .as_array_mut()
        .unwrap();
    clocks.push(clocks[0].clone());
    let error = CapturedEditSlice::from_json(&wire.to_string()).unwrap_err();
    assert!(error.message.contains("duplicate sound clock"), "{error}");
}
