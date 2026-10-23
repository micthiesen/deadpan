//! Qualified inputs and public edit/register paths for introduced sound groups.
//! PCM and temporal-processor correctness are owned by the audio tests.
use super::*;
use deadpan_core::{
    AudioTreatments, BeatNode, EqualizerRecipe, GainDb, GroupSelectionIdentities, LinkRelation,
    SourceNode, SourceTimeBase, SourceVideo, SourceVideoMapping, Subtree,
};
use deadpan_media::audio_index::AudioLayoutInterpretation;
use std::collections::BTreeSet;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}

fn span(first: i64, end: i64, rate: u32) -> Result<SourceSpan> {
    let time_base = SourceTimeBase::new(1, rate)?;
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: first,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )?)
}

fn treatment(gains: [i32; 3]) -> AudioTreatments {
    AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(gains.map(|gain| GainDb::new(gain).unwrap()), None, None).unwrap(),
        ))
        .unwrap()
}

fn hold(duration: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            picture_context: None,
            audio,
        },
    )
}

// The existing group_endpoints/fixed_retime_output fixture admission path:
// register the actual video and independent 44.1kHz WAV, never a made-up receipt.
fn setup(parent: &Path, source_owner: bool) -> Result<(PathBuf, ProjectStore)> {
    let (path, mut store) = project(parent)?;
    let original = retain(&mut store, "cfr-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    store.register_source(
        &request(&store, &original, "register-original", "camera", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let wav = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav")
        .canonicalize()?;
    let original = store
        .retain_original(&wav, OriginalOwnership::Managed, limits(), &active())?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits(), &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let audio = AudioSession::open_input(input, 0, AudioSessionLimits::default(), &active())?;
    assert_eq!(audio.index().stream().sample_rate, 44100);
    let decoded = DecodedSourceQualification::for_registration(
        None,
        Some(&audio),
        Some(AudioLayoutInterpretation::Mono),
    )?;
    store.register_source(
        &request(&store, &original, "register-event", "event", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;

    let rate = FrameRate::new(30000, 1001)?;
    let mut owner = hold(
        12,
        HoldAudio::RoomTone {
            source: SourceAudio {
                asset: id("camera"),
                span: span(12000, 22000, 48000)?,
            },
        },
    );
    if source_owner {
        let audio = span(0, 16016, 48000)?;
        owner.kind = NodeKind::Source {
            source: SourceNode {
                duration: frames(12),
                edit_window: None,
                video: SourceVideo::Stream {
                    asset: id("camera"),
                    span: span(0, 12012, 30000)?,
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: id("camera"),
                    span: audio,
                }),
                audio_mapping: SourceAudioMapping::natural_rate(audio, rate)?,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        };
    }
    owner.audio_treatments = treatment([3000, -1500, 1000]);
    owner.audio_edges.node_start = AudioEdgePolicy::Hard;
    owner.audio_edges.node_end = AudioEdgePolicy::Hard;
    for (index, name, recipe) in [
        (0, "lead", hold(2, HoldAudio::Silence)),
        (1, "owner", owner),
        (2, "tail", hold(2, HoldAudio::Silence)),
    ] {
        store.commit(&edit(
            &store.snapshot()?,
            &format!("insert-{name}"),
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), recipe)]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        ))?;
    }
    let source = SourceAudio {
        asset: id("event"),
        span: span(3100, 16000, 44100)?,
    };
    store.commit(&edit(
        &store.snapshot()?,
        "attach-event",
        Command::SetBeatSound {
            owner: node("owner"),
            id: SoundId::new("overlay")?,
            event: BeatSound {
                label: "Finite independent event".into(),
                mapping: SourceAudioMapping::natural_rate(source.span, rate)?,
                source,
                offset: AudioSample(37),
                treatments: treatment([6000, -6000, 3000]),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    ))?;
    // Top exists before the first placement capture. Its ancestor EQ and all
    // sibling/leaf recipes must remain intact when the inner wrapper is born.
    store.commit(&edit(
        &store.snapshot()?,
        "top",
        Command::Group {
            parent: node("root"),
            start: 1,
            end: 3,
            id: node("top"),
            label: "Top".into(),
        },
    ))?;
    store.commit(&edit(
        &store.snapshot()?,
        "top-eq",
        Command::SetAudioTreatments {
            node: node("top"),
            treatments: treatment([4500, -2500, 1500]),
        },
    ))?;
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    store.commit(&edit(
        &store.snapshot()?,
        "prefix",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: node("prefix"),
            identities: SplitIdentities::default(),
            timing: timing("prefix"),
        },
    ))?;
    let current = store.snapshot()?;
    let journal =
        &current.audio_bindings().sound_clocks()[&node("owner")][&SoundId::new("overlay")?];
    assert_eq!(journal.scope(), &node("top"));
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(journal.clocks()[0].scope(), &node("top"));
    assert!(journal.clocks()[0].introduced_groups().is_empty());
    Ok((path, store))
}

fn group(document: &ProjectDocument) -> Result<CommandRequest> {
    let selection = SliceCaptureSelection::Child {
        node: node("owner"),
    };
    assert_eq!(
        document
            .group_selection(&node("top"), &selection)?
            .required_split_ids,
        0
    );
    Ok(edit(
        document,
        "introduced-group",
        Command::GroupSelection {
            parent: node("top"),
            selection,
            label: "New group".into(),
            identities: GroupSelectionIdentities {
                group: node("new-g"),
                split: SplitIdentities::default(),
            },
            timing: timing("introduced-group"),
        },
    ))
}

fn tables(path: &Path) -> Result<BTreeMap<String, Vec<Vec<String>>>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    let names = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut result = BTreeMap::new();
    for name in names {
        let identifier = format!("\"{}\"", name.replace('"', "\"\""));
        let mut statement = database.prepare(&format!("SELECT * FROM {identifier}"))?;
        let width = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..width)
                    .map(|column| Ok(format!("{:?}", row.get_ref(column)?)))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.sort();
        result.insert(name, rows);
    }
    Ok(result)
}

