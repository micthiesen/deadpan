//! Real qualified inputs, synthetic authored graph. PCM belongs to audio tests.
use super::*;
use deadpan_core::{
    AudioTreatments, BeatNode, EqualizerRecipe, FrameRange, GainDb, GroupSelectionIdentities,
    LinkRelation, SourceNode, SourceTimeBase, SourceVideo, SourceVideoMapping, Subtree,
};
use deadpan_media::audio_index::AudioLayoutInterpretation;

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

fn span(start: i64, end: i64, denominator: u32) -> Result<SourceSpan> {
    let time_base = SourceTimeBase::new(1, denominator)?;
    Ok(SourceSpan::new(
        SourceTimestamp {
            ticks: start,
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

fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Silent background",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

// Use the same actual audio fixture and admission path as fixed_retime_output.rs,
// rather than manufacturing an AssetRecord or a source qualification receipt.
fn register_independent_audio(store: &mut ProjectStore) -> Result {
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
        &request(store, &original, "register-event", "event", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    Ok(())
}

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
    register_independent_audio(&mut store)?;
    let rate = FrameRate::new(30000, 1001)?;
    let mut owner = hold(12);
    owner.label = "Complete treated owner".into();
    owner.audio_treatments = treatment([3000, -1500, 1000]);
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
    owner.audio_edges.node_start = AudioEdgePolicy::Hard;
    owner.audio_edges.node_end = AudioEdgePolicy::Hard;
    for (index, name, recipe) in [
        (0, "lead", hold(2)),
        (1, "owner", owner),
        (2, "tail", hold(2)),
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
    // Finite independent 44.1 kHz event, offset in canonical output samples,
    // with treatments distinct from the full twelve-frame owner's treatments.
    let source = SourceAudio {
        asset: id("event"),
        span: span(3100, 16000, 44100)?,
    };
    let event = BeatSound {
        label: "Finite independent event".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, rate)?,
        source,
        offset: AudioSample(37),
        treatments: treatment([6000, -6000, 3000]),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    assert!(
        event
            .mapping
            .duration_frames(frames(12))?
            .compare_integer(12)
            .is_lt()
    );
    store.commit(&edit(
        &store.snapshot()?,
        "attach-event",
        Command::SetBeatSound {
            owner: node("owner"),
            id: SoundId::new("overlay")?,
            event,
        },
    ))?;
    let NodeKind::Hold { recipe } = hold(1).kind else {
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
    Ok((path, store))
}

fn group_request(
    document: &ProjectDocument,
    selection: SliceCaptureSelection,
    name: &str,
) -> Result<CommandRequest> {
    let plan = document.group_selection(document.root(), &selection)?;
    Ok(edit(
        document,
        name,
        Command::GroupSelection {
            parent: document.root().clone(),
            selection,
            label: "Neutral endpoint group".into(),
            identities: GroupSelectionIdentities {
                group: node(&format!("{name}-group")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|i| NodeId::new(format!("{name}-split-{i}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            },
            timing: timing(name),
        },
    ))
}

// Counts miss rewritten rows and receipts. Retain every typed value, including
// BLOB bytes, from every durable table. Sort only the row presentation.
// This excludes schema objects, so creating/dropping the deliberate fault
// trigger does not itself change the baseline. All table contents are covered.
fn durable_tables(path: &Path) -> Result<BTreeMap<String, Vec<Vec<String>>>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    let tables = database
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut saved = BTreeMap::new();
    for name in tables {
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
        saved.insert(name, rows);
    }
    Ok(saved)
}

fn slice_sounds(
    slice: &CapturedEditSlice,
) -> Result<BTreeMap<NodeId, BTreeMap<SoundId, BeatSound>>> {
    let wire: serde_json::Value = serde_json::from_str(&slice.to_json()?)?;
    Ok(serde_json::from_value(wire["beat_sounds"].clone())?)
}

fn assert_complete_recipes(
    document: &ProjectDocument,
    event: &BeatSound,
    aliases: usize,
) -> Result {
    let sound = SoundId::new("overlay")?;
    assert_eq!(document.beat_sounds().len(), aliases);
    for (owner, events) in document.beat_sounds() {
        assert_eq!(events, &BTreeMap::from([(sound.clone(), event.clone())]));
        assert_eq!(document.node_duration(owner)?, frames(12));
        assert_eq!(
            document.nodes()[owner].audio_treatments,
            treatment([3000, -1500, 1000])
        );
        assert_eq!(events[&sound].offset, AudioSample(37));
        assert_eq!(events[&sound].source.asset, id("event"));
    }
    Ok(())
}

#[test]
fn endpoint_groups_commit_atomically_keep_prior_sound_journals_and_reopen_undo_redo() -> Result {
    for source_owner in [false, true] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), source_owner)?;
        let before = store.snapshot()?;
        let sound = SoundId::new("overlay")?;
        let recipe = before.beat_sounds()[&node("owner")][&sound].clone();
        let prior = before.audio_bindings().sound_clocks()[&node("owner")][&sound].clone();
        assert_eq!(prior.scope(), &node("owner"));
        assert_eq!(prior.clocks().len(), 1);
        assert_eq!(prior.clocks()[0].timing(), &timing("prefix"));
        assert_eq!(prior.clocks()[0].scope(), &node("owner"));
        assert_eq!(prior.clocks()[0].owner(), &node("owner"));
        assert_ne!(prior.scope(), before.root());
        assert_eq!(before.duration()?, frames(17));

        // The aligned neutral path must remain byte-identical in all sound
        // bindings and recipes, even when an existing journal is present.
        let aligned = group_request(
            &before,
            SliceCaptureSelection::Child {
                node: node("owner"),
            },
            "aligned",
        )?;
        let aligned_preview = store.preview(&aligned)?;
        let aligned_after = aligned_preview.forward.apply(&before)?;
        assert_eq!(aligned_after.audio_bindings(), before.audio_bindings());
        assert_eq!(aligned_after.beat_sounds(), before.beat_sounds());
        assert_eq!(aligned_preview.inverse.apply(&aligned_after)?, before);

        // A nonempty immutable register bank makes bank/receipt rollback
        // meaningful. Saving it does not alter the authored revision.
        let seed = capture_child(&before, before.root(), &node("owner"), "seed-capture")?;
        let bank = store.save_register(
            before.project_id(),
            before.revision_id(),
            RegisterName::new('z')?,
            RegisterValue::Edited {
                slice: Arc::new(seed),
            },
        )?;
        let selection = SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(5), ProjectFrame(11))?,
        };
        assert_eq!(
            before
                .group_selection(before.root(), &selection)?
                .required_split_ids,
            5
        );
        let command = group_request(&before, selection, "endpoints")?;
        let expected_layout = deadpan_core::FrozenAudioLayout::capture(&before)?;
        let rows = durable_tables(&path)?;
        let preview = store.preview(&command)?;
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert_eq!(durable_tables(&path)?, rows);
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(
            "CREATE TRIGGER fail_endpoint_group BEFORE INSERT ON history BEGIN
             SELECT RAISE(FAIL,'forced endpoint group history failure'); END;",
        )?;
        assert!(store.commit(&command).is_err());
        assert_eq!(store.snapshot()?, before);
        assert_eq!(store.registers()?, bank);
        assert_eq!(durable_tables(&path)?, rows);
        database.execute_batch("DROP TRIGGER fail_endpoint_group")?;
        drop(database);
        assert_eq!(store.commit(&command)?.edit, preview);
        let grouped = store.snapshot()?;
        assert_eq!(grouped.duration()?, frames(17));
        assert_eq!(grouped.node_duration(&node("endpoints-group"))?, frames(6));
        assert_complete_recipes(&grouped, &recipe, 3)?;
        for (owner, events) in grouped.audio_bindings().sound_clocks() {
            let journal = &events[&sound];
            assert_eq!(journal.scope(), owner);
            assert_eq!(journal.clocks().len(), 2);
            assert_eq!(journal.clocks()[0], prior.clocks()[0]);
            assert_eq!(journal.clocks()[1].timing(), &timing("endpoints"));
            assert_eq!(journal.clocks()[1].scope(), &node("owner"));
            assert_eq!(journal.clocks()[1].owner(), &node("owner"));
        }
        // Each alias uses the same complete pre-edit table. No per-endpoint
        // recapture may substitute a second table or discard the prior table.
        assert!(
            grouped
                .audio_bindings()
                .timings()
                .contains_key(&timing("prefix"))
        );
        assert!(
            grouped
                .audio_bindings()
                .timings()
                .contains_key(&timing("endpoints"))
        );
        assert_eq!(
            grouped.audio_bindings().timings()[&timing("prefix")],
            before.audio_bindings().timings()[&timing("prefix")],
        );
        assert_eq!(
            grouped.audio_bindings().timings()[&timing("endpoints")],
            expected_layout,
        );
        assert_eq!(
            preview.inverse.apply(&grouped)?.to_json()?,
            before.to_json()?
        );
        assert_eq!(store.registers()?, bank);
        store.validate()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, grouped);
        assert_eq!(store.registers()?, bank);
        store.undo(grouped.revision_id(), revision("undo-endpoints"))?;
        assert_authored(&store.snapshot()?, &before)?;
        assert_eq!(store.registers()?, bank);
        store.redo(&revision("undo-endpoints"), revision("redo-endpoints"))?;
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

#[test]
fn endpoint_group_capture_seam_paste_recopy_and_reopen_keep_complete_sound_recipes() -> Result {
    for source_owner in [false, true] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup(scratch.path(), source_owner)?;
        let before = store.snapshot()?;
        let sound = SoundId::new("overlay")?;
        let recipe = before.beat_sounds()[&node("owner")][&sound].clone();
        let command = group_request(
            &before,
            SliceCaptureSelection::Range {
                range: FrameRange::new(ProjectFrame(5), ProjectFrame(11))?,
            },
            "endpoints",
        )?;
        store.commit(&command)?;
        let grouped = store.snapshot()?;
        let slice = capture_child(
            &grouped,
            grouped.root(),
            &node("endpoints-group"),
            "group-capture",
        )?;
        assert_eq!(slice.duration(), frames(6));
        let captured_sounds = slice_sounds(&slice)?;
        assert_eq!(captured_sounds.len(), 1);
        let captured_owner = captured_sounds
            .keys()
            .next()
            .ok_or("captured owner")?
            .clone();
        assert_eq!(captured_sounds[&captured_owner][&sound], recipe);
        let captured_clock = captured_journal(&slice, &captured_owner, &sound)?;
        assert_eq!(captured_clock.clocks().len(), 3);
        assert_eq!(captured_clock.clocks()[0].timing(), &timing("prefix"));
        assert_eq!(captured_clock.clocks()[1].timing(), &timing("endpoints"));
        assert_eq!(
            captured_clock.clocks()[2].timing(),
            &timing("group-capture")
        );
        let bank = store.save_register(
            grouped.project_id(),
            grouped.revision_id(),
            RegisterName::new('a')?,
            RegisterValue::Edited {
                slice: Arc::new(slice),
            },
        )?;
        let saved_a = register_slice(&bank, 'a').to_json()?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, grouped);
        assert_eq!(store.registers()?, bank);

        // paste() uses SpliceSlice at the ordinary root's terminal child seam.
        let slice = register_slice(&bank, 'a');
        let command = paste(&grouped, &slice, "group-paste")?;
        let rows = durable_tables(&path)?;
        let preview = store.preview(&command)?;
        assert_eq!(durable_tables(&path)?, rows);
        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch(
            "CREATE TRIGGER fail_endpoint_paste BEFORE INSERT ON history BEGIN
             SELECT RAISE(FAIL,'forced endpoint paste history failure'); END;",
        )?;
        assert!(store.commit(&command).is_err());
        assert_eq!(store.snapshot()?, grouped);
        assert_eq!(store.registers()?, bank);
        assert_eq!(durable_tables(&path)?, rows);
        database.execute_batch("DROP TRIGGER fail_endpoint_paste")?;
        drop(database);
        assert_eq!(store.commit(&command)?.edit, preview);
        let pasted = store.snapshot()?;
        assert_eq!(pasted.duration()?, frames(23));
        assert_complete_recipes(&pasted, &recipe, 4)?;
        let copied_owner = pasted
            .beat_sounds()
            .keys()
            .find(|owner| !grouped.beat_sounds().contains_key(*owner))
            .ok_or("new pasted owner")?
            .clone();
        let pasted_clock = &pasted.audio_bindings().sound_clocks()[&copied_owner][&sound];
        assert_eq!(pasted_clock.clocks().len(), captured_clock.clocks().len());
        for (old, copied) in captured_clock.clocks().iter().zip(pasted_clock.clocks()) {
            assert_eq!(copied.timing().allocation, revision("group-paste"));
            assert_ne!(copied.scope(), old.scope());
            assert_ne!(copied.owner(), old.owner());
        }
        assert_eq!(register_slice(&store.registers()?, 'a').to_json()?, saved_a);

        let recopy = capture_child(
            &pasted,
            pasted.root(),
            &node("group-paste-node-0"),
            "group-recapture",
        )?;
        let recaptured_sounds = slice_sounds(&recopy)?;
        assert_eq!(recaptured_sounds.len(), 1);
        assert_eq!(recaptured_sounds[&copied_owner][&sound], recipe);
        assert_eq!(
            captured_journal(&recopy, &copied_owner, &sound)?
                .clocks()
                .len(),
            pasted_clock.clocks().len() + 1,
        );
        let bank = store.save_register(
            pasted.project_id(),
            pasted.revision_id(),
            RegisterName::new('b')?,
            RegisterValue::Edited {
                slice: Arc::new(recopy),
            },
        )?;
        assert_eq!(register_slice(&bank, 'a').to_json()?, saved_a);
        store.commit(&paste(
            &pasted,
            &register_slice(&bank, 'b'),
            "group-repaste",
        )?)?;
        let twice = store.snapshot()?;
        assert_eq!(twice.duration()?, frames(29));
        assert_complete_recipes(&twice, &recipe, 5)?;
        drop(store);
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, twice);
        assert_eq!(store.registers()?, bank);
        store.undo(twice.revision_id(), revision("undo-repaste"))?;
        assert_authored(&store.snapshot()?, &pasted)?;
        store.undo(&revision("undo-repaste"), revision("undo-paste"))?;
        assert_authored(&store.snapshot()?, &grouped)?;
        store.undo(&revision("undo-paste"), revision("undo-group"))?;
        assert_authored(&store.snapshot()?, &before)?;
        store.redo(&revision("undo-group"), revision("redo-group"))?;
        assert_authored(&store.snapshot()?, &grouped)?;
        store.redo(&revision("redo-group"), revision("redo-paste"))?;
        assert_authored(&store.snapshot()?, &pasted)?;
        store.redo(&revision("redo-paste"), revision("redo-repaste"))?;
        assert_authored(&store.snapshot()?, &twice)?;
        assert_eq!(store.registers()?, bank);
        assert_eq!(register_slice(&bank, 'a').to_json()?, saved_a);
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

#[test]
fn endpoint_group_inside_retained_scope_still_refuses_without_durable_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = setup(scratch.path(), true)?;
    let current = store.snapshot()?;
    let sound = SoundId::new("overlay")?;
    let recipe = current.beat_sounds()[&node("owner")][&sound].clone();
    store.commit(&edit(
        &current,
        "remove-owner-event",
        Command::DeleteBeatSound {
            owner: node("owner"),
            id: sound.clone(),
        },
    ))?;
    // This neutral wrapper is legal outside the old scope. Its own newly
    // attached event will then make the wrapper the complete processing owner.
    store.commit(&edit(
        &store.snapshot()?,
        "make-processing-scope",
        Command::Group {
            parent: node("root"),
            start: 2,
            end: 3,
            id: node("top"),
            label: "Retained sound scope".into(),
        },
    ))?;
    store.commit(&edit(
        &store.snapshot()?,
        "attach-scope-event",
        Command::SetBeatSound {
            owner: node("top"),
            id: sound.clone(),
            event: recipe,
        },
    ))?;
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    store.commit(&edit(
        &store.snapshot()?,
        "shift-processing-scope",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: node("second-prefix"),
            identities: SplitIdentities::default(),
            timing: timing("shift-processing-scope"),
        },
    ))?;
    let before = store.snapshot()?;
    let journal = &before.audio_bindings().sound_clocks()[&node("top")][&sound];
    assert_eq!(journal.scope(), &node("top"));
    assert_eq!(journal.clocks().len(), 1);
    let selection = SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(6), ProjectFrame(12))?,
    };
    let plan = before.group_selection(&node("top"), &selection)?;
    assert_eq!(plan.required_split_ids, 5);
    let command = edit(
        &before,
        "refused-interior",
        Command::GroupSelection {
            parent: node("top"),
            selection,
            label: "Refused scope interior".into(),
            identities: GroupSelectionIdentities {
                group: node("refused-group"),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|i| NodeId::new(format!("refused-split-{i}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            },
            timing: timing("refused-interior"),
        },
    );
    let bank = store.registers()?;
    let rows = durable_tables(&path)?;
    for outcome in [
        store.preview(&command).map(|_| ()),
        store.commit(&command).map(|_| ()),
    ] {
        let error = outcome.unwrap_err();
        assert!(
            error.to_string().contains(
                "group ownership changes inside a retained beat sound scope are not yet supported",
            ),
            "{error}",
        );
    }
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.registers()?, bank);
    assert_eq!(durable_tables(&path)?, rows);
    store.validate()?;
    Ok(())
}