fn assert_group(before: &ProjectDocument, grouped: &ProjectDocument) -> Result {
    assert_eq!(grouped.duration()?, before.duration()?);
    assert_eq!(grouped.beat_sounds(), before.beat_sounds());
    assert_eq!(grouped.sounds(), before.sounds());
    assert_eq!(
        grouped.audio_bindings().timings(),
        before.audio_bindings().timings()
    );
    assert_eq!(
        grouped.audio_bindings().bindings(),
        before.audio_bindings().bindings()
    );
    assert_eq!(
        grouped.audio_bindings().gap_bindings(),
        before.audio_bindings().gap_bindings()
    );
    for name in ["owner", "tail", "lead", "prefix"] {
        assert_eq!(grouped.nodes()[&node(name)], before.nodes()[&node(name)]);
    }
    assert_eq!(
        grouped.nodes()[&node("top")].audio_treatments,
        before.nodes()[&node("top")].audio_treatments
    );
    assert_eq!(grouped.node_duration(&node("new-g"))?, frames(12));
    let sound = SoundId::new("overlay")?;
    let old = &before.audio_bindings().sound_clocks()[&node("owner")][&sound];
    let new = &grouped.audio_bindings().sound_clocks()[&node("owner")][&sound];
    assert_eq!(new.scope(), old.scope());
    assert_eq!(new.clocks().len(), 1);
    assert_eq!(new.clocks()[0].timing(), old.clocks()[0].timing());
    assert_eq!(new.clocks()[0].scope(), old.clocks()[0].scope());
    assert_eq!(new.clocks()[0].owner(), old.clocks()[0].owner());
    assert_eq!(new.clocks()[0].repeats(), old.clocks()[0].repeats());
    assert_eq!(
        new.clocks()[0].introduced_groups(),
        &BTreeSet::from([node("new-g")])
    );
    Ok(())
}

#[test]
fn aligned_introduced_group_is_atomic_and_durable_without_recapturing_sound_tables() -> Result {
    for source_owner in [true, false] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), source_owner)?;
        let before = store.snapshot()?;
        let seed = capture_child(&before, &node("top"), &node("owner"), "seed-capture")?;
        let bank = store.save_register(
            before.project_id(),
            before.revision_id(),
            RegisterName::new('z')?,
            RegisterValue::Edited {
                slice: Arc::new(seed),
            },
        )?;
        let rows = tables(&path)?;
        let command = group(&before)?;
        let preview = store.preview(&command)?;
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert_eq!(tables(&path)?, rows);
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(
            "CREATE TRIGGER fail_introduced_group BEFORE INSERT ON history BEGIN
             SELECT RAISE(FAIL,'forced introduced group history failure'); END;",
        )?;
        assert!(store.commit(&command).is_err());
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert_eq!(tables(&path)?, rows);
        database.execute_batch("DROP TRIGGER fail_introduced_group")?;
        drop(database);
        assert_eq!(store.commit(&command)?.edit, preview);
        let grouped = store.snapshot()?;
        assert_group(&before, &grouped)?;
        assert_eq!(preview.inverse.apply(&grouped)?, before);
        store.validate()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, grouped);
        assert_eq!(store.registers()?, bank);
        store.undo(grouped.revision_id(), revision("undo-introduced-group"))?;
        assert_authored(&store.snapshot()?, &before)?;
        assert_eq!(store.registers()?, bank);
        store.redo(
            &revision("undo-introduced-group"),
            revision("redo-introduced-group"),
        )?;
        assert_authored(&store.snapshot()?, &grouped)?;
        assert_eq!(store.registers()?, bank);
        store.validate()?;
        let final_snapshot = store.snapshot()?;
        drop(store);
        let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        assert_eq!(store.snapshot()?, final_snapshot);
        assert_eq!(store.registers()?, bank);
        store.validate()?;
    }
    Ok(())
}

fn assert_mixed_contexts(document: &ProjectDocument, owner: &NodeId, group: &NodeId) -> Result {
    let sound = SoundId::new("overlay")?;
    let journal = &document.audio_bindings().sound_clocks()[owner][&sound];
    assert_eq!(journal.scope(), group);
    assert_eq!(journal.clocks().len(), 2);
    assert_eq!(journal.clocks()[0].scope(), journal.clocks()[0].owner());
    assert_eq!(
        journal.clocks()[0].introduced_groups(),
        &BTreeSet::from([group.clone()])
    );
    assert!(journal.clocks()[1].introduced_groups().is_empty());
    let plan = deadpan_plan::RenderPlan::compile(document)?;
    let context = plan.audio_sound_instance_context(
        &InstancePath {
            node: group.clone(),
            repeats: vec![],
        },
        Default::default(),
    )?;
    let scopes = plan.beat_sound_clock_scopes(owner, &sound, deadpan_core::MAX_DOCUMENT_NODES)?;
    assert_eq!(scopes.len(), 2);
    let old = scopes[0]
        .try_remap_context_with_work(&context, deadpan_core::MAX_DOCUMENT_NODES)?
        .0;
    let new = scopes[1]
        .try_remap_context_with_work(&context, deadpan_core::MAX_DOCUMENT_NODES)?
        .0;
    assert_eq!(old, None);
    assert_eq!(
        new.as_ref().map(|instance| &instance.node),
        Some(journal.clocks()[1].scope())
    );
    assert!(new.unwrap().repeats.is_empty());
    for (scope, reference) in scopes.iter().zip(journal.clocks()) {
        let historical = scope.remap_instance(&InstancePath {
            node: owner.clone(),
            repeats: vec![],
        })?;
        assert_eq!(&historical.node, reference.owner());
        assert!(historical.repeats.is_empty());
    }
    Ok(())
}

#[test]
fn introduced_group_birth_context_survives_register_paste_history_and_portable_copy() -> Result {
    for source_owner in [true, false] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), source_owner)?;
        let before = store.snapshot()?;
        store.commit(&group(&before)?)?;
        let grouped = store.snapshot()?;
        assert_group(&before, &grouped)?;
        store.commit(&edit(
            &grouped,
            "new-group-eq",
            Command::SetAudioTreatments {
                node: node("new-g"),
                treatments: treatment([-2500, 4500, -1500]),
            },
        ))?;
        let born = store.snapshot()?;
        let slice = capture_child(&born, &node("top"), &node("new-g"), "birth-capture")?;
        let sound = SoundId::new("overlay")?;
        let captured = captured_journal(&slice, &node("owner"), &sound)?;
        assert_eq!(captured.scope(), &node("new-g"));
        assert_eq!(captured.clocks().len(), 2);
        assert_eq!(captured.clocks()[0].timing(), &timing("prefix"));
        assert_eq!(captured.clocks()[0].scope(), &node("owner"));
        assert_eq!(captured.clocks()[0].owner(), &node("owner"));
        assert_eq!(
            captured.clocks()[0].introduced_groups(),
            &BTreeSet::from([node("new-g")])
        );
        assert_eq!(captured.clocks()[1].timing(), &timing("birth-capture"));
        assert_eq!(captured.clocks()[1].scope(), &node("new-g"));
        assert!(captured.clocks()[1].introduced_groups().is_empty());
        let bank = store.save_register(
            born.project_id(),
            born.revision_id(),
            RegisterName::new('g')?,
            RegisterValue::Edited {
                slice: Arc::new(slice),
            },
        )?;
        let saved = register_slice(&bank, 'g').to_json()?;
        let saved_rows = register_rows(&path)?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, born);
        assert_eq!(store.registers()?, bank);
        assert_eq!(register_slice(&store.registers()?, 'g').to_json()?, saved);
        let slice = register_slice(&bank, 'g');
        let command = paste(&born, &slice, "birth-paste")?;
        let rows = tables(&path)?;
        let preview = store.preview(&command)?;
        assert_eq!(tables(&path)?, rows);
        assert_eq!(store.commit(&command)?.edit, preview);
        let pasted = store.snapshot()?;
        let copied_g = node("birth-paste-node-1");
        let copied_owner = node("birth-paste-node-2");
        assert_eq!(pasted.node_duration(&copied_g)?, frames(12));
        assert_eq!(
            pasted.nodes()[&copied_g].audio_treatments,
            born.nodes()[&node("new-g")].audio_treatments
        );
        assert_eq!(
            pasted.beat_sounds()[&copied_owner],
            born.beat_sounds()[&node("owner")]
        );
        assert_eq!(pasted.duration()?, frames(29));
        let journal = &pasted.audio_bindings().sound_clocks()[&copied_owner][&sound];
        // Captured layouts sort birth-capture before prefix, while references
        // retain chronological prefix-then-birth order. Seam reserves slot0.
        assert_eq!(
            journal.clocks()[0].timing(),
            &AudioTimingId {
                allocation: revision("birth-paste"),
                ordinal: 2,
            }
        );
        assert_eq!(
            journal.clocks()[1].timing(),
            &AudioTimingId {
                allocation: revision("birth-paste"),
                ordinal: 1,
            }
        );
        assert_mixed_contexts(&pasted, &copied_owner, &copied_g)?;
        assert_eq!(register_rows(&path)?, saved_rows);
        assert_eq!(register_slice(&store.registers()?, 'g').to_json()?, saved);
        store.validate()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, pasted);
        assert_eq!(store.registers()?, bank);
        assert_mixed_contexts(&store.snapshot()?, &copied_owner, &copied_g)?;
        store.undo(pasted.revision_id(), revision("undo-birth-paste"))?;
        assert_authored(&store.snapshot()?, &born)?;
        assert_eq!(store.registers()?, bank);
        store.redo(&revision("undo-birth-paste"), revision("redo-birth-paste"))?;
        let redone = store.snapshot()?;
        assert_authored(&redone, &pasted)?;
        assert_mixed_contexts(&redone, &copied_owner, &copied_g)?;
        assert_eq!(register_rows(&path)?, saved_rows);
        assert_eq!(store.registers()?, bank);
        store.validate()?;
        drop(store);
        let destination = scratch.path().join("Portable.deadpan");
        let report = deadpan_store::portable::copy_portable(&path, &destination, &active())?;
        assert_eq!(report.revision_id, redone.revision_id().as_str());
        let portable = ProjectStore::open(&destination, AccessMode::ReadOnly)?;
        assert_eq!(portable.snapshot()?, redone);
        assert_eq!(portable.registers()?, bank);
        assert_eq!(
            register_slice(&portable.registers()?, 'g').to_json()?,
            saved
        );
        assert_mixed_contexts(&portable.snapshot()?, &copied_owner, &copied_g)?;
        portable.validate()?;
    }
    Ok(())
}

fn assert_split_forest(
    document: &ProjectDocument,
    owners: &BTreeSet<NodeId>,
    group: &NodeId,
    scope: &NodeId,
    clock_count: usize,
) -> Result {
    let NodeKind::Sequence { children } = &document.nodes()[group].kind else {
        panic!("Split group is not a Sequence");
    };
    assert_eq!(children.len(), 2);
    let pieces = children
        .iter()
        .map(|partition| {
            let NodeKind::Retime {
                child,
                purpose: deadpan_core::RetimePurpose::Partition,
                ..
            } = &document.nodes()[partition].kind
            else {
                panic!("Split child is not a delivery Partition");
            };
            deadpan_core::SoundClockSplitPiece {
                partition: partition.clone(),
                full_owner: child.clone(),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pieces
            .iter()
            .map(|piece| piece.full_owner.clone())
            .collect::<BTreeSet<_>>(),
        *owners
    );
    let sound = SoundId::new("overlay")?;
    for owner in owners {
        let journal = &document.audio_bindings().sound_clocks()[owner][&sound];
        assert_eq!(journal.scope(), scope);
        assert_eq!(journal.clocks().len(), clock_count);
        for (index, reference) in journal.clocks().iter().enumerate() {
            let layout = &document.audio_bindings().timings()[reference.timing()];
            assert!(layout.nodes().contains_key(reference.scope()));
            assert!(layout.nodes().contains_key(reference.owner()));
            assert_eq!(
                reference.introduced_groups(),
                &if index == 0 {
                    BTreeSet::from([group.clone()])
                } else {
                    BTreeSet::new()
                }
            );
            if index < 2 {
                assert_eq!(reference.introduced_splits().len(), 1);
                let forest = &reference.introduced_splits()[0];
                assert_eq!(&forest.historical_node, reference.owner());
                assert_eq!(forest.pieces, pieces);
            } else {
                assert!(reference.introduced_splits().is_empty());
            }
        }
    }
    Ok(())
}

#[test]
fn explicit_split_forests_survive_registered_history_reopen_and_portable_copy() -> Result {
    for source_owner in [true, false] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), source_owner)?;
        store.commit(&group(&store.snapshot()?)?)?;
        let grouped = store.snapshot()?;
        let original_node = grouped.nodes()[&node("owner")].clone();
        let original_events = grouped.beat_sounds()[&node("owner")].clone();
        store.commit(&edit(
            &grouped,
            "forest-split",
            Command::Split {
                node: node("owner"),
                at: frames(6),
                identities: SplitIdentities {
                    nodes: vec![node("left"), node("right"), node("right-full")],
                },
            },
        ))?;
        let split = store.snapshot()?;
        let owners = BTreeSet::from([node("owner"), node("right-full")]);
        assert_eq!(split.duration()?, grouped.duration()?);
        assert_split_forest(&split, &owners, &node("new-g"), &node("top"), 2)?;
        for owner in &owners {
            assert_eq!(split.nodes()[owner], original_node);
            assert_eq!(split.beat_sounds()[owner], original_events);
        }
        let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
            unreachable!()
        };
        store.commit(&edit(
            &split,
            "forest-move",
            Command::InsertTime {
                at: ProjectFrame(0),
                hold: recipe,
                id: node("forest-prefix"),
                identities: SplitIdentities::default(),
                timing: timing("forest-move"),
            },
        ))?;
        let moved = store.snapshot()?;
        assert_split_forest(&moved, &owners, &node("new-g"), &node("top"), 3)?;
        let sound = SoundId::new("overlay")?;
        for owner in &owners {
            let journal = &moved.audio_bindings().sound_clocks()[owner][&sound];
            assert_eq!(
                journal
                    .clocks()
                    .iter()
                    .map(|reference| reference.timing().clone())
                    .collect::<Vec<_>>(),
                vec![
                    timing("prefix"),
                    timing("forest-split"),
                    timing("forest-move")
                ]
            );
            assert_eq!(
                &journal.clocks()[..2],
                split.audio_bindings().sound_clocks()[owner][&sound].clocks()
            );
            assert_eq!(moved.beat_sounds()[owner], original_events);
        }
        let slice = capture_child(&moved, &node("top"), &node("new-g"), "forest-capture")?;
        let bank = store.save_register(
            moved.project_id(),
            moved.revision_id(),
            RegisterName::new('f')?,
            RegisterValue::Edited {
                slice: Arc::new(slice),
            },
        )?;
        let saved = register_slice(&bank, 'f').to_json()?;
        store.commit(&paste(&moved, &register_slice(&bank, 'f'), "forest-paste")?)?;
        let pasted = store.snapshot()?;
        let copied = pasted
            .beat_sounds()
            .keys()
            .filter(|owner| !owners.contains(*owner))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(copied.len(), 2);
        let copied_scope = pasted.audio_bindings().sound_clocks()[copied.first().unwrap()][&sound]
            .scope()
            .clone();
        assert_ne!(copied_scope, node("new-g"));
        assert_split_forest(&pasted, &copied, &copied_scope, &copied_scope, 4)?;
        for owner in &copied {
            assert_eq!(pasted.nodes()[owner], original_node);
            assert_eq!(pasted.beat_sounds()[owner], original_events);
        }
        store.validate()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, pasted);
        assert_eq!(store.registers()?, bank);
        store.undo(pasted.revision_id(), revision("undo-forest-paste"))?;
        assert_authored(&store.snapshot()?, &moved)?;
        store.undo(&revision("undo-forest-paste"), revision("undo-forest-move"))?;
        assert_authored(&store.snapshot()?, &split)?;
        store.undo(&revision("undo-forest-move"), revision("undo-forest-split"))?;
        assert_authored(&store.snapshot()?, &grouped)?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.redo(
            &revision("undo-forest-split"),
            revision("redo-forest-split"),
        )?;
        assert_authored(&store.snapshot()?, &split)?;
        store.redo(&revision("redo-forest-split"), revision("redo-forest-move"))?;
        assert_authored(&store.snapshot()?, &moved)?;
        store.redo(&revision("redo-forest-move"), revision("redo-forest-paste"))?;
        let redone = store.snapshot()?;
        assert_authored(&redone, &pasted)?;
        assert_split_forest(&redone, &owners, &node("new-g"), &node("top"), 3)?;
        assert_split_forest(&redone, &copied, &copied_scope, &copied_scope, 4)?;
        assert_eq!(store.registers()?, bank);
        assert_eq!(register_slice(&bank, 'f').to_json()?, saved);
        store.validate()?;
        drop(store);
        let destination = scratch.path().join("Split-portable.deadpan");
        let report = deadpan_store::portable::copy_portable(&path, &destination, &active())?;
        assert_eq!(report.revision_id, redone.revision_id().as_str());
        let portable = ProjectStore::open(&destination, AccessMode::ReadOnly)?;
        assert_eq!(portable.snapshot()?, redone);
        assert_eq!(portable.registers()?, bank);
        assert_eq!(
            register_slice(&portable.registers()?, 'f').to_json()?,
            saved
        );
        portable.validate()?;
    }
    Ok(())
}
